mod query;

use crate::{config::*, server::*, voicehunt::mode::Join};

use enum_primitive::FromPrimitive;
use query::Query;
use serenity::{model::prelude::*, prelude::TypeMapKey};
use std::{collections::HashMap, sync::Arc};
use tokio::fs;
use tokio_postgres::{Client, Error as SqlError, NoTls, Statement};
use tracing::error;

pub struct Db;

impl TypeMapKey for Db {
	type Value = Arc<FelyneDb>;
}

pub struct FelyneDb {
	db: Client,
	statements: HashMap<Query, Statement>,
}

pub async fn init_db_tables(db: &Client) -> Result<HashMap<Query, Statement>, SqlError> {
	db.batch_execute(
		&fs::read_to_string(format!("{}/{}", query::QUERY_DIR, "init.sql"))
			.await
			.expect("Failed to find database creation query."),
	)
	.await?;

	let mut out = HashMap::new();

	for i in 0..=(Query::DeleteOptOut as u32) {
		let query_type = Query::from_u32(i).unwrap();
		let query_dir = query_type.query_dir();
		let data = fs::read_to_string(&query_dir).await.expect(&format!(
			"Failed to find database creation query for {:?} -- {}.",
			query_type, query_dir
		));

		let query = db.prepare(&data).await;

		if query.is_err() {
			error!("Failed to prepare statement for {:?}", query_type);
		}

		out.insert(query_type, query?);
	}

	Ok(out)
}

#[inline]
pub async fn db_conn(config: &DatabaseConfig) -> Result<FelyneDb, SqlError> {
	// let connector = TlsConnector::builder()
	// 	.build()
	// 	.expect("Invalid TLS?");
	// let connector = MakeTlsConnector::new(connector);

	let conn_str = format!(
		"user={} host={} password={} port='{}'",
		config.user,
		config.host,
		config.password,
		config.port.unwrap_or(5432)
	);

	let (client, connection) = tokio_postgres::connect(
		&conn_str, // connector,
		NoTls,
	)
	.await?;

	tokio::spawn(async move {
		if let Err(e) = connection.await {
			eprintln!("Database connection error {:?}", e);
		}
	});

	// Try and build tables, if we don't have them.
	init_db_tables(&client)
		.await
		.map(|statements| FelyneDb {
			db: client,
			statements,
		})
		.map_err(|e| {
			error!("Nya nya nya?!?! (Couldn't setup db tables: {:?})", e);
			e
		})
}

impl FelyneDb {
	#[inline]
	fn get_statement(&self, query: Query) -> Statement {
		self.statements
			.get(&query)
			.expect(&format!("Query {:?} not inserted at runtime!", query))
			.clone()
	}

	#[inline]
	pub async fn select_watchcat(&self, guild_id: GuildId) -> Result<u64, SqlError> {
		let GuildId(t_id) = guild_id;
		let t_id = t_id as i64;

		let query = self.get_statement(Query::SelectUndelete);

		self.db.query_one(&query, &[&t_id]).await.map(move |row| {
			let a: i64 = row.get(0);
			a as u64
		})
	}

	#[inline]
	pub async fn upsert_watchcat(&self, guild_id: GuildId, channel_id: ChannelId) {
		let GuildId(t_id) = guild_id;
		let ChannelId(t_c_id) = channel_id;
		let t_id = t_id as i64;
		let t_c_id = t_c_id as i64;

		let query = self.get_statement(Query::UpsertUndelete);

		let val = self.db.execute(&query, &[&t_id, &t_c_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write message_undelete db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_prefix(&self, guild_id: GuildId) -> Result<String, SqlError> {
		let GuildId(t_id) = guild_id;
		let t_id = t_id as i64;

		let query = self.get_statement(Query::SelectPrefix);

		self.db.query_one(&query, &[&t_id]).await.map(move |row| {
			let a: String = row.get(0);
			a
		})
	}

	#[inline]
	pub async fn upsert_prefix(&self, guild_id: GuildId, prefix: &str) {
		let GuildId(t_id) = guild_id;
		let t_id = t_id as i64;

		let query = self.get_statement(Query::UpsertPrefix);

		let val = self.db.execute(&query, &[&t_id, &prefix]).await;

		if let Err(e) = val {
			error!(
				"Nya?! (Couldn't write guild_prefix_override db updates.){:?}",
				e
			);
		}
	}

	#[inline]
	pub async fn select_optout_users(&self) -> Result<Vec<UserId>, SqlError> {
		let query = self.get_statement(Query::SelectOptOuts);

		self.db.query(&query, &[]).await.map(move |rows| {
			rows.into_iter()
				.map(|row| {
					let a: i64 = row.get(0);
					UserId(a as u64)
				})
				.collect()
		})
	}

	#[inline]
	pub async fn upsert_optout(&self, user_id: UserId) {
		let u_id = user_id.0 as i64;

		let query = self.get_statement(Query::UpsertOptOut);

		let val = self.db.execute(&query, &[&u_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write user_optout db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn delete_optout(&self, user_id: UserId) {
		let u_id = user_id.0 as i64;

		let query = self.get_statement(Query::DeleteOptOut);

		let val = self.db.execute(&query, &[&u_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write user_optout db removal.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_gather_cfg(&self, guild_id: GuildId) -> Result<GatherMode, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectGather);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| GatherMode::from(&row))
	}

	#[inline]
	pub async fn upsert_gather_cfg(&self, guild_id: GuildId, mode: GatherMode) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertGather);

		let val = self.db.execute(&query, &[&g_id, &(mode as i32)]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write gather_config db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_join_cfg(&self, guild_id: GuildId) -> Result<Join, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectJoin);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| Join::from(&row))
	}

	#[inline]
	pub async fn upsert_join_cfg(&self, guild_id: GuildId, mode: Join) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertJoin);

		let val = self
			.db
			.execute(
				&query,
				&[
					&g_id,
					&(mode.to_val()),
					&(mode.to_channel().unwrap_or(0i64)),
				],
			)
			.await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write join_config db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_control_cfg(&self, guild_id: GuildId) -> Result<Control, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectVoiceCtl);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| Control::from(&row))
	}

	#[inline]
	pub async fn upsert_control_cfg(&self, guild_id: GuildId, mode: Control) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertVoiceCtl);

		let val = self
			.db
			.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0i64))],
			)
			.await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write control_config db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_control_admin_cfg(&self, guild_id: GuildId) -> Result<Control, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectAdminCtl);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| Control::from(&row))
	}

	#[inline]
	pub async fn upsert_control_admin_cfg(&self, guild_id: GuildId, mode: Control) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertAdminCtl);

		let val = self
			.db
			.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0))],
			)
			.await;

		if let Err(e) = val {
			error!(
				"Nya?! (Couldn't write control_admin_config db updates.){:?}",
				e
			);
		}
	}

	#[inline]
	pub async fn select_opt_in_out(&self, guild_id: GuildId) -> Result<OptInOut, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectServerOptIn);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| OptInOut::from(&row))
	}

	#[inline]
	pub async fn upsert_opt_in_out(&self, guild_id: GuildId, mode: OptInOut) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertServerOptIn);

		let val = self
			.db
			.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0i64))],
			)
			.await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write opt_in_out db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_server_type(&self, guild_id: GuildId) -> Result<Label, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectLabel);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| Label::from(&row))
	}

	#[inline]
	pub async fn upsert_server_type(&self, guild_id: GuildId, label: Label) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertLabel);

		let val = self.db.execute(&query, &[&g_id, &(label as i32)]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write server_type db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn delete_server_type(&self, guild_id: GuildId) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::DeleteLabel);

		let val = self.db.execute(&query, &[&g_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write server_type db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_guild_ack(&self, guild_id: GuildId) -> Result<String, SqlError> {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::SelectGuildAck);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| row.get(1))
	}

	#[inline]
	pub async fn upsert_guild_ack(&self, guild_id: GuildId, ack: &str) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::UpsertGuildAck);

		let val = self.db.execute(&query, &[&g_id, &ack]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write guild_ack db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn delete_guild_ack(&self, guild_id: GuildId) {
		let g_id = guild_id.0 as i64;

		let query = self.get_statement(Query::DeleteGuildAck);

		let val = self.db.execute(&query, &[&g_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write guild_ack db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn select_user_ack(&self, user_id: UserId) -> Result<String, SqlError> {
		let g_id = user_id.0 as i64;

		let query = self.get_statement(Query::SelectAck);

		self.db
			.query_one(&query, &[&g_id])
			.await
			.map(move |row| row.get(1))
	}

	#[inline]
	pub async fn upsert_user_ack(&self, user_id: UserId, ack: &str) {
		let g_id = user_id.0 as i64;

		let query = self.get_statement(Query::UpsertAck);

		let val = self.db.execute(&query, &[&g_id, &ack]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write user_ack db updates.){:?}", e);
		}
	}

	#[inline]
	pub async fn delete_user_ack(&self, user_id: UserId) {
		let g_id = user_id.0 as i64;

		let query = self.get_statement(Query::DeleteAck);

		let val = self.db.execute(&query, &[&g_id]).await;

		if let Err(e) = val {
			error!("Nya?! (Couldn't write user_ack db updates.){:?}", e);
		}
	}
}
