use rusqlite::{Connection, Result as SQLResult};

pub fn init_db_tables(db: &Connection) -> SQLResult<()> {
	db.execute_batch("BEGIN;

					CREATE TABLE IF NOT EXISTS del_watchcat(
					guild_id TEXT PRIMARY KEY,
					channel_id TEXT
					);

					COMMIT;")
}