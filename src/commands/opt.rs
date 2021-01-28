// use super::*;

// use crate::{
// 	audio_resources::*,
// 	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
// 	constants::*,
// 	dbs::*,
// 	event_handler::*,
// 	voicehunt::*,
// 	watchcat::*,
// };

// use serenity::{
// 	async_trait,
// 	client::*,
// 	framework::standard::{
// 		macros::{check, command, group, help},
// 		Args,
// 		CommandOptions,
// 		CommandResult,
// 		Reason as CheckReason,
// 		StandardFramework,
// 	},
// 	http::client::Http,
// 	model::prelude::*,
// 	prelude::*,
// 	utils::*,
// 	Result as SResult,
// };

// use std::{
// 	collections::{HashMap, HashSet},
// 	convert::TryInto,
// 	env,
// 	fs::File,
// 	io::prelude::*,
// 	sync::Arc,
// };

// #[command]
// #[aliases("log-to")]
// #[owner_privilege]
// pub async fn log_to(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
// 	let out_chan = parse_chan_mention(&mut args);

// 	if out_chan.is_none() {
// 		return confused(&ctx, msg).await;
// 	}

// 	let out_chan = out_chan.unwrap();

// 	// Get the guild ID.
// 	let guild_id = match msg.guild(&ctx.cache).await {
// 		Some(c) => c.id,
// 		None => {
// 			return confused(&ctx, msg).await;
// 		},
// 	};

// 	watchcat(&ctx, guild_id, WatchcatCommand::SetChannel(out_chan)).await;

// 	check_msg(
// 		msg.channel_id
// 			.say(&ctx.http, "Mrowrorr! (I'll keep you nyotified!)")
// 			.await,
// 	);

// 	Ok(())
// }

// #[command]
// #[aliases("felyne-prefix")]
// #[owner_privilege]
// pub async fn felyne_prefix(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
// 	let new_prefix = args.single::<String>();

// 	if new_prefix.is_err() {
// 		return confused(&ctx, msg).await;
// 	}

// 	let new_prefix = new_prefix.unwrap();

// 	// Get the guild ID.
// 	let guild_id = match msg.guild(&ctx.cache).await {
// 		Some(c) => c.id,
// 		None => {
// 			return confused(&ctx, msg).await;
// 		},
// 	};

// 	let datas = ctx.data.read().await;
// 	let db = datas.get::<Db>().expect("DB conn installed...");

// 	upsert_prefix(&db, guild_id, &new_prefix).await;

// 	check_msg(
// 		msg.channel_id
// 			.say(
// 				&ctx.http,
// 				format!("Listening to nyew prefix: {}", &new_prefix),
// 			)
// 			.await,
// 	);

// 	Ok(())
// }

// // #[command]
// // #[aliases("admin-ctl-mode")]
// // #[owner_privilege]
// // pub async fn admin_ctl_mode(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
// // 	ctl_mode_basis(ctx, msg, args, true).await
// // }

// // #[command]
// // #[aliases("ctl-mode")]
// // #[owner_privilege]
// // pub async fn ctl_mode(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
// // 	ctl_mode_basis(ctx, msg, args, false).await
// // }

// // async fn ctl_mode_basis(
// // 	ctx: &Context,
// // 	msg: &Message,
// // 	mut args: Args,
// // 	do_for_admin: bool,
// // ) -> CommandResult {
// // 	match CfgControl::parse(&mut args) {
// // 		Ok(Some(cm)) => {
// // 			let datas = ctx.data.read().await;
// // 			let db = datas.get::<Db>().expect("DB conn installed...");

// // 			if let Some(g_id) = msg.guild_id {
// // 				if do_for_admin {
// // 					upsert_control_admin_cfg(&db, g_id, cm).await;
// // 				} else {
// // 					upsert_control_cfg(&db, g_id, cm).await;
// // 				}
// // 				check_msg(
// // 					msg.channel_id
// // 						.say(
// // 							&ctx.http,
// // 							format!(
// // 								"Now accepting{} commands from: {:?}",
// // 								if do_for_admin { " admin" } else { "" },
// // 								&cm,
// // 							),
// // 						)
// // 						.await,
// // 				);
// // 			}

// // 			// new mode
// // 		},
// // 		Ok(None) => {
// // 			check_msg(
// // 				msg.channel_id
// // 					.say(
// // 						&ctx.http,
// // 						format!("I support the modes: {:?}", &ControlMode::LABEL_LIST),
// // 					)
// // 					.await,
// // 			);
// // 		},
// // 		Err(e) => {
// // 			check_msg(msg.channel_id.say(&ctx.http, match e {
// // 				ConfigParseError::ArgTake => {
// // 					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
// // 				},
// // 				ConfigParseError::BadMode => {
// // 					"Mrowr?! That's an illegal mode! Use this commyand without any extra info to see valid chyoices."
// // 				},
// // 				ConfigParseError::IllegalRole => {
// // 					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
// // 				},
// // 				ConfigParseError::MissingRole => {
// // 					"Try that command again, with a role mention or ID!"
// // 				},
// // 			}).await);
// // 		},
// // 	}

// // 	Ok(())
// // }
