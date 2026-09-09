-- Vacation auto-reply deliveries tracking (24h / interval deduplication)
CREATE TABLE IF NOT EXISTS vacation_deliveries (
    id TEXT PRIMARY KEY,
    mailbox_id TEXT NOT NULL,
    recipient TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    UNIQUE(mailbox_id, recipient)
);
CREATE INDEX IF NOT EXISTS idx_vacation_deliveries_mailbox_recipient ON vacation_deliveries(mailbox_id, recipient);
