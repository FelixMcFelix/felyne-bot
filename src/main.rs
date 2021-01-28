mod audio_resources;
mod automata;
mod commands;
mod config;
mod constants;
mod dbs;
mod event_handler;
mod server;
mod voicehunt;
mod watchcat;

use crate::{
	audio_resources::*,
	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	event_handler::*,
	voicehunt::*,
	watchcat::*,
};
use dashmap::DashMap;
use serenity::{
	async_trait,
	client::{*, bridge::gateway::GatewayIntents},
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
use songbird::{
	self,
	input::{
		cached::{Compressed, Memory},
		Input,
	},
	Bitrate,
	SerenityInit,
};
use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};
use tokio_postgres::Client as DbClient;
use tracing::*;

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
		.unwrap_or_else(|_| panic!("Mraaw, mrow!?! (Grr! '{}' wasn't there?)", &args[1]))
		.read_to_string(&mut token)
		.unwrap_or_else(|_| panic!("Nya!! (I can see '{}', but I can't read it!)", &args[1]));

	let bot_config: BotConfig =
		serde_json::from_str(&token).expect("Mrrya?! (Configuration file was invalid!)");

	let token_raw = bot_config.token.as_str().trim();

	validate_token(&token_raw).expect("Naa nya! (Token invalid!)");

	// Init the Database
	let mut db = match db_conn(&bot_config.database).await {
		Ok(d) => d,
		Err(e) => {
			error!("Nya nya nya?!?! (Couldn't init database: {:?})", e);
			return;
		},
	};

	// Try and build tables, if we don't have them.
	if let Err(e) = init_db_tables(&db).await {
		error!("Nya nya nya?!?! (Couldn't setup db tables: {:?})", e);
		return;
	}

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
			c
			// .prefix("!")
			.dynamic_prefix(|ctx, msg| { Box::pin(async move {
				let datas = ctx.data.read().await;

				let db = datas.get::<Db>().expect("DB conn installed...");

				Some((if let Some(g_id) = msg.guild(&ctx.cache).await {
					select_prefix(&db, g_id.id).await.ok()
				} else {None}).unwrap_or_else(|| "!".to_string()))
			})})
			.on_mention(Some(bot_id))
			.owners(move_owners)
			.case_insensitivity(true)
		})
		.group(&commands::PUBLIC_GROUP)
		.group(&commands::CONTROL_GROUP)
		.group(&commands::ADMIN_GROUP);

	let client = Client::builder(&token_raw)
		.event_handler(event_handler::FelyneEvts)
		.framework(framework)
		.register_songbird()
		.intents(
			GatewayIntents::GUILDS
				| GatewayIntents::GUILD_MESSAGES
				| GatewayIntents::GUILD_VOICE_STATES, // | GatewayIntents::GUILD_MEMBERS
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
		data.insert::<VoiceHunt>(HashMap::new());

		data.insert::<Db>(db);
		data.insert::<Owners>(owners);

		let resources = DashMap::new();
		add_resources(&resources, "bgm", BBQ, false).await;
		add_resources(&resources, "bgm", BBQ_RESULT, false).await;
		add_resources(&resources, "bgm", SLEEP, true).await;
		add_resources(&resources, "bgm", START, true).await;
		add_resources(&resources, "bgm", AMBIENCE, true).await;
		add_resources(&resources, "bgm", BGM, true).await;

		add_resources(&resources, "sfx", SFX, false).await;
		add_resources(&resources, "sfx", BONUS_SFX, false).await;

		data.insert::<Resources>(Arc::new(resources));
	}

	// Now, log in.
	client.start().await.expect("Argh! I couldn't connyect?!");

	println!("Uh");
}
