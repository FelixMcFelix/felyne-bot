use crate::{config::*, server::*};

use serenity::{model::prelude::*, prelude::TypeMapKey};
use std::sync::Arc;
use tokio_postgres::{Client, Error as SqlError, NoTls};
use tracing::error;

pub struct Db;

impl TypeMapKey for Db {
	type Value = Arc<Client>;
}

pub async fn init_db_tables(db: &Client) -> Result<(), SqlError> {
	db.batch_execute(
		"
BEGIN;

CREATE TABLE IF NOT EXISTS message_undelete(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	channel_id BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS guild_prefix_override(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	prefix TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_optout(
	user_id BIGINT PRIMARY KEY NOT NULL
);

/* Allow users/guilds to appear in public acknowledgement if they have contributed */
CREATE TABLE IF NOT EXISTS user_ack(
	user_id BIGINT PRIMARY KEY NOT NULL,
	ack_as TEXT,
	used BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS guild_ack(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	ack_as TEXT,
	used BOOLEAN NOT NULL
);

/* map with Enum: should be config::GatherMode */
CREATE TABLE IF NOT EXISTS gather_config(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	mode INTEGER NOT NULL
);

/* map with Enum: should be config::Control */
CREATE TABLE IF NOT EXISTS control_config(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	mode INTEGER NOT NULL,
	role BIGINT
);

/* map with Enum: should be config::Control */
CREATE TABLE IF NOT EXISTS control_admin_config(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	mode INTEGER NOT NULL,
	role BIGINT
);

/* map with Enum: should be server::Label */
CREATE TABLE IF NOT EXISTS server_type(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	label INTEGER NOT NULL
);

/* map with Enum: should be config::OptInOut */
CREATE TABLE IF NOT EXISTS opt_in_out(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	mode INTEGER NOT NULL,
	role_id BIGINT
);

COMMIT;
	",
	)
	.await
	.map(|_| ())
}

#[inline]
pub async fn db_conn(config: &DatabaseConfig) -> Result<Client, SqlError> {
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

	Ok(client)
}

#[inline]
pub async fn select_watchcat(db: &Client, guild_id: GuildId) -> Result<u64, SqlError> {
	let GuildId(t_id) = guild_id;
	let t_id = t_id as i64;

	let query = db
		.prepare("SELECT channel_id FROM message_undelete WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&t_id]).await.map(move |row| {
		let a: i64 = row.get(0);
		a as u64
	})
}

#[inline]
pub async fn upsert_watchcat(db: &Client, guild_id: GuildId, channel_id: ChannelId) {
	let GuildId(t_id) = guild_id;
	let ChannelId(t_c_id) = channel_id;
	let t_id = t_id as i64;
	let t_c_id = t_c_id as i64;

	let query = db
		.prepare(
			"INSERT INTO message_undelete (guild_id, channel_id) VALUES ($1,$2)
		ON CONFLICT (guild_id) DO UPDATE SET channel_id=EXCLUDED.channel_id;",
		)
		.await;

	let val = match query {
		Ok(query) => {
			println!("Queried! {} -> {}", t_id, t_c_id);
			db.execute(&query, &[&t_id, &t_c_id]).await
		},
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write message_undelete db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_prefix(db: &Client, guild_id: GuildId) -> Result<String, SqlError> {
	let GuildId(t_id) = guild_id;
	let t_id = t_id as i64;

	let query = db
		.prepare("SELECT prefix FROM guild_prefix_override WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&t_id]).await.map(move |row| {
		let a: String = row.get(0);
		a
	})
}

#[inline]
pub async fn upsert_prefix(db: &Client, guild_id: GuildId, prefix: &str) {
	let GuildId(t_id) = guild_id;
	let t_id = t_id as i64;

	let query = db
		.prepare(
			"INSERT INTO guild_prefix_override (guild_id, prefix) VALUES ($1,$2)
		ON CONFLICT (guild_id) DO UPDATE SET prefix=EXCLUDED.prefix;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&t_id, &prefix]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!(
			"Nya?! (Couldn't write guild_prefix_override db updates.){:?}",
			e
		);
	}
}

#[inline]
pub async fn select_optout_users(db: &Client) -> Result<Vec<UserId>, SqlError> {
	let query = db.prepare("SELECT user_id FROM user_optout").await?;

	db.query(&query, &[]).await.map(move |rows| {
		rows.into_iter()
			.map(|row| {
				let a: i64 = row.get(0);
				UserId(a as u64)
			})
			.collect()
	})
}

#[inline]
pub async fn upsert_optout(db: &Client, user_id: UserId) {
	let u_id = user_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO user_optout (user_id) VALUES ($1)
		ON CONFLICT (user_id) DO NOTHING;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&u_id]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write user_optout db updates.){:?}", e);
	}
}

#[inline]
pub async fn delete_optout(db: &Client, user_id: UserId) {
	let u_id = user_id.0 as i64;

	let query = db
		.prepare("DELETE FROM user_optout WHERE user_id=$1;")
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&u_id]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write user_optout db removal.){:?}", e);
	}
}

#[inline]
pub async fn select_gather_cfg(db: &Client, guild_id: GuildId) -> Result<GatherMode, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT mode FROM gather_config WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| GatherMode::from(&row))
}

#[inline]
pub async fn upsert_gather_cfg(db: &Client, guild_id: GuildId, mode: GatherMode) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO gather_config (guild_id, mode) VALUES ($1,$2)
		ON CONFLICT (guild_id) DO UPDATE SET mode=EXCLUDED.mode;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id, &(mode as i32)]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write gather_config db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_control_cfg(db: &Client, guild_id: GuildId) -> Result<Control, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT mode, role FROM control_config WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| Control::from(&row))
}

#[inline]
pub async fn upsert_control_cfg(db: &Client, guild_id: GuildId, mode: Control) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO control_config (guild_id, mode, role) VALUES ($1,$2,$3)
		ON CONFLICT (guild_id) DO UPDATE SET mode=EXCLUDED.mode, role=EXCLUDED.role;",
		)
		.await;

	let val = match query {
		Ok(query) =>
			db.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0i64))],
			)
			.await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write control_config db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_control_admin_cfg(db: &Client, guild_id: GuildId) -> Result<Control, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT mode, role FROM control_admin_config WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| Control::from(&row))
}

#[inline]
pub async fn upsert_control_admin_cfg(db: &Client, guild_id: GuildId, mode: Control) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO control_admin_config (guild_id, mode, role) VALUES ($1,$2,$3)
		ON CONFLICT (guild_id) DO UPDATE SET mode=EXCLUDED.mode, role=EXCLUDED.role;",
		)
		.await;

	let val = match query {
		Ok(query) =>
			db.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0))],
			)
			.await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!(
			"Nya?! (Couldn't write control_admin_config db updates.){:?}",
			e
		);
	}
}

#[inline]
pub async fn select_opt_in_out(db: &Client, guild_id: GuildId) -> Result<OptInOut, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT mode, role_id FROM opt_in_out WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| OptInOut::from(&row))
}

#[inline]
pub async fn upsert_opt_in_out(db: &Client, guild_id: GuildId, mode: OptInOut) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO opt_in_out (guild_id, mode, role_id) VALUES ($1,$2,$3)
		ON CONFLICT (guild_id) DO UPDATE SET mode=EXCLUDED.mode, role_id=EXCLUDED.role_id;",
		)
		.await;

	let val = match query {
		Ok(query) =>
			db.execute(
				&query,
				&[&g_id, &(mode.to_val()), &(mode.to_role().unwrap_or(0i64))],
			)
			.await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write opt_in_out db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_server_type(db: &Client, guild_id: GuildId) -> Result<Label, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT mode FROM server_type WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| Label::from(&row))
}

#[inline]
pub async fn upsert_server_type(db: &Client, guild_id: GuildId, label: Label) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO server_type (guild_id, label) VALUES ($1,$2)
		ON CONFLICT (guild_id) DO UPDATE SET label=EXCLUDED.label;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id, &(label as i32)]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write server_type db updates.){:?}", e);
	}
}

#[inline]
pub async fn delete_server_type(db: &Client, guild_id: GuildId) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("DELETE FROM server_type WHERE guild_id=$1;")
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write server_type db updates.){:?}", e);
	}
}

#[inline]
pub async fn select_guild_ack(db: &Client, guild_id: GuildId) -> Result<String, SqlError> {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare("SELECT ack_as FROM guild_ack WHERE guild_id = $1")
		.await?;

	db.query_one(&query, &[&g_id])
		.await
		.map(move |row| row.get(1))
}

#[inline]
pub async fn upsert_guild_ack(db: &Client, guild_id: GuildId, ack: &str) {
	let g_id = guild_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO guild_ack (guild_id, ack_as, used) VALUES ($1,$2,FALSE)
		ON CONFLICT (guild_id) DO UPDATE SET ack_as=EXCLUDED.ack_as, used=FALSE;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id, &ack]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write guild_ack db updates.){:?}", e);
	}
}

#[inline]
pub async fn delete_guild_ack(db: &Client, guild_id: GuildId) {
	let g_id = guild_id.0 as i64;

	let query = db.prepare("DELETE FROM guild_ack WHERE guild_id=$1;").await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write guild_ack db updates.){:?}", e);
	}
}

#[inline]
pub async fn upsert_user_ack(db: &Client, user_id: UserId, ack: &str) {
	let g_id = user_id.0 as i64;

	let query = db
		.prepare(
			"INSERT INTO user_ack (user_id, ack_as, used) VALUES ($1,$2,FALSE)
		ON CONFLICT (user_id) DO UPDATE SET ack_as=EXCLUDED.ack_as, used=FALSE;",
		)
		.await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id, &ack]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write user_ack db updates.){:?}", e);
	}
}

#[inline]
pub async fn delete_user_ack(db: &Client, user_id: UserId) {
	let g_id = user_id.0 as i64;

	let query = db.prepare("DELETE FROM user_ack WHERE user_id=$1;").await;

	let val = match query {
		Ok(query) => db.execute(&query, &[&g_id]).await,
		Err(e) => Err(e),
	};

	if let Err(e) = val {
		error!("Nya?! (Couldn't write user_ack db updates.){:?}", e);
	}
}
