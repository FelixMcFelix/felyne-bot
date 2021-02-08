use crate::{constants::*, Db};
use dashmap::DashMap;
use rand::random;
use serenity::{client::*, model::prelude::*, prelude::*, utils::*};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::RwLock;
use tracing::*;

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

			tokio::spawn(async move {
				match a.download().await {
					Ok(val) => {
						let mut store_space = inner_obj.lock().await;
						*store_space = Some(val);
					},
					Err(e) => warn!("Couldn't download attachment {}: {:?}", a.filename, e),
				}
			});

			store.push((name, obj));
		}

		AttachmentHolder { store }
	}
}

pub struct GuildDeleteData {
	output_channel: Option<ChannelId>,
	backup: VecDeque<(Box<Message>, AttachmentHolder)>,
}

impl GuildDeleteData {
	fn new(output_channel: Option<ChannelId>) -> Self {
		GuildDeleteData {
			output_channel,
			backup: VecDeque::with_capacity(BACKUP_SIZE),
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

pub async fn watchcat(ctx: &Context, guild_id: GuildId, cmd: WatchcatCommand) {
	let datas = ctx.data.read().await;
	let top_dog_holder = datas.get::<DeleteWatchcat>().unwrap();

	if !top_dog_holder.contains_key(&guild_id) {
		top_dog_holder.insert(guild_id, Arc::new(RwLock::new(GuildDeleteData::new(None))));
	}

	let db = datas.get::<Db>().expect("DB conn installed...");

	if let SetChannel(_) = cmd {
	} else if let Ok(chan) = db.select_watchcat(guild_id).await {
		let top_do = top_dog_holder
			.get(&guild_id)
			.expect("Guaranteed to exist by above insertion");
		let mut top_dog = top_do.write().await;
		top_dog.output_channel = Some(ChannelId(chan));
	}

	use crate::WatchcatCommand::*;

	match cmd {
		SetChannel(chan) => {
			let top_do = top_dog_holder
				.get(&guild_id)
				.expect("Guaranteed to exist by above insertion");
			let mut top_dog = top_do.write().await;
			top_dog.output_channel = Some(chan);

			db.upsert_watchcat(guild_id, chan).await;
		},
		ReportDelete(event_chan, msgs) => {
			let top_do = top_dog_holder
				.get(&guild_id)
				.expect("Guaranteed to exist by above insertion");
			let top_dog = top_do.read().await;
			for msg in msgs {
				report_delete(&top_dog, event_chan, msg, ctx).await;
			}
		},
		BufferMsg(msg) => {
			let top_do = top_dog_holder
				.get(&guild_id)
				.expect("Guaranteed to exist by above insertion");
			let mut m = msg.clone();
			let attachments = AttachmentHolder::new(&mut m.attachments);
			let mut top_dog = top_do.write().await;
			if top_dog.backup.len() == top_dog.backup.capacity() {
				top_dog.backup.pop_front();
			}
			top_dog.backup.push_back((m, attachments));
		},
	}
}

async fn report_delete(
	delete_data: &GuildDeleteData,
	chan: ChannelId,
	removed_msg: MessageId,
	ctx: &Context,
) {
	if let Some(out_channel) = delete_data.output_channel {
		// Watchdog messages should be removable, if needed!
		if out_channel == chan {
			return;
		}
		// Try to find it!
		let msgs = &delete_data.backup;

		let mut content = String::from("Hiss... (I couldn't find what it was?!)");
		let mut author_img = String::new();
		let mut author_name = String::from("Unknyown author");
		let mut author_mention = String::from("Unknyown author");
		let mut attachment_text = String::new();

		let recovered = msgs.iter().find(|message| message.0.id == removed_msg);

		if let Some((message, attachments_holder)) = recovered {
			content = message.content_safe(&ctx.cache).await;

			attachment_text = match attachments_holder.store.len() {
				0 => String::new(),
				n => format!(
					"{} attachment{}! I'm digging them up---wait patiently, nya!",
					n,
					if n > 1 { "s" } else { "" }
				),
			};

			let author = &message.author;
			author_name = author.tag();
			author_mention = author.mention().to_string();
			author_img = author.face();
		}

		match out_channel
			.send_message(&ctx.http, |m| {
				m.embed(|e| {
					let base = e
						.colour(Colour::from_rgb(236, 98, 0))
						.author(|a| a.name(author_name.as_str()).icon_url(author_img.as_str()))
						.description(format!(
							"**Grraow?! (Myessage by {} in {} stolen!)**\n{}",
							author_mention,
							chan.mention(),
							content
						))
						.footer(|f| {
							f.text(format!(
								"ID: {}. Nyarowr... (I think that {} has it...)",
								removed_msg,
								MONSTERS[random::<usize>() % MONSTERS.len()]
							))
						});

					if !attachment_text.is_empty() {
						base.field("I think they dropped something!", attachment_text, true)
					} else {
						base
					}
				})
			})
			.await
		{
			Ok(_) => {},
			Err(e) => {
				warn!("Issue recording delete: {:?}", e);
			},
		}

		if let Some(message) = recovered {
			for (i, (name, locked_maybe_file)) in message.1.store.iter().enumerate() {
				let maybe_file = locked_maybe_file.lock().await;
				match *maybe_file {
					Some(ref val) => {
						let block = vec![(val.as_slice(), name.as_str())];
						let _ = out_channel
							.send_files(&ctx.http, block, |m| m.content(format!("File {}!", i)))
							.await;
					},
					None => {
						let _ = out_channel
							.send_message(&ctx.http, |m| {
								m.content(format!("Couldn't recover file {}...", i))
							})
							.await;
					},
				}
			}
		}
	}
}
