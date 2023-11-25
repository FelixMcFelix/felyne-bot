use crate::{config::Control as CfgControl, guild::GuildStates};

use serenity::{
	client::*,
	framework::standard::{macros::check, Args, CommandOptions, Reason as CheckReason},
	model::prelude::*,
};
use std::sync::Arc;

#[check]
#[name = "Control"]
pub async fn can_control_cat(
	ctx: &Context,
	msg: &Message,
	args: &mut Args,
	opts: &CommandOptions,
) -> Result<(), CheckReason> {
	let g_id = match msg.guild_id {
		Some(id) => id,
		_ =>
			return Err(CheckReason::User(
				"Control commands only valid in guild channel (i.e., not DMs).".into(),
			)),
	};

	let gs = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<GuildStates>().unwrap())
	};

	let ctl = if let Some(state) = gs.get(&g_id) {
		let lock = state.read().await;
		lock.voice_control_mode()
	} else {
		Default::default()
	};

	let o = match shared_ctl_check(ctl, ctx, msg).await {
		Err(_e) => can_admin_cat(ctx, msg, args, opts).await,
		a => a,
	};

	println!("{:?}", o);
	o
}

#[check]
#[name = "Admin"]
pub async fn can_admin_cat(
	ctx: &Context,
	msg: &Message,
	_args: &mut Args,
	_opts: &CommandOptions,
) -> Result<(), CheckReason> {
	let g_id = match msg.guild_id {
		Some(id) => id,
		_ =>
			return Err(CheckReason::User(
				"Control commands only valid in guild channel (i.e., not DMs).".into(),
			)),
	};

	let gs = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<GuildStates>().unwrap())
	};

	let ctl = if let Some(state) = gs.get(&g_id) {
		let lock = state.read().await;
		lock.admin_control_mode()
	} else {
		Default::default()
	};

	shared_ctl_check(ctl, ctx, msg).await
}

async fn shared_ctl_check(
	control: CfgControl,
	ctx: &Context,
	msg: &Message,
) -> Result<(), CheckReason> {
	use CfgControl::*;

	(match control {
		OwnerOnly => msg.guild(&ctx.cache).map(|guild| {
			if guild.owner_id == msg.author.id {
				Ok(())
			} else {
				Err(CheckReason::User("User is not server/bot owner.".into()))
			}
		}),
		WithRole(role) => msg.member(ctx).await.ok().map(|member| {
			if member.roles.contains(&role) {
				Ok(())
			} else {
				Err(CheckReason::User("User lacks necessary role.".into()))
			}
		}),
		All => Some(Ok(())),
	})
	.unwrap_or_else(|| {
		Err(CheckReason::User(
			"Checked command occurred outside of a Guild Channel.".into(),
		))
	})
}
