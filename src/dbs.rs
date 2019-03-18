use rusqlite::{Connection, Result as SqlResult};

pub fn init_db_tables(db: &Connection) -> SqlResult<()> {
	db.execute_batch("BEGIN;

					CREATE TABLE IF NOT EXISTS del_watchcat(
					guild_id TEXT PRIMARY KEY,
					channel_id TEXT
					);

					COMMIT;")
}

#[inline]
pub fn db_conn() -> SqlResult<Connection> {
	Connection::open("felyne.db")
}
