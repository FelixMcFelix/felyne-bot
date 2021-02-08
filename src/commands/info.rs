use super::*;

use crate::GuildStates;
use serenity::{
	client::*,
	framework::standard::{macros::command, Args, CommandResult},
	model::prelude::*,
};
use std::sync::Arc;

#[command]
#[description = "Mya! (Let me tell you about my home village!)"]
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

#[command]
#[description = "Mya! (Let me tell you about how I'm feeling!)"]
pub async fn info(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let gs = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<GuildStates>().unwrap())
	};

	let reply_txt = if let Some(state) = gs.get(&guild.id) {
		let lock = state.read().await;
		lock.to_info_message(ctx, &msg.author, msg.guild_id.unwrap_or_default())
			.await
	} else {
		"Hiss... (I couldn't find any relevant info for your server...)".into()
	};

	check_msg(msg.author.dm(&ctx, |m| m.content(reply_txt)).await);

	Ok(())
}
