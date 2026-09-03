-- Scope calendar_events per mailbox/tenant so calendars no longer share one global table
ALTER TABLE calendar_events ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE calendar_events ADD COLUMN mailbox_id TEXT NOT NULL DEFAULT '';
