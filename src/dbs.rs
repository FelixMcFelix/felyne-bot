use crate::{
	config::*,
	server::*,
};
use log::error;
use serenity::model::prelude::*;
use sqlx::{
	pool::PoolConnection,
	prelude::*,
	sqlite::{SqlitePool, SqliteRow},
	Error as SqlError,
	Sqlite,
};
use super::DbPools;

pub type OurPool = SqlitePool;

pub async fn init_db_tables(db: &OurPool) -> Result<(), SqlError> {
	db.execute("
PRAGMA synchronous = NORMAL;
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 10000;

BEGIN;

CREATE TABLE IF NOT EXISTS del_watchcat(
guild_id TEXT PRIMARY KEY NOT NULL,
channel_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS guild_prefix_override(
guild_id TEXT PRIMARY KEY NOT NULL,
prefix TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_optout(
user_id TEXT PRIMARY KEY NOT NULL
);

/* Allow users/guilds to appear in public acknowledgement if they have contributed */
CREATE TABLE IF NOT EXISTS user_ack(
user_id TEXT PRIMARY KEY NOT NULL,
ack_as TEXT,
used INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS guild_ack(
guild_id TEXT PRIMARY KEY NOT NULL,
ack_as TEXT,
used INTEGER NOT NULL
);

/* map with Enum: should be config::GatherMode */
CREATE TABLE IF NOT EXISTS gather_config(
guild_id TEXT PRIMARY KEY NOT NULL,
mode INTEGER NOT NULL
);

/* map with Enum: should be config::Control */
CREATE TABLE IF NOT EXISTS control_config(
guild_id TEXT PRIMARY KEY NOT NULL,
mode INTEGER NOT NULL,
role TEXT
);

/* map with Enum: should be config::Control */
CREATE TABLE IF NOT EXISTS control_admin_config(
guild_id TEXT PRIMARY KEY NOT NULL,
mode INTEGER NOT NULL,
role TEXT
);

/* map with Enum: should be server::Label */
CREATE TABLE IF NOT EXISTS server_type(
guild_id TEXT PRIMARY KEY NOT NULL,
label INTEGER NOT NULL
);

/* map with Enum: should be config::OptInOut */
CREATE TABLE IF NOT EXISTS opt_in_out(
guild_id TEXT PRIMARY KEY NOT NULL,
mode INTEGER NOT NULL,
role_id TEXT
);

COMMIT;
	").await
	.map(|_| ())
}

#[inline]
pub async fn db_conn() -> Result<DbPools, SqlError> {
	let read = SqlitePool::connect("sqlite:felyne.db").await?;
	let write = SqlitePool::connect("sqlite:felyne.db").await?;

	Ok(DbPools {
		read,
		write,
	})
}

#[inline]
pub async fn select_watchcat(db: &OurPool, guild_id: GuildId) -> Result<u64, SqlError> {
	let GuildId(t_id) = guild_id;

	sqlx::query("SELECT channel_id FROM del_watchcat WHERE guild_id = ?")
		.bind(&t_id.to_string())
		.fetch_one(db)
		.await
		.map(move |row: SqliteRow| {
			let a: &str = row.get(0);
			a.parse::<u64>().unwrap()
			}
		)
}

#[inline]
pub async fn upsert_watchcat(db: &OurPool, guild_id: GuildId, channel_id: ChannelId) {
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

#[inline]
pub async fn select_prefix(db: &OurPool, guild_id: GuildId) -> Result<String, SqlError> {
	let GuildId(t_id) = guild_id;

	sqlx::query("SELECT prefix FROM guild_prefix_override WHERE guild_id = ?")
		.bind(&t_id.to_string())
		.fetch_one(db)
		.await
		.map(|row: SqliteRow| {
			let a: String = row.get(0);
			a
		})
}

#[inline]
pub async fn upsert_prefix(db: &OurPool, guild_id: GuildId, prefix: &str) {
	let GuildId(t_g_id) = guild_id;

	let query = sqlx::query("INSERT OR REPLACE INTO guild_prefix_override (guild_id, prefix) VALUES (?,?);")
		.bind(&t_g_id.to_string())
		.bind(&prefix)
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write guild_prefix_override db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_optout(db: &OurPool, user_id: UserId) -> Result<UserId, SqlError> {
	sqlx::query("SELECT user_id FROM user_optout WHERE user_id = ?")
		.bind(user_id.0.to_string())
		.fetch_one(db)
		.await
		.map(move |row: SqliteRow| {
			let a: &str = row.get(0);
			UserId(a.parse::<u64>().unwrap())
			}
		)
}

#[inline]
pub async fn upsert_optout(db: &OurPool, user_id: UserId) {
	let query = sqlx::query("INSERT OR REPLACE INTO user_optout (user_id) VALUES (?);")
		.bind(user_id.0.to_string())
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write user_optout db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_gather_cfg(db: &OurPool, guild_id: GuildId) -> Result<GatherMode, SqlError> {
	sqlx::query_as("SELECT mode FROM gather_config WHERE guild_id = ?")
		.bind(guild_id.0.to_string())
		.fetch_one(db)
		.await
}

#[inline]
pub async fn upsert_gather_cfg(db: &OurPool, guild_id: GuildId, mode: GatherMode) {
	let query = sqlx::query("INSERT OR REPLACE INTO gather_config (guild_id, mode) VALUES (?,?);")
		.bind(guild_id.0.to_string())
		.bind(mode as i16)
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write gather_config db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_control_cfg(db: &OurPool, guild_id: GuildId) -> Result<Control, SqlError> {
	sqlx::query_as("SELECT mode, role FROM control_config WHERE guild_id = ?")
		.bind(guild_id.0.to_string())
		.fetch_one(db)
		.await
}

#[inline]
pub async fn upsert_control_cfg(db: &OurPool, guild_id: GuildId, mode: Control) {
	let query = sqlx::query("INSERT OR REPLACE INTO control_config (guild_id, mode, role) VALUES (?,?,?);")
		.bind(guild_id.0.to_string())
		.bind(mode.to_val())
		.bind(mode.to_role())
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write control_config db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_control_admin_cfg(db: &OurPool, guild_id: GuildId) -> Result<Control, SqlError> {
	sqlx::query_as("SELECT mode, role FROM control_admin_config WHERE guild_id = ?")
		.bind(guild_id.0.to_string())
		.fetch_one(db)
		.await
}

#[inline]
pub async fn upsert_control_admin_cfg(db: &OurPool, guild_id: GuildId, mode: Control) {
	let query = sqlx::query("INSERT OR REPLACE INTO control_admin_config (guild_id, mode, role) VALUES (?,?,?);")
		.bind(guild_id.0.to_string())
		.bind(mode.to_val())
		.bind(mode.to_role())
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write control_admin_config db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_opt_in_out(db: &OurPool, guild_id: GuildId) -> Result<OptInOut, SqlError> {
	sqlx::query_as("SELECT mode, role FROM opt_in_out WHERE guild_id = ?")
		.bind(guild_id.0.to_string())
		.fetch_one(db)
		.await
}

#[inline]
pub async fn upsert_opt_in_out(db: &OurPool, guild_id: GuildId, mode: OptInOut) {
	let query = sqlx::query("INSERT OR REPLACE INTO opt_in_out (guild_id, mode, role) VALUES (?,?,?);")
		.bind(guild_id.0.to_string())
		.bind(mode.to_val())
		.bind(mode.to_role())
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write opt_in_out db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_server_type(db: &OurPool, guild_id: GuildId) -> Result<Label, SqlError> {
	sqlx::query_as("SELECT mode FROM server_type WHERE guild_id = ?")
		.bind(guild_id.0.to_string())
		.fetch_one(db)
		.await
}

#[inline]
pub async fn upsert_server_type(db: &OurPool, guild_id: GuildId, label: Label) {
	let query = sqlx::query("INSERT OR REPLACE INTO server_type (guild_id, label) VALUES (?,?);")
		.bind(guild_id.0.to_string())
		.bind(label as i16)
		.execute(db)
		.await;

	if let Err(e) = query {
		error!("Nya?! (Couldn't write server_type db updates.){:?}", e);
	}
}
