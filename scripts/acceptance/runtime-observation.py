#!/usr/bin/env python3
"""Capture a private, aggregate-only Patina runtime observation sample."""

import argparse
from contextlib import closing
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sqlite3
import subprocess


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--candidate-sha256", required=True)
    args = parser.parse_args()

    if not args.database.is_file() or args.database.is_symlink():
        parser.error("database must be an existing regular file, not a symlink")
    if not args.output_dir.is_dir() or args.output_dir.is_symlink():
        parser.error("output directory must exist and must not be a symlink")
    output_stat = args.output_dir.stat()
    if output_stat.st_uid != os.getuid() or output_stat.st_mode & 0o077:
        parser.error("output directory must belong to the current user and be owner-only")
    if len(args.candidate_sha256) != 64 or any(c not in "0123456789abcdef" for c in args.candidate_sha256):
        parser.error("candidate SHA256 must be 64 lowercase hexadecimal characters")

    service_output = subprocess.check_output(
        ["systemctl", "--user", "show", "patinad.service", "--no-pager",
         "-p", "ActiveState", "-p", "SubState", "-p", "MainPID",
         "-p", "InvocationID", "-p", "NRestarts", "-p", "MemoryCurrent",
         "-p", "CPUUsageNSec", "-p", "ActiveEnterTimestamp"],
        text=True,
    )
    service = dict(line.split("=", 1) for line in service_output.splitlines() if "=" in line)
    uri = args.database.resolve().as_uri() + "?mode=ro"
    with closing(sqlite3.connect(uri, uri=True, timeout=5)) as connection:
        connection.execute("PRAGMA query_only=ON")
        check = connection.execute("PRAGMA quick_check").fetchone()[0]
        count, last_start, last_end = connection.execute(
            "SELECT COUNT(*), MAX(start_time), MAX(end_time) FROM sessions"
        ).fetchone()
        last_sample_row = connection.execute(
            "SELECT value FROM settings WHERE key='__tracker_last_successful_sample_ms'"
        ).fetchone()

    now = datetime.now(timezone.utc)
    sample = {
        "observed_at_utc": now.isoformat(),
        "candidate_sha256": args.candidate_sha256,
        "service": service,
        "database_bytes": args.database.stat().st_size,
        "database_wal_bytes": Path(str(args.database) + "-wal").stat().st_size
        if Path(str(args.database) + "-wal").exists() else 0,
        "database_quick_check": check,
        "session_count": count,
        "latest_session_start_ms": last_start,
        "latest_session_end_ms": last_end,
        "last_successful_sample_ms": int(last_sample_row[0]) if last_sample_row else None,
        "scope": "read-only aggregate; no titles, URLs, or credentials",
    }
    path = args.output_dir / f"sample-{now:%Y%m%dT%H%M%SZ}.json"
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
    with os.fdopen(os.open(path, flags, 0o600), "w", encoding="utf-8") as handle:
        json.dump(sample, handle, indent=2)
        handle.write("\n")
    print(path)


if __name__ == "__main__":
    main()
