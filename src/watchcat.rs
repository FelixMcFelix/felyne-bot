use crate::{
	dbs::*,
	constants::*,
	Db,
	TokioHandle,
};
use dashmap::DashMap;
use log::*;
use parking_lot::RwLock;
use rand::random;
use serenity::{
	client::*,
	model::prelude::*,
	prelude::*,
	utils::*,
};
use sqlx::{
	executor::RefExecutor,
	prelude::*,
	sqlite::SqliteRow,
	Error as SqlError,
	SqlitePool,
};
use std::{
	collections::HashMap,
	sync::Arc,
	thread,
};
use tokio::task;

type Space = (String, Arc<Mutex<Option<Vec<u8>>>>);

#[derive(Clone)]
pub struct AttachmentHolder {
	pub store: Vec<Space>,
}

impl AttachmentHolder {
	pub fn new(attachments: &mut Vec<Attachment>) -> Self {
		// need to store a vec of mutexed option types
		// need a thread for each attachment (threads can panic of whatever, have a timeout)
		// thread replaces option contents on the inside of the mutex
		let mut store = Vec::new();

		for a in attachments.drain(..) {
			let obj = Arc::new(Mutex::new(None));
			let inner_obj = obj.clone();
			let name = a.filename.clone();

			thread::spawn(move || {
				match a.download() {
					Ok(val) => {
						let mut store_space = inner_obj.lock();
						*store_space = Some(val);
					},
					Err(e) => warn!("Couldn't download attachment {}: {:?}", a.filename, e),
				}
			});

			store.push((name, obj));
		}

		AttachmentHolder {
			store
		}
	}
}

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
		if index < self.data.len() {
			&self.data[wrap(self.base, index, &self.data)]
		} else {
			&None
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
	backup: CircQueue<(Box<Message>, AttachmentHolder)>,
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

impl TypeMapKey for DeleteWatchcat {
	type Value = DashMap<GuildId, Arc<RwLock<GuildDeleteData>>>;
}

pub enum WatchcatCommand {
	SetChannel(ChannelId),
	ReportDelete(ChannelId, Vec<MessageId>),
	BufferMsg(Box<Message>),
}

pub fn watchcat(ctx: &Context, guild_id: GuildId, cmd: WatchcatCommand) {
	let mut datas = ctx.data.read();
	let top_dog_holder = datas.get::<DeleteWatchcat>()
		.unwrap();

	if !top_dog_holder.contains_key(&guild_id) {
		top_dog_holder.insert(guild_id, Arc::new(RwLock::new(GuildDeleteData::new(None))));
	}

	let db = datas.get::<Db>().expect("DB conn installed...");
	let handle = datas.get::<TokioHandle>().expect("Handle installed...");

	if let SetChannel(_) = cmd {
		
	} else if let Ok(chan) = handle.block_on(select_watchcat(&db, guild_id)) {
		top_dog_holder.update(&guild_id, |_k, top_do| {
			let mut top_dog = top_do.write();
			top_dog.output_channel = Some(ChannelId(chan));
			top_do.clone()
		});
	}

	use crate::WatchcatCommand::*;

	match cmd {
		SetChannel(chan) => {
			top_dog_holder.update(&guild_id, |_k, top_do| {
				let mut top_dog = top_do.write();
				top_dog.output_channel = Some(chan);
				top_do.clone()
			});

			handle.block_on(upsert_watchcat(&db, guild_id, chan));
		},
		ReportDelete(event_chan, msgs) => {
			let top_do = top_dog_holder.get(&guild_id).unwrap();
			let top_dog = top_do.read();
			for msg in msgs {
				report_delete(&top_dog, event_chan, msg, ctx);
			}
		},
		BufferMsg(mut msg) => {
			top_dog_holder.update(&guild_id, move |_k, top_do| {
				let mut m = msg.clone();
				let attachments = AttachmentHolder::new(&mut m.attachments);
				let mut top_dog = top_do.write();
				top_dog.backup.add_and_march((m, attachments));
				top_do.clone()
			});
		},
	}
}

async fn select_watchcat(db: &SqlitePool, guild_id: GuildId) -> Result<u64, SqlError> {
	let GuildId(t_id) = guild_id;

	// let db = db.acquire().await?;

	sqlx::query("SELECT channel_id FROM del_watchcat WHERE guild_id = ?")
		.bind(&t_id.to_string())
		.map(|row: SqliteRow| {
			let a: &str = row.get(0);
			a.parse::<u64>()
				.unwrap()
			}
		)
		.fetch_one(db)
		.await
}

async fn upsert_watchcat(db: &SqlitePool, guild_id: GuildId, channel_id: ChannelId) {
	let GuildId(t_g_id) = guild_id;
	let ChannelId(t_c_id) = channel_id;

	let query = sqlx::query("INSERT OR REPLACE INTO del_watchcat (guild_id, channel_id) VALUES (?,?);")
		.bind(&t_g_id.to_string())
		.bind(&t_c_id.to_string())
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write del_watchcat db updates.){:?}", e);
	}
}

fn report_delete(delete_data: &GuildDeleteData, chan: ChannelId, msg: MessageId, ctx: &Context) {
	if let Some(out_channel) = delete_data.output_channel {
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
				Some(ref message) => {
					if message.0.id == msg {
						msg_full = Some(message);
					}
				},
				None => info!("{}: None", curr),
			}
			curr += 1;
		};

		let mut content = String::from("Hiss... (I couldn't find what it was?!)");
		let mut author_img = String::new();
		let mut author_name = String::from("Unknyown author");
		let mut author_mention = String::from("Unknyown author");
		let mut attachment_text = String::new();

		if let Some((message, attachments_holder)) = msg_full {
			content = message.content_safe(&ctx.cache);

			attachment_text = match attachments_holder.store.len() {
				0 => String::new(),
				n => format!("{} attachment{}! I'm digging them up---wait patiently, nya!", n, if n > 1 {"s"} else {""}),
			};

			let author = &message.author;
			author_name = author.tag();
			author_mention = author.mention();
			author_img = author.face();
		}
		
		match out_channel.send_message(&ctx.http, |m| m
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
			Ok(_) => {},
			Err(e) => {warn!("Issue recording delete: {:?}", e);},
		}

		if let Some(message) = msg_full {
			for (i, (name, locked_maybe_file)) in message.1.store.iter().enumerate() {
				let maybe_file = locked_maybe_file.lock();
				match *maybe_file {
					Some(ref val) => {
						let block = vec![(val.as_slice(), name.as_str())];
						let _ = out_channel.send_files(
							&ctx.http,
							block, 
							|m| m.content(format!("File {}!", i))
						);
					},
					None => {
						let _ = out_channel.send_message(
							&ctx.http,
							|m| m.content(format!("Couldn't recover file {}...", i))
						);
					},
				}
			}
		}
	}
}
