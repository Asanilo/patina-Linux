#!/usr/bin/env python3
"""Verify actual daemon startup permission before SQLite or storage work.

Uses synthetic Production profiles in a private directory, no API listener,
tracking, service control or host data. Supply a current branch headless binary.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="patina-runtime-admission-"))
    binary = root / "patinad"
    shutil.copyfile(args.binary.resolve(strict=True), binary)
    binary.chmod(0o700)
    result = {"passed": False, "binary_sha256": digest(binary), "cases": [], "production_state_used": False}
    try:
        for label, permission, allowed in (("denied", False, False), ("invalid", "false", False),
                                           ("legacy", None, True), ("allowed", True, True)):
            profile = root / label
            control = profile / "config/Patina"
            control.mkdir(parents=True, mode=0o700)
            record = control / "standalone-activation.json"
            data = profile / "data/Patina"
            # Unknown audit fields and phase formats must not affect the stable
            # startup contract, which old clients/installers can also inspect.
            value = {"format_version": 1, "runtime_root": str(profile / "runtime"),
                     "config_root": str(profile / "config"), "data_root": str(profile / "data"),
                     "phase": {"future_format": 2}, "future_audit": {"format": 2}}
            if permission is not None:
                value["runtime_start_allowed"] = permission
            record.write_text(json.dumps(value), encoding="utf-8")
            record.chmod(0o600)
            env = dict(os.environ)
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "APPIMAGE", "APPDIR", "LD_PRELOAD", "LD_LIBRARY_PATH",
                        "INVOCATION_ID", "SYSTEMD_EXEC_PID", "NOTIFY_SOCKET", "PATINA_SYSTEMD_SERVICE"):
                env.pop(key, None)
            env.update(XDG_CONFIG_HOME=str(profile / "config"), XDG_DATA_HOME=str(profile / "data"),
                       XDG_RUNTIME_DIR=str(profile / "xdg-runtime"), XDG_CACHE_HOME=str(profile / "cache"),
                       DBUS_SESSION_BUS_ADDRESS=f"unix:path={profile}/no-session", DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={profile}/no-system",
                       PULSE_SERVER=f"unix:{profile}/no-pulse")
            metadata = subprocess.run([str(binary), "--build-info"], env=env, capture_output=True, timeout=10, check=True)
            assert json.loads(metadata.stdout)["desktop_feature"] is False
            before = digest(record)

            def start(name):
                # No --serve-api or --track: successful initialization shuts down.
                with (profile / (name + ".log")).open("wb") as log:
                    return subprocess.run([str(binary), "--profile", "production"], env=env,
                                          cwd=profile, stdout=log, stderr=subprocess.STDOUT, timeout=30)

            outcome = start("startup")
            assert (outcome.returncode == 0) == allowed, (label, outcome.returncode)
            assert digest(record) == before, "startup must not change installation policy"
            database = data / "patina.db"
            assert database.exists() == allowed
            if not allowed:
                assert not data.exists(), "denial must precede storage directory preparation"
                if permission is False:
                    assert "startup is disabled" in (profile / "startup.log").read_text()
            else:
                with sqlite3.connect(database) as connection:
                    connection.execute("CREATE TABLE acceptance_marker(value TEXT NOT NULL)")
                    connection.execute("INSERT INTO acceptance_marker VALUES ('preserve')")
                before_database = digest(database)
                value["runtime_start_allowed"] = False
                record.write_text(json.dumps(value), encoding="utf-8")
                assert start("denied-existing-profile").returncode != 0
                assert digest(database) == before_database, "denied restart changed existing SQLite data"
            result["cases"].append({"name": label, "startup_allowed": allowed, "passed": True})
        result["passed"] = True
    finally:
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(root / "result.json", flush=True)


if __name__ == "__main__":
    main()
