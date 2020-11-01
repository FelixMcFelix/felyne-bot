mod automata;
mod config;
mod constants;
mod dbs;
mod server;
mod voicehunt;
mod watchcat;

use crate::{
	config::{ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	voicehunt::*,
	watchcat::*,
};
use core::future::Future;
use dashmap::DashMap;
use futures::future::FutureExt;
use log::*;
use serenity::{
	async_trait,
	client::*,
	framework::standard::{
		macros::{check, command, group, help},
		Args,
		Check,
		CheckResult,
		CommandError,
		CommandGroup,
		CommandOptions,
		CommandResult,
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
use sqlx::sqlite::SqlitePool;
use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};

struct Db;

impl TypeMapKey for Db {
	type Value = DbPools;
}

struct Owners;

impl TypeMapKey for Owners {
	type Value = HashSet<UserId>;
}

struct DbPools {
	read: SqlitePool,
	write: SqlitePool,
}

struct Resources;

type RxMap = Arc<DashMap<&'static str, CachedSound>>;

impl TypeMapKey for Resources {
	type Value = RxMap;
}

enum CachedSound {
	Compressed(Compressed),
	Uncompressed(Memory),
}

impl From<&CachedSound> for Input {
	fn from(obj: &CachedSound) -> Self {
		use CachedSound::*;
		match obj {
			Compressed(c) => c.new_handle()
				// .expect("Opus errors on decoder creation are rare if we're copying valid settings.")
				.into(),
			Uncompressed(u) => u.new_handle().try_into().unwrap(),
		}
	}
}

struct FelyneEvts;

#[async_trait]
impl EventHandler for FelyneEvts {
	async fn message(&self, ctx: Context, msg: Message) {
		// Place the message in our "deleted messages cache".
		// This should probably be the last action...
		// Get the guild ID.
		let guild_id = match msg.guild(&ctx.cache).await {
			Some(guild) => guild.id,
			None => {
				return;
			},
		};

		watchcat(&ctx, guild_id, WatchcatCommand::BufferMsg(Box::new(msg))).await;
	}

	async fn message_delete(&self, ctx: Context, chan: ChannelId, msg: MessageId) {
		// Get the guild ID.
		let guild = chan.to_channel(&ctx).await.map(Channel::guild);

		let guild_id = match guild {
			Ok(Some(c)) => c.guild_id,
			_ => {
				return;
			},
		};

		watchcat(
			&ctx,
			guild_id,
			WatchcatCommand::ReportDelete(chan, vec![msg]),
		)
		.await;
	}

	async fn message_delete_bulk(&self, ctx: Context, chan: ChannelId, msgs: Vec<MessageId>) {
		// Get the guild ID.
		let guild = chan.to_channel(&ctx).await.map(Channel::guild);

		let guild_id = match guild {
			Ok(Some(c)) => c.guild_id,
			_ => {
				return;
			},
		};

		watchcat(&ctx, guild_id, WatchcatCommand::ReportDelete(chan, msgs)).await;
	}

	// Should provide us with a set of full guild info as we connect to each!
	async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
		voicehunt_complete_update(&ctx, guild.id, guild.voice_states).await;
	}

	async fn voice_state_update(
		&self,
		ctx: Context,
		maybe_guild: Option<GuildId>,
		_old_vox: Option<VoiceState>,
		vox: VoiceState,
	) {
		if let Some(guild_id) = maybe_guild {
			voicehunt_update(&ctx, guild_id, vox).await;
		}
	}

	async fn ready(&self, ctx: Context, _rdy: Ready) {
		ctx.set_activity(Activity::listening("scary monsters!"))
			.await;
	}
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

	let token_raw = token.as_str().trim();

	validate_token(&token_raw).expect("Naa nya! (Token invalid!)");

	// Init the Database
	let mut db = match db_conn().await {
		Ok(d) => d,
		Err(e) => {
			error!("Nya nya nya?!?! (Couldn't init database: {:?})", e);
			return;
		},
	};

	// Try and build tables, if we don't have them.
	if let Err(e) = init_db_tables(&mut db.write).await {
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
					select_prefix(&db.read, g_id.id).await.ok()
				} else {None}).unwrap_or_else(|| "!".to_string()))
			})})
			.on_mention(Some(bot_id))
			.owners(move_owners)
			.case_insensitivity(true)
		})
		.group(&PUBLIC_GROUP)
		.group(&CONTROL_GROUP)
		.group(&ADMIN_GROUP);

	let mut client = Client::builder(&token)
		.event_handler(FelyneEvts)
		.framework(framework)
		.register_songbird()
		.await
		.expect("Err creating client");

	// Okay, copy the client's voice manager into its data area so that commands can see it.
	{
		let mut data = client.data.write().await;
		data.insert::<DeleteWatchcat>(DashMap::new());
		data.insert::<VoiceHunt>(HashMap::new());

		data.insert::<Db>(db);
		data.insert::<Owners>(owners);

		let resources = DashMap::new();
		// println!("A");
		add_resources(&resources, "bgm", BBQ, false).await;
		// println!("A");
		add_resources(&resources, "bgm", BBQ_RESULT, false).await;
		add_resources(&resources, "bgm", SLEEP, true).await;
		add_resources(&resources, "bgm", START, true).await;
		add_resources(&resources, "bgm", AMBIENCE, true).await;
		add_resources(&resources, "bgm", BGM, true).await;

		add_resources(&resources, "sfx", SFX, false).await;
		add_resources(&resources, "sfx", BONUS_SFX, false).await;

		// println!("A");

		data.insert::<Resources>(Arc::new(resources));
	}

	// Now, log in.
	client.start().await.expect("Argh! I couldn't connyect?!");

	println!("Uh");
}

async fn add_resources<'a>(
	rx: &'a DashMap<&'static str, CachedSound>,
	folder: &'static str,
	files: &'static [&'static str],
	compress: bool,
) {
	for file_id in files {
		let file_name = format!("{}/{}", folder, file_id);
		let base = songbird::ffmpeg(&file_name)
			.await
			.expect("File should be in root folder.");
		let file = if compress {
			let src = Compressed::new(base, Bitrate::BitsPerSecond(128_000))
				.expect("Apparent critical failure to make file...");
			let _ = src.raw.spawn_loader();
			CachedSound::Compressed(src)
		} else {
			let src = Memory::new(base).expect("Apparent critical failure to make file...");
			let _ = src.raw.spawn_loader();
			CachedSound::Uncompressed(src)
		};
		rx.insert(file_id, file);
	}
}

#[check]
#[name = "Control"]
async fn can_control_cat(
	ctx: &Context,
	msg: &Message,
	args: &mut Args,
	opts: &CommandOptions,
) -> CheckResult {
	let g_id = match msg.guild_id {
		Some(id) => id,
		_ =>
			return CheckResult::new_user(
				"Control commands only valid in guild channel (i.e., not DMs).",
			),
	};

	let datas = ctx.data.read().await;

	let db = datas.get::<Db>().expect("DB conn installed...");

	let ctl = select_control_cfg(&db.read, g_id)
		.await
		.ok()
		.unwrap_or_default();

	println!("Need ctl check: {:?}", ctl);

	let o = match shared_ctl_check(ctl, ctx, msg).await {
		CheckResult::Success => CheckResult::Success,
		a => match can_admin_cat(ctx, msg, args, opts).await {
			CheckResult::Success => CheckResult::Success,
			a => a,
		},
	};
	println!("{:?}", o);
	o
}

#[check]
#[name = "Admin"]
async fn can_admin_cat(
	ctx: &Context,
	msg: &Message,
	args: &mut Args,
	opts: &CommandOptions,
) -> CheckResult {
	let g_id = match msg.guild_id {
		Some(id) => id,
		_ =>
			return CheckResult::new_user(
				"Control commands only valid in guild channel (i.e., not DMs).",
			),
	};

	let datas = ctx.data.read().await;

	let db = datas.get::<Db>().expect("DB conn installed...");

	let ctl = select_control_admin_cfg(&db.read, g_id)
		.await
		.ok()
		.unwrap_or_default();

	println!("Need admin check: {:?}", ctl);

	shared_ctl_check(ctl, ctx, msg).await
}

async fn shared_ctl_check(control: CfgControl, ctx: &Context, msg: &Message) -> CheckResult {
	use CfgControl::*;

	(match control {
		OwnerOnly => msg.guild(ctx).await.map(|guild| {
			if guild.owner_id == msg.author.id {
				CheckResult::Success
			} else {
				println!(
					"Not a valid person: {:?} vs {:?}",
					msg.author.id, guild.owner_id
				);
				CheckResult::new_user("User is not server/bot owner.")
			}
		}),
		WithRole(role) => msg.member(ctx).await.ok().map(|member| {
			if member.roles.contains(&role) {
				CheckResult::Success
			} else {
				CheckResult::new_user("User lacks necessary role.")
			}
		}),
		All => Some(CheckResult::Success),
	})
	.unwrap_or_else(|| {
		CheckResult::new_user("Checked command occurred outside of a Guild Channel.")
	})
}

#[group]
#[commands(github, ids)]
struct Public;

#[group]
#[checks(Control)]
#[commands(hunt, cart, volume, watch)]
struct Control;

#[group]
#[checks(Admin)]
#[commands(log_to, felyne_prefix, admin_ctl_mode, ctl_mode)]
struct Admin;

#[command]
#[aliases("log-to")]
async fn log_to(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let out_chan = parse_chan_mention(&mut args);

	if out_chan.is_none() {
		return confused(&ctx, msg).await;
	}

	let out_chan = out_chan.unwrap();

	// Get the guild ID.
	let guild_id = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, msg).await;
		},
	};

	watchcat(&ctx, guild_id, WatchcatCommand::SetChannel(out_chan)).await;

	check_msg(
		msg.channel_id
			.say(&ctx.http, "Mrowrorr! (I'll keep you nyotified!)")
			.await,
	);

	Ok(())
}

#[command]
#[aliases("felyne-prefix")]
async fn felyne_prefix(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	let new_prefix = args.single::<String>();

	if new_prefix.is_err() {
		return confused(&ctx, msg).await;
	}

	let new_prefix = new_prefix.unwrap();

	// Get the guild ID.
	let guild_id = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, msg).await;
		},
	};

	let mut datas = ctx.data.read().await;
	let db = datas.get::<Db>().expect("DB conn installed...");

	upsert_prefix(&db.write, guild_id, &new_prefix).await;

	check_msg(
		msg.channel_id
			.say(
				&ctx.http,
				format!("Listening to nyew prefix: {}", &new_prefix),
			)
			.await,
	);

	Ok(())
}

#[command]
#[aliases("admin-ctl-mode")]
async fn admin_ctl_mode(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	ctl_mode_basis(ctx, msg, args, true).await
}

#[command]
#[aliases("ctl-mode")]
async fn ctl_mode(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	ctl_mode_basis(ctx, msg, args, false).await
}

async fn ctl_mode_basis(
	ctx: &Context,
	msg: &Message,
	mut args: Args,
	do_for_admin: bool,
) -> CommandResult {
	match CfgControl::parse(&mut args) {
		Ok(Some(cm)) => {
			let mut datas = ctx.data.read().await;
			let db = datas.get::<Db>().expect("DB conn installed...");

			if let Some(g_id) = msg.guild_id {
				if do_for_admin {
					upsert_control_admin_cfg(&db.write, g_id, cm).await;
				} else {
					upsert_control_cfg(&db.write, g_id, cm).await;
				}
				check_msg(
					msg.channel_id
						.say(
							&ctx.http,
							format!("Now accepting admin commands from: {:?}", &cm),
						)
						.await,
				);
			}

			// new mode
		},
		Ok(None) => {
			check_msg(
				msg.channel_id
					.say(
						&ctx.http,
						format!("I support the modes: {:?}", &ControlMode::LabelList),
					)
					.await,
			);
		},
		Err(e) => {
			check_msg(msg.channel_id.say(&ctx.http, match e {
				ConfigParseError::ArgTake => {
					"Uhh, this shouldn't have happened. Report this to FelixMcFelix#2443?"
				},
				ConfigParseError::BadMode => {
					"Mrowr?! That's an illegal mode! Use this commyand without any extra info to see valid chyoices."
				},
				ConfigParseError::IllegalRole => {
					"Myeh? That role doesn't look valid to me: make sure it's a valid mention or ID!"
				},
				ConfigParseError::MissingRole => {
					"Try that command again, with a role mention or ID!"
				},
			}).await);
		},
	}

	Ok(())
}

#[command]
async fn hunt(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
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
			None => VoiceHuntCommand::BraveHunt,
		},
	)
	.await;

	check_msg(msg.channel_id.say(&ctx.http, "Mrowr!").await);

	Ok(())
}

#[command]
async fn watch(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	// Turn first arg (hopefully a channel mention) into a real channel
	voicehunt_control(&ctx, guild, VoiceHuntCommand::Stalk).await;

	check_msg(msg.channel_id.say(&ctx.http, "...").await);

	Ok(())
}

#[command]
async fn cart(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Carted).await;

	check_msg(msg.channel_id.say(&ctx.http, "Mrr... :zzz:").await);

	Ok(())
}

#[command]
#[aliases("vol")]
async fn volume(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
	// Get the guild ID.
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let vol = match args.single::<f32>().ok() {
		Some(c) => c,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	if !vol.is_finite() || vol < 0.0 || vol > 2.0 {
		return confused(&ctx, &msg).await;
	}

	voicehunt_control(&ctx, guild, VoiceHuntCommand::Volume(vol)).await;

	Ok(())
}

#[command]
async fn ids(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	let guild = match msg.guild(&ctx.cache).await {
		Some(c) => c.id,
		None => {
			return confused(&ctx, &msg).await;
		},
	};

	let mut content = MessageBuilder::new();
	content.push_bold_line(format!("ChannelIDs for {}:", guild));

	for channel in guild.channels(&ctx.http).await.unwrap().values() {
		if channel.kind == ChannelType::Voice {
			content
				.push(&channel.name)
				.push_bold(" --- ")
				.push_line(&channel.id);
		}
	}

	let out = content.build();

	check_msg(msg.author.dm(&ctx, |m| m.content(out)).await);

	Ok(())
}

#[command]
async fn github(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
	// yeah whatever
	check_msg(
		msg.channel_id
			.say(
				&ctx.http,
				"Mya! :heart: (https://github.com/FelixMcFelix/felyne-bot)",
			)
			.await,
	);

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

pub async fn confused(ctx: &Context, msg: &Message) -> CommandResult {
	check_msg(msg.reply(ctx, "???").await);
	Ok(())
}

pub fn check_msg(result: SResult<Message>) {
	if let Err(why) = result {
		warn!("Error sending message: {:?}", why);
	}
}
