pub mod receiver;

use crate::{automata::*, constants::*, CachedSound, Resources, RxMap};
use crossbeam::channel::{self, Receiver, Sender, TryRecvError};
use dashmap::DashMap;
use log::*;
use rand::{distributions::*, random, thread_rng, Rng};
use receiver::{listen_in, ReceiverSignal, VoiceHuntReceiver};
use serenity::{async_trait, client::*, model::prelude::*, prelude::*};
use songbird::{
	events::{Event, EventContext, EventHandler, TrackEvent},
	ffmpeg,
	serenity::SongbirdKey,
	tracks::{Track, TrackCommand, TrackHandle},
	Call,
	Songbird,
};
use std::{
	collections::hash_map::{Entry, HashMap},
	sync::Arc,
	thread,
	time::{Duration, Instant},
};
use tokio::time;

pub struct VoiceHunt;

impl TypeMapKey for VoiceHunt {
	type Value = HashMap<GuildId, VHState>;
}

#[derive(Debug)]
pub enum VoiceHuntCommand {
	Carted,
	BraveHunt,
	Stalk,
	DirectedHunt(ChannelId),
	Volume(f32),
}

#[derive(Debug)]
enum VoiceHuntMessage {
	Channel(ChannelId, bool),
	Stealth,
	Unstealth,
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

	huntsim_tx: Option<Sender<VoiceHuntMessage>>,
	huntsim_rx: Option<Receiver<VoiceHuntResponse>>,
}

impl VHState {
	fn new(
		guild_id: GuildId,
		user_id: UserId,
		vox_manager: Arc<Mutex<Call>>,
		resources: RxMap,
	) -> Self {
		// NOTE: will need some further changes if I want to start
		// in the Stalk or BraveHunt states...
		let mut out = VHState {
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
		};

		out.control(vox_manager, VoiceHuntCommand::Stalk, resources);

		out
	}

	fn launch_felyne_thread(&mut self, vox_manager: Arc<Mutex<Call>>, resources: RxMap) {
		let (sender, receiver) = channel::unbounded();
		let (reverse_sender, reverse_receiver) = channel::unbounded();
		let guild_id = self.guild_id;
		let vol = self.volume;

		let self_tx = sender.clone();

		// Begin!
		tokio::spawn(async move {
			// Init state here
			felyne_life(
				receiver,
				reverse_sender,
				vox_manager,
				guild_id,
				vol,
				self_tx,
				resources,
			)
			.await;
		});

		self.huntsim_tx = Some(sender);
		self.huntsim_rx = Some(reverse_receiver);
	}

	fn control(
		&mut self,
		vox_manager: Arc<Mutex<Call>>,
		mode: VoiceHuntCommand,
		resources: RxMap,
	) -> &mut Self {
		// Note: we can read from the ShareMap because entrant code is guaranteed to have the lock.
		use crate::VoiceHuntCommand::*;

		if let Carted = self.join_mode {
			match mode {
				Carted => {},
				Volume(_) => {},
				_ => {
					// Moving from Carted to active mode.
					// Spawn thread.
					self.launch_felyne_thread(vox_manager, resources);
				},
			}
		}

		let chan_change = match mode {
			Carted => {
				self.send(VoiceHuntMessage::Cart);
				if let Some(rx) = self.huntsim_rx.as_ref() {
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
				self.send(VoiceHuntMessage::Unstealth);
				true
			},
			BraveHunt => {
				// Delete forced state.
				self.active_channel = None;
				self.join_mode = mode;
				self.send(VoiceHuntMessage::Unstealth);
				true
			},
			Stalk => {
				// Delete forced state.
				// And instruct voice thread to not play anything...
				self.active_channel = None;
				self.join_mode = mode;
				self.send(VoiceHuntMessage::Stealth);
				true
			},
			Volume(vol) => {
				self.send(VoiceHuntMessage::Volume(vol));
				self.volume = vol;
				false
			},
		};

		if chan_change {
			self.update_channel();
		} // Now set the channel.

		self
	}

	fn send(&mut self, msg: VoiceHuntMessage) {
		if let Some(tx) = self.huntsim_tx.as_ref() {
			if let Err(e) = tx.send(msg) {
				warn!("[VoiceHunt] Failed to send message: {:?}", e);
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
				self.incumbent_channel = if count == 0 {
					None
				} else {
					Some(Incumbent(count, channel))
				};

				count < count_old
			} else {
				false
			}
		} else {
			self.incumbent_channel = if count == 0 {
				None
			} else {
				Some(Incumbent(count, channel))
			};
			false
		}
	}

	fn update_channel(&mut self) {
		info!("{:?}", self.population_counts);
		if let Some(chan) = self.active_channel {
			self.send(VoiceHuntMessage::Channel(chan, true));
		} else if let Some(Incumbent(_, chan)) = self.incumbent_channel {
			self.send(VoiceHuntMessage::Channel(chan, false));
		} else {
			self.send(VoiceHuntMessage::NoChannel);
		}
	}
}

#[inline]
async fn quit_vox_channel(
	manager_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	receiver_chan: &mut Option<Sender<ReceiverSignal>>,
) {
	let mut manager = manager_lock.lock().await;

	manager.stop();
	if let Some(chan) = receiver_chan {
		chan.send(ReceiverSignal::Poison);
	}
	*receiver_chan = None;

	manager.leave().await;
}

#[inline]
fn random_element<'a, T>(arr: &'a [T]) -> &'a T {
	let mut rng = thread_rng();
	&arr[Uniform::new(0, arr.len()).sample(&mut rng)]
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
enum BgmClass {
	NoBgm,
	Intro,
	Ambience,
	Music,
	Bonus,
	BonusResult,
	Outro,
}

impl BgmClass {
	fn no_gargwa(self) -> bool {
		use BgmClass::*;

		self == Music || self == Bonus || self == BonusResult
	}
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
enum BgmInput {
	TryIntro,
	Advance,
	MoveOutro,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
enum SfxClass {
	NoSfx,
	Cat,
	Bonus,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
enum SfxInput {
	Advance,
}

fn play_bgm(
	state: BgmClass,
	vox: &mut Call,
	stealth: bool,
	resources: &RxMap,
	donezo: &Sender<FelyneEvt>,
) -> Option<TrackHandle> {
	use BgmClass::*;

	if stealth {
		return None;
	}

	println!("Trying to play...");

	let el_list = match state {
		NoBgm => {
			return None;
		},
		Intro => START,
		Ambience => AMBIENCE,
		Music => BGM,
		Bonus => BBQ,
		BonusResult => BBQ_RESULT,
		Outro => SLEEP,
	};

	let chan = donezo.clone();

	resources
		.get(random_element(el_list))
		.map(|guard| vox.play_source(guard.value().into()))
		.map(move |track| {
			track.add_event(
				Event::Track(TrackEvent::End),
				FelyneEndTrack {
					chan,
					msg: FelyneEvt::BgmEnd,
				},
			);
			track
		})
}

struct FelyneEndTrack {
	chan: Sender<FelyneEvt>,
	msg: FelyneEvt,
}

#[async_trait]
impl EventHandler for FelyneEndTrack {
	async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
		self.chan.send(self.msg);
		println!("Died. Sent a message... {:?}", self.msg);
		None
	}
}

fn play_sfx(
	state: SfxClass,
	vox: &mut Call,
	stealth: bool,
	resources: &RxMap,
	donezo: &Sender<FelyneEvt>,
) -> Option<TrackHandle> {
	use SfxClass::*;

	if stealth {
		return None;
	}

	println!("Trying to play... sfx");

	let el_list = match state {
		NoSfx => {
			return None;
		},
		Cat => SFX,
		Bonus => BONUS_SFX,
	};

	let chan = donezo.clone();

	resources
		.get(random_element(el_list))
		.map(|guard| vox.play_source(guard.value().into()))
		.map(move |track| {
			track.add_event(
				Event::Track(TrackEvent::End),
				FelyneEndTrack {
					chan,
					msg: FelyneEvt::SfxEnd,
				},
			);
			track
		})
}

enum WaitState {
	Limited,
	Queued(ChannelId),
}

#[derive(Clone, Copy, Debug)]
enum FelyneEvt {
	BgmEnd,
	SfxEnd,
}

async fn felyne_life(
	rx: Receiver<VoiceHuntMessage>,
	tx: Sender<VoiceHuntResponse>,
	manager_lock: Arc<Mutex<Call>>,
	guild_id: GuildId,
	vol: f32,
	self_tx: Sender<VoiceHuntMessage>,
	resources: RxMap,
) {
	println!("In felyne loop");
	let timer = Duration::from_millis(VOICEHUNT_FRAME_TIME);
	let vol_range = Uniform::new(0.3, 0.4);
	let bgm_vol_range = Uniform::new(0.15, 0.2);
	let music_vol = 0.3;

	let (sound_tx, sound_rx) = channel::bounded(2);

	let mut curr_vol = vol;

	let mut curr_sfx: Option<TrackHandle> = None;
	let mut curr_bgm: Option<TrackHandle> = None;

	let mut leaving = false;

	let mut curr_chan = None;
	let next_chan = Arc::new(Mutex::new(None));

	let mut stealthy = false;

	let mut bgm_machine = TimedMachine::new(BgmClass::NoBgm);

	let mut receiver_chan = None;
	{
		use BgmClass::*;
		use BgmInput::*;
		bgm_machine
			// Intro (only once per few runs ideally).
			.add_priority_transition(
				NoBgm, Intro, TryIntro, 0,
				Some(Cooldown::new(Duration::from_secs(300).into(), false, false))
				)
			.add_transition(Intro, Ambience, Advance)
			.add_transition(NoBgm, Ambience, Advance)
			// Main ambience/bgm/bonus loop.
			// Self.
			.add_transition(Ambience, Ambience, Advance)

			// In/out BGM.
			.add_priority_transition(
				Ambience, Music, Advance, 1,
				Some(Cooldown::new(
					Uniform::new(Duration::from_secs(300), Duration::from_secs(600)).into(),
					true,
					true,
				)))
			.add_transition(Music, Ambience, Advance)

			// Poogie + Result + Out.
			.add_priority_transition(
				Ambience, Bonus, Advance, 2,
				Some(Cooldown::new(
					Uniform::new(Duration::from_secs(600), Duration::from_secs(1200)).into(),
					true,
					true,
				)))
			.add_transition(Bonus, BonusResult, Advance)
			.add_transition(BonusResult, Ambience, Advance)
			.all_transition(Outro, MoveOutro);
	}

	let mut sfx_machine = TimedMachine::new(SfxClass::NoSfx);
	{
		use SfxClass::*;
		use SfxInput::*;
		sfx_machine
			.add_priority_transition(
				NoSfx,
				Cat,
				Advance,
				0,
				Some(Cooldown::new(
					Uniform::new(Duration::from_millis(600), Duration::from_millis(7_000)).into(),
					true,
					false,
				)),
			)
			.add_transition(Cat, NoSfx, Advance)
			.add_priority_transition(
				NoSfx,
				Bonus,
				Advance,
				1,
				Some(Cooldown::new(
					Uniform::new(Duration::from_secs(20), Duration::from_secs(60)).into(),
					true,
					true,
				)),
			)
			.add_transition(Bonus, NoSfx, Advance);
	}

	'escape: loop {
		match rx.try_recv() {
			Ok(VoiceHuntMessage::Channel(chan, demand)) => {
				println!("Channel");
				let new_join = match curr_chan {
					Some(chan_old) => chan_old != chan,
					None => true,
				};

				// Connect to a different vox channel.
				let mut manager = manager_lock.lock().await;

				if new_join {
					if !demand {
						let spawn_joiner = {
							let mut nc = next_chan.lock().await;
							let already_spawned = nc.is_some();
							if already_spawned {
								*nc = Some(WaitState::Queued(chan));
							} else {
								*nc = Some(WaitState::Limited);
							}

							!already_spawned
						};

						if spawn_joiner {
							// spawn a thread and block future non-demands, but
							// ensure that the current demand is acted upon.
							let inner_chan = next_chan.clone();
							let newer_tx = self_tx.clone();
							thread::spawn(move || {
								thread::sleep(Duration::from_secs(5));
								let mut nc = futures::executor::block_on(inner_chan.lock());
								if let Some(WaitState::Queued(chan)) = *nc {
									newer_tx.send(VoiceHuntMessage::Channel(chan, false));
								}
								*nc = None;
							});
						} else {
							// don't actually join; just go home...
							continue;
						}
					}

					bgm_machine.refresh();
					sfx_machine.refresh();
					curr_sfx = None;
					curr_bgm = None;

					if manager.join(chan.into()).await.is_ok() {
						// test play
						manager.stop();
						// Testing voice receive---example 10.
						// GOAL: ducking!
						receiver_chan = Some(listen_in(&mut manager));

						let state = if let Some(s) = bgm_machine.advance(BgmInput::TryIntro) {
							sfx_machine.cause_cooldown(
								SfxClass::NoSfx,
								SfxClass::Cat,
								SfxInput::Advance,
								0,
								Some(Cooldown::new(Duration::from_secs(13).into(), true, true)),
							);
							s
						} else {
							bgm_machine
								.advance(BgmInput::Advance)
								.expect("Should have reached Ambience...")
						};

						curr_bgm = play_bgm(state, &mut manager, stealthy, &resources, &sound_tx)
							.map(|track| {
								track.set_volume(curr_vol);
								track
							});

						if stealthy {
							// Play one sound so that discord will ACTUALLY give us voice packets...
							curr_sfx =
								play_sfx(SfxClass::Cat, &mut manager, false, &resources, &sound_tx)
									.map(|track| {
										track.set_volume(0.1 * curr_vol);
										track
									});
						}

						curr_chan = Some(chan);
					} else {
						error!("Failed to connect to {:?}!", chan);
					}
				}
			},
			Ok(VoiceHuntMessage::NoChannel) => {
				println!("Leave");
				quit_vox_channel(manager_lock.clone(), guild_id, &mut receiver_chan).await;
				curr_chan = None;
			},
			Ok(VoiceHuntMessage::Cart) => {
				println!("Cart");
				if leaving {
					println!("About to leave vox.");
					quit_vox_channel(manager_lock.clone(), guild_id, &mut receiver_chan).await;
					break 'escape;
				} else {
					leaving = true;

					if let Some(track) = curr_sfx.as_ref() {
						track.pause();
					}

					curr_sfx = None;

					if let Some(track) = curr_bgm.as_ref() {
						track.pause();
					}

					println!("Getting manager...");
					let mut manager = manager_lock.lock().await;
					println!("Got manager...");

					let state = bgm_machine
						.advance(BgmInput::MoveOutro)
						.expect("Can always use outro...");

					curr_bgm = play_bgm(state, &mut manager, stealthy, &resources, &sound_tx).map(
						|track| {
							track.set_volume(0.6 * curr_vol);
							track
						},
					);
				}
			},
			Ok(VoiceHuntMessage::Volume(new_vol)) => {
				if let Some(track) = curr_sfx.as_ref() {
					track.action(move |true_track| {
						let vol = true_track.volume();
						true_track.set_volume((vol / curr_vol) * new_vol);
					});
				}

				if let Some(track) = curr_bgm.as_ref() {
					track.action(move |true_track| {
						let vol = true_track.volume();
						true_track.set_volume((vol / curr_vol) * new_vol);
					});
				}

				curr_vol = new_vol;
			},
			Ok(VoiceHuntMessage::Stealth) => {
				stealthy = true;
			},
			Ok(VoiceHuntMessage::Unstealth) => {
				stealthy = false;
			},
			Err(TryRecvError::Empty) => {
				// If we receieved nothing, then we can perform an update.
				// Iteration, then wait.
				let mut bgm_done = curr_bgm.is_none();
				let mut sfx_done = curr_sfx.is_none();

				'localcheck: loop {
					match sound_rx.try_recv() {
						Ok(FelyneEvt::BgmEnd) => {
							bgm_done = true;
							curr_bgm = None;
						},
						Ok(FelyneEvt::SfxEnd) => {
							sfx_done = true;
							curr_sfx = None;
						},
						_ => break 'localcheck,
					}
				}

				let can_play_sfx = bgm_machine.state() != BgmClass::Bonus && sfx_done;

				if can_play_sfx || bgm_done {
					let mut manager = manager_lock.lock().await;

					if can_play_sfx {
						if let Some(state) = sfx_machine.advance(SfxInput::Advance) {
							if state != SfxClass::NoSfx {
								curr_sfx =
									play_sfx(state, &mut manager, stealthy, &resources, &sound_tx)
										.map(|track| {
											let mut rng = thread_rng();
											track.set_volume(vol_range.sample(&mut rng) * curr_vol);
											track
										});
							}
						}
					}

					if bgm_done {
						if let Some(state) = bgm_machine.advance(BgmInput::Advance) {
							curr_bgm =
								play_bgm(state, &mut manager, stealthy, &resources, &sound_tx).map(
									|track| {
										let vol = if state.no_gargwa() {
											music_vol
										} else {
											let mut rng = thread_rng();
											bgm_vol_range.sample(&mut rng)
										} * curr_vol;

										track.set_volume(vol);
										track
									},
								);
						}
					}
				}

				if bgm_machine.state() == BgmClass::Outro && bgm_done {
					quit_vox_channel(manager_lock.clone(), guild_id, &mut receiver_chan).await;
					break 'escape;
				}

				time::delay_for(timer).await;
			},
			Err(TryRecvError::Disconnected) => {
				break 'escape;
			},
		}
	}
	tx.send(VoiceHuntResponse::Done).unwrap_or_else(|_| {
		panic!(
			"[VoiceHunt] Final Done send for {:?}'s handler failed.",
			guild_id
		)
	});
}

pub async fn voicehunt_control(ctx: &Context, guild_id: GuildId, mode: VoiceHuntCommand) {
	let mut datas = ctx.data.write().await;
	let voice_manager_lock = datas
		.get::<SongbirdKey>()
		.cloned()
		.unwrap()
		.clone()
		.get_or_insert(guild_id.into());
	let also_vox_lock = voice_manager_lock.clone();
	let resources = datas
		.get::<Resources>()
		.expect("Resources must exists after init...")
		.clone();

	let u_id = ctx.cache.current_user_id().await;

	datas
		.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, u_id, also_vox_lock, resources.clone()))
		.control(voice_manager_lock, mode, resources);
}

pub async fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let mut datas = ctx.data.write().await;
	let voice_manager_lock = datas
		.get::<SongbirdKey>()
		.cloned()
		.unwrap()
		.clone()
		.get_or_insert(guild_id.into());
	let resources = datas
		.get::<Resources>()
		.expect("Resources must exists after init...")
		.clone();

	let u_id = ctx.cache.current_user_id().await;

	datas
		.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, u_id, voice_manager_lock, resources))
		.register_user_state(&vox, true);
}

pub async fn voicehunt_complete_update(
	ctx: &Context,
	guild_id: GuildId,
	voice_states: HashMap<UserId, VoiceState>,
) {
	let mut datas = ctx.data.write().await;
	let voice_manager_lock = datas
		.get::<SongbirdKey>()
		.cloned()
		.unwrap()
		.clone()
		.get_or_insert(guild_id.into());
	let resources = datas
		.get::<Resources>()
		.expect("Resources must exists after init...")
		.clone();

	let u_id = ctx.cache.current_user_id().await;

	datas
		.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert_with(|| VHState::new(guild_id, u_id, voice_manager_lock, resources.clone()))
		.register_user_states(voice_states);
}
