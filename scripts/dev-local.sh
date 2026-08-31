#!/usr/bin/env bash
# Aivory Mail — local dev stack launcher (API + web)
# Usage: ./scripts/dev-local.sh   (dari root repo)
# Stops with: ./scripts/stop-local.sh
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PID_DIR="$REPO/.local-pids"
API_PORT="${PORT:-8095}"
WEB_PORT="${WEB_PORT:-3005}"
API_LOG="$REPO/.local-pids/api.log"
WEB_LOG="$REPO/.local-pids/web.log"

mkdir -p "$PID_DIR"

log() { printf '\033[1;32m[aivory-mail]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[aivory-mail]\033[0m %s\n' "$*" >&2; exit 1; }

port_listening() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }

# 1) API
if port_listening "$API_PORT"; then
  log "API sudah jalan di :$API_PORT (skip)"
else
  log "Build & start API di :$API_PORT …"
  (cd "$REPO" && cargo build --bin aivory-mail-api >/dev/null 2>&1) || die "cargo build gagal"
  (cd "$REPO" && nohup ./target/debug/aivory-mail-api >"$API_LOG" 2>&1 & echo $! > "$PID_DIR/api.pid")
  sleep 2
  if ! port_listening "$API_PORT"; then
    die "API gagal start. Log: $API_LOG"
  fi
  log "API up → http://localhost:$API_PORT/health"
fi

# 2) Web
if port_listening "$WEB_PORT"; then
  log "Web sudah jalan di :$WEB_PORT (skip)"
else
  if [ ! -x "$REPO/web/node_modules/.bin/next" ]; then
    log "Install web deps (npm install --legacy-peer-deps) …"
    (cd "$REPO/web" && npm install --legacy-peer-deps >/dev/null 2>&1) || die "npm install gagal"
  fi
  (cd "$REPO/web" && nohup ./node_modules/.bin/next dev -p "$WEB_PORT" >"$WEB_LOG" 2>&1 & echo $! > "$PID_DIR/web.pid")
  sleep 4
  if ! port_listening "$WEB_PORT"; then
    die "Web gagal start. Log: $WEB_LOG"
  fi
  log "Web up → http://localhost:$WEB_PORT"
fi

log "───── Aivory Mail local ─────"
log "API : http://localhost:$API_PORT/health"
log "Web : http://localhost:$WEB_PORT  (/, /settings, /settings/mail, /calendar)"
log "Sqlite: $REPO/data/mail.db · Pid files: $PID_DIR"
log "Stop semua: ./scripts/stop-local.sh"