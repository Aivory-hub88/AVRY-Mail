#!/usr/bin/env bash
# Aivory Mail — stop local dev stack (API + web) yang distart oleh dev-local.sh
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PID_DIR="$REPO/.local-pids"

kill_if() {
  local name="$1" pidfile="$2"
  if [ -f "$pidfile" ]; then
    local pid
    pid="$(cat "$pidfile" 2>/dev/null || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null && printf '[aivory-mail] %s (pid %s) stopped\n' "$name" "$pid"
    fi
    rm -f "$pidfile"
  fi
}

# Fallback: kill berdasarkan port bila pid file hilang
port_kill() {
  local port="$1" name="$2"
  local pids
  pids="$(lsof -ti tcp:"$port" 2>/dev/null || true)"
  if [ -n "$pids" ]; then
    kill $pids 2>/dev/null && printf '[aivory-mail] %s di :%s stopped\n' "$name" "$port"
  fi
}

kill_if "API" "$PID_DIR/api.pid"
kill_if "Web" "$PID_DIR/web.pid"

port_kill 8095 "API"
port_kill 3005 "Web"

printf '[aivory-mail] selesai.\n'