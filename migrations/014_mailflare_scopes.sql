-- Mailflare scopes + useAllDomains + routing 3-phase
ALTER TABLE mail_filters ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'mailbox';
CREATE INDEX IF NOT EXISTS idx_mail_filters_scope ON mail_filters(tenant_id, scope, priority);

ALTER TABLE mailboxes ADD COLUMN IF NOT EXISTS use_all_domains BOOLEAN NOT NULL DEFAULT FALSE;

-- For mailbox aliases (Mailflare: mailbox_aliases domainId+localPart)
CREATE TABLE IF NOT EXISTS mailbox_aliases (
    id TEXT PRIMARY KEY,
    domain_id TEXT NOT NULL REFERENCES domains(id) ON DELETE CASCADE,
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    local_part TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(domain_id, local_part)
);
