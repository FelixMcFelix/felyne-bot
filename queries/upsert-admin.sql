INSERT INTO control_admin_config (guild_id, mode, role)
VALUES ($1,$2,$3)
ON CONFLICT (guild_id)
DO UPDATE SET mode=EXCLUDED.mode, role=EXCLUDED.role;