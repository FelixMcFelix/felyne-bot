#[macro_use] extern crate serenity;

use std::env;
use std::fs::File;
use std::io::prelude::*;

use serenity::client::Client;
use serenity::prelude::EventHandler;

struct Handler;

impl EventHandler for Handler {}

fn help() {
	panic!("Mrow mia mrowr?! (Myaster! One file! One token?!)");
}

fn main() {
	let args: Vec<_> = env::args().collect();

	if args.len() != 2 {
		help();
	}

	let mut token = String::new();

	File::open(&args[1])
		.expect(format!("Mraaw, mrow!?! (Grr! '{}' wasn't there?)", &args[1]).as_str())
		.read_to_string(&mut token)
		.expect(format!("Nya!! (I can see '{}', but I can't read it!)", &args[1]).as_str());

	println!("I see:");
	println!("{}", token.as_str());

	let mut client = Client::new(token.as_str(), Handler)
		.expect("(I couldn't connyect...)");
}
