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

use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};

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

	let datas = ctx.data.read().await;

	let db = datas.get::<Db>().expect("DB conn installed...");

	let ctl = select_control_cfg(&db, g_id).await.ok().unwrap_or_default();

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

	let datas = ctx.data.read().await;

	let db = datas.get::<Db>().expect("DB conn installed...");

	let ctl = select_control_admin_cfg(&db, g_id)
		.await
		.ok()
		.unwrap_or_default();

	shared_ctl_check(ctl, ctx, msg).await
}

async fn shared_ctl_check(
	control: CfgControl,
	ctx: &Context,
	msg: &Message,
) -> Result<(), CheckReason> {
	use CfgControl::*;

	(match control {
		OwnerOnly => msg.guild(ctx).await.map(|guild| {
			if guild.owner_id == msg.author.id {
				Ok(())
			} else {
				println!(
					"Not a valid person: {:?} vs {:?}",
					msg.author.id, guild.owner_id
				);
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
