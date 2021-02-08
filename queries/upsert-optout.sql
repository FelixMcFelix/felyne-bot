INSERT INTO user_optout (user_id)
VALUES ($1)
ON CONFLICT (user_id)
DO NOTHING;