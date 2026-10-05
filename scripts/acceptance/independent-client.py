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
import shutil
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
    # Launch a private copy so path replacement cannot touch the user's build.
    running_binary = root / "patinad"
    shutil.copy2(daemon, running_binary)
    expected_binary_sha256 = hashlib.sha256(daemon.read_bytes()).hexdigest()
    with (root / "daemon.log").open("x") as log:
        # Prepare only the new Local profile, with no tracking or listener.
        subprocess.run([str(daemon), "--profile", "local"], env=env, stdout=log,
                       stderr=subprocess.STDOUT, timeout=20, check=True)
        db = root / "data/Patina Local/patina.db"
        assert db.is_file()
        with sqlite3.connect(db) as connection:
            connection.executemany("INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
                                   [("web_activity_enabled", "0"), ("audio_participation_enabled", "0")])
            connection.execute("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES('Fixture','fixture-app','synthetic',1000,2000,1000)")
        owner = subprocess.Popen([str(running_binary), "--profile", "local", "--serve-api", "--track", "--port", "0"],
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
            token = token_file.read_text().strip()
            def tools_request(path, payload=None, *, raw=False):
                request = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/{path}",
                    data=None if payload is None else json.dumps(payload).encode(),
                    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
                    method="GET" if payload is None else "POST")
                with opener.open(request, timeout=5) as response:
                    value = json.load(response)
                    return value if raw else value["data"]

            until = time.monotonic() + 10
            while not tools_request("capabilities")["tools"]["ready"]:
                assert time.monotonic() < until, "isolated Tools owner never became ready"
                time.sleep(.1)
            service_before = tools_request("system/service")
            assert service_before["executable"]["binary_sha256"] == expected_binary_sha256
            assert service_before["executable"]["build"]["desktop_feature"] is False
            assert not service_before.get("executable_error")
            artifact_version = service_before["executable"]["build"]["package_version"]
            assert tools_request("health")["version"] == artifact_version
            assert tools_request("capabilities")["server_version"] == artifact_version
            assert tools_request("openapi.json", raw=True)["info"]["version"] == artifact_version
            assert subprocess.check_output([str(daemon), "--version"], env=env, text=True).strip() == "patinad " + artifact_version
            # Replacing the launch path must not report the replacement as running.
            replacement = root / "replacement"
            replacement.write_bytes(b"not the running image\n")
            replacement.replace(running_binary)
            service_after = tools_request("system/service")
            assert service_after == service_before
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
                assert all(("executable_sha256=" + expected_binary_sha256).encode() in value for value in buffers)
                token = token_file.read_text().strip()
                request = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/apps/fixture-app/classify",
                    data=json.dumps({"category": "research"}).encode(),
                    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"}, method="POST")
                with opener.open(request, timeout=5) as response:
                    assert response.status == 200
                receive(b"event=tracking-data-changed cursor=")
                started = tools_request("tools/timer/start", {"mode": "stopwatch", "label": "SDK sync fixture"})
                assert started["current_timer"]["status"] == "running"
                receive(b"event=tools-runtime-changed cursor=")
                lapped = tools_request("tools/timer/laps", {})
                assert len(lapped["timer_laps"]) == 1
                paused = tools_request("tools/timer/pause", {})
                observed = tools_request("tools/snapshot")
                assert paused["current_timer"]["status"] == "paused"
                assert observed["current_timer"] == paused["current_timer"]
                assert observed["timer_laps"] == paused["timer_laps"]
                tools_request("tools/timer/reset", {})
                for index, value in enumerate(buffers):
                    assert token.encode() not in value
                    (root / f"client-{index}.log").write_bytes(value)
                cursors = [re.findall(rb"event=tracking-data-changed cursor=(\d+)", value) for value in buffers]
                assert set(cursors[0]).intersection(cursors[1]), "clients did not share a committed event"
                tools_cursors = [re.findall(rb"event=tools-runtime-changed cursor=(\d+)", value) for value in buffers]
                assert set(tools_cursors[0]).intersection(tools_cursors[1]), "clients did not share a Tools event"
                result = {"passed": True, "daemon_binary": str(daemon),
                          "daemon_sha256": hashlib.sha256(daemon.read_bytes()).hexdigest(),
                          "sdk_probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(),
                          "profile": "local", "production_state_used": False,
                          "two_independent_sdk_processes": True, "shared_classification_event": True,
                          "shared_tools_event": True, "tools_http_actions_observed": True,
                          "unauthenticated_read_rejected": True,
                          "tracking_hardware_verified": False, "gui_or_tui_verified": False}
                result.update(running_executable_sha256=expected_binary_sha256,
                              replaced_launch_path_preserves_running_identity=True,
                              sdk_clients_read_running_identity=True)
            def resource_request(payload=None):
                path = "/api/v1/settings/resources" + ("/conditional" if payload is not None else "")
                request = urllib.request.Request(
                    f"http://127.0.0.1:{port}{path}",
                    data=None if payload is None else json.dumps(payload).encode(),
                    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
                    method="GET" if payload is None else "POST",
                )
                with opener.open(request, timeout=10) as response:
                    return json.load(response)["data"]

            baseline = resource_request()
            first_resource = resource_request({"expected_revision": baseline["revision"],
                "patch": {"browser_activity": {"token": "fixture-resource-token"}}})
            rotated_resource = resource_request({"expected_revision": first_resource["revision"],
                "patch": {"browser_activity": {"token": "fixture-resource-rotated"}}})
            assert first_resource["browser_activity"] == rotated_resource["browser_activity"]
            assert first_resource["revision"] != rotated_resource["revision"]
            assert not rotated_resource["browser_activity"]["enabled"]
            assert not rotated_resource["audio_participation_enabled"]
            assert "fixture-resource" not in json.dumps(rotated_resource)
            try:
                resource_request({"expected_revision": first_resource["revision"],
                                  "patch": {"audio_participation_enabled": True}})
            except urllib.error.HTTPError as error:
                assert error.code == 409, "stale resource revision was not rejected"
            else:
                raise AssertionError("stale resource revision was accepted")
            assert resource_request()["revision"] == rotated_resource["revision"]
            result.update(resource_patch_preserves_omitted_fields=True,
                          credential_rotation_invalidates_revision=True,
                          stale_resource_write_rejected=True)
            history_request = urllib.request.Request(
                f"http://127.0.0.1:{port}/api/v1/activity/history-product?from_ms=1000&to_ms=2000&language=en-US",
                headers={"Authorization": "Bearer " + token})
            with opener.open(history_request, timeout=10) as response:
                product = json.load(response)["data"]
            assert len(product["history"]["records"]) == 1
            assert product["history"]["records"][0]["origin"] == "native"
            assert len(product["hours"]) == 24
            assert sum(hour["active_ms"] for hour in product["hours"]) == 1000
            assert sum(category["active_ms"] for hour in product["hours"] for category in hour["categories"]) == 1000
            result["history_product_totals_agree"] = True
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
                generation_query = "SELECT value FROM settings WHERE key='__runtime_resource_generation'"
                generation = connection.execute(generation_query).fetchone()
                assert generation and int(generation[0]) >= 2
            # Reacquire the released lease and reopen the same schema without a UI.
            subprocess.run([str(daemon), "--profile", "local"], env=env, stdout=log,
                           stderr=subprocess.STDOUT, timeout=20, check=True)
            with sqlite3.connect(db) as connection:
                assert connection.execute(classification_query).fetchone() == before
                assert connection.execute(generation_query).fetchone() == generation
                assert connection.execute("SELECT duration FROM sessions WHERE exe_name='fixture-app' AND start_time=1000").fetchone() == (1000,)
                assert connection.execute(
                    "SELECT version, description, checksum FROM _sqlx_migrations ORDER BY version"
                ).fetchall() == migrations
                assert connection.execute("PRAGMA integrity_check").fetchone() == ("ok",)
            result.update(graceful_shutdown=True, lease_reacquired=True,
                          classification_survives_restart=True, migration_checksums_preserved=True,
                          resource_generation_survives_restart=True)
            result["fixture_history_survives_restart"] = True
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
