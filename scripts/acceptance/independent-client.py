#!/usr/bin/env python3
"""Probe two independent SDK processes against an explicit isolated daemon.

Usage: independent-client.py /absolute/path/patinad /absolute/path/inspect
Does not install anything, use production state, or claim GUI/TUI acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    daemon, probe = (Path(value).resolve(strict=True) for value in sys.argv[1:])
    assert daemon.is_file() and probe.is_file()
    os.umask(0o077)
    root = Path(tempfile.mkdtemp(prefix="patina-independent-client-"))
    env = os.environ.copy()
    for key in ["INVOCATION_ID", "SYSTEMD_EXEC_PID", "NOTIFY_SOCKET", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"]:
        env.pop(key, None)
    for key, directory in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime")]:
        path = root / directory
        path.mkdir(mode=0o700)
        env[key] = str(path)
    env.update(DBUS_SESSION_BUS_ADDRESS=f"unix:path={root}/no-session-bus",
               DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={root}/no-system-bus",
               PULSE_SERVER=f"unix:{root}/no-pulse", XDG_SESSION_TYPE="unspecified",
               XDG_CURRENT_DESKTOP="")
    children = []
    with (root / "daemon.log").open("x") as log:
        # Prepare only the new Local profile, with no tracking or listener.
        subprocess.run([str(daemon), "--profile", "local"], env=env, stdout=log,
                       stderr=subprocess.STDOUT, timeout=20, check=True)
        db = root / "data/Patina Local/patina.db"
        assert db.is_file()
        with sqlite3.connect(db) as connection:
            connection.executemany("INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
                                   [("web_activity_enabled", "0"), ("audio_participation_enabled", "0")])
        owner = subprocess.Popen([str(daemon), "--profile", "local", "--serve-api", "--track", "--port", "0"],
                                 env=env, stdout=log, stderr=subprocess.STDOUT)
        children.append(owner)
        try:
            deadline = time.monotonic() + 20
            port = None
            while time.monotonic() < deadline:
                assert owner.poll() is None, "isolated daemon exited"
                match = re.search(r"listening on http://127\.0\.0\.1:(\d+)", (root / "daemon.log").read_text())
                if match:
                    port = int(match[1]); break
                time.sleep(.1)
            assert port and port != 14840, "isolated ephemeral API was not ready"
            token_file = root / "data/Patina Local/api_token"
            assert token_file.is_file()
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            try:
                opener.open(f"http://127.0.0.1:{port}/api/v1/settings/classification", timeout=5)
            except urllib.error.HTTPError as error:
                assert error.code == 401, "unauthenticated read was not rejected"
            else:
                raise AssertionError("unauthenticated read succeeded")
            with selectors.DefaultSelector() as selector:
                for index in range(2):
                    child = subprocess.Popen([str(probe), str(port), str(token_file), "--watch"],
                                             env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    children.append(child)
                    selector.register(child.stdout, selectors.EVENT_READ, index)
                buffers = [b"", b""]
                def receive(marker):
                    until = time.monotonic() + 10
                    while time.monotonic() < until:
                        if all(marker in value for value in buffers):
                            return
                        for key, _ in selector.select(.2):
                            chunk = os.read(key.fd, 4096)
                            assert chunk, "SDK process exited before expected event"
                            buffers[key.data] += chunk
                            assert len(buffers[key.data]) < 16384
                    raise AssertionError("SDK clients did not observe expected state")
                receive(b"subscribed\n")
                token = token_file.read_text().strip()
                request = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/apps/fixture-app/classify",
                    data=json.dumps({"category": "research"}).encode(),
                    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"}, method="POST")
                with opener.open(request, timeout=5) as response:
                    assert response.status == 200
                receive(b"event=tracking-data-changed cursor=")
                for index, value in enumerate(buffers):
                    assert token.encode() not in value
                    (root / f"client-{index}.log").write_bytes(value)
                cursors = [re.findall(rb"event=tracking-data-changed cursor=(\d+)", value) for value in buffers]
                assert set(cursors[0]).intersection(cursors[1]), "clients did not share a committed event"
                result = {"passed": True, "daemon_binary": str(daemon),
                          "daemon_sha256": hashlib.sha256(daemon.read_bytes()).hexdigest(),
                          "sdk_probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(),
                          "profile": "local", "production_state_used": False,
                          "two_independent_sdk_processes": True, "shared_classification_event": True,
                          "unauthenticated_read_rejected": True,
                          "tracking_hardware_verified": False, "gui_or_tui_verified": False}
            owner.send_signal(signal.SIGINT)
            assert owner.wait(timeout=10) == 0, "daemon did not shut down cleanly"
            with sqlite3.connect(db) as connection:
                assert connection.execute("PRAGMA integrity_check").fetchone() == ("ok",)
                classification_query = "SELECT value FROM settings WHERE key='__app_override::fixture-app'"
                before = connection.execute(classification_query).fetchone()
                assert before and json.loads(before[0])["category"] == "research", "classification write did not persist"
                migrations = connection.execute(
                    "SELECT version, description, checksum FROM _sqlx_migrations ORDER BY version"
                ).fetchall()
            # Reacquire the released lease and reopen the same schema without a UI.
            subprocess.run([str(daemon), "--profile", "local"], env=env, stdout=log,
                           stderr=subprocess.STDOUT, timeout=20, check=True)
            with sqlite3.connect(db) as connection:
                assert connection.execute(classification_query).fetchone() == before
                assert connection.execute(
                    "SELECT version, description, checksum FROM _sqlx_migrations ORDER BY version"
                ).fetchall() == migrations
                assert connection.execute("PRAGMA integrity_check").fetchone() == ("ok",)
            result.update(graceful_shutdown=True, lease_reacquired=True,
                          classification_survives_restart=True, migration_checksums_preserved=True)
            (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps({"passed": True, "evidence": str(root), "clients": 2}))
        finally:
            for child in reversed(children[1:]):
                if child.poll() is None: child.terminate()
                try: child.wait(timeout=5)
                except subprocess.TimeoutExpired: child.kill(); child.wait()
            if owner.poll() is None: owner.send_signal(signal.SIGINT)
            try: owner.wait(timeout=10)
            except subprocess.TimeoutExpired: owner.kill(); owner.wait()


if __name__ == "__main__":
    main()
