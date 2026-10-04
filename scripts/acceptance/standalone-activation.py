#!/usr/bin/env python3
"""Exercise the native installer using a private D-Bus manager and a real daemon.

Usage: standalone-activation.py controller candidate manifest [second-candidate second-manifest] [--allow-debug]
This verifies the adapter and recovery flow, not a real systemd login or host installation.
Requires /usr/bin/python3 with dbus-python and GLib, plus dbus-run-session.
"""
import json
import os
from pathlib import Path
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time


def main():
    args = sys.argv[1:]
    if args[:1] != ["--inside"]:
        env = dict(os.environ, PATINA_ACCEPTANCE_PARENT_BUS=os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""))
        subprocess.run(["dbus-run-session", "--", "/usr/bin/python3", str(Path(__file__).resolve()), "--inside", *args], env=env, check=True)
        return
    args.pop(0)
    allow_debug = args[-1:] == ["--allow-debug"]
    if allow_debug:
        args.pop()
    if len(args) not in (3, 5):
        raise SystemExit(__doc__)
    private_bus = os.environ.get("DBUS_SESSION_BUS_ADDRESS")
    assert private_bus and private_bus != os.environ.get("PATINA_ACCEPTANCE_PARENT_BUS", "")
    controller, source, manifest = args[:3]
    second = args[3:] or None
    controller, source = str(Path(controller).resolve(strict=True)), str(Path(source).resolve(strict=True))
    os.umask(0o077)
    root = Path(tempfile.mkdtemp(prefix="patina-activate-private-"))
    runtime, config, data = root / "runtime", root / "config", root / "data"
    env = os.environ.copy()
    for key in ["INVOCATION_ID", "SYSTEMD_EXEC_PID", "NOTIFY_SOCKET", "PATINA_SYSTEMD_SERVICE", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY"]:
        env.pop(key, None)
    env.update(XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(data), XDG_CACHE_HOME=str(root / "cache"),
               XDG_RUNTIME_DIR=str(root / "xdg-runtime"), DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={root}/no-system-bus",
               PULSE_SERVER=f"unix:{root}/no-pulse", XDG_SESSION_TYPE="unspecified", XDG_CURRENT_DESKTOP="")
    (root / "xdg-runtime").mkdir(mode=0o700)
    debug = ["--allow-debug"] if allow_debug else []

    def command(*values):
        result = subprocess.run([controller, *values, "--runtime-root", str(runtime), *debug], env=env,
                                capture_output=True, timeout=90)
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    command("--stage-runtime", source, "--manifest-sha256", manifest)
    selected = command("--select-runtime", manifest, "--expected-current", "none")
    plan = command("--print-runtime-service", manifest, "--config-root", str(config), "--data-root", str(data))
    unit_path = Path(plan["unit_path"])
    unit_path.parent.mkdir(parents=True)
    unit_path.write_text(plan["unit_text"])
    binary = runtime / "current/bin/patinad"
    with (root / "prepare.log").open("w") as log:
        subprocess.run([str(binary), "--profile", "production"], env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    db = data / "Patina/patina.db"
    with sqlite3.connect(db) as connection:
        connection.executemany("INSERT OR REPLACE INTO settings(key,value) VALUES(?,?)",
                               [("local_api_port", str(port)), ("web_activity_enabled", "0"), ("audio_participation_enabled", "0")])
        connection.execute("INSERT INTO sessions(app_name,exe_name,window_title,start_time,end_time,duration) VALUES('Fixture','fixture','synthetic',1000,2000,1000)")
    # Model an interrupted first installation, after its unit and cutover were prepared.
    # This also isolates the API port without changing the actual unit command.
    control = config / "Patina"
    control.mkdir(parents=True, exist_ok=True)
    cutover_id = "cutover_" + "a" * 32
    cutover = dict(version=1, request_id=cutover_id, profile="production", status="prepared", requested_at_ms=1,
                   updated_at_ms=1, requested_desktop_pid=os.getpid(), background_tracking_at_login=False,
                   desktop_launch_at_login=False, failure_code=None, failure_message=None)
    (control / "runtime-owner-cutover.json").write_text(json.dumps(cutover))
    activation = dict(format_version=1, runtime_root=str(runtime), config_root=str(config), data_root=str(data),
                      manifest_sha256=manifest, binary_sha256=selected["binary_sha256"], cutover_request_id=cutover_id,
                      phase="prepared", last_error=None)
    (control / "standalone-activation.json").write_text(json.dumps(activation))
    fixture = dict(private_bus=private_bus, config=str(config), data=str(data), binary=str(binary),
                   unit_path=str(unit_path), unit_text=plan["unit_text"])
    fixture_path = root / "fixture.json"
    fixture_path.write_text(json.dumps(fixture))
    manager_log = (root / "manager.log").open("w")
    manager = subprocess.Popen(["/usr/bin/python3", str(Path(__file__).with_name("standalone-systemd-mock.py")), str(fixture_path)],
                               env=env, stdout=manager_log, stderr=subprocess.STDOUT)
    try:
        deadline = time.monotonic() + 10
        while not (root / "manager-ready").exists():
            assert manager.poll() is None, (root / "manager.log").read_text()
            assert time.monotonic() < deadline
            time.sleep(.05)
        first = command("--activate-runtime", manifest, "--api-port", str(port))
        assert first["phase"] == "completed" and first["binary_sha256"] == selected["binary_sha256"]
        counts = json.loads((root / "manager-counts.json").read_text())
        assert command("--activate-runtime", manifest, "--api-port", str(port)) == first
        assert json.loads((root / "manager-counts.json").read_text()) == counts
        assert counts == {"start": 1, "stop": 0, "reload": 1}
        switched = False
        if second:
            source_b, manifest_b = second
            staged_b = command("--stage-runtime", str(Path(source_b).resolve(strict=True)), "--manifest-sha256", manifest_b)
            assert staged_b["binary_sha256"] != selected["binary_sha256"]
            command("--select-runtime", manifest_b, "--expected-current", manifest)
            updated = command("--activate-runtime", manifest_b, "--api-port", str(port))
            assert updated["phase"] == "completed" and updated["binary_sha256"] == staged_b["binary_sha256"]
            counts = json.loads((root / "manager-counts.json").read_text())
            assert counts == {"start": 2, "stop": 1, "reload": 2}
            switched = True
        cutover_after = json.loads((control / "runtime-owner-cutover.json").read_text())
        assert cutover_after["status"] == "completed" and cutover_after["request_id"] == cutover_id
        assert cutover_after["background_tracking_at_login"] is False and cutover_after["desktop_launch_at_login"] is False
        assert not (unit_path.parent / "default.target.wants/patinad.service").exists()
        with sqlite3.connect(db) as connection:
            assert connection.execute("SELECT value FROM settings WHERE key='background_tracking_at_login'").fetchone() == ("0",)
            assert connection.execute("SELECT duration FROM sessions WHERE exe_name='fixture'").fetchone() == (1000,)
        result = dict(passed=True, evidence=str(root), binary_sha256=selected["binary_sha256"], real_daemon=True,
                      private_dbus_manager=True, actual_systemd=False, production_state_used=False,
                      resumed_cutover=True, repeated_activation_no_restart=True, login_disabled_preserved=True,
                      history_preserved=True, controlled_binary_switch=switched, counts=counts)
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result))
    finally:
        manager.send_signal(signal.SIGTERM)
        try:
            manager.wait(timeout=20)
        except subprocess.TimeoutExpired:
            manager.kill()
            manager.wait()
        manager_log.close()


if __name__ == "__main__":
    main()
