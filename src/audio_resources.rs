use crate::{
	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	voicehunt::*,
	watchcat::*,
};
use dashmap::DashMap;
use serenity::{
	async_trait,
	client::*,
	framework::standard::{
		macros::{check, command, group, help},
		Args,
		CommandOptions,
		CommandResult,
		Reason as CheckReason,
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
use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};
use tokio_postgres::Client as DbClient;
use tracing::*;

pub struct Resources;

pub type RxMap = Arc<DashMap<&'static str, CachedSound>>;

impl TypeMapKey for Resources {
	type Value = RxMap;
}

pub enum CachedSound {
	Compressed(Compressed),
	Uncompressed(Memory),
}

impl From<&CachedSound> for Input {
	fn from(obj: &CachedSound) -> Self {
		use CachedSound::*;
		match obj {
			Compressed(c) => c.new_handle().into(),
			Uncompressed(u) => u.new_handle().try_into().unwrap(),
		}
	}
}

pub async fn add_resources<'a>(
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
