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

#[command]
pub async fn ids(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let mut content = MessageBuilder::new();
	content.push_bold_line(format!("ChannelIDs for {}:", guild));

	for channel in guild.channels(&ctx.http).await.unwrap().values() {
		if channel.kind == ChannelType::Voice {
			content
				.push(&channel.name)
				.push_bold(" --- ")
				.push_line(&channel.id);
		}
	}

	let out = content.build();

	check_msg(msg.author.dm(&ctx, |m| m.content(out)).await);

	Ok(())
}

#[command]
pub async fn github(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	check_msg(
		msg.channel_id
			.say(
				&ctx.http,
				"Mya! :heart: (https://github.com/FelixMcFelix/felyne-bot)",
			)
			.await,
	);

	Ok(())
}
