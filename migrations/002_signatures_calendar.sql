-- Signatures per mailbox (Outlook/Gmail parity)
CREATE TABLE IF NOT EXISTS signatures (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT '',
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    name TEXT NOT NULL DEFAULT 'Default',
    html TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL DEFAULT '',
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_signatures_mailbox ON signatures(mailbox_id);

-- Calendar proposals cache (optional, for follow-up scheduling)
CREATE TABLE IF NOT EXISTS calendar_proposals (
    id TEXT PRIMARY KEY,
    thread_id TEXT,
    message_id TEXT,
    event_type_slug TEXT,
    proposed_slots_json TEXT NOT NULL DEFAULT '[]',
    booking_url TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);
