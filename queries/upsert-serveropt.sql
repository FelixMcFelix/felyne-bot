INSERT INTO opt_in_out (guild_id, mode, role_id)
VALUES ($1,$2,$3)
ON CONFLICT (guild_id)
DO UPDATE SET mode=EXCLUDED.mode, role_id=EXCLUDED.role_id;