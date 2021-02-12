use crate::{config::*, dbs::*, server::Label, voicehunt::mode::Join, UserStateKey};
use dashmap::DashMap;
use serenity::{
	client::Context,
	model::prelude::{GuildId, User},
	prelude::*,
	utils::MessageBuilder,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type GuildMap = Arc<DashMap<GuildId, Arc<RwLock<GuildState>>>>;

pub struct GuildStates;

impl TypeMapKey for GuildStates {
	type Value = GuildMap;
}

pub struct GuildState {
	db: Arc<FelyneDb>,
	guild: GuildId,

	gather: GatherMode,
	server_opt: OptInOut,
	admin_control_mode: Control,
	voice_control_mode: Control,
	voicehunt_mode: Join,
	label: Label,
	custom_ack: Option<String>,
	custom_prefix: Option<String>,
}

impl GuildState {
	pub async fn new(db: Arc<FelyneDb>, guild: GuildId) -> Self {
		let gather = db.select_gather_cfg(guild).await.unwrap_or_default();

		let server_opt = db.select_opt_in_out(guild).await.unwrap_or_default();

		let admin_control_mode = db.select_control_admin_cfg(guild).await.unwrap_or_default();

		let voice_control_mode = db.select_control_cfg(guild).await.unwrap_or_default();

		let label = db.select_server_type(guild).await.unwrap_or_default();

		let custom_ack = db.select_guild_ack(guild).await.ok();

		let custom_prefix = db.select_prefix(guild).await.ok();

		let voicehunt_mode = db.select_join_cfg(guild).await.unwrap_or_default();

		Self {
			db,
			guild,

			gather,
			server_opt,
			admin_control_mode,
			voice_control_mode,
			voicehunt_mode,
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
		builder.push_italic_line_safe(format!("{:?}", self.custom_prefix));

		builder.push_bold_line(format!("Measurement details for {}:", self.guild));

		builder.push("Server opted in: ");
		builder.push_italic_line(format!("{:?}", self.server_opt));

		if !self.server_opt.opted_out() {
			builder.push("Server measure mode: ");
			builder.push_italic_line(format!("{:?}", self.gather));
			builder.push("Server type: ");
			builder.push_italic_line(format!("{:?}", self.label()));

			if let Some(ack) = &self.custom_ack {
				builder.push("Server acknowledgement as: ");
				builder.push_italic_line_safe(ack);
			} else {
				builder.push_line("Server not being explicitly acknowledged.");
			}
		}

		builder.build()
	}

	pub async fn to_info_message(&self, ctx: &Context, user: &User, guild: GuildId) -> String {
		let us = {
			let data = ctx.data.read().await;
			Arc::clone(data.get::<UserStateKey>().unwrap())
		};

		let mut builder = MessageBuilder::new();

		builder.push_bold_line_safe(format!(
			"Hello! Here's how I work in {}",
			self.guild
				.to_guild_cached(ctx)
				.await
				.map(|g| g.name)
				.unwrap_or_else(|| "<No name!>".to_string())
		));

		builder.push_line_safe(format!(
			"You can speak to me using `{}` as a prefix, and {} can tell me where to hunt!",
			self.custom_prefix
				.clone()
				.unwrap_or_else(|| "!".to_string()),
			self.voice_control_mode.user_friendly_print(ctx).await
		));

		builder.push_italic_line_safe(format!(
			"I'm {}!",
			self.voicehunt_mode.user_friendly_print(ctx).await
		));

		if !self.server_opt.opted_out() {
			builder.push_italic_line("This server is opted in to help measure VoIP traffic!");
			builder.push("I'm listening in");
			builder.push_safe(self.server_opt.user_friendly_print(ctx).await); // opt-in method
			builder.push_safe(self.gather.user_friendly_print().await); // gather mode
			builder.push_line(".");

			if us.is_opted_out(user.id) {
				builder.push_bold_line("You're currently opted out!");
			} else if self.server_opt.is_user_explicit_in(ctx, user, guild).await {
				// check if user has role.
				builder.push_bold_line("You're currently opted in!");
			}

			if let Ok(ack_as) = self.db.select_user_ack(user.id).await {
				builder.push_bold("You've asked to be mentioned as: ");
				builder.push_italic_line_safe(&ack_as);
			}

			builder.push_italic_line("\nSee https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md for more info!");
		}

		builder.build()
	}

	pub fn guild(&self) -> GuildId {
		self.guild
	}

	pub fn gather(&self) -> GatherMode {
		self.gather
	}

	pub async fn set_gather(&mut self, val: GatherMode) {
		self.gather = val;
		self.db.upsert_gather_cfg(self.guild, val).await;
	}

	pub fn join(&self) -> Join {
		self.voicehunt_mode
	}

	pub async fn set_join(&mut self, val: Join) {
		self.voicehunt_mode = val;
		self.db.upsert_join_cfg(self.guild, val).await;
	}

	pub fn server_opt(&self) -> OptInOut {
		self.server_opt
	}

	pub async fn set_server_opt(&mut self, val: OptInOut) {
		self.server_opt = val;
		self.db.upsert_opt_in_out(self.guild, val).await;
	}

	pub fn admin_control_mode(&self) -> Control {
		self.admin_control_mode
	}

	pub async fn set_admin_control_mode(&mut self, val: Control) {
		self.admin_control_mode = val;
		self.db.upsert_control_admin_cfg(self.guild, val).await;
	}

	pub fn voice_control_mode(&self) -> Control {
		self.voice_control_mode
	}

	pub async fn set_voice_control_mode(&mut self, val: Control) {
		self.voice_control_mode = val;
		self.db.upsert_control_cfg(self.guild, val).await;
	}

	pub fn label(&self) -> Label {
		self.label
	}

	pub async fn set_label(&mut self, val: Label) {
		self.label = val;
		self.db.upsert_server_type(self.guild, val).await;
	}

	pub async fn remove_label(&mut self) {
		self.label = Default::default();
		self.db.delete_server_type(self.guild).await;
	}

	pub async fn set_custom_ack(&mut self, val: String) {
		self.custom_ack = Some(val.clone());
		self.db.upsert_guild_ack(self.guild, &val).await;
	}

	pub async fn remove_custom_ack(&mut self) {
		self.custom_ack = None;
		self.db.delete_guild_ack(self.guild).await;
	}

	pub fn custom_prefix(&self) -> &Option<String> {
		&self.custom_prefix
	}

	pub async fn set_custom_prefix(&mut self, val: String) {
		self.custom_prefix = Some(val.clone());
		self.db.upsert_prefix(self.guild, &val).await;
	}
}
