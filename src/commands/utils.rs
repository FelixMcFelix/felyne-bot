use serenity::{
	client::*,
	framework::standard::{Args, CommandResult},
	model::prelude::*,
	utils::*,
	Result as SResult,
};

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