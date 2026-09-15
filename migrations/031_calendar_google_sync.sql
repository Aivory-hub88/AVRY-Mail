-- 031: Google Calendar sync (per-mailbox OAuth connect + two-way sync).
-- Postgres version (sqlx migrate only succeeds on Postgres here — SQLite's
-- schema for this app comes from ensure_schema() in main.rs, same pattern
-- as every other table already in this file's sibling migrations).
--
-- calendar_accounts.id/calendar_event_links.id are UUID (app-generated,
-- Uuid::new_v4() from Rust — no DEFAULT) to match how the Rust code binds
-- them, same convention as tenants/domains in 001_initial.sql. mailbox_id
-- stays TEXT with no FK to mailboxes(id): mailboxes.id is a native UUID in
-- Postgres, but every mailbox_id column added after the original schema
-- (calendar_events, mail_filters, contacts, ...) stores it as TEXT and
-- relies on app-level scoping rather than a DB FK — see CALENDAR.md's note
-- on `mailbox_id` trust. Mixing TEXT mailbox_id with a real FK to a UUID
-- column is what broke the first version of this migration
-- ("foreign key constraint ... cannot be implemented").
CREATE TABLE IF NOT EXISTS calendar_accounts (
    id UUID PRIMARY KEY,
    mailbox_id TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL DEFAULT 'google',
    google_account_email TEXT NOT NULL,
    access_token_encrypted TEXT NOT NULL,
    refresh_token_encrypted TEXT NOT NULL,
    token_expires_at TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT '',
    sync_token TEXT,
    calendar_id TEXT NOT NULL DEFAULT 'primary',
    status TEXT NOT NULL DEFAULT 'connected',
    last_synced_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_calendar_accounts_mailbox ON calendar_accounts(mailbox_id);

-- Maps a local calendar_events row to the Google event it mirrors, so the
-- sync engine can tell "already imported" from "new" in both directions.
-- local_event_id stays TEXT to match calendar_events.id, which is TEXT
-- (never normalized to UUID) — the sync engine's Postgres join already
-- casts both sides to text explicitly for this same reason, see
-- push_local_events() in calendar_google.rs.
CREATE TABLE IF NOT EXISTS calendar_event_links (
    id UUID PRIMARY KEY,
    calendar_account_id UUID NOT NULL REFERENCES calendar_accounts(id) ON DELETE CASCADE,
    local_event_id TEXT NOT NULL UNIQUE REFERENCES calendar_events(id) ON DELETE CASCADE,
    google_event_id TEXT NOT NULL,
    google_etag TEXT,
    origin TEXT NOT NULL DEFAULT 'google',
    created_at TEXT NOT NULL,
    UNIQUE(calendar_account_id, google_event_id)
);
CREATE INDEX IF NOT EXISTS idx_calendar_event_links_account ON calendar_event_links(calendar_account_id);

-- 'local' = created in Aivory Mail (push to Google on sync);
-- 'google' = imported from Google (mirrored, edits push back to Google).
ALTER TABLE calendar_events ADD COLUMN source TEXT NOT NULL DEFAULT 'local';
