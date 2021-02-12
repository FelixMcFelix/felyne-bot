mod audio_resources;
mod automata;
mod commands;
mod config;
mod constants;
mod dbs;
mod event_handler;
mod guild;
mod server;
mod user;
mod voicehunt;
mod watchcat;

use crate::{
	audio_resources::*,
	config::BotConfig,
	dbs::*,
	guild::GuildStates,
	user::*,
	voicehunt::*,
	watchcat::*,
};
use dashmap::DashMap;
use serenity::{
	client::{bridge::gateway::GatewayIntents, *},
	framework::standard::StandardFramework,
	http::client::Http,
	model::prelude::*,
	prelude::*,
};
use songbird::{self, SerenityInit};
use std::{collections::HashSet, env, sync::Arc};
use tokio::{fs::File, io::AsyncReadExt};

use tracing::*;

struct MyId;

impl TypeMapKey for MyId {
	type Value = UserId;
}

struct Owners;

impl TypeMapKey for Owners {
	type Value = HashSet<UserId>;
}

fn help() {
	println!("Mrow mia mrowr?! (Myaster! One file! One token?!)");
}

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();
	// Check arg count -- do we have a file??
	let args: Vec<_> = env::args().collect();

	if args.len() != 2 {
		help();
		return;
	}

	// Okay, take token from the file!
	let mut token = String::new();

	File::open(&args[1])
		.await
		.unwrap_or_else(|_| panic!("Mraaw, mrow!?! (Grr! '{}' wasn't there?)", &args[1]))
		.read_to_string(&mut token)
		.await
		.unwrap_or_else(|_| panic!("Nya!! (I can see '{}', but I can't read it!)", &args[1]));

	let bot_config: BotConfig =
		serde_json::from_str(&token).expect("Mrrya?! (Configuration file was invalid!)");

	let token_raw = bot_config.token.as_str().trim();

	validate_token(&token_raw).expect("Naa nya! (Token invalid!)");

	// Init the Database
	let db = match db_conn(&bot_config.database).await {
		Ok(d) => Arc::new(d),
		Err(e) => {
			error!("Nya nya nya?!?! (Couldn't init database: {:?})", e);
			return;
		},
	};

	let (owners, bot_id) = {
		let http = Http::new_with_token(&token_raw);

		match http.get_current_application_info().await {
			Ok(info) => {
				let mut owners = HashSet::new();
				owners.insert(info.owner.id);

				(owners, info.id)
			},
			Err(why) => panic!("Could not access application info: {:?}", why),
		}
	};

	let move_owners = owners.clone();

	// Establish the bot's config, register all our commands...
	let framework = StandardFramework::new()
		.configure(|c| {
			c.prefix("")
				.dynamic_prefix(|ctx, msg| {
					Box::pin(async move {
						let gs = {
							let data = ctx.data.read().await;
							Arc::clone(data.get::<GuildStates>().unwrap())
						};

						let id = if let Some(guild) = msg.guild(&ctx.cache).await {
							guild.id
						} else {
							return None;
						};

						let out = if let Some(state) = gs.get(&id) {
							let lock = state.read().await;
							lock.custom_prefix()
								.as_ref()
								.cloned()
								.unwrap_or_else(|| "!".to_string())
						} else {
							"!".to_string()
						};

						Some(out)
					})
				})
				.on_mention(Some(bot_id))
				.owners(move_owners)
				.case_insensitivity(true)
		})
		.group(&commands::EVERYONE_GROUP)
		.group(&commands::CONTROL_GROUP)
		.group(&commands::ADMIN_GROUP)
		.help(&commands::MY_HELP);

	let client = Client::builder(&token_raw)
		.event_handler(event_handler::FelyneEvts)
		.framework(framework)
		.register_songbird()
		.intents(
			GatewayIntents::GUILDS
				| GatewayIntents::GUILD_MESSAGES
				| GatewayIntents::GUILD_VOICE_STATES,
		)
		.await;

	if let Err(e) = &client {
		println!("MRAOWR! ({})", e);
	}

	let mut client = client.expect("Err creating client");

	// Okay, copy the client's voice manager into its data area so that commands can see it.
	{
		let mut data = client.data.write().await;
		data.insert::<DeleteWatchcat>(DashMap::new());
		data.insert::<VoiceHunt>(DashMap::new());
		data.insert::<GuildStates>(Default::default());
		data.insert::<UserStateKey>(Arc::new(UserState::new(db.clone()).await));

		data.insert::<Db>(db);
		data.insert::<Owners>(owners);
		data.insert::<MyId>(bot_id);

		data.insert::<Resources>(preload_resources().await);
	}

	// Now, log in.
	client.start().await.expect("Argh! I couldn't connyect?!");

	println!("Uh");
}
