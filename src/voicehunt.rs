use constants::*;
use VoiceManager;

use parking_lot::Mutex;
use rand::{random, thread_rng, Rng};
use rand::distributions::*;
use serenity::client::*;
use serenity::client::bridge::voice::ClientVoiceManager;
use serenity::model::prelude::*;
use serenity::utils::*;
use serenity::voice::{ffmpeg, LockedAudio};
use std::collections::hash_map::{HashMap, Entry};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, channel, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use typemap::{Key, ShareMap};

pub struct VoiceHunt;

impl Key for VoiceHunt {
	type Value = HashMap<GuildId, VHState>;
}

#[derive(Debug)]
pub enum VoiceHuntJoinMode {
	Carted,
	BraveHunt,
	DirectedHunt(ChannelId),
}

#[derive(Debug)]
enum VoiceHuntMessage {
	Channel(ChannelId),
	NoChannel,
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

	join_mode: VoiceHuntJoinMode,

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

			join_mode: VoiceHuntJoinMode::Carted,

			active_channel: None,
			incumbent_channel: None,

			huntsim_tx: None,
			huntsim_rx: None,
		}
	}

	fn join_control(&mut self, vox_manager: Arc<Mutex<ClientVoiceManager>>, mode: VoiceHuntJoinMode) -> &mut Self {
		// Note: we can read from the ShareMap because entrant code is guaranteed to have the lock.
		use VoiceHuntJoinMode::*;

		if let Carted = self.join_mode {
			match mode {
				Carted => {},
				_ => {
					// Moving from Carted to active mode.
					// Spawn thread.
					let (sender, receiver) = channel();
					let (reverse_sender, reverse_receiver) = channel();
					let guild_id = self.guild_id;			

					// Begin!
					thread::spawn(move || {
						// Init state here
						felyne_life(receiver, reverse_sender, vox_manager, guild_id);
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
				false
			},
			DirectedHunt(chan) => {
				// Force new state.
				self.active_channel = Some(chan);
				true
			},
			BraveHunt => {
				// Delete forced state.
				self.active_channel = None;
				true
			}
		};

		if chan_change {self.update_channel();} // Now set the channel.

		self.join_mode = mode;

		self
	}

	fn send(&mut self, msg: VoiceHuntMessage) {
		match self.huntsim_tx.as_ref() {
			Some(lock_arc) => {
				let lock = lock_arc.clone();
				let tx = lock.lock();
				tx.send(msg);
			},
			None => {},
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
					*v -= 1;
					v.clone()
				};

				// If we lower the incumbent, we need to do a rescan.
				scan_incumbent |= self.update_incumbent(count, channel);
			}
		}

		if let Some(channel) = state.channel_id {
			let count = {
				let v = self.population_counts.entry(channel).or_insert(0);
				*v += 1;
				v.clone()
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

	let is_in_voicechat_here = match manager.get_mut(guild_id) {
		Some(handler) => {handler.stop(); true}
		None => false,
	};

	if is_in_voicechat_here {
		manager.remove(guild_id);
	}
}

fn random_element<'a, T, R: Rng>(arr: &'a[T], rng: &mut R) -> &'a T {
	&arr[Range::new(0, arr.len()).ind_sample(rng)]
}

fn felyne_life(rx: Receiver<VoiceHuntMessage>, tx: Sender<VoiceHuntResponse>, manager_lock: Arc<Mutex<ClientVoiceManager>>, guild_id: GuildId) {
	let timer = Duration::from_millis(VOICEHUNT_FRAME_TIME);
	let mut rng = thread_rng();
	let vol_range = Range::new(0.2,0.5);
	let bgm_vol_range = Range::new(0.15,0.2);

	let mut curr_noice: Option<LockedAudio> = None;
	let mut curr_bgm: Option<LockedAudio> = None;
	let mut outro: Option<LockedAudio> = None;

	let mut last_noice = Instant::now();
	let mut last_bgm_intro = Instant::now();

	let mut next_noice = Duration::new(0, 0);
	let mut next_bgm_intro = Duration::from_secs(0);
	let mut next_bgm = Duration::new(0, 0);

	let mut leaving = false;

	'escape: loop {
		match rx.try_recv() {
			Ok(VoiceHuntMessage::Channel(chan)) => {
				// Reset state!
				curr_noice = None;
				curr_bgm = None;

				// Connect to a different vox channel.
				let mut manager = manager_lock.lock();

				if manager.join(guild_id, chan).is_some() {
					// test play
					let mut handler = manager.get_mut(guild_id).unwrap();

					let source = ffmpeg(format!("bgm/{}",
						if last_bgm_intro.elapsed() > next_bgm_intro {
							last_bgm_intro = Instant::now();
							next_noice = Duration::from_secs(13);
							next_bgm_intro = Duration::from_secs(300);
							random_element(START, &mut rng)
						} else {
							random_element(AMBIENCE, &mut rng)
						})).unwrap();

					let safe_aud = handler.play_returning(source);

					{
						let aud_lock = safe_aud.clone();
						let mut aud = aud_lock.lock();

						aud.volume(1.0);
					}

					curr_bgm = Some(safe_aud);
				} else {
					println!("Failed to connect to {:?}!", chan);
				}
			},
			Ok(VoiceHuntMessage::NoChannel) => {
				quit_vox_channel(manager_lock.clone(), guild_id);
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
					
					if let Some(mut handler) = manager.get_mut(guild_id){
						let source = ffmpeg(format!("sfx/{}", SLEEP)).unwrap();
						
						let safe_aud = handler.play_returning(source);
		
						{
							let aud_lock = safe_aud.clone();
							let mut aud = aud_lock.lock();
		
							aud.volume(0.6);
						}
	
						outro = Some(safe_aud);
					}
				}
			},
			Err(TryRecvError::Empty) => {
				// If we receieved nothing, then we can perform an update.
				// Iteration, then wait.

				let play_new = curr_noice.is_none() || {
					let lock = curr_noice.as_ref().expect("wtf").clone();
					let aud = lock.lock();

					aud.finished
				};

				let play_new_bgm = curr_bgm.is_none() || {
					let lock = curr_bgm.as_ref().expect("wtf").clone();
					let aud = lock.lock();

					aud.finished
				};

				if play_new || play_new_bgm {
					let mut manager = manager_lock.lock();
					
					if let Some(mut handler) = manager.get_mut(guild_id){
						if play_new {
							
							if last_noice.elapsed() > next_noice {
								last_noice = Instant::now();
								next_noice = Duration::from_millis(Range::new(600, 7000).ind_sample(&mut rng));

								let source = ffmpeg(format!("sfx/{}", random_element(SFX, &mut rng))).unwrap();

								let safe_aud = handler.play_returning(source);
			
								{
									let aud_lock = safe_aud.clone();
									let mut aud = aud_lock.lock();
				
									aud.volume(vol_range.ind_sample(&mut rng));
								}
			
								curr_noice = Some(safe_aud);
							};
						}

						if play_new_bgm{
							let source2 = ffmpeg(format!("bgm/{}", random_element(AMBIENCE, &mut rng))).unwrap();
							
							let safe_aud2 = handler.play_returning(source2);
			
							{
								let aud_lock = safe_aud2.clone();
								let mut aud = aud_lock.lock();
			
								aud.volume(bgm_vol_range.ind_sample(&mut rng));
							}
		
							curr_bgm = Some(safe_aud2);
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
	tx.send(VoiceHuntResponse::Done);
}

pub fn voicehunt_control(ctx: &Context, guild_id: GuildId, mode: VoiceHuntJoinMode) {
	let mut datas = ctx.data.lock();
	let voice_manager_lock = datas.get::<VoiceManager>().cloned().unwrap().clone();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id, CACHE.read().user.id))
		.join_control(voice_manager_lock, mode);
}


pub fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let mut datas = ctx.data.lock();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id, CACHE.read().user.id))
		.register_user_state(&vox, true);
}

pub fn voicehunt_complete_update(ctx: &Context, guild_id: GuildId, voice_states: HashMap<UserId, VoiceState>) {
	let mut datas = ctx.data.lock();
	datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id, CACHE.read().user.id))
		.register_user_states(voice_states);
}
