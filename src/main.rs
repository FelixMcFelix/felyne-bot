#[macro_use] extern crate serenity;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::prelude::*;

use serenity::client::*;
use serenity::framework::standard::{Args, DispatchError, StandardFramework, HelpBehaviour, CommandOptions, help_commands};
use serenity::model::*;
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::Result as SResult;
use serenity::voice;

struct Felyne {

}

impl EventHandler for Felyne {
	fn message(&self, ctx: Context, msg: Message) {
		match msg.channel_id.say("Mreow!") {
			Ok(_) => println!("Send!"),
			Err(_) => println!("Not send!"),
		};
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

	// Establish the bot's config, register all our commands...
	let mut client = Client::new(&token_raw, Felyne {})
		.expect("(I couldn't connyect...)");

	client.with_framework(
		StandardFramework::new()
			.configure(|c| c
				.prefix("!")
				.case_insensitivity(true))
			.cmd("help", cmd_help)
			.command("join", |c| c
				.check(can_wrangle_cats)
				.cmd(cmd_join))
	);

	// Now, log in.
	client.start();
}

fn can_wrangle_cats(_context: &mut Context, message: &Message, _: &mut Args, _: &CommandOptions) -> bool {
	true
}

command!(cmd_help(ctx, msg) {
	check_msg(msg.channel_id.say("Mrowr!"))
});

command!(cmd_join(ctx, msg) {
	
});

fn check_msg(result: SResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}