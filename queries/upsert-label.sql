INSERT INTO server_type (guild_id, label)
VALUES ($1,$2)
ON CONFLICT (guild_id)
DO UPDATE SET label=EXCLUDED.label;