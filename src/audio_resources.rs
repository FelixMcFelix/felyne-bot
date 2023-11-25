use crate::constants::*;
use dashmap::DashMap;
use serenity::prelude::*;
use songbird::{
	self,
	input::{File, Input},
};
use std::sync::Arc;

pub struct Resources;

pub type RxMap = Arc<DashMap<&'static str, CachedSound>>;

impl TypeMapKey for Resources {
	type Value = RxMap;
}

pub struct CachedSound(File<String>);

impl From<&CachedSound> for Input {
	fn from(obj: &CachedSound) -> Self {
		obj.0.clone().into()
	}
}

pub async fn preload_resources() -> RxMap {
	let resources = DashMap::new();
	add_resources(&resources, "bgm", BBQ).await;
	add_resources(&resources, "bgm", BBQ_RESULT).await;
	add_resources(&resources, "bgm", SLEEP).await;
	add_resources(&resources, "bgm", START).await;
	add_resources(&resources, "bgm", AMBIENCE).await;
	add_resources(&resources, "bgm", BGM).await;

	add_resources(&resources, "sfx", SFX).await;
	add_resources(&resources, "sfx", BONUS_SFX).await;

	Arc::new(resources)
}

async fn add_resources<'a>(
	rx: &'a DashMap<&'static str, CachedSound>,
	folder: &'static str,
	files: &'static [&'static str],
) {
	for file_id in files {
		rx.insert(
			file_id,
			CachedSound(File::new(format!("{}/{}", folder, file_id))),
		);
	}
}
