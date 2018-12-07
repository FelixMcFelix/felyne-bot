use dbs::*;
use constants::*;
use parking_lot::Mutex;
use rand::random;
use rusqlite::{Connection, Result as SQLResult};
use serenity::client::*;
use serenity::model::prelude::*;
use serenity::utils::*;
use std::{
    collections::HashMap,
    sync::Arc,
    thread,
};
use typemap::Key;

type Space = Arc<Mutex<Option<(String, Vec<u8>)>>>;

#[derive(Clone)]
pub struct AttachmentHolder {
    store: Vec<Space>,
}

impl AttachmentHolder {
	pub fn new(base_msg: &Message) -> Self {
		// need to store a vec of mutexed option types
		// need a thread for each attachment (threads can panic of whatever, have a timeout)
		// thread replaces option contents on the inside of the mutex
        let store = Vec::new();

		return AttachmentHolder {
            store
        };
	}
}
//			if let Some(message) = msg_full {
//				for (i, a) in message.attachments.iter().enumerate() {
//					match a.download() {
//						Ok(val) => {
//							let block = vec![(val.as_slice(), a.filename.as_str())];
//							out_channel.send_files(block, |m| m.content(format!("Myah! {}: file {}!", msg, i)));
//						},
//						Err(e) => println!("Couldn't download attachment {}: {:?}", a.filename, e),
//					}
//				}
//			}

pub struct CircQueue<T> {
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

pub struct GuildDeleteData {
	output_channel: Option<ChannelId>,
	//backup: CircQueue<(Message,AttachmentHolder)>,
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

pub struct DeleteWatchcat;

impl Key for DeleteWatchcat {
	type Value = HashMap<GuildId, GuildDeleteData>;
}

pub enum WatchcatCommand {
	SetChannel(ChannelId),
	ReportDelete(ChannelId, Vec<MessageId>),
	BufferMsg(Message),
}

pub fn watchcat(ctx: &Context, guild_id: GuildId, cmd: WatchcatCommand) {
	let mut datas = ctx.data.lock();
	let top_dog = datas.get_mut::<DeleteWatchcat>()
		.unwrap()
		.entry(guild_id)
		.or_insert(GuildDeleteData::new(None));

	let db = db_conn().unwrap();

	if let SetChannel(_) = cmd {
		
	} else if let Ok(chan) = select_watchcat(&db, guild_id) {
		top_dog.output_channel = Some(ChannelId(chan));
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
			// Watchdog messages should be removable, if needed!
			if out_channel == chan {return;}
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
			let mut attachment_text = String::new();

			if let Some(message) = msg_full {
				let message = msg_full.unwrap();
				content = message.content_safe();

				attachment_text = match message.attachments.len() {
					n if n <= 0 => String::new(),
					n @ _ => format!("{} attachment{}! I'm digging them up---wait patiently, nya!", n, if n > 1 {"s"} else {""}),
				};

				let author = &message.author;
				author_name = author.tag();
				author_mention = author.mention();
				author_img = author.face();
			}
			
			match out_channel.send_message(|m| m
				.embed(|e| {
					let base = e.colour(Colour::from_rgb(236, 98, 0))
					.author(|a| a
						.name(author_name.as_str())
						.icon_url(author_img.as_str()))
					.description(
						format!("**Grraow?! (Myessage by {} in {} stolen!)**\n{}",
							author_mention,
							chan.mention(),
							content))
					.footer(|f| f
						.text(format!("ID: {}. Nyarowr... (I think that {} has it...)", msg, MONSTERS[random::<usize>()%MONSTERS.len()])));

					if !attachment_text.is_empty() {
						base.field(
							"I think they dropped something!",
							attachment_text,
							true
						)
					} else {
						base
					}
				})
			) {
				Ok(_) => {
					// println!("Apparently sent: {:?}", mess);
				},
				Err(e) => {println!("{:?}", e);},
			}

			if let Some(message) = msg_full {
				for (i, a) in message.attachments.iter().enumerate() {
					match a.download() {
						Ok(val) => {
							let block = vec![(val.as_slice(), a.filename.as_str())];
							out_channel.send_files(block, |m| m.content(format!("Myah! {}: file {}!", msg, i)));
						},
						Err(e) => println!("Couldn't download attachment {}: {:?}", a.filename, e),
					}
				}
			}
		},
		None => {},
	}
}
