use constants::*;

use rand::random;
use serenity::client::*;
use serenity::model::prelude::*;
use serenity::utils::*;
use std::collections::hash_map::{HashMap, Entry};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::thread;
use typemap::Key;

pub struct VoiceHunt;

impl Key for VoiceHunt {
	type Value = HashMap<GuildId, VHState>;
}

pub enum VoiceHuntJoinMode {
	Carted,
	BraveHunt,
	DirectedHunt(ChannelId),
}

pub struct VHState {
	user_states: HashMap<UserId, VoiceState>,
	population_counts: HashMap<ChannelId, u64>,

	join_mode: VoiceHuntJoinMode,


}

impl VHState {
	fn new() -> Self {
		VHState {
			user_states: HashMap::new(),
			population_counts: HashMap::new(),

			join_mode: VoiceHuntJoinMode::Carted,
		}
	}

	fn join_control(&mut self, _ctx: &Context, mode: VoiceHuntJoinMode) -> &mut Self {
		// TODO: spawn thread.

		// TODO: kill thread.

		self.join_mode = mode;

		self
	}

	fn register_user_states(&mut self, ctx: &Context, voice_states: HashMap<UserId, VoiceState>) -> &mut Self {
		self.user_states = voice_states;

		// This is a complete reset -- regenerate the membership tables.
		self.population_counts = HashMap::new();

		for vox in self.user_states.values() {
			if let Some(channel) = vox.channel_id {
				*self.population_counts.entry(channel).or_insert(0) += 1;
			}
		}

		// Swap server?
		// TODO

		self
	}

	fn register_user_state(&mut self, ctx: &Context, state: VoiceState) -> &mut Self {
		if let Entry::Occupied(mut prior_state) = self.user_states.entry(state.user_id) {
			if let Some(channel) = prior_state.get().channel_id {
				*self.population_counts.entry(channel).or_insert(1) -= 1;
			}
		}

		if let Some(channel) = state.channel_id {
			*self.population_counts.entry(channel).or_insert(0) += 1;
		}


		// Swap server?
		// TODO

		self
	}
}

pub fn voicehunt_control(ctx: &Context, guild_id: GuildId, mode: VoiceHuntJoinMode) {
	let mut datas = ctx.data.lock();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new())
		.join_control(ctx, mode);
}


pub fn voicehunt_update(ctx: &Context, guild_id: GuildId, vox: VoiceState) {
	let mut datas = ctx.data.lock();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new())
		.register_user_state(ctx, vox);
}

pub fn voicehunt_complete_update(ctx: &Context, guild_id: GuildId, voice_states: HashMap<UserId, VoiceState>) {
	let mut datas = ctx.data.lock();
	let mut vh_state = datas.get_mut::<VoiceHunt>()
		.unwrap()
		.entry(guild_id)
		.or_insert(VHState::new())
		.register_user_states(ctx, voice_states);
}