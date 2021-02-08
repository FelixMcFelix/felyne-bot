INSERT INTO gather_config (guild_id, mode)
VALUES ($1,$2)
ON CONFLICT (guild_id)
DO UPDATE SET mode=EXCLUDED.mode;