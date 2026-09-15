-- 031: Google Calendar sync (per-mailbox OAuth connect + two-way sync).
-- calendar_accounts: one row per mailbox that connected a Google Calendar.
-- Tokens are AES-256-GCM encrypted the same way as
-- mailboxes.imap_password_encrypted (see imap_password_vault.rs), scoped by
-- calendar_accounts.id as additional authenticated data.
CREATE TABLE IF NOT EXISTS calendar_accounts (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL UNIQUE REFERENCES mailboxes(id) ON DELETE CASCADE,
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
CREATE TABLE IF NOT EXISTS calendar_event_links (
    id TEXT PRIMARY KEY,
    calendar_account_id TEXT NOT NULL REFERENCES calendar_accounts(id) ON DELETE CASCADE,
    local_event_id TEXT NOT NULL REFERENCES calendar_events(id) ON DELETE CASCADE,
    google_event_id TEXT NOT NULL,
    google_etag TEXT,
    origin TEXT NOT NULL DEFAULT 'google',
    created_at TEXT NOT NULL,
    UNIQUE(calendar_account_id, google_event_id),
    UNIQUE(local_event_id)
);
CREATE INDEX IF NOT EXISTS idx_calendar_event_links_account ON calendar_event_links(calendar_account_id);

-- 'local' = created in Aivory Mail (push to Google on sync);
-- 'google' = imported from Google (mirrored, edits push back to Google).
ALTER TABLE calendar_events ADD COLUMN source TEXT NOT NULL DEFAULT 'local';
