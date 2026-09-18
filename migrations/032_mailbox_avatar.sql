-- 032: User avatar for mailboxes (profile picture, user settings parity).
-- Postgres version (sqlx migrate only succeeds on Postgres here — SQLite's
-- schema for this app comes from ensure_schema() in main.rs, same pattern
-- as every other table already in this file's sibling migrations).
--
-- Avatar bytes live in the ObjectStore under avatars/<mailbox_id>.<ext>
-- (see api/profile.rs); the DB only keeps lightweight metadata so listing
-- mailboxes stays cheap. avatar_updated_at doubles as the cache-buster the
-- web client appends as ?v=.
ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS avatar_content_type TEXT;
ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS avatar_updated_at TEXT;
