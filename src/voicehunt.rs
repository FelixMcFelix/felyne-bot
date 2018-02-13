use constants::*;
use VoiceManager;

use parking_lot::Mutex;
use rand::random;
use serenity::client::*;
use serenity::client::bridge::voice::ClientVoiceManager;
use serenity::model::prelude::*;
use serenity::utils::*;
use serenity::voice::ffmpeg;
use std::collections::hash_map::{HashMap, Entry};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, channel, TryRecvError};
use std::thread;
use std::time::Duration;
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
	Cart,
}

#[derive(Debug)]
struct Incumbent(u64, ChannelId);

#[derive(Debug)]
pub struct VHState {
	guild_id: GuildId,
	user_states: HashMap<UserId, VoiceState>,
	population_counts: HashMap<ChannelId, u64>,

	join_mode: VoiceHuntJoinMode,

	active_channel: Option<ChannelId>,
	incumbent_channel: Option<Incumbent>,

	huntsim_tx: Option<Arc<Mutex<Sender<VoiceHuntMessage>>>>,
}

impl VHState {
	fn new(guild_id: GuildId) -> Self {
		VHState {
			guild_id,
			user_states: HashMap::new(),
			population_counts: HashMap::new(),

			join_mode: VoiceHuntJoinMode::Carted,

			active_channel: None,
			incumbent_channel: None,
			huntsim_tx: None,
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
					let guild_id = self.guild_id;			

					// Begin!
					thread::spawn(move || {
						// Init state here
						felyne_life(receiver, vox_manager, guild_id);
					});


					self.huntsim_tx = Some(Arc::new(Mutex::new(sender)));
				}
			}
		}


		let chan_change = match mode {
			Carted => {
				self.send(VoiceHuntMessage::Cart);
				self.huntsim_tx = None;
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
				println!("About to 'successfully' send {:?}", &msg);
				let lock = lock_arc.clone();
				let tx = lock.lock();
				tx.send(msg);
			},
			None => {println!("Send of {:?} failed, no channel.", msg);},
		}
	}

	fn register_user_states(&mut self, voice_states: HashMap<UserId, VoiceState>) -> &mut Self {
		self.user_states = voice_states;

		// This is a complete reset -- regenerate the membership tables.
		self.population_counts = HashMap::new();

		for vox in self.user_states.clone().values() {
			if let Some(channel) = vox.channel_id {
				let count = {
					let v = self.population_counts.entry(channel).or_insert(0);
					*v += 1;
					v.clone()
				};
				self.update_incumbent(count, channel);
			}
		}

		self.update_channel();

		self
	}

	fn register_user_state(&mut self, state: VoiceState) -> &mut Self {
		if let Entry::Occupied(mut prior_state) = self.user_states.clone().entry(state.user_id) {
			if let Some(channel) = prior_state.get().channel_id {
				let count = {
					let v = self.population_counts.entry(channel).or_insert(1);
					*v -= 1;
					v.clone()
				};
				self.update_incumbent(count, channel);
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


		self.update_channel();

		self
	}

	fn update_incumbent(&mut self, count: u64, channel: ChannelId) {
		if let Some(Incumbent(count_old, chan_old)) = self.incumbent_channel {
			if chan_old == channel || count > count_old {
				self.incumbent_channel = if count == 0 {None} else {Some(Incumbent(count, channel))};
			}
		} else {
			self.incumbent_channel = Some(Incumbent(count, channel));
		}
	}

	fn update_channel(&mut self) {
		println!("tried to update channel. Self? {:?}", &self);
		if let Some(chan) = self.active_channel {
			self.send(VoiceHuntMessage::Channel(chan));
		} else if let Some(Incumbent(_, chan)) = self.incumbent_channel {
			self.send(VoiceHuntMessage::Channel(chan));
		}
	}
}

fn felyne_life(rx: Receiver<VoiceHuntMessage>, manager_lock: Arc<Mutex<ClientVoiceManager>>, guild_id: GuildId) {
	let timer = Duration::from_millis(VOICEHUNT_FRAME_TIME);
	'escape: loop {
		match rx.try_recv() {
			Ok(VoiceHuntMessage::Channel(chan)) => {
				// Connect to a different vox channel.
				let mut manager = manager_lock.lock();

				if manager.join(guild_id, chan).is_some() {
					// test play
					let mut handler = manager.get_mut(guild_id).unwrap();

					let source = ffmpeg("sfx/mewl-wiggle2.ogg").unwrap();

					let safe_aud = handler.play_returning(source);

					{
						let aud_lock = safe_aud.clone();
						let mut aud = aud_lock.lock();

						aud.volume(1.0);
					}

				}

				println!("I really want to connect to: {:?}", chan);
			},
			Ok(VoiceHuntMessage::Cart) => {
				let mut manager = manager_lock.lock();

				let is_in_voicechat_here = match manager.get_mut(guild_id) {
					Some(handler) => {handler.stop(); true}
					None => false,
				};

				if is_in_voicechat_here {
					manager.remove(guild_id);
				}
				break 'escape;
			},
			Err(TryRecvError::Empty) => {
				// If we receieved nothing, then we can perform an update.
				// Iteration, then wait.
				// TODO (???)
				thread::sleep(timer);
			},
			Err(TryRecvError::Disconnected) => {
				break 'escape;
			},
		}
	}
}

pub fn voicehunt_control(ctx: &Context, guild_id: GuildId, mode: VoiceHuntJoinMode) {
	let mut datas = ctx.data.lock();
	let voice_manager_lock = datas.get::<VoiceManager>().cloned().unwrap().clone();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id))
		.join_control(voice_manager_lock, mode);
}


pub fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let mut datas = ctx.data.lock();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id))
		.register_user_state(vox);
}

pub fn voicehunt_complete_update(ctx: &Context, guild_id: GuildId, voice_states: HashMap<UserId, VoiceState>) {
	let mut datas = ctx.data.lock();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new(guild_id))
		.register_user_states(voice_states);
}