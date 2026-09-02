-- Snoozing support (Mailflare parity)
-- Postgres: use TIMESTAMPTZ, SQLite: TEXT (compatible via IF NOT EXISTS guard in ensure_schema)
ALTER TABLE messages ADD COLUMN IF NOT EXISTS snoozed_until TIMESTAMPTZ;
-- SQLite fallback (ignored if column exists, handled via ensure_schema alters)
