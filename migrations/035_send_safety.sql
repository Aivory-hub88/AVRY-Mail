-- 035: send safety (Postgres; SQLite gets the same tables from ensure_schema()).
--
-- send_attempts: at-most-once sending for requests carrying an
-- Idempotency-Key. id = sha256(caller \0 key); payload_hash binds the key to
-- one message so reusing it for different content is refused.
CREATE TABLE IF NOT EXISTS send_attempts (
    id TEXT PRIMARY KEY,
    caller TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    message_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- agent_send_counts: per-mailbox, per-UTC-day counter behind
-- AGENT_DAILY_SEND_LIMIT for sends made by AI agents over MCP.
CREATE TABLE IF NOT EXISTS agent_send_counts (
    mailbox_id TEXT NOT NULL,
    day TEXT NOT NULL,
    count BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (mailbox_id, day)
);
