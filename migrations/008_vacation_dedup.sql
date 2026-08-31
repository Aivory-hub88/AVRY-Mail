-- Tracks the last vacation auto-reply sent to a given sender per mailbox,
-- so a repeat sender only gets one auto-reply per interval_days, not one
-- per inbound message (loop/spam prevention).
CREATE TABLE IF NOT EXISTS vacation_replies_sent (
    mailbox_id TEXT NOT NULL,
    sender_addr TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (mailbox_id, sender_addr)
);
