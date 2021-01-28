use super::*;

use crate::{
	audio_resources::*,
	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	event_handler::*,
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

pub fn parse_chan_mention(args: &mut Args) -> Option<ChannelId> {
	let chan_name = args.single::<String>().ok()?;
	let channel_id = parse_channel(chan_name.as_str())?;
	Some(ChannelId(channel_id))
}

pub async fn confused(ctx: &Context, msg: &Message) -> CommandResult {
	check_msg(msg.reply(ctx, "???").await);
	Ok(())
}

pub fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		warn!("Error sending message: {:?}", why);
	}
}
