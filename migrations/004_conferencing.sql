-- Conferencing preference per event (Google Meet / Teams / Zoom / LiveKit).
-- IF NOT EXISTS preserves installations where the bootstrap schema already
-- supplied these columns before SQLx began tracking this migration.
ALTER TABLE calendar_events ADD COLUMN IF NOT EXISTS conferencing TEXT NOT NULL DEFAULT 'none';
ALTER TABLE calendar_events ADD COLUMN IF NOT EXISTS conferencing_link TEXT NOT NULL DEFAULT '';
