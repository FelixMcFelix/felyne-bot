#[macro_use] extern crate serenity;
extern crate typemap;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;
use typemap::Key;

use serenity::client::*;
use serenity::client::bridge::voice::ClientVoiceManager;
use serenity::Error;
use serenity::framework::standard::{Args, CommandError, DispatchError, StandardFramework, HelpBehaviour, CommandOptions, help_commands};
use serenity::model::*;
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::Result as SResult;
use serenity::utils::*;
use serenity::voice;

struct VoiceManager;

impl Key for VoiceManager {
    type Value = Arc<Mutex<ClientVoiceManager>>;
}

struct Felyne {

}

impl EventHandler for Felyne {
	// fn message(&self, ctx: Context, msg: Message) {
		// match msg.channel_id.say("Mreow!") {
		// 	Ok(_) => println!("Send!"),
		// 	Err(_) => println!("Not send!"),
		// };
	// }
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

	// Establish the bot's config, register all our commands...
	let mut client = Client::new(&token_raw, Felyne {})
		.expect("(I couldn't connyect...)");

	// Okay, copy the client's voice manager into its data area so that commands can see it.
	{
		let mut data = client.data.lock();
		data.insert::<VoiceManager>(Arc::clone(&client.voice_manager));
	}

	// Register all our nice commands etc.
	client.with_framework(
		StandardFramework::new()
			.configure(|c| c
				.prefix("!")
				.case_insensitivity(true))
			.command("join", |c| c
				.known_as("hunt")
				.check(can_wrangle_cats)
				.cmd(cmd_join))
			.command("leave", |c| c
				.known_as("cart")
				.check(can_wrangle_cats)
				.cmd(cmd_leave))
	);

	// Now, log in.
	client.start();
}

fn can_wrangle_cats(_context: &mut Context, message: &Message, _: &mut Args, _: &CommandOptions) -> bool {
	true
}

fn parse_chan_mention(args: &mut Args) -> Option<ChannelId> {
	let chan_name = args.single::<String>().ok()?;
	let channel_id = parse_channel(chan_name.as_str())?;
	Some(ChannelId(channel_id))
}

fn confused(msg: &Message) -> Result<(), CommandError> {
	check_msg(msg.reply("???"));
	Ok(())
}

command!(cmd_join(ctx, msg, args) {
	println!("Saw command: !join");

	// Turn first arg (hopefully a channel mention) into a real channel
	let channel = match args.single::<u64>().ok() {
		Some(c) => ChannelId(c),
		None => {
			return confused(&msg);
		},
	};

	// Get the guild ID.
	let guild = match CACHE.read().guild_channel(msg.channel_id) {
		Some(c) => c.read().guild_id,
		None => {
			return confused(&msg);
		},
	};

	// Invoke some black magic to get the voice manager (???)
	let mut manager_lock = ctx.data.lock().get::<VoiceManager>().cloned().unwrap();
	let mut manager = manager_lock.lock();

	if manager.join(guild, channel).is_some() {
		check_msg(msg.channel_id.say("Mrowr!"));
	} else {
		return confused(&msg);
	}
});

command!(cmd_begin_autojoin(_ctx, _msg) {

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

	// Invoke some more black magic (???)
	let mut manager_lock = ctx.data.lock().get::<VoiceManager>().cloned().unwrap();
	let mut manager = manager_lock.lock();
	let is_in_voicechat_here = manager.get(guild).is_some();

	if is_in_voicechat_here {
		manager.remove(guild);
		check_msg(msg.channel_id.say("Nya..."));
	} else {
		return confused(&msg);
	}
});

fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		println!("Error sending message: {:?}", why);
	}
}