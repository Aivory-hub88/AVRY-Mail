-- 036: inbox tabs (Postgres; SQLite gets the same via ensure_schema()).
--
-- messages.category: primary | promotions | social | updates | forums.
-- NULL = arrived before tabs existed; read as primary until the owner runs
-- the backfill (POST /v1/categories/backfill).
ALTER TABLE messages ADD COLUMN IF NOT EXISTS category TEXT;
CREATE INDEX IF NOT EXISTS idx_messages_mailbox_folder_category
    ON messages (mailbox_id, folder, category);

-- A user's "always put this sender in <tab>" choice. Wins over every rule.
CREATE TABLE IF NOT EXISTS sender_categories (
    mailbox_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    category TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (mailbox_id, sender)
);
