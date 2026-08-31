-- Calendar events for Google parity (week view, CRUD)
CREATE TABLE IF NOT EXISTS calendar_events (
    id TEXT PRIMARY KEY,
    calendar TEXT NOT NULL DEFAULT 'Daemon Larkin',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    guests TEXT NOT NULL DEFAULT '[]',
    color TEXT NOT NULL DEFAULT 'blue',
    recurring TEXT NOT NULL DEFAULT 'never',
    notifications TEXT NOT NULL DEFAULT '10m',
    location TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cal_events_start ON calendar_events(start_at);
