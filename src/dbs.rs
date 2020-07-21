use sqlx::{
	prelude::*,
	sqlite::SqlitePool,
	Error as SqlError,
};

pub async fn init_db_tables<C: Executor>(db: &mut C) -> Result<u64, SqlError> {
	db.execute("
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
guild_id TEXT PRIMARY KEY NOT NULL,
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

/* map with Enum: should be config::ControlMode */
CREATE TABLE IF NOT EXISTS control_config(
guild_id TEXT PRIMARY KEY NOT NULL,
mode INTEGER NOT NULL,
role TEXT
);

/* map with Enum: should be server::Label */
CREATE TABLE IF NOT EXISTS server_type(
guild_id TEXT PRIMARY KEY NOT NULL,
label INTEGER NOT NULL
);

COMMIT;
	").await
}

#[inline]
pub async fn db_conn() -> Result<SqlitePool, SqlError> {
	SqlitePool::new("sqlite:felyne.db").await
}
