-- Allow the same sender to have an independent contact record in each mailbox.
-- Legacy NULL mailbox_id rows remain valid and do not conflict with scoped rows.
ALTER TABLE contacts DROP CONSTRAINT IF EXISTS contacts_tenant_id_email_key;
DROP INDEX IF EXISTS contacts_tenant_id_email_key;
CREATE UNIQUE INDEX IF NOT EXISTS idx_contacts_tenant_mailbox_email ON contacts(tenant_id, mailbox_id, email);
