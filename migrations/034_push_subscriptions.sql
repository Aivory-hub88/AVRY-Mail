-- 034: Web Push subscriptions (new-mail notifications with no tab open).
-- Postgres version (sqlx migrate only succeeds on Postgres here — SQLite's
-- schema comes from ensure_schema() in main.rs, like 031/033).
--
-- One row per browser push endpoint. mailbox_id is TEXT with app-level
-- scoping, same convention as calendar_accounts (see 031). The endpoint is
-- the natural key: re-subscribing the same browser updates its keys instead
-- of adding a duplicate, and a 404/410 from the push service deletes it.
CREATE TABLE IF NOT EXISTS push_subscriptions (
    id UUID PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    user_agent TEXT,
    created_at TEXT NOT NULL,
    last_success_at TEXT,
    failure_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS push_subscriptions_mailbox_idx ON push_subscriptions (mailbox_id);
