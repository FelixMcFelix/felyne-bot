pub mod live;
pub mod mode;
pub mod receiver;

use crate::{automata::*, constants::*, guild::*, user::*, Resources, RxMap};
use dashmap::{mapref::entry::Entry as DashEntry, DashMap};
use flume::{self, Receiver, Sender, TryRecvError};
use rand::{distributions::*, thread_rng};
use receiver::{listen_in, ReceiverSignal};
use serenity::{async_trait, client::*, model::prelude::*, prelude::*};
use songbird::{
	events::{Event, EventContext, EventHandler, TrackEvent},
	serenity::SongbirdKey,
	tracks::TrackHandle,
	Call,
};
use std::{
	collections::hash_map::{Entry, HashMap},
	sync::Arc,
	time::Duration,
};
use tokio::time;
use tracing::*;

pub struct VoiceHunt;

impl TypeMapKey for VoiceHunt {
	type Value = DashMap<GuildId, Arc<Mutex<VHState>>>;
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
	Channel(ChannelId, bool, usize),
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
	async fn new(
		guild_id: GuildId,
		user_id: UserId,
		vox_manager: Arc<Mutex<Call>>,
		resources: RxMap,
		ctx: &Context,
		guild_state: &Arc<RwLock<GuildState>>,
		user_states: &Arc<UserState>,
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

		let base_state = {
			let gs = guild_state.read().await;
			gs.join()
		};

		out.control(
			vox_manager,
			base_state.as_command(),
			resources,
			ctx,
			guild_state,
			user_states,
		)
		.await;

		out
	}

	fn launch_felyne_thread(
		&mut self,
		vox_manager: Arc<Mutex<Call>>,
		resources: RxMap,
		guild_state: Arc<RwLock<GuildState>>,
		user_states: Arc<UserState>,
		ctx: Context,
	) {
		let (sender, receiver) = flume::unbounded();
		let (reverse_sender, reverse_receiver) = flume::unbounded();
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
				guild_state,
				user_states,
				ctx,
			)
			.await;
		});

		self.huntsim_tx = Some(sender);
		self.huntsim_rx = Some(reverse_receiver);
	}

	async fn control(
		&mut self,
		vox_manager: Arc<Mutex<Call>>,
		mode: VoiceHuntCommand,
		resources: RxMap,
		ctx: &Context,
		guild_state: &Arc<RwLock<GuildState>>,
		user_states: &Arc<UserState>,
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
					self.launch_felyne_thread(
						vox_manager,
						resources,
						guild_state.clone(),
						user_states.clone(),
						ctx.clone(),
					);
				},
			}
		}

		let chan_change = match mode {
			Carted => {
				self.send(VoiceHuntMessage::Cart);
				if let Some(rx) = self.huntsim_rx.as_ref() {
					// inner thread needs to die one way or another.
					let _ = rx.recv_async().await;
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
			self.update_incumbent(*count, *chan);
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
			let chan_users = (*self.population_counts.get(&chan).unwrap_or(&0)) as usize;
			self.send(VoiceHuntMessage::Channel(chan, true, chan_users));
		} else if let Some(Incumbent(chan_users, chan)) = self.incumbent_channel {
			self.send(VoiceHuntMessage::Channel(chan, false, chan_users as usize));
		} else {
			self.send(VoiceHuntMessage::NoChannel);
		}
	}
}

#[inline]
async fn quit_vox_channel(
	manager_lock: Arc<Mutex<Call>>,
	_guild_id: GuildId,
	receiver_chan: &mut Option<Sender<ReceiverSignal>>,
) {
	let mut manager = manager_lock.lock().await;

	manager.stop();
	if let Some(chan) = receiver_chan {
		let _ = chan.send(ReceiverSignal::Poison);
	}
	*receiver_chan = None;

	manager.remove_all_global_events();

	let _ = manager.leave().await;
}

#[inline]
fn random_element<T>(arr: &[T]) -> &T {
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
			let _ = track.add_event(
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
	async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
		let _ = self.chan.send(self.msg);
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
			let _ = track.add_event(
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
	guild_state: Arc<RwLock<GuildState>>,
	user_states: Arc<UserState>,
	ctx: Context,
) {
	let timer = Duration::from_millis(VOICEHUNT_FRAME_TIME);
	let vol_range = Uniform::new(0.3, 0.4);
	let bgm_vol_range = Uniform::new(0.15, 0.2);
	let music_vol = 0.3;

	let (sound_tx, sound_rx) = flume::bounded(2);

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
			Ok(VoiceHuntMessage::Channel(chan, demand, chan_users)) => {
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
							tokio::spawn(async move {
								tokio::time::sleep(Duration::from_secs(5)).await;
								let mut nc = inner_chan.lock().await;
								if let Some(WaitState::Queued(chan)) = *nc {
									let _ = newer_tx
										.send(VoiceHuntMessage::Channel(chan, false, chan_users));
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

						let (opt_in, gather_mode) = {
							let lock = guild_state.read().await;
							(lock.server_opt(), lock.gather())
						};

						receiver_chan = listen_in(
							&mut manager,
							opt_in,
							gather_mode,
							user_states.clone(),
							guild_state.clone(),
							guild_id,
							chan,
							!stealthy,
							chan_users,
							ctx.clone(),
						)
						.await;

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
								let _ = track.set_volume(curr_vol);
								track
							});

						if stealthy {
							// Play one sound so that discord will ACTUALLY give us voice packets...
							curr_sfx =
								play_sfx(SfxClass::Cat, &mut manager, false, &resources, &sound_tx)
									.map(|track| {
										let _ = track.set_volume(0.1 * curr_vol);
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
				quit_vox_channel(manager_lock.clone(), guild_id, &mut receiver_chan).await;
				curr_chan = None;
			},
			Ok(VoiceHuntMessage::Cart) =>
				if leaving {
					quit_vox_channel(manager_lock.clone(), guild_id, &mut receiver_chan).await;
					break 'escape;
				} else {
					leaving = true;

					if let Some(track) = curr_sfx.as_ref() {
						let _ = track.pause();
					}

					curr_sfx = None;

					if let Some(track) = curr_bgm.as_ref() {
						let _ = track.pause();
					}

					let mut manager = manager_lock.lock().await;

					let state = bgm_machine
						.advance(BgmInput::MoveOutro)
						.expect("Can always use outro...");

					curr_bgm = play_bgm(state, &mut manager, stealthy, &resources, &sound_tx).map(
						|track| {
							let _ = track.set_volume(0.6 * curr_vol);
							track
						},
					);
				},
			Ok(VoiceHuntMessage::Volume(new_vol)) => {
				if let Some(track) = curr_sfx.as_ref() {
					let _ = track.action(move |true_track| {
						let vol = true_track.volume();
						let _ = true_track.set_volume((vol / curr_vol) * new_vol);
					});
				}

				if let Some(track) = curr_bgm.as_ref() {
					let _ = track.action(move |true_track| {
						let vol = true_track.volume();
						let _ = true_track.set_volume((vol / curr_vol) * new_vol);
					});
				}

				curr_vol = new_vol;
			},
			Ok(VoiceHuntMessage::Stealth) => {
				stealthy = true;
				if let Some(chan) = &receiver_chan {
					let _ = chan.send(ReceiverSignal::Inactive);
				}
			},
			Ok(VoiceHuntMessage::Unstealth) => {
				stealthy = false;
				if let Some(chan) = &receiver_chan {
					let _ = chan.send(ReceiverSignal::Active);
				}
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
											let _ = track
												.set_volume(vol_range.sample(&mut rng) * curr_vol);
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

										let _ = track.set_volume(vol);
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

				time::sleep(timer).await;
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
	let vhstate = try_create_vh_state(ctx, guild_id).await;
	let (guild_state, user_states, resources, voice_manager_lock) = {
		let datas = ctx.data.read().await;
		let voice_manager_lock = datas
			.get::<SongbirdKey>()
			.cloned()
			.unwrap()
			.get_or_insert(guild_id.into());

		let resources = datas
			.get::<Resources>()
			.expect("Resources must exists after init...")
			.clone();

		let guild_state = datas
			.get::<GuildStates>()
			.expect("Resources must exists after init...")
			.get(&guild_id)
			.expect("Tried to act on a guild I haven't yet installed!")
			.clone();

		let user_states = datas
			.get::<UserStateKey>()
			.expect("Resources must exists after init...")
			.clone();

		(guild_state, user_states, resources, voice_manager_lock)
	};
	vhstate
		.lock()
		.await
		.control(
			voice_manager_lock,
			mode,
			resources,
			ctx,
			&guild_state,
			&user_states,
		)
		.await;
}

pub async fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let vhstate = try_create_vh_state(ctx, guild_id).await;

	vhstate.lock().await.register_user_state(&vox, true);
}

pub async fn voicehunt_complete_update(
	ctx: &Context,
	guild_id: GuildId,
	voice_states: HashMap<UserId, VoiceState>,
) {
	let vhstate = try_create_vh_state(ctx, guild_id).await;

	vhstate.lock().await.register_user_states(voice_states);
}

async fn try_create_vh_state(ctx: &Context, guild_id: GuildId) -> Arc<Mutex<VHState>> {
	let datas = ctx.data.read().await;
	let voice_manager_lock = datas
		.get::<SongbirdKey>()
		.cloned()
		.unwrap()
		.get_or_insert(guild_id.into());

	let resources = datas
		.get::<Resources>()
		.expect("Resources must exists after init...")
		.clone();

	let u_id = ctx.cache.current_user_id().await;

	let guild_state = datas
		.get::<GuildStates>()
		.expect("Resources must exists after init...")
		.get(&guild_id)
		.expect("Tried to act on a guild I haven't yet installed!")
		.clone();

	let user_states = datas
		.get::<UserStateKey>()
		.expect("Resources must exists after init...")
		.clone();

	let proto_out = datas.get::<VoiceHunt>().unwrap().entry(guild_id);

	match proto_out {
		DashEntry::Vacant(space) => {
			let out = Arc::new(Mutex::new(
				VHState::new(
					guild_id,
					u_id,
					voice_manager_lock,
					resources,
					ctx,
					&guild_state,
					&user_states,
				)
				.await,
			));

			space.insert(out.clone());

			out
		},
		DashEntry::Occupied(out) => out.get().clone(),
	}
}
