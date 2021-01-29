use super::*;

use crate::{
	config::{
		ConfigParseError,
		Control as CfgControl,
		ControlMode,
		GatherMode,
		OptInOut,
		OptInOutMode,
	},
	guild::*,
	server::Label,
	watchcat::*,
};

use serenity::{
	client::*,
	framework::standard::{
		macros::{command, help},
		Args,
		CommandResult,
	},
	model::prelude::*,
};

use std::sync::Arc;

#[command]
#[aliases("see-config")]
#[owner_privilege]
pub async fn see_config(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
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
		lock.to_message()
	} else {
		"Hiss... (I couldn't find any relevant info for your server...)".into()
	};

	check_msg(msg.author.dm(&ctx, |m| m.content(reply_txt)).await);

	Ok(())
}

#[command]
#[aliases("log-to")]
#[owner_privilege]
pub async fn log_to(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let out_chan = parse_chan_mention(&mut args);

	if out_chan.is_none() {
		return confused(&ctx, msg).await;
	}

	let out_chan = out_chan.unwrap();

	// Get the guild ID.
	let guild_id = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, msg).await;
		},
	};

	watchcat(&ctx, guild_id, WatchcatCommand::SetChannel(out_chan)).await;

	check_msg(
		msg.channel_id
			.say(&ctx.http, "Mrowrorr! (I'll keep you nyotified!)")
			.await,
	);

	Ok(())
}

#[command]
#[aliases("felyne-prefix")]
#[owner_privilege]
pub async fn felyne_prefix(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let new_prefix = args.single::<String>();

	if new_prefix.is_err() {
		return confused(&ctx, msg).await;
	}

	let new_prefix = new_prefix.unwrap();

	// Get the guild ID.
	let guild_id = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, msg).await;
		},
	};

	let gs = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<GuildStates>().unwrap())
	};

	if let Some(state) = gs.get(&guild_id) {
		let mut lock = state.write().await;
		lock.set_custom_prefix(new_prefix.clone()).await;
	}

	check_msg(
		msg.channel_id
			.say(
				&ctx.http,
				format!("Listening to nyew prefix: {}", &new_prefix),
			)
			.await,
	);

	Ok(())
}

#[command]
#[aliases("admin-ctl-mode")]
#[owner_privilege]
pub async fn admin_ctl_mode(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
	ctl_mode_basis(ctx, msg, args, true).await
}

#[command]
#[aliases("ctl-mode")]
#[owner_privilege]
pub async fn ctl_mode(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
	ctl_mode_basis(ctx, msg, args, false).await
}

async fn ctl_mode_basis(
	ctx: &Context,
	msg: &Message,
	mut args: Args,
	do_for_admin: bool,
) -> CommandResult {
	match CfgControl::parse(&mut args) {
		Ok(Some(cm)) => {
			let gs = {
				let datas = ctx.data.read().await;
				Arc::clone(datas.get::<GuildStates>().unwrap())
			};

			if let Some(g_id) = msg.guild_id {
				if let Some(gs) = gs.get(&g_id) {
					if do_for_admin {
						let mut gs_lock = gs.write().await;
						gs_lock.set_admin_control_mode(cm).await;
					} else {
						let mut gs_lock = gs.write().await;
						gs_lock.set_voice_control_mode(cm).await;
					}
					check_msg(
						msg.channel_id
							.say(
								&ctx.http,
								format!(
									"Now accepting{} commands from: {:?}",
									if do_for_admin { " admin" } else { "" },
									&cm,
								),
							)
							.await,
					);
				}
			}

			// new mode
		},
		Ok(None) => {
			check_msg(
				msg.channel_id
					.say(
						&ctx.http,
						format!("I support the modes: {:?}", &ControlMode::LABEL_LIST),
					)
					.await,
			);
		},
		Err(e) => {
			check_msg(msg.channel_id.say(&ctx.http, match e {
				ConfigParseError::ArgTake => {
					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
				},
				ConfigParseError::BadMode => {
					"Mrowr?! That's an illegal mode! Use this commyand without any extra info to see valid chyoices."
				},
				ConfigParseError::IllegalRole => {
					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
				},
				ConfigParseError::MissingRole => {
					"Try that command again, with a role mention or ID!"
				},
			}).await);
		},
	}

	Ok(())
}

#[command]
#[aliases("server-opt")]
#[owner_privilege]
pub async fn server_opt(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	match OptInOut::parse(&mut args) {
		Ok(Some(om)) =>
			if let Some(g_id) = msg.guild_id {
				let gs = {
					let data = ctx.data.read().await;
					Arc::clone(data.get::<GuildStates>().unwrap())
				};

				if let Some(state) = gs.get(&g_id) {
					let mut lock = state.write().await;
					lock.set_server_opt(om).await;
				}

				check_msg(
					msg.channel_id
						.say(&ctx.http, format!("Voice stats measurement: {:?}", &om,))
						.await,
				);
			},
		Ok(None) => {
			check_msg(
				msg.channel_id
					.say(
						&ctx.http,
						format!("I support the modes: {:?}", &OptInOutMode::LABEL_LIST),
					)
					.await,
			);
		},
		Err(e) => {
			check_msg(msg.channel_id.say(&ctx.http, match e {
				ConfigParseError::ArgTake => {
					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
				},
				ConfigParseError::BadMode => {
					"Mrowr?! That's an illegal mode! Use this commyand without any extra info to see valid chyoices."
				},
				ConfigParseError::IllegalRole => {
					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
				},
				ConfigParseError::MissingRole => {
					"Try that command again, with a role mention or ID!"
				},
			}).await);
		},
	}

	Ok(())
}

#[command]
#[aliases("server-ack")]
#[owner_privilege]
pub async fn server_ack(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
	if let Some(g_id) = msg.guild_id {
		let new_str = args.rest().trim();
		let ack = if !new_str.is_empty() {
			new_str.to_string()
		} else if let Some(g) = msg.guild(ctx).await {
			g.name.clone()
		} else {
			"".into()
		};

		let gs = {
			let data = ctx.data.read().await;
			Arc::clone(data.get::<GuildStates>().unwrap())
		};

		if let Some(state) = gs.get(&g_id) {
			let mut lock = state.write().await;
			lock.set_custom_ack(ack.to_string()).await;
		}

		check_msg(
			msg.channel_id
				.say(&ctx.http, format!("Crediting this server as: {:?}", ack))
				.await,
		);
	}

	Ok(())
}

#[command]
#[aliases("remove-server-ack")]
#[owner_privilege]
pub async fn remove_server_ack(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	if let Some(g_id) = msg.guild_id {
		let gs = {
			let data = ctx.data.read().await;
			Arc::clone(data.get::<GuildStates>().unwrap())
		};

		if let Some(state) = gs.get(&g_id) {
			let mut lock = state.write().await;
			lock.remove_custom_ack().await;
		}

		check_msg(
			msg.channel_id
				.say(&ctx.http, "No longer crediting this server...".to_string())
				.await,
		);
	}

	Ok(())
}

#[command]
#[aliases("server-label")]
#[owner_privilege]
pub async fn server_label(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	match Label::parse(&mut args) {
		Ok(Some(label)) =>
			if let Some(g_id) = msg.guild_id {
				let gs = {
					let data = ctx.data.read().await;
					Arc::clone(data.get::<GuildStates>().unwrap())
				};

				if let Some(state) = gs.get(&g_id) {
					let mut lock = state.write().await;
					lock.set_label(label).await;
				}

				check_msg(
					msg.channel_id
						.say(&ctx.http, format!("Server label set as: {:?}", &label))
						.await,
				);
			},
		Ok(None) => {
			check_msg(
				msg.channel_id
					.say(
						&ctx.http,
						format!("I support the labels: {:?}", &Label::LABEL_LIST),
					)
					.await,
			);
		},
		Err(e) => {
			check_msg(msg.channel_id.say(&ctx.http, match e {
				ConfigParseError::ArgTake => {
					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
				},
				ConfigParseError::BadMode => {
					"Mrowr?! That's an illegal label! Use this commyand without any extra info to see valid chyoices."
				},
				ConfigParseError::IllegalRole => {
					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
				},
				ConfigParseError::MissingRole => {
					"Try that command again, with a role mention or ID!"
				},
			}).await);
		},
	}

	Ok(())
}

#[command]
#[aliases("server-unlabel")]
#[owner_privilege]
pub async fn server_unlabel(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
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

	if let Some(state) = gs.get(&guild.id) {
		let mut lock = state.write().await;
		lock.remove_label().await;
	}

	Ok(())
}

#[command]
#[aliases("gather-mode")]
#[owner_privilege]
pub async fn gather_mode(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	match GatherMode::parse(&mut args) {
		Ok(Some(gm)) =>
			if let Some(g_id) = msg.guild_id {
				let gs = {
					let data = ctx.data.read().await;
					Arc::clone(data.get::<GuildStates>().unwrap())
				};

				if let Some(state) = gs.get(&g_id) {
					let mut lock = state.write().await;
					lock.set_gather(gm).await;
				}

				check_msg(
					msg.channel_id
						.say(&ctx.http, format!("Server gather-mode set as: {:?}", &gm))
						.await,
				);
			},
		Ok(None) => {
			check_msg(
				msg.channel_id
					.say(
						&ctx.http,
						format!("I support the labels: {:?}", &GatherMode::LABEL_LIST),
					)
					.await,
			);
		},
		Err(e) => {
			check_msg(msg.channel_id.say(&ctx.http, match e {
				ConfigParseError::ArgTake => {
					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
				},
				ConfigParseError::BadMode => {
					"Mrowr?! That's an illegal label! Use this commyand without any extra info to see valid chyoices."
				},
				ConfigParseError::IllegalRole => {
					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
				},
				ConfigParseError::MissingRole => {
					"Try that command again, with a role mention or ID!"
				},
			}).await);
		},
	}

	Ok(())
}
