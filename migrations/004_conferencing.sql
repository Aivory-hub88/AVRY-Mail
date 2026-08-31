-- Conferencing preference per event (Google Meet / Teams / Zoom / LiveKit)
ALTER TABLE calendar_events ADD COLUMN conferencing TEXT NOT NULL DEFAULT 'none';
ALTER TABLE calendar_events ADD COLUMN conferencing_link TEXT NOT NULL DEFAULT '';
