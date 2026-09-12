-- Scope calendar_events per mailbox/tenant so calendars no longer share one global table.
-- IF NOT EXISTS preserves installations where the bootstrap schema already
-- supplied these columns before SQLx began tracking this migration.
ALTER TABLE calendar_events ADD COLUMN IF NOT EXISTS tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE calendar_events ADD COLUMN IF NOT EXISTS mailbox_id TEXT NOT NULL DEFAULT '';
