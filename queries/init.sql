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

/* map with Enum: should be voicehunt::mode::Join */
CREATE TABLE IF NOT EXISTS join_config(
	guild_id BIGINT PRIMARY KEY NOT NULL,
	mode INTEGER NOT NULL,
	channel BIGINT
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