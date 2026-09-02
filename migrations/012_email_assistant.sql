-- 012 email assistant: chat history + mission control notifications
CREATE TABLE IF NOT EXISTS ai_chat_history (
    id UUID PRIMARY KEY,
    mailbox_id TEXT,
    user_email TEXT NOT NULL DEFAULT '',
    question TEXT NOT NULL,
    answer TEXT NOT NULL,
    context_json JSONB,
    model TEXT NOT NULL DEFAULT 'heuristic',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ai_chat_mailbox ON ai_chat_history(mailbox_id, created_at DESC);

CREATE TABLE IF NOT EXISTS mission_control_notifications (
    id UUID PRIMARY KEY,
    type TEXT NOT NULL DEFAULT 'email_assistant',
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    action_url TEXT,
    metadata_json JSONB,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_mcn_created ON mission_control_notifications(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mcn_type ON mission_control_notifications(type);
