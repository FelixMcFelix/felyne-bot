INSERT INTO guild_prefix_override (guild_id, prefix)
VALUES ($1,$2)
ON CONFLICT (guild_id)
DO UPDATE SET prefix=EXCLUDED.prefix;