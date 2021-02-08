INSERT INTO join_config (guild_id, mode, channel)
VALUES ($1,$2,$3)
ON CONFLICT (guild_id)
DO UPDATE SET mode=EXCLUDED.mode, channel=EXCLUDED.channel;