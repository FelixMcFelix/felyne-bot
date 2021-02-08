INSERT INTO user_ack (user_id, ack_as, used)
VALUES ($1,$2,FALSE)
ON CONFLICT (user_id)
DO UPDATE SET ack_as=EXCLUDED.ack_as, used=FALSE;