# Aivory Mail — Calendar (a.k.a. "Aivory Cal")

Google-parity week/day/month calendar built into Aivory Mail, wired at
`web/app/calendar`. This doc covers what it is, how data is scoped per
mailbox, how it relates to **Calnode** (the separate public booking product
at `book.aivory.uk`), and what's still open.

## Two separate systems, not one

"Aivory Cal" today is actually two products that share a sidebar link, not
one synced system:

| | **Local calendar** (this doc) | **Calnode** (`book.aivory.uk`) |
|---|---|---|
| What it is | Personal week/day/month grid inside Aivory Mail | Standalone self-hosted Calendly-style booking app |
| Repo | this repo (`AVRY-Mail`) | separate — `Calnode/calnode`, Apache-2.0 |
| Data | `calendar_events` table in Aivory Mail's own DB | its own SQLite, on its own VPS container `aivory-cal` |
| Used for | Blocking time, internal meetings, quick events created from the mail UI | Public booking page other people use to book a slot with you |
| Auth model | Scoped by `mailbox_id` (see below) | Its own member/role system (owner/admin/member), one workspace per deployment |

**They do not sync.** A booking made on `book.aivory.uk/book/aivory-call`
does **not** appear in the Aivory Mail grid, and an event created in the grid
does not appear on Calnode. The only connection today is one-directional
linking: the calendar page's sidebar lists Calnode event types
(`GET /v1/calendar/event-types` → proxies to Calnode) as "Booking pages" you
can click out to, and there's a dormant bridge (`calendar.rs`:
`get_slots` / `create_booking`) that could push a booking into Calnode from
inside Aivory Mail, but the grid's own "Create event" flow does not call it —
it POSTs straight to the local `/v1/calendar/events` table.

Syncing Calnode bookings into the local grid (e.g. via Calnode's
`booking.created` webhook, same pattern as
[`calnode-booking-intake-notify`](../../../.claude/projects/-Users-ireichmann-Documents-Aivory-V2/memory/calnode-booking-intake-notify.md))
is on the roadmap, not done.

## Storage model

**`calendar_events`** (migration `003_calendar_events.sql`, scoping added in
`009_calendar_mailbox_scope.sql`):

```
id                 TEXT PRIMARY KEY
tenant_id          TEXT NOT NULL DEFAULT 'default'
mailbox_id         TEXT NOT NULL DEFAULT ''   -- owning mailbox; '' = pre-isolation legacy rows
calendar           TEXT NOT NULL DEFAULT 'Daemon Larkin'  -- sub-calendar / category label, e.g. "My calendar", "Tasks"
title              TEXT NOT NULL
description        TEXT NOT NULL DEFAULT ''
start_at           TEXT NOT NULL
end_at             TEXT NOT NULL
guests             TEXT NOT NULL DEFAULT '[]'   -- JSON array
color              TEXT NOT NULL DEFAULT 'blue'
recurring          TEXT NOT NULL DEFAULT 'never'
notifications      TEXT NOT NULL DEFAULT '10m'
location           TEXT NOT NULL DEFAULT ''
conferencing       TEXT NOT NULL DEFAULT 'none'   -- none | google-meet | teams | zoom | custom
conferencing_link  TEXT NOT NULL DEFAULT ''
created_at         TEXT NOT NULL
```

Note `calendar` (category label like "Tasks"/"Birthdays") and `mailbox_id`
(owning user) are different axes — one mailbox can have several category
calendars shown/hidden via the sidebar checkboxes; `mailbox_id` is what keeps
one mailbox's events from ever being another mailbox's problem.

## Per-mailbox isolation (added 2026-09-03)

Before this pass, `calendar_events` had no owner column at all — every
mailbox on an Aivory Mail instance read and wrote the same global table.
Fixed to match the scoping pattern already used by `signatures`,
`vacation_responders`, and `send_as_aliases` elsewhere in this app:

- `GET /v1/calendar/events` **requires** `?mailbox_id=` — `400` without it.
- `POST /v1/calendar/events` **requires** `mailbox_id` in the body.
- `PUT /v1/calendar/events/:id` and `DELETE /v1/calendar/events/:id` filter
  `WHERE id=? AND mailbox_id=?` — passing someone else's event id with your
  own `mailbox_id` silently matches zero rows instead of touching their data.
- Web UI (`calendar/page.tsx`) fetches the real mailbox list from
  `GET /v1/mailboxes`, remembers the chosen one in
  `localStorage["aivory_calendar_mailbox_id"]`, and sends it on every call.
  A mailbox switcher appears in the header whenever there's more than one
  mailbox.

**What this is not**: session-enforced security. `mailbox_id` is supplied by
the client and trusted, the same way every other scoped table in this app
works today — `auth_middleware` (`crates/aivory-mail-api/src/auth.rs`)
verifies a JWT is *valid* but never decodes `Claims` into request
extensions, so no handler in the codebase can currently answer "who is
actually logged in" from the token itself. Real enforcement (deriving
`mailbox_id`/`tenant_id` from the verified JWT instead of trusting the
request) is a cross-cutting fix, not specific to calendar — tracked as a
follow-up, not done here.

## API

| Method | Path | Notes |
|---|---|---|
| `GET` | `/v1/calendar/events?mailbox_id=&from=&to=&calendar=` | `mailbox_id` required; `from`/`to`/`calendar` optional filters |
| `POST` | `/v1/calendar/events` | body requires `mailbox_id`, `title`, `start_at` |
| `PUT` | `/v1/calendar/events/:id` | body requires `mailbox_id`; only fields present are updated |
| `DELETE` | `/v1/calendar/events/:id?mailbox_id=` | `mailbox_id` required as query param |
| `GET` | `/v1/calendar/status` | proxies Calnode `/v1/calendar/status` |
| `GET` | `/v1/calendar/event-types` | proxies Calnode `/v1/event-types` — powers the "Booking pages" sidebar links |
| `GET` | `/v1/calendar/slots` | proxies Calnode `/v1/event-types/:slug/slots` — not called from the grid UI today |
| `POST` | `/v1/calendar/bookings` | proxies Calnode `POST /v1/bookings` — not called from the grid UI today |
| `POST` | `/v1/calendar/propose` | writes to `calendar_proposals` (thread → suggested slots), used by the AI/compose flow |

Calnode base URL/key: `CALNODE_URL` / `CALNODE_API_KEY` env vars (fallback
`CALENDAR_URL` / `CAL_API_KEY`), default `https://book.aivory.uk` — see
`crates/aivory-mail-api/src/calendar.rs`.

## UI — `/calendar`

Week / Day / Month toggle, mini-month picker, "My calendars" checkboxes
(category filter, not mailbox filter), click-a-slot to create, click-an-event
for detail + delete, conferencing preference picker (Meet/Teams/Zoom/custom
link — just stores a link today, doesn't create real meetings), people
search. Mailbox switcher in the header when the instance has more than one
mailbox.

## Example

```bash
# List Alice's events for the current week
curl -s "http://localhost:8095/v1/calendar/events?mailbox_id=$ALICE_MAILBOX_ID&from=2026-09-01T00:00:00Z&to=2026-09-08T00:00:00Z" | jq .data

# Create an event for Alice
curl -s -X POST http://localhost:8095/v1/calendar/events \
  -H 'content-type: application/json' \
  -d "{\"mailbox_id\":\"$ALICE_MAILBOX_ID\",\"title\":\"Standup\",\"start_at\":\"2026-09-04T09:00:00Z\",\"end_at\":\"2026-09-04T09:30:00Z\"}"

# Delete it (fails silently, event untouched, if mailbox_id doesn't match the owner)
curl -s -X DELETE "http://localhost:8095/v1/calendar/events/$EVENT_ID?mailbox_id=$ALICE_MAILBOX_ID"
```

## Status

| Area | Schema | API | UI | Notes |
|---|---|---|---|---|
| Local event CRUD | ✅ | ✅ | ✅ | week/day/month grid, create/detail/delete |
| Per-mailbox isolation | ✅ (009) | ✅ | ✅ | app-level (`mailbox_id` trusted from client), not JWT-enforced |
| Conferencing preference | ✅ (004) | ✅ | ✅ | stores a link/placeholder; doesn't create a real Meet/Teams/Zoom meeting |
| Recurring events | ✅ (field only) | ⏳ | ✅ (picker) | `recurring` is stored but not expanded into occurrences anywhere |
| Notifications/reminders | ✅ (field only) | ⏳ | ✅ (picker) | `notifications` stored, no reminder is actually sent |
| Calnode event-type linking | — | ✅ | ✅ | sidebar "Booking pages", link-out only |
| Calnode booking sync (inbound) | ❌ | ❌ | ❌ | a Calnode booking never appears in this grid — roadmap |
| Calnode booking creation (outbound) | — | bridge exists (`create_booking`), unused by UI | ❌ | dormant code path |
| Session-enforced auth | ❌ | ❌ | ❌ | `mailbox_id` is client-supplied everywhere in this app today, calendar included |

## Related

- [`FEATURE-OVERVIEW.md`](FEATURE-OVERVIEW.md) — where Calendar sits in the wider Aivory Mail feature set
- [`USER_SETTINGS.md`](USER_SETTINGS.md) — same `tenant_id`/`mailbox_id` scoping pattern this doc follows
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — `DbPool` Postgres/SQLite dual-query pattern used throughout
