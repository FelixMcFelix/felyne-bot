use crate::MyId;
use serenity::{
	client::*,
	framework::standard::{Args, CommandResult},
	model::prelude::*,
	Result as SResult,
};

use tracing::*;

pub fn parse_chan_mention(args: &mut Args) -> Option<ChannelId> {
	let channel_id = args.single::<String>().ok()?;

	serenity::utils::parse_channel_mention(channel_id.as_str()).or_else(|| {
		channel_id.parse::<u64>().ok().and_then(|v| {
			if v == 0 {
				None
			} else {
				Some(ChannelId::new(v))
			}
		})
	})
}

pub async fn confused(ctx: &Context, msg: &Message) -> CommandResult {
	check_msg(msg.reply(ctx, "???").await);
	Ok(())
}

pub async fn mentions_me(ctx: &Context, msg: &Message) -> bool {
	let my_id = {
		let lock = ctx.data.read().await;
		*lock.get::<MyId>().unwrap()
	};

	msg.mentions.iter().any(|u| u.id == my_id)
}

pub fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		warn!("Error sending message: {:?}", why);
	}
}
