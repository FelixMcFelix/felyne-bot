use super::*;

use serenity::{
	client::*,
	framework::standard::{macros::command, Args, CommandResult},
	model::prelude::*,
};

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
