use crate::{config::*, dbs::*, server::Label};
use dashmap::DashMap;
use serenity::{model::prelude::GuildId, prelude::*, utils::MessageBuilder};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_postgres::Client;

pub type GuildMap = Arc<DashMap<GuildId, Arc<RwLock<GuildState>>>>;

pub struct GuildStates;

impl TypeMapKey for GuildStates {
	type Value = GuildMap;
}

pub struct GuildState {
	db: Arc<Client>,
	guild: GuildId,

	gather: GatherMode,
	server_opt: OptInOut,
	admin_control_mode: Control,
	voice_control_mode: Control,
	label: Label,
	custom_ack: Option<String>,
	custom_prefix: Option<String>,
}

impl GuildState {
	pub async fn new(db: Arc<Client>, guild: GuildId) -> Self {
		let gather = select_gather_cfg(&db, guild).await.unwrap_or_default();

		let server_opt = select_opt_in_out(&db, guild).await.unwrap_or_default();

		let admin_control_mode = select_control_admin_cfg(&db, guild)
			.await
			.unwrap_or_default();

		let voice_control_mode = select_control_cfg(&db, guild).await.unwrap_or_default();

		let label = select_server_type(&db, guild).await.unwrap_or_default();

		let custom_ack = select_guild_ack(&db, guild).await.ok();

		let custom_prefix = select_prefix(&db, guild).await.ok();

		Self {
			db,
			guild,

			gather,
			server_opt,
			admin_control_mode,
			voice_control_mode,
			label,
			custom_ack,
			custom_prefix,
		}
	}

	pub fn to_message(&self) -> String {
		let mut builder = MessageBuilder::new();

		builder.push_bold_line(format!("Admin details for {}:", self.guild));

		builder.push("Admin control: ");
		builder.push_italic_line(format!("{:?}", self.admin_control_mode));
		builder.push("Voice control: ");
		builder.push_italic_line(format!("{:?}", self.voice_control_mode));
		builder.push("Custom command prefix: ");
		builder.push_italic_line(format!("{:?}", self.custom_prefix));

		builder.push_bold_line(format!("Measurement details for {}:", self.guild));

		builder.push("Server opted in: ");
		builder.push_italic_line(format!("{:?}", self.server_opt));

		if !self.server_opt.opted_out() {
			builder.push("Server measure mode: ");
			builder.push_italic_line(format!("{:?}", self.gather));
			builder.push("Server type: ");
			builder.push_italic_line(format!("{:?}", self.label));

			if let Some(ack) = &self.custom_ack {
				builder.push("Server acknowledgement as: ");
				builder.push_italic_line(ack);
			} else {
				builder.push_line("Server not being explicitly acknowledged.");
			}
		}

		builder.build()
	}

	pub fn gather(&self) -> GatherMode {
		self.gather
	}

	pub async fn set_gather(&mut self, val: GatherMode) {
		self.gather = val;
		upsert_gather_cfg(&self.db, self.guild, val).await;
	}

	pub fn server_opt(&self) -> OptInOut {
		self.server_opt
	}

	pub async fn set_server_opt(&mut self, val: OptInOut) {
		self.server_opt = val;
		upsert_opt_in_out(&self.db, self.guild, val).await;
	}

	pub fn admin_control_mode(&self) -> Control {
		self.admin_control_mode
	}

	pub async fn set_admin_control_mode(&mut self, val: Control) {
		self.admin_control_mode = val;
		upsert_control_admin_cfg(&self.db, self.guild, val).await;
	}

	pub fn voice_control_mode(&self) -> Control {
		self.voice_control_mode
	}

	pub async fn set_voice_control_mode(&mut self, val: Control) {
		self.voice_control_mode = val;
		upsert_control_cfg(&self.db, self.guild, val).await;
	}

	pub fn label(&self) -> Label {
		self.label
	}

	pub async fn set_label(&mut self, val: Label) {
		self.label = val;
		upsert_server_type(&self.db, self.guild, val).await;
	}

	pub async fn remove_label(&mut self) {
		self.label = Default::default();
		delete_server_type(&self.db, self.guild).await;
	}

	pub async fn set_custom_ack(&mut self, val: String) {
		self.custom_ack = Some(val.clone());
		upsert_guild_ack(&self.db, self.guild, &val).await;
	}

	pub async fn remove_custom_ack(&mut self) {
		self.custom_ack = None;
		delete_guild_ack(&self.db, self.guild).await;
	}

	pub fn custom_prefix(&self) -> &Option<String> {
		&self.custom_prefix
	}

	pub async fn set_custom_prefix(&mut self, val: String) {
		self.custom_prefix = Some(val.clone());
		upsert_prefix(&self.db, self.guild, &val).await;
	}
}
