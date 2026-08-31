#!/usr/bin/env python3
"""
Generate docs/openapi.json from the route catalog defined in crates/aivory-mail-api/src/api/mod.rs.

Usage:
    python3 scripts/gen_openapi.py

Keeps the OpenAPI spec in sync with the actual axum router without hand-writing JSON.
The route catalog below mirrors `api/mod.rs` (order preserved). If a route is added
there, add it here too (or extend the parser to derive it automatically).
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MOD_RS = ROOT / "crates" / "aivory-mail-api" / "src" / "api" / "mod.rs"
OUT = ROOT / "docs" / "openapi.json"

TITLE = "Aivory Mail API"
DESC = "Business email infrastructure — Cloudflare + VPS compatible. Rust core, Next.js UI."
VERSION = "0.1.0"
SERVERS = [
    {"url": "http://localhost:8095", "description": "Local dev"},
    {"url": "https://mail.aivory.uk", "description": "Production"},
]

# fallback summary per handler prefix (when not derivable from path)
SUMMARY_HINTS = {
    "list": "List resources",
    "create": "Create resource",
    "get": "Get resource",
    "get_one": "Get resource",
    "update": "Update resource",
    "remove": "Delete resource",
    "verify": "Verify resource",
    "health": "Health check",
    "stats": "Stats",
    "set": "Upsert setting",
    "delete_label": "Delete label",
    "remove": "Delete resource",
}


def norm_path(p: str) -> str:
    """Convert axum `:param` segments to OpenAPI `{param}`."""
    return re.sub(r":([A-Za-z_]+)", r"{\1}", p)


def route_ops(path: str, ops: list[tuple[str, str]]) -> dict:
    entry: dict = {}
    for method, handler in ops:
        summary = f"{method.upper()} {path}"
        entry[method.lower()] = {
            "summary": summary,
            "operationId": handler.replace("::", "_").replace("axum::routing::", ""),
            "responses": {"200": {"description": "OK"}},
        }
    return entry


def main() -> int:
    if not MOD_RS.exists():
        print(f"err: {MOD_RS} not found", file=sys.stderr)
        return 1

    src = MOD_RS.read_text()
    paths: dict[str, dict] = {}
    # Work line-by-line: axum routes are one-per-line in mod.rs, avoiding
    # nested-paren truncation when matching get()/post()/... handlers.
    for line in src.splitlines():
        rm = re.search(r'\.route\("([^"]+)"\s*,\s*(.*)', line)
        if not rm:
            continue
        path = rm.group(1)
        rest = rm.group(2)
        ops: list[tuple[str, str]] = []
        # match get(handler), post(...), put(...), delete(...), axum::routing::put(...)
        for mm in re.finditer(r"(?:axum::routing::)?(get|post|put|delete)\(\s*([A-Za-z0-9_:]+)\s*\)", rest):
            ops.append((mm.group(1), mm.group(2)))
        if ops:
            paths[norm_path(path)] = route_ops(path, ops)

    spec = {
        "openapi": "3.0.3",
        "info": {"title": TITLE, "version": VERSION, "description": DESC},
        "servers": SERVERS,
        "paths": dict(sorted(paths.items())),
    }
    OUT.write_text(json.dumps(spec, indent=2, sort_keys=True) + "\n")
    print(f"ok: wrote {len(paths)} paths → {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())