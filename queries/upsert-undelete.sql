INSERT INTO message_undelete (guild_id, channel_id)
VALUES ($1,$2)
ON CONFLICT (guild_id) 
DO UPDATE SET channel_id=EXCLUDED.channel_id;