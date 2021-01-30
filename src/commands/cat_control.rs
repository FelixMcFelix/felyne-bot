use super::*;

use crate::voicehunt::*;

use serenity::{
	client::*,
	framework::standard::{macros::command, Args, CommandResult},
	model::prelude::*,
};

#[command]
#[description = "Mraa! (I'll come hang out wherever folks are, or what channel you tell me!)"]
#[owner_privilege]
pub async fn hunt(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	// Turn first arg (hopefully a channel mention) into a real channel
	voicehunt_control(
		&ctx,
		guild,
		match args.single::<u64>().ok() {
			Some(c) => {
				// TODO: make use of string parsing for greeat good.
				VoiceHuntCommand::DirectedHunt(ChannelId(c))
			},
			None => VoiceHuntCommand::BraveHunt,
		},
	)
	.await;

	check_msg(msg.channel_id.say(&ctx.http, "Mrowr!").await);

	Ok(())
}

#[command]
#[description = "Mreh... (I'll come hang out wherever folks are, real quietly!)"]
#[owner_privilege]
pub async fn watch(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	// Turn first arg (hopefully a channel mention) into a real channel
	voicehunt_control(&ctx, guild, VoiceHuntCommand::Stalk).await;

	check_msg(msg.channel_id.say(&ctx.http, "...").await);

	Ok(())
}

#[command]
#[description = "Myowr! (Another hunt ends in success!)"]
#[owner_privilege]
pub async fn cart(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Carted).await;

	check_msg(msg.channel_id.say(&ctx.http, "Mrr... :zzz:").await);

	Ok(())
}

#[command]
#[aliases("vol")]
#[description = "Mya... (I'll be a little quieter...)"]
#[owner_privilege]
pub async fn volume(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let vol = match args.single::<f32>().ok() {
		Some(c) => c,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	if !vol.is_finite() || vol < 0.0 || vol > 2.0 {
		return confused(&ctx, &msg).await;
	}

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Volume(vol)).await;

	Ok(())
}
