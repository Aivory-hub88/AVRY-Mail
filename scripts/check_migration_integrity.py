#!/usr/bin/env python3
"""Fail closed when SQLx migration filenames have ambiguous versions."""

from collections import defaultdict
from pathlib import Path
import re
import sys


MIGRATIONS_DIR = Path(__file__).resolve().parents[1] / "migrations"
MIGRATION_NAME = re.compile(r"^(?P<version>\d+)_[^/]+\.sql$")


def main() -> int:
    migrations = sorted(MIGRATIONS_DIR.glob("*.sql"))
    by_version: dict[str, list[Path]] = defaultdict(list)
    invalid: list[Path] = []

    for migration in migrations:
        match = MIGRATION_NAME.match(migration.name)
        if match is None:
            invalid.append(migration)
        else:
            by_version[match.group("version")].append(migration)

    duplicates = {
        version: files for version, files in by_version.items() if len(files) > 1
    }
    if not invalid and not duplicates:
        print(f"Migration integrity check passed: {len(migrations)} SQL files, unique versions.")
        return 0

    print("Migration integrity check failed; refusing to permit migration execution.", file=sys.stderr)
    if invalid:
        print("Invalid migration filenames (expected <version>_<name>.sql):", file=sys.stderr)
        for migration in invalid:
            print(f"  - {migration.name}", file=sys.stderr)
    if duplicates:
        print("Duplicate SQLx migration versions:", file=sys.stderr)
        for version in sorted(duplicates, key=int):
            print(f"  - {version}: {', '.join(path.name for path in duplicates[version])}", file=sys.stderr)
        print(
            "Reconcile these files against the deployed _sqlx_migrations table before renaming or applying anything;"
            " do not change an already-applied migration filename or contents blindly.",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
