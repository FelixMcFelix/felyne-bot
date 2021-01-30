use super::*;

use crate::{config::OptInOut, dbs::*, guild::*, user::UserStateKey};

use serenity::{
	client::*,
	framework::standard::{macros::command, Args, CommandResult},
	model::prelude::*,
};
use std::sync::Arc;

#[command]
#[description = "Mya! (You don't want to help with network measurement? That's okay!)"]
pub async fn optout(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	let us = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<UserStateKey>().unwrap())
	};

	us.optout(msg.author.id).await;

	check_msg(
		msg.reply(
			&ctx.http,
			"Mya! :heart: (Thanks for the heads up! I won't pay you any mind!)",
		)
		.await,
	);

	Ok(())
}

#[command]
#[description = "Mya! (You want to help out with network measurement!)"]
#[only_in(guilds)]
pub async fn optin(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let (us, gs) = {
		let data = ctx.data.read().await;
		(
			Arc::clone(data.get::<UserStateKey>().unwrap()),
			Arc::clone(data.get::<GuildStates>().unwrap()),
		)
	};

	us.optin(msg.author.id).await;

	if let Some(gs) = gs.get(&guild.id) {
		let kind = {
			let lock = gs.read().await;
			lock.server_opt()
		};

		match kind {
			OptInOut::ServerOut => {
				check_msg(
					msg.reply(
						&ctx.http,
						"Mreh... (Thanks, but this server is not participating...)",
					)
					.await,
				);
			},
			OptInOut::UserIn(r) => {
				let role_added = guild.member(ctx, msg.author.id).await;
				let role_added = match role_added {
					Ok(mut member) => member.add_role(ctx, r).await,
					Err(e) => Err(e),
				};

				let msg_txt = if role_added.is_ok() {
					"Mya! (Successfully opted in with a tag!)".to_string()
				} else {
					format!(
						"Mrowr?! (I couldn't give you the role {}. Ask an admin?!)",
						if let Some(r_full) = r.to_role_cached(ctx).await {
							r_full.name.clone()
						} else {
							format!("(ID {:?})", r.0)
						}
					)
				};

				check_msg(msg.reply(&ctx.http, msg_txt).await);
			},
			OptInOut::ServerIn => {
				check_msg(msg.reply(&ctx.http, "Mya! (Thanks!)").await);
			},
		}
	}

	Ok(())
}

#[command]
#[description = "Mraww? (If I'm measuring how folks talk, should I credit you?)"]
#[owner_privilege]
pub async fn ack(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
	let user_id = msg.author.id;

	let new_str = args.rest().trim();
	let ack = if !new_str.is_empty() {
		new_str.to_string()
	} else {
		msg.author.name.clone()
	};

	let db = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<Db>().unwrap())
	};

	upsert_user_ack(&db, user_id, &ack).await;

	check_msg(
		msg.channel_id
			.say(&ctx.http, format!("Crediting you as: {:?}", ack))
			.await,
	);

	Ok(())
}

#[command]
#[aliases("remove-ack")]
#[description = "Mya!? (You don't want to be credited anymore?)"]
#[owner_privilege]
pub async fn remove_ack(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	let user_id = msg.author.id;

	let db = {
		let data = ctx.data.read().await;
		Arc::clone(data.get::<Db>().unwrap())
	};

	delete_user_ack(&db, user_id).await;

	check_msg(
		msg.channel_id
			.say(&ctx.http, "No longer crediting you...".to_string())
			.await,
	);

	Ok(())
}
