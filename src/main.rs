extern crate parking_lot;
extern crate rand;
#[macro_use] extern crate serenity;
extern crate typemap;
extern crate rusqlite;

mod dbs;
mod watchcat;
mod constants;
mod voicehunt;

use dbs::*;
use watchcat::*;
use constants::*;
use voicehunt::*;

use rusqlite::{Connection, Result as SQLResult};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;
use typemap::Key;

use serenity::client::*;
use serenity::client::bridge::voice::ClientVoiceManager;
use serenity::framework::standard::{Args, CommandError, DispatchError, StandardFramework, HelpBehaviour, CommandOptions, help_commands};
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::Result as SResult;
use serenity::utils::*;
use serenity::voice::{opus, pcm, ytdl, ffmpeg, Bitrate};

struct VoiceManager;

impl Key for VoiceManager {
	type Value = Arc<Mutex<ClientVoiceManager>>;
}

struct FelyneEvts;

impl EventHandler for FelyneEvts {
	fn message(&self, ctx: Context, msg: Message) {
		// match msg.channel_id.say("Mreow!") {
		//  Ok(_) => println!("Send!"),
		//  Err(_) => println!("Not send!"),
		// };

		// Place the message in our "deleted messages cache".
		// This should probably be the last action...
		// Get the guild ID.
		let guild_id = match CACHE.read().guild_channel(msg.channel_id) {
			Some(c) => c.read().guild_id,
			None => {
				return;
			},
		};
		
		watchcat(&ctx, guild_id, WatchcatCommand::BufferMsg(msg));
	}

	fn message_delete(&self, ctx: Context, chan: ChannelId, msg: MessageId) {
		// Get the guild ID.
		let guild_id = match CACHE.read().guild_channel(chan) {
			Some(c) => c.read().guild_id,
			None => {
				return;
			},
		};
		
		watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, vec![msg]));
	}

	fn message_delete_bulk(&self, ctx: Context, chan: ChannelId, msgs: Vec<MessageId>) {
		// Get the guild ID.
		let guild_id = match CACHE.read().guild_channel(chan) {
			Some(c) => c.read().guild_id,
			None => {
				return;
			},
		};

		watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, msgs));
	}

	// Should provide us with a set of full guild info as we connect to each!
	fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
		voicehunt_complete_update(&ctx, guild.id, guild.voice_states);
	}

	fn voice_state_update(&self, ctx: Context, maybe_guild: Option<GuildId>, vox: VoiceState) {
		if maybe_guild.is_none() {return;}
		let guild_id = maybe_guild.unwrap();

		// Okay, now we can get the voice state.
		voicehunt_update(&ctx, guild_id, vox);
	}

	fn ready(&self, ctx: Context, rdy: Ready) {
		ctx.set_game(Game::listening("scary monsters!"));
	}
}

fn help() {
	println!("Mrow mia mrowr?! (Myaster! One file! One token?!)");
}

fn main() {
	// Check arg count -- do we have a file??
	let args: Vec<_> = env::args().collect();

	if args.len() != 2 {
		help();
		return;
	}

	// Okay, take token from the file!
	let mut token = String::new();

	File::open(&args[1])
		.expect(format!("Mraaw, mrow!?! (Grr! '{}' wasn't there?)", &args[1]).as_str())
		.read_to_string(&mut token)
		.expect(format!("Nya!! (I can see '{}', but I can't read it!)", &args[1]).as_str());

	let token_raw = token.as_str().trim();

	validate_token(&token_raw)
		.expect("Naa nya! (Token invalid!)");

	// Init the Database
	let db = match db_conn() {
		Ok(d) => d,
		Err(e) => {
			println!("Nya nya nya?!?! (Couldn't init database: {:?})", e);
			return;
		}
	};

	// Try and build tables, if we don't have them.
	match init_db_tables(&db) {
		Err(e) => {
			println!("Nya nya nya?!?! (Couldn't setup db tables: {:?})", e);
			return;
		},
		_ => {},
	};

	// Establish the bot's config, register all our commands...
	let mut client = Client::new(&token_raw, FelyneEvts {})
		.expect("(I couldn't connyect...)");

	// Okay, copy the client's voice manager into its data area so that commands can see it.
	{
		let mut data = client.data.lock();
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
			.command("hunt", |c| c
				.allowed_roles(MANAGE_ROLES)
				.cmd(cmd_join))
			.command("cart", |c| c
				.allowed_roles(MANAGE_ROLES)
				.cmd(cmd_leave))
			.command("log-to", |c| c
				.allowed_roles(MANAGE_ROLES)
				.cmd(cmd_log_to))
			.command("github", |c| c
				.cmd(cmd_github))
			.command("ids", |c| c
				.cmd(cmd_enumerate_voice_channels))
	);

	// Now, log in.
	client.start();
}

command!(cmd_log_to(ctx, msg, args) {
	let out_chan = parse_chan_mention(&mut args);

	if out_chan.is_none() {
		return confused(msg);
	}

	let out_chan = out_chan.unwrap();

	// Get the guild ID.
	let guild_id = match CACHE.read().guild_channel(out_chan) {
		Some(c) => c.read().guild_id,
		None => {
			return confused(msg);
		},
	};

	watchcat(&ctx, guild_id, WatchcatCommand::SetChannel(out_chan));

	check_msg(msg.channel_id.say("Mrowrorr! (I'll keep you nyotified!)"));
});

command!(cmd_join(ctx, msg, args) {
	// Get the guild ID.
	let guild = match CACHE.read().guild_channel(msg.channel_id) {
		Some(c) => c.read().guild_id,
		None => {
			return confused(&msg);
		},
	};

	// Turn first arg (hopefully a channel mention) into a real channel
	voicehunt_control(
		&ctx,
		guild,
		match args.single::<u64>().ok() {
			Some(c) => {
				// TODO: make use of string parsing for greeat good.
				VoiceHuntJoinMode::DirectedHunt(ChannelId(c))
			},
			None => {VoiceHuntJoinMode::BraveHunt},
	});

	// // Invoke some black magic to get the voice manager (???)
	// let mut manager_lock = ctx.data.lock().get::<VoiceManager>().cloned().unwrap();
	// let mut manager = manager_lock.lock();

	// if manager.join(guild, channel).is_some() {
	// 	check_msg(msg.channel_id.say("Mrowr!"));

	// 	// test play
	// 	let mut handler = manager.get_mut(guild).unwrap();

	// 	let source = ffmpeg("sfx/mewl-wiggle2.ogg").unwrap();

	// 	let safe_aud = handler.play_returning(source);

	// 	{
	// 		let aud_lock = safe_aud.clone();
	// 		let mut aud = aud_lock.lock();

	// 		aud.volume(1.0);
	// 	}

	// 	handler.set_bitrate(Bitrate::Bits(128_000));

	// } else {
	// 	return confused(&msg);
	// }
});

command!(cmd_leave(ctx, msg) {
	println!("Saw command: !leave");

	// Get the guild ID.
	let guild = match CACHE.read().guild_channel(msg.channel_id) {
		Some(c) => c.read().guild_id,
		None => {
			return confused(&msg);
		},
	};

	voicehunt_control(&ctx, guild, VoiceHuntJoinMode::Carted);

	// // Invoke some more black magic (???)
	// let mut manager_lock = ctx.data.lock().get::<VoiceManager>().cloned().unwrap();
	// let mut manager = manager_lock.lock();
	// let is_in_voicechat_here = match manager.get_mut(guild) {
	// 	Some(handler) => {handler.stop(); true}
	// 	None => false,
	// };

	// if is_in_voicechat_here {
	// 	manager.remove(guild);
	// 	check_msg(msg.channel_id.say("Nya..."));
	// } else {
	// 	return confused(&msg);
	// }
});

command!(cmd_enumerate_voice_channels(_ctx, msg) {
	let guild = match CACHE.read().guild_channel(msg.channel_id) {
		Some(c) => c.read().guild_id,
		None => {
			return confused(&msg);
		},
	};

	let mut content = MessageBuilder::new()
		.push_bold_line(format!("ChannelIDs for {}:", guild.get().unwrap().name));

	for channel in guild.channels().unwrap().values() {
		if channel.kind == ChannelType::Voice {
			content = content.push(&channel.name)
				.push_bold(" --- ")
				.push_line(&channel.id);
		}
	}

	let out = content.build();

	check_msg(msg.author.dm(|m| m.content(out)))
});

command!(cmd_github(_ctx, msg) {
	// yeah whatever
	check_msg(msg.channel_id.say("Mya! :heart: (https://github.com/FelixMcFelix/felyne-bot)"))
});

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

pub fn confused(msg: &Message) -> Result<(), CommandError> {
	check_msg(msg.reply("???"));
	Ok(())
}

pub fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		println!("Error sending message: {:?}", why);
	}
}