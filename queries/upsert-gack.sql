INSERT INTO guild_ack (guild_id, ack_as, used)
VALUES ($1,$2,FALSE)
ON CONFLICT (guild_id)
DO UPDATE SET ack_as=EXCLUDED.ack_as, used=FALSE;