use crate::{
	audio_resources::*,
	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	voicehunt::*,
	watchcat::*,
};
use dashmap::DashMap;
use serenity::{
	async_trait,
	client::*,
	framework::standard::{
		macros::{check, command, group, help},
		Args,
		CommandOptions,
		CommandResult,
		Reason as CheckReason,
		StandardFramework,
	},
	http::client::Http,
	model::prelude::*,
	prelude::*,
	utils::*,
	Result as SResult,
};
use songbird::{
	self,
	input::{
		cached::{Compressed, Memory},
		Input,
	},
	Bitrate,
	SerenityInit,
};
use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};
use tokio_postgres::Client as DbClient;
use tracing::*;

pub struct FelyneEvts;

#[async_trait]
impl EventHandler for FelyneEvts {
	async fn message(&self, ctx: Context, msg: Message) {
		// Place the message in our "deleted messages cache".
		// This should probably be the last action...
		// Get the guild ID.
		let guild_id = match msg.guild(&ctx.cache).await {
			Some(guild) => guild.id,
			None => {
				return;
			},
		};

		watchcat(&ctx, guild_id, WatchcatCommand::BufferMsg(Box::new(msg))).await;
	}

	async fn message_delete(
		&self,
		ctx: Context,
		chan: ChannelId,
		msg: MessageId,
		guild_id: Option<GuildId>,
	) {
		if let Some(guild_id) = guild_id {
			watchcat(
				&ctx,
				guild_id,
				WatchcatCommand::ReportDelete(chan, vec![msg]),
			)
			.await;
		}
	}

	async fn message_delete_bulk(
		&self,
		ctx: Context,
		chan: ChannelId,
		msgs: Vec<MessageId>,
		guild_id: Option<GuildId>,
	) {
		if let Some(guild_id) = guild_id {
			watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, msgs)).await;
		}
	}

	// Should provide us with a set of full guild info as we connect to each!
	async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
		voicehunt_complete_update(&ctx, guild.id, guild.voice_states).await;
	}

	async fn voice_state_update(
		&self,
		ctx: Context,
		maybe_guild: Option<GuildId>,
		_old_vox: Option<VoiceState>,
		vox: VoiceState,
	) {
		if let Some(guild_id) = maybe_guild {
			voicehunt_update(&ctx, guild_id, vox).await;
		}
	}

	async fn ready(&self, ctx: Context, _rdy: Ready) {
		ctx.set_activity(Activity::listening("scary monsters!"))
			.await;
	}
}
