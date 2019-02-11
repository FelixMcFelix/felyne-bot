use crate::constants::*;
use crate::VoiceManager;

use parking_lot::Mutex;
use rand::{thread_rng, Rng};
use rand::distributions::*;
use serenity::client::*;
use serenity::client::bridge::voice::ClientVoiceManager;
use serenity::model::prelude::*;
use serenity::voice::{ffmpeg, AudioReceiver, LockedAudio};
use std::collections::hash_map::{HashMap, Entry};
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, channel, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use typemap::Key;

struct VoiceHuntReceiver;

impl VoiceHuntReceiver {
	pub fn new() -> Self { 
		Self { }
	}
}

impl AudioReceiver for VoiceHuntReceiver {
	fn speaking_update(&mut self, _ssrc: u32, _user_id: u64, _speaking: bool) {
		// You can implement logic here so that you can differentiate users'
		// SSRCs and map the SSRC to the User ID and maintain a state in
		// `Receiver`. Using this map, you can map the `ssrc` in `voice_packet`
		// to the user ID and handle their audio packets separately.
	}

	fn voice_packet(&mut self, _ssrc: u32, _sequence: u16, _timestamp: u32, _stereo: bool, _data: &[i16]) {
		// println!("Audio packet's first 5 bytes: {:?}", data.get(..5));
		// println!(
		// 	"Audio packet sequence {:05} has {:04} bytes, SSRC {}",
		// 	sequence,
		// 	data.len(),
		// 	ssrc,
		// );
	}
}

pub struct VoiceHunt;

impl Key for VoiceHunt {
	type Value = HashMap<GuildId, VHState>;
}

#[derive(Debug)]
pub enum VoiceHuntCommand {
	Carted,
	BraveHunt,
	DirectedHunt(ChannelId),
	Volume(f32),
}

#[derive(Debug)]
enum VoiceHuntMessage {
	Channel(ChannelId),
	NoChannel,
	Volume(f32),
	Cart,
}

#[derive(Debug)]
enum VoiceHuntResponse {
	Done,
}

#[derive(Debug)]
struct Incumbent(u64, ChannelId);

#[derive(Debug)]
pub struct VHState {
	guild_id: GuildId,
	user_id: UserId,
	user_states: HashMap<UserId, VoiceState>,
	population_counts: HashMap<ChannelId, u64>,

	join_mode: VoiceHuntCommand,
	volume: f32,

	active_channel: Option<ChannelId>,
	incumbent_channel: Option<Incumbent>,

	huntsim_tx: Option<Arc<Mutex<Sender<VoiceHuntMessage>>>>,
	huntsim_rx: Option<Arc<Mutex<Receiver<VoiceHuntResponse>>>>,
}

impl VHState {
	fn new(guild_id: GuildId, user_id: UserId) -> Self {
		VHState {
			guild_id,
			user_id,
			user_states: HashMap::new(),
			population_counts: HashMap::new(),

			join_mode: VoiceHuntCommand::Carted,
			volume: 1.0,

			active_channel: None,
			incumbent_channel: None,

			huntsim_tx: None,
			huntsim_rx: None,
		}
	}

	fn control(&mut self, vox_manager: Arc<Mutex<ClientVoiceManager>>, mode: VoiceHuntCommand) -> &mut Self {
		// Note: we can read from the ShareMap because entrant code is guaranteed to have the lock.
		use crate::VoiceHuntCommand::*;

		if let Carted = self.join_mode {
			match mode {
				Carted => {},
				Volume(_) => {},
				_ => {
					// Moving from Carted to active mode.
					// Spawn thread.
					let (sender, receiver) = channel();
					let (reverse_sender, reverse_receiver) = channel();
					let guild_id = self.guild_id;
					let vol = self.volume;

					// Begin!
					thread::spawn(move || {
						// Init state here
						felyne_life(receiver, reverse_sender, vox_manager, guild_id, vol);
					});

					self.huntsim_tx = Some(Arc::new(Mutex::new(sender)));
					self.huntsim_rx = Some(Arc::new(Mutex::new(reverse_receiver)));
				}
			}
		}


		let chan_change = match mode {
			Carted => {
				self.send(VoiceHuntMessage::Cart);
				if let Some(ref rx_safe) = self.huntsim_rx{
					let rx_lock = rx_safe.clone();
					let rx = rx_lock.lock();

					'waitdone: loop {
						match rx.try_recv() {
							Ok(VoiceHuntResponse::Done) => {
								break 'waitdone;
							},
							Err(TryRecvError::Empty) => {},
							Err(TryRecvError::Disconnected) => {
								break 'waitdone;	
							},
						}
					}
				}

				self.huntsim_tx = None;
				self.huntsim_rx = None;
				self.active_channel = None;
				self.join_mode = mode;
				false
			},
			DirectedHunt(chan) => {
				// Force new state.
				self.active_channel = Some(chan);
				self.join_mode = mode;
				true
			},
			BraveHunt => {
				// Delete forced state.
				self.active_channel = None;
				self.join_mode = mode;
				true
			},
			Volume(vol) => {
				self.send(VoiceHuntMessage::Volume(vol));
				self.volume = vol;
				false
			}
		};

		if chan_change {self.update_channel();} // Now set the channel.

		self
	}

	fn send(&mut self, msg: VoiceHuntMessage) {
		if let Some(lock_arc) = self.huntsim_tx.as_ref() {
			let lock = lock_arc.clone();
			let tx = lock.lock();

			if let Err(e) = tx.send(msg) {
				println!("[VoiceHunt] Failed to send message: {:?}", e);
			}
		}
	}

	fn register_user_states(&mut self, voice_states: HashMap<UserId, VoiceState>) -> &mut Self {
		self.user_states = voice_states;

		let mut scan_incumbent = false;

		for vox in self.user_states.clone().values() {
			scan_incumbent |= self.register_user_state(vox, false);
		}

		if scan_incumbent {
			self.recalc_incumbent();
		}
		self.update_channel();

		self
	}

	fn register_user_state(&mut self, state: &VoiceState, do_update: bool) -> bool {
		if state.user_id == self.user_id {
			return false;
		}

		let mut scan_incumbent = false;
		if let Entry::Occupied(prior_state) = self.user_states.clone().entry(state.user_id) {
			if let Some(channel) = prior_state.get().channel_id {
				let count = {
					let v = self.population_counts.entry(channel).or_insert(1);
					*v = *v.max(&mut 1) - 1;
					*v
				};

				// If we lower the incumbent, we need to do a rescan.
				scan_incumbent |= self.update_incumbent(count, channel);
			}
		}

		if let Some(channel) = state.channel_id {
			let count = {
				let v = self.population_counts.entry(channel).or_insert(0);
				*v += 1;
				*v
			};

			self.update_incumbent(count, channel);
		}

		self.user_states.insert(state.user_id, state.clone());

		if do_update {
			self.recalc_incumbent();
			self.update_channel();
			return false;
		}

		scan_incumbent
	}

	fn recalc_incumbent(&mut self) {
		for (chan, count) in self.population_counts.clone().iter() {
			self.update_incumbent(count.clone(), chan.clone());
		}
	}

	fn update_incumbent(&mut self, count: u64, channel: ChannelId) -> bool {
		if let Some(Incumbent(count_old, chan_old)) = self.incumbent_channel {
			if chan_old == channel || count > count_old {
				self.incumbent_channel = if count == 0 {None} else {Some(Incumbent(count, channel))};

				count < count_old
			} else {
				false
			}
		} else {
			self.incumbent_channel = if count == 0 {None} else {Some(Incumbent(count, channel))};
			false
		}
	}

	fn update_channel(&mut self) {
		println!("{:?}", self.population_counts);
		if let Some(chan) = self.active_channel {
			self.send(VoiceHuntMessage::Channel(chan));
		} else if let Some(Incumbent(_, chan)) = self.incumbent_channel {
			self.send(VoiceHuntMessage::Channel(chan));
		} else {
			self.send(VoiceHuntMessage::NoChannel);
		}
	}
}

#[inline]
fn quit_vox_channel(manager_lock: Arc<Mutex<ClientVoiceManager>>, guild_id: GuildId) {
	let mut manager = manager_lock.lock();

	if let Some(handler) = manager.get_mut(guild_id) {
		handler.stop();
	}

	manager.leave(guild_id);
}

fn random_element<'a, T, R: Rng>(arr: &'a[T], rng: &mut R) -> &'a T {
	&arr[Range::new(0, arr.len()).ind_sample(rng)]
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum BgmClass {
	NoBgm,
	Intro,
	Ambience,
	Music,
	Bonuser,
	Force,
}

impl BgmClass {
	fn no_gargwa(self) -> bool {
		use self::BgmClass::*;

		self == Music ||
		self == Bonuser ||
		self == Force
	}
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum SfxClass {
	NoSfx,
	Cat,
	Bonus,
	Force,
}

fn felyne_life(rx: Receiver<VoiceHuntMessage>, tx: Sender<VoiceHuntResponse>, manager_lock: Arc<Mutex<ClientVoiceManager>>, guild_id: GuildId, vol: f32) {
	let timer = Duration::from_millis(VOICEHUNT_FRAME_TIME);
	let mut rng = thread_rng();
	let vol_range = Range::new(0.3,0.4);
	let bgm_vol_range = Range::new(0.15,0.2);
	let music_vol = 0.3;

	let noice_range = Range::new(600, 7_000);
	let bonus_time_range = Range::new(20_000,60_000);
	let bonuser_time_range = Range::new(600_000,1_200_000);
	let bgm_time_range = Range::new(300_000,600_000);

	let mut curr_vol = vol;

	let mut curr_noice: Option<LockedAudio> = None;
	let mut curr_bgm: Option<LockedAudio> = None;
	let mut outro: Option<LockedAudio> = None;

	let mut curr_noice_class = SfxClass::NoSfx;
	let mut curr_bgm_class = BgmClass::NoBgm;

	let mut force_next_aud: Option<&str> = None;
	let mut force_next_bgm: Option<&str> = None;

	let mut last_noice = Instant::now();
	let mut last_noice_bonus = Instant::now();
	let mut last_bgm_bonuser = Instant::now();
	let mut last_bgm_intro = Instant::now();
	let mut last_bgm = Instant::now();

	let mut next_noice = Duration::new(0, 0);
	let mut next_noice_bonus = Duration::from_millis(bonus_time_range.ind_sample(&mut rng));
	let mut next_bgm_bonuser = Duration::from_millis(bonuser_time_range.ind_sample(&mut rng));
	let mut next_bgm_intro = Duration::from_secs(0);
	let mut next_bgm = Duration::from_millis(bgm_time_range.ind_sample(&mut rng));

	let mut leaving = false;

	let mut curr_chan = None;

	// {
	// 	let mut manager = manager_lock.lock();
	// 	println!("---\n{:?}", *manager);
	// }

	'escape: loop {
		match rx.try_recv() {
			Ok(VoiceHuntMessage::Channel(chan)) => {
				let new_join = match curr_chan {
					Some(chan_old) => chan_old != chan,
					None => true,
				};

				// Connect to a different vox channel.
				let mut manager = manager_lock.lock();

				if new_join {
					if manager.join(guild_id, chan).is_some() {
						// test play
						let handler = manager.get_mut(guild_id).unwrap();
						// Testing voice receive---example 10.
						// GOAL: ducking!
						handler.listen(Some(Box::new(VoiceHuntReceiver::new())));

						let source = ffmpeg(format!("bgm/{}",
							if last_bgm_intro.elapsed() > next_bgm_intro {
								last_bgm_intro = Instant::now();
								next_noice = Duration::from_secs(13);
								next_bgm_intro = Duration::from_secs(300);
								curr_bgm_class = BgmClass::Intro;
								random_element(START, &mut rng)
							} else {
								curr_bgm_class = BgmClass::Ambience;
								random_element(AMBIENCE, &mut rng)
							})).unwrap();

						let safe_aud = handler.play_returning(source);

						{
							let aud_lock = safe_aud.clone();
							let mut aud = aud_lock.lock();

							aud.volume(1.0 * curr_vol);
						}

						curr_bgm = Some(safe_aud);
						curr_chan = Some(chan);
					} else {
						println!("Failed to connect to {:?}!", chan);
					}
				}
			},
			Ok(VoiceHuntMessage::NoChannel) => {
				quit_vox_channel(manager_lock.clone(), guild_id);
				curr_chan = None;
			},
			Ok(VoiceHuntMessage::Cart) => {
				if leaving {
					quit_vox_channel(manager_lock.clone(), guild_id);
					break 'escape;
				} else {
					leaving = true;

					if let Some(ref safe) = curr_noice {
						let lock = safe.clone();
						let mut aud = lock.lock();

						aud.pause();
					}

					if let Some(ref safe) = curr_bgm {
						let lock = safe.clone();
						let mut aud = lock.lock();

						aud.pause();
					}

					let mut manager = manager_lock.lock();
					
					if let Some(handler) = manager.get_mut(guild_id){
						let source = ffmpeg(format!("sfx/{}", SLEEP)).unwrap();
						
						let safe_aud = handler.play_returning(source);
		
						{
							let aud_lock = safe_aud.clone();
							let mut aud = aud_lock.lock();
		
							aud.volume(0.6 * curr_vol);
						}
	
						outro = Some(safe_aud);
					}
				}
			},
			Ok(VoiceHuntMessage::Volume(new_vol)) => {
				if let Some(ref safe) = curr_noice {
					let lock = safe.clone();
					let mut aud = lock.lock();

					aud.volume /= curr_vol;
					aud.volume *= new_vol;
				}

				if let Some(ref safe) = curr_bgm {
					let lock = safe.clone();
					let mut aud = lock.lock();

					aud.volume /= curr_vol;
					aud.volume *= new_vol;
				}

				curr_vol = new_vol;
			},
			Err(TryRecvError::Empty) => {
				// If we receieved nothing, then we can perform an update.
				// Iteration, then wait.

				let play_new = curr_bgm_class != BgmClass::Bonuser && (curr_noice.is_none() || {
					let lock = curr_noice.as_ref().expect("wtf").clone();
					let aud = lock.lock();

					if aud.finished {
						curr_noice_class = SfxClass::NoSfx;
					}

					aud.finished
				});

				let play_new_bgm = curr_bgm.is_none() || {
					let lock = curr_bgm.as_ref().expect("wtf").clone();
					let aud = lock.lock();

					if aud.finished {
						curr_bgm_class = BgmClass::NoBgm;
					}

					aud.finished
				};

				if play_new || play_new_bgm {
					let mut manager = manager_lock.lock();
					
					if let Some(handler) = manager.get_mut(guild_id){
						if play_new {
							let sfx_name =
								if let Some(aud_name) = force_next_aud {
									force_next_aud = None;
									last_noice = Instant::now();
									next_noice = Duration::from_millis(0);
									curr_noice_class = SfxClass::Force;
									aud_name
								} else if last_noice_bonus.elapsed() > next_noice_bonus && !curr_bgm_class.no_gargwa() {
									last_noice_bonus = Instant::now();
									next_noice_bonus = Duration::from_millis(bonus_time_range.ind_sample(&mut rng));
									curr_noice_class = SfxClass::Bonus;
									random_element(BONUS_SFX, &mut rng)
								} else if last_noice.elapsed() > next_noice {
									last_noice = Instant::now();
									next_noice = Duration::from_millis(noice_range.ind_sample(&mut rng));
									curr_noice_class = SfxClass::Cat;
									random_element(SFX, &mut rng)
								} else {
									""
								};

							if !sfx_name.is_empty(){

								let source = ffmpeg(format!("sfx/{}", sfx_name)).unwrap();

								let safe_aud = handler.play_returning(source);
			
								{
									let aud_lock = safe_aud.clone();
									let mut aud = aud_lock.lock();
				
									aud.volume(vol_range.ind_sample(&mut rng) * curr_vol);
								}
			
								curr_noice = Some(safe_aud);
							}
						}

						if play_new_bgm {

							let bgm_name =
								if let Some(aud_name) = force_next_bgm {
									force_next_bgm = None;
									last_bgm = Instant::now();
									next_bgm = Duration::from_millis(0);
									curr_bgm_class = BgmClass::Force;
									aud_name
								} else if last_bgm_bonuser.elapsed() > next_bgm_bonuser {
									last_bgm_bonuser = Instant::now();
									next_bgm_bonuser = Duration::from_millis(bonuser_time_range.ind_sample(&mut rng));
									curr_bgm_class = BgmClass::Bonuser;
									force_next_bgm = Some(&random_element(BONUSER_SFX_FOLLOW, &mut rng));
									BONUSER_SFX
								} else if last_bgm.elapsed() > next_bgm {
									last_bgm = Instant::now();
									next_bgm = Duration::from_millis(bgm_time_range.ind_sample(&mut rng));
									curr_bgm_class = BgmClass::Music;
									random_element(BGM, &mut rng)
								} else {
									curr_bgm_class = BgmClass::Ambience;
									random_element(AMBIENCE, &mut rng)
								};

							if !bgm_name.is_empty(){

								let source2 = ffmpeg(format!("bgm/{}", bgm_name)).unwrap();

								let safe_aud2 = handler.play_returning(source2);
				
								{
									let aud_lock = safe_aud2.clone();
									let mut aud = aud_lock.lock();
				
									aud.volume(if curr_bgm_class.no_gargwa() {music_vol} else {bgm_vol_range.ind_sample(&mut rng)} * curr_vol);
								}
			
								curr_bgm = Some(safe_aud2);
							}
						}
					}
				}

				let outro_done = outro.is_some() && {
					let lock = outro.as_ref().expect("wtf").clone();
					let aud = lock.lock();

					aud.finished
				};

				if leaving && outro_done {
					quit_vox_channel(manager_lock.clone(), guild_id);
					break 'escape;
				}

				thread::sleep(timer);
			},
			Err(TryRecvError::Disconnected) => {
				break 'escape;
			},
		}
	}
	tx.send(VoiceHuntResponse::Done)
		.unwrap_or_else(|_|
			panic!("[VoiceHunt] Final Done send for {:?}'s handler failed.", guild_id)
		);
}

pub fn voicehunt_control(ctx: &Context, guild_id: GuildId, mode: VoiceHuntCommand) {
	let mut datas = ctx.data.write();
	let voice_manager_lock = datas.get::<VoiceManager>().cloned().unwrap().clone();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, ctx.cache.read().user.id))
		.control(voice_manager_lock, mode);
}


pub fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let mut datas = ctx.data.write();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, ctx.cache.read().user.id))
		.register_user_state(&vox, true);
}

pub fn voicehunt_complete_update(ctx: &Context, guild_id: GuildId, voice_states: HashMap<UserId, VoiceState>) {
	let mut datas = ctx.data.write();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, ctx.cache.read().user.id))
		.register_user_states(voice_states);
}
