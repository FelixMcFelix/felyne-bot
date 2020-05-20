mod automata;
mod constants;
mod dbs;
mod voicehunt;
mod watchcat;

use crate::{
	dbs::*,
	watchcat::*,
	constants::*,
	voicehunt::*,
};
use log::*;
use serenity::{
	client::{
		*,
		bridge::voice::ClientVoiceManager,
	},
	framework::standard::{
		macros::{
			check,
			command,
			group,
			help,
		},
		Args,
		CheckResult,
		CommandError,
		CommandGroup,
		CommandOptions,
		CommandResult,
		StandardFramework,
	},
	model::prelude::*,
	prelude::*,
	Result as SResult,
	utils::*,
};
use std::{
	collections::HashMap,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};

struct VoiceManager;

impl TypeMapKey for VoiceManager {
	type Value = Arc<Mutex<ClientVoiceManager>>;
}

struct FelyneEvts;

impl EventHandler for FelyneEvts {
	fn message(&self, ctx: Context, msg: Message) {
		// Place the message in our "deleted messages cache".
		// This should probably be the last action...
		// Get the guild ID.
		let guild_id = match msg.guild(&ctx.cache) {
			Some(c) => c.read().id,
			None => {
				return;
			},
		};
		
		watchcat(&ctx, guild_id, WatchcatCommand::BufferMsg(Box::new(msg)));
	}

	fn message_delete(&self, ctx: Context, chan: ChannelId, msg: MessageId) {
		// Get the guild ID.
		let guild = chan.to_channel(&ctx)
			.map(Channel::guild);

		let guild_id = match guild {
			Ok(Some(c)) => c.read().guild_id,
			_ => {
				return;
			},
		};
		
		watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, vec![msg]));
	}

	fn message_delete_bulk(&self, ctx: Context, chan: ChannelId, msgs: Vec<MessageId>) {
		// Get the guild ID.
		let guild = chan.to_channel(&ctx)
			.map(Channel::guild);

		let guild_id = match guild {
			Ok(Some(c)) => c.read().guild_id,
			_ => {
				return;
			},
		};

		watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, msgs));
	}

	// Should provide us with a set of full guild info as we connect to each!
	fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
		voicehunt_complete_update(&ctx, guild.id, guild.voice_states);
	}

	fn voice_state_update(&self, ctx: Context, maybe_guild: Option<GuildId>, _old_vox: Option<VoiceState>, vox: VoiceState) {
		if maybe_guild.is_none() {return;}
		let guild_id = maybe_guild.unwrap();

		// Okay, now we can get the voice state.
		voicehunt_update(&ctx, guild_id, vox);
	}

	fn ready(&self, ctx: Context, _rdy: Ready) {
		ctx.set_activity(Activity::listening("scary monsters!"));
	}
}

fn help() {
	println!("Mrow mia mrowr?! (Myaster! One file! One token?!)");
}

fn main() {
	env_logger::init();
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

	let token_raw = token.as_str().trim();

	validate_token(&token_raw)
		.expect("Naa nya! (Token invalid!)");

	// Init the Database
	let db = match db_conn() {
		Ok(d) => d,
		Err(e) => {
			error!("Nya nya nya?!?! (Couldn't init database: {:?})", e);
			return;
		}
	};

	// Try and build tables, if we don't have them.
	if let Err(e) = init_db_tables(&db) {
		error!("Nya nya nya?!?! (Couldn't setup db tables: {:?})", e);
		return;
	}

	// Establish the bot's config, register all our commands...
	let mut client = Client::new(&token_raw, FelyneEvts {})
		.expect("(I couldn't connyect...)");

	// Okay, copy the client's voice manager into its data area so that commands can see it.
	{
		let mut data = client.data.write();
		data.insert::<VoiceManager>(Arc::clone(&client.voice_manager));
		data.insert::<DeleteWatchcat>(HashMap::new());
		data.insert::<VoiceHunt>(HashMap::new());
	}

	// Register all our nice commands etc.
	client.with_framework(
		StandardFramework::new()
			.configure(|c| c
				.prefix("!")
				.case_insensitivity(true))
			.group(&PUBLIC_GROUP)
			.group(&CONTROL_GROUP)
	);

	// Now, log in.
	client.start().expect("Argh! I couldn't connyect?!");
}

#[group]
#[commands(github, ids)]
struct Public;

#[group]
#[allowed_roles("certified cat wrangler")]
#[commands(hunt, cart, volume, watch, log_to)]
struct Control;

#[command]
#[aliases("log-to")]
fn log_to(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
	let out_chan = parse_chan_mention(&mut args);

	if out_chan.is_none() {
		return confused(&ctx, msg);
	}

	let out_chan = out_chan.unwrap();

	// Get the guild ID.
	let guild_id = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, msg);
		},
	};

	watchcat(&ctx, guild_id, WatchcatCommand::SetChannel(out_chan));

	check_msg(msg.channel_id.say(&ctx.http, "Mrowrorr! (I'll keep you nyotified!)"));

	Ok(())
}

#[command]
fn hunt(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, &msg);
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
			None => {VoiceHuntCommand::BraveHunt},
	});

	check_msg(msg.channel_id.say(&ctx.http, "Mrowr!"));

	Ok(())
}

#[command]
fn watch(ctx: &mut Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, &msg);
		},
	};

	// Turn first arg (hopefully a channel mention) into a real channel
	voicehunt_control(
		&ctx,
		guild,
		VoiceHuntCommand::Stalk,
	);

	check_msg(msg.channel_id.say(&ctx.http, "..."));

	Ok(())
}

#[command]
fn cart(ctx: &mut Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, &msg);
		},
	};

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Carted);

	check_msg(msg.channel_id.say(&ctx.http, "Mrr... :zzz:"));

	Ok(())
}

#[command]
#[aliases("vol")]
fn volume(ctx: &mut Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, &msg);
		},
	};

	let vol = match args.single::<f32>().ok() {
		Some(c) => c,
		None => {return confused(&ctx, &msg);},
	};

	if !vol.is_finite() || vol < 0.0 || vol > 2.0 {
		return confused(&ctx, &msg);	
	}

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Volume(vol));

	Ok(())
}

#[command]
fn ids(ctx: &mut Context, msg: &Message, _args: Args) -> CommandResult { 
	let guild = match msg.guild(&ctx.cache) {
		Some(c) => c.read().id,
		None => {
			return confused(&ctx, &msg);
		},
	};

	let mut content = MessageBuilder::new();
	content.push_bold_line(format!("ChannelIDs for {}:", guild));

	for channel in guild.channels(&ctx.http).unwrap().values() {
		if channel.kind == ChannelType::Voice {
			content.push(&channel.name)
				.push_bold(" --- ")
				.push_line(&channel.id);
		}
	}

	let out = content.build();

	check_msg(msg.author.dm(&ctx, |m| m.content(out)));

	Ok(())
}

#[command]
fn github(ctx: &mut Context, msg: &Message, _args: Args) -> CommandResult {
	// yeah whatever
	check_msg(msg.channel_id.say(&ctx.http, "Mya! :heart: (https://github.com/FelixMcFelix/felyne-bot)"));
	
	Ok(())
}

pub fn parse_chan_mention(args: &mut Args) -> Option<ChannelId> {
	let chan_name = args.single::<String>().ok()?;
	let channel_id = parse_channel(chan_name.as_str())?;
	Some(ChannelId(channel_id))
}

// pub fn guild_from_chan(channel_id: ChannelId) {
// 	match CACHE.read().guild_channel(channel_id) {
// 		Some(c) => c.read().guild_id,
// 		None => 0,
// 	};
// }

pub fn confused(ctx: &Context, msg: &Message) -> Result<(), CommandError> {
	check_msg(msg.reply(ctx, "???"));
	Ok(())
}

pub fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		warn!("Error sending message: {:?}", why);
	}
}
