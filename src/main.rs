extern crate rand;
#[macro_use] extern crate serenity;
extern crate typemap;
extern crate rusqlite;

mod dbs;

use dbs::{db_conn, init_db_tables};

use rand::random;
use rusqlite::{Connection, Result as SQLResult};
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
use serenity::http::get_message;
use serenity::model::*;
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::Result as SResult;
use serenity::utils::*;
use serenity::voice::{opus, pcm, ytdl, ffmpeg, Bitrate};

const BACKUP_SIZE: usize = 500;

struct VoiceManager;

impl Key for VoiceManager {
	type Value = Arc<Mutex<ClientVoiceManager>>;
}

struct CircQueue<T> {
	data: Box<[Option<T>]>,
	base: usize,
	len: usize,
}

impl <T:Clone> CircQueue<T> {
	fn new(size: usize) -> Self {
		CircQueue {
			data: vec![None; size].into_boxed_slice(),
			base: 0,
			len: 0,
		}
	}

	fn add_and_march(&mut self, element: T) {
		if self.len == self.data.len() {
			self.base = wrap(self.base, 1, &self.data);
		} else {
			self.len += 1;
		}

		self.data[wrap(self.base, self.len-1, &self.data)] = Some(element);
	}

	fn head(&self) -> &Option<T> {
		&self.data[self.base]
	}

	fn tail(&self) -> &Option<T> {
		&self.data[wrap(self.base, self.data.len().max(1)-1, &self.data)]
	}

	fn get(&self, index: usize) -> &Option<T> {
		match index < self.data.len() {
			true => &self.data[wrap(self.base, index, &self.data)],
			false => &None,
		}
	}

	// fn as_slice(&self) -> &[Option<T>] {
	//  let tail_pos = wrap(self.base, self.data.len()-1, &self.data);
	//  match tail_pos < self.base {
	//      false => &self.data[self.base..tail_pos],
	//      true => [
	//              &self.data[self.base..self.data.len()-1],
	//              &self.data[0..tail_pos],
	//          ].concat(),
	//  }
	// }
}

#[inline]
fn wrap<T>(position: usize, increment: usize, buf: &[T]) -> usize {(position + increment) % buf.len()}

struct GuildDeleteData {
	output_channel: Option<ChannelId>,
	backup: CircQueue<Message>,
}

impl GuildDeleteData {
	fn new(output_channel: Option<ChannelId>) -> Self {
		GuildDeleteData{
			output_channel,
			backup: CircQueue::new(BACKUP_SIZE),
		}
	}
}

struct DeleteWatchcat;

impl Key for DeleteWatchcat {
	type Value = HashMap<GuildId, GuildDeleteData>;
}

struct FelyneDb;

impl Key for FelyneDb {
	type Value = Connection;
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
}

enum WatchcatCommand {
	SetChannel(ChannelId),
	ReportDelete(ChannelId, Vec<MessageId>),
	BufferMsg(Message),
}

fn watchcat(ctx: &Context, guild_id: GuildId, cmd: WatchcatCommand) {
	let mut datas = ctx.data.lock();
	let mut top_dog = datas.get_mut::<DeleteWatchcat>()
		.unwrap()
		.entry(guild_id)
		.or_insert(GuildDeleteData::new(None));

	let db = db_conn().unwrap();

	match cmd {
		SetChannel(_) => {},
		_ => {
			match select_watchcat(&db, guild_id) {
				Ok(chan) => {top_dog.output_channel = Some(ChannelId(chan));},
				_ => {},
			}
		}
	}

	use WatchcatCommand::*;

	match cmd {
		SetChannel(chan) => {
			top_dog.output_channel = Some(chan);

			upsert_watchcat(&db, guild_id, chan);
		},
		ReportDelete(event_chan, msgs) => {
			for msg in msgs {
				report_delete(&top_dog, event_chan, msg);
			}
		},
		BufferMsg(msg) => {
			top_dog.backup.add_and_march(msg);	
		},
	}
}

fn select_watchcat(db: &Connection, guild_id: GuildId) -> SQLResult<u64> {
	let GuildId(t_id) = guild_id;
	db.query_row("SELECT channel_id FROM del_watchcat WHERE guild_id=?", &[&t_id.to_string()],
		|row| {let r: String = row.get(0); r.parse::<u64>().unwrap()})
}

fn upsert_watchcat(db: &Connection, guild_id: GuildId, channel_id: ChannelId) {
	let GuildId(t_g_id) = guild_id;
	let ChannelId(t_c_id) = channel_id;
	match db
		.execute("INSERT OR REPLACE INTO del_watchcat (guild_id, channel_id)
					VALUES (?,?);", &[&t_g_id.to_string(), &t_c_id.to_string()]) {
		Err(e) => {println!("Nya?! (Couldn't write del_watchcat db updates.){:?}", e);}
		Ok(_) => {},
	}
}

fn report_delete(delete_data: &GuildDeleteData, chan: ChannelId, msg: MessageId) {
	match delete_data.output_channel {
		Some(out_channel) => {
			// let content = match get_message(true_chan_id, true_msg_id) {
			//  Ok(true_msg) => true_msg.content,
			//  Err(_) => String::from("Hiss... (I couldn't find what it was?!)"),
			// };

			// Try to find it!
			let msgs = &delete_data.backup;
			let len = delete_data.backup.len;
			let mut curr = 0;

			let mut msg_full = None;
			while curr < len {
				let s = msgs.get(curr);
				match s {
					&Some(ref message) => {
						if message.id == msg {
							msg_full = Some(message);
						}
					},
					&None => {println!("{}: None", curr);},
				}
				curr += 1;
			};

			let mut content = String::from("Hiss... (I couldn't find what it was?!)");
			let mut author_img = String::new();
			let mut author_name = String::from("Unknyown author");
			let mut author_mention = String::from("Unknyown author");

			if msg_full.is_some() {
				let message = msg_full.unwrap();
				content = message.content_safe();
				let author = &message.author;
				author_name = author.tag();
				author_mention = author.mention();
				author_img = author.face();
			}
			
			match out_channel.send_message(|m| m
				.embed(|e| e
					.colour(Colour::from_rgb(236, 98, 0))
					.author(|a| a
						.name(author_name.as_str())
						.icon_url(author_img.as_str()))
					.description(
						format!("**Grraow?! (Myessage by {} in {} stolen!)**\n{}",
							author_mention,
							chan.mention(),
							content))
					.footer(|f| f
						.text(format!("ID: {}. Nyarowr... (I think that {} has it...)", msg, MONSTERS[random::<usize>()%MONSTERS.len()])))
				)
			) {
				Ok(mess) => {
					// println!("Apparently sent: {:?}", mess);
				},
				Err(e) => {println!("{:?}", e);},
			}
		},
		None => {},
	}
}

fn help() {
	println!("Mrow mia mrowr?! (Myaster! One file! One token?!)");
}

const MANAGE_ROLES: &'static [&'static str] = &["certified cat wrangler"];
const MONSTERS: &'static [&'static str] = &[
	"Jaggi",
	"Nargacuga",
	"Barroth",
	"Zinogre",
	"Arzuroth",
	"Deviljho"
];

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
		// data.insert::<FelyneDb>(db);
	}

	// Register all our nice commands etc.
	client.with_framework(
		StandardFramework::new()
			.configure(|c| c
				.prefix("!")
				.case_insensitivity(true))
			.command("hunt", |c| c
				.known_as("join")
				.allowed_roles(MANAGE_ROLES)
				.cmd(cmd_join))
			.command("go-hunt", |c| c
				.allowed_roles(MANAGE_ROLES)
				.cmd(cmd_begin_autojoin))
			.command("cart", |c| c
				.known_as("leave")
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

fn parse_chan_mention(args: &mut Args) -> Option<ChannelId> {
	let chan_name = args.single::<String>().ok()?;
	let channel_id = parse_channel(chan_name.as_str())?;
	Some(ChannelId(channel_id))
}

fn confused(msg: &Message) -> Result<(), CommandError> {
	check_msg(msg.reply("???"));
	Ok(())
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

		// test play
		let mut handler = manager.get_mut(guild).unwrap();

		let source = ffmpeg("sfx/mewl-wiggle2.ogg").unwrap();

		let safe_aud = handler.play_returning(source);

		{
			let aud_lock = safe_aud.clone();
			let mut aud = aud_lock.lock();

			aud.volume(1.0);
		}

		handler.set_bitrate(Bitrate::Bits(512_000));


	} else {
		return confused(&msg);
	}
});

command!(cmd_begin_autojoin(_ctx, _msg) {
	// TODO: haha?!
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
	let is_in_voicechat_here = match manager.get_mut(guild) {
		Some(handler) => {handler.stop(); true}
		None => false,
	};

	if is_in_voicechat_here {
		manager.remove(guild);
		check_msg(msg.channel_id.say("Nya..."));
	} else {
		return confused(&msg);
	}
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

fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		println!("Error sending message: {:?}", why);
	}
}