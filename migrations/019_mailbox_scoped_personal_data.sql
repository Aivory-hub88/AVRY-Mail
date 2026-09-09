-- Mailbox scoping for settings data.
-- Existing rows remain NULL and are treated as legacy/global admin data.
ALTER TABLE mail_labels ADD COLUMN IF NOT EXISTS mailbox_id TEXT;
ALTER TABLE mail_filters ADD COLUMN IF NOT EXISTS mailbox_id TEXT;
ALTER TABLE contacts ADD COLUMN IF NOT EXISTS mailbox_id TEXT;

CREATE INDEX IF NOT EXISTS idx_mail_labels_mailbox ON mail_labels(tenant_id, mailbox_id, name);
CREATE INDEX IF NOT EXISTS idx_mail_filters_mailbox ON mail_filters(tenant_id, mailbox_id, priority, created_at);
CREATE INDEX IF NOT EXISTS idx_contacts_mailbox_email ON contacts(tenant_id, mailbox_id, email);

-- This join table existed only in the SQLite bootstrap. PostgreSQL needs the
-- same table so label attachment/listing works after migrations alone.
CREATE TABLE IF NOT EXISTS message_labels (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    label_id TEXT NOT NULL REFERENCES mail_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, label_id)
);
CREATE INDEX IF NOT EXISTS idx_message_labels_label ON message_labels(label_id);
