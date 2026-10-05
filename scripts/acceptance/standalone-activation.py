#!/usr/bin/env python3
"""Exercise the native installer using a private D-Bus manager and a real daemon.

Usage: standalone-activation.py controller candidate manifest [second-candidate second-manifest] [--allow-debug]
This verifies the adapter and recovery flow, not a real systemd login or host installation.
Requires /usr/bin/python3 with dbus-python and GLib, plus dbus-run-session.
"""
import json
import argparse
import hashlib
import os
from pathlib import Path
import signal
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request


def main():
    args = sys.argv[1:]
    if args[:1] != ["--inside"]:
        env = dict(os.environ, PATINA_ACCEPTANCE_PARENT_BUS=os.environ.get("DBUS_SESSION_BUS_ADDRESS", ""))
        subprocess.run(["dbus-run-session", "--", "/usr/bin/python3", str(Path(__file__).resolve()), "--inside", *args], env=env, check=True)
        return
    args.pop(0)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("controller")
    parser.add_argument("candidate")
    parser.add_argument("manifest")
    parser.add_argument("second", nargs="*")
    parser.add_argument("--allow-debug", action="store_true")
    parser.add_argument("--migrate-from", choices=["packaged", "appimage"])
    parser.add_argument("--fail-first-reload", action="store_true")
    parser.add_argument("--login-enabled", action="store_true")
    parser.add_argument("--change-unit-after-stop", action="store_true")
    parser.add_argument("--reload-test-binary", type=Path)
    parser.add_argument("--deactivate-roundtrip", action="store_true")
    parser.add_argument("--deactivate-after-failed-start", action="store_true")
    options = parser.parse_args(args)
    assert len(options.second) in (0, 2)
    assert not options.fail_first_reload or options.migrate_from
    assert not options.change_unit_after_stop or options.migrate_from == "appimage"
    assert not options.reload_test_binary or (len(options.second) == 2 and not options.migrate_from)
    assert not options.deactivate_roundtrip or (not options.second and not options.reload_test_binary)
    assert not options.deactivate_after_failed_start or (options.deactivate_roundtrip and not options.fail_first_reload and not options.change_unit_after_stop)
    private_bus = os.environ.get("DBUS_SESSION_BUS_ADDRESS")
    assert private_bus and private_bus != os.environ.get("PATINA_ACCEPTANCE_PARENT_BUS", "")
    controller, source, manifest = options.controller, options.candidate, options.manifest
    second = options.second or None
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
    debug = ["--allow-debug"] if options.allow_debug else []

    def command(*values, error=None):
        result = subprocess.run([controller, *values, "--runtime-root", str(runtime), *debug], env=env,
                                capture_output=True, timeout=90)
        if error is not None:
            assert result.returncode != 0 and error in result.stderr.decode(), result.stderr.decode()
            return
        assert result.returncode == 0, result.stderr.decode()
        return json.loads(result.stdout)

    command("--stage-runtime", source, "--manifest-sha256", manifest)
    selected = command("--select-runtime", manifest, "--expected-current", "none")
    plan = command("--print-runtime-service", manifest, "--config-root", str(config), "--data-root", str(data))
    unit_path = Path(plan["unit_path"])
    unit_path.parent.mkdir(parents=True)
    binary = runtime / "current/bin/patinad"
    legacy_binary = Path("/usr/bin/patinad")
    migration_args, source_definition = [], None
    if options.migrate_from:
        source_version = subprocess.check_output([str(legacy_binary), "--version"], env=env, text=True).strip().removeprefix("patinad ")
        packaged_path = Path("/usr/lib/systemd/user/patinad.service")
        original_unit = packaged_path.read_text()
        source_argv = [str(legacy_binary)]
        source_environment = ["PATINA_SYSTEMD_SERVICE=patinad.service"]
        if options.migrate_from == "appimage":
            # Synthetic AppDir layout with the installed daemon, not an AppImage
            # distribution/FUSE acceptance. No host package or service is changed.
            old_root = data / "Patina/runtime-appimage"
            destination = old_root / "versions" / ("a" * 64)
            (destination / "usr/bin").mkdir(parents=True)
            for filename in ("patinad", "Patina"):
                shutil.copyfile(legacy_binary, destination / "usr/bin" / filename)
                (destination / "usr/bin" / filename).chmod(0o700)
            launcher = destination / "AppRun"
            launcher.write_text('#!/bin/sh\n[ "$1" = "--patinad" ] || exit 2\nshift\nbase=$(dirname "$(readlink -f "$0")")\nexec "$base/usr/bin/patinad" "$@"\n')
            launcher.chmod(0o700)
            (destination / ".patina-runtime.json").write_text(json.dumps(dict(format=1, version=source_version, image_sha256="a" * 64)))
            (old_root / "install.lock").touch(mode=0o600)
            (old_root / "current").symlink_to("versions/" + "a" * 64)
            stable_launcher = old_root / "current/AppRun"
            original_unit = "# Managed by Patina AppImage runtime v1\n" + original_unit.replace(
                "ExecStart=/usr/bin/patinad", f'ExecStart="{stable_launcher}" --patinad').replace(
                "[Service]\n", f'[Service]\nEnvironment="XDG_CONFIG_HOME={config}"\nEnvironment="XDG_DATA_HOME={data}"\n')
            source_argv = [str(stable_launcher), "--patinad"]
            source_environment.extend(["XDG_CONFIG_HOME=" + str(config), "XDG_DATA_HOME=" + str(data)])
            unit_path.write_text(original_unit)
        source_definition = dict(unit_path=str(packaged_path if options.migrate_from == "packaged" else unit_path),
                                 argv=source_argv + ["--profile", "production", "--serve-api", "--track"], environment=source_environment)
        migration_args = ["--migrate-from", options.migrate_from, "--source-unit-sha256",
                          hashlib.sha256(original_unit.encode()).hexdigest(), "--source-version", source_version]
    else:
        unit_path.write_text(plan["unit_text"])
    wants = unit_path.parent / "default.target.wants/patinad.service"
    if options.login_enabled:
        wants.parent.mkdir()
        wants.symlink_to(source_definition["unit_path"] if source_definition else unit_path)
    wants_before = wants.readlink() if options.login_enabled else None
    with (root / "prepare.log").open("w") as log:
        subprocess.run([str(legacy_binary if options.migrate_from else binary), "--profile", "production"], env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=30)
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
    cutover = dict(version=1, request_id=cutover_id, profile="production", status="completed" if options.migrate_from else "prepared", requested_at_ms=1,
                   updated_at_ms=1, requested_desktop_pid=os.getpid(), background_tracking_at_login=options.login_enabled,
                   desktop_launch_at_login=False, failure_code=None, failure_message=None)
    (control / "runtime-owner-cutover.json").write_text(json.dumps(cutover))
    activation = dict(format_version=1, runtime_root=str(runtime), config_root=str(config), data_root=str(data),
                      manifest_sha256=manifest, binary_sha256=selected["binary_sha256"], cutover_request_id=cutover_id,
                      phase="prepared", last_error=None)
    if not options.migrate_from:
        (control / "standalone-activation.json").write_text(json.dumps(activation))
    fixture = dict(private_bus=private_bus, config=str(config), data=str(data), binary=str(binary),
                   unit_path=str(unit_path), unit_text=plan["unit_text"], source=source_definition,
                   enabled=options.login_enabled, fail_first_reload=options.fail_first_reload,
                   change_unit_after_stop=options.change_unit_after_stop)
    fixture["restart_on_exit"] = bool(options.reload_test_binary)
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
        if options.migrate_from:
            deadline = time.monotonic() + 30
            while True:
                try:
                    token = (data / "Patina/api_token").read_text().strip()
                    request = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/capabilities", headers={"Authorization": "Bearer " + token})
                    with urllib.request.urlopen(request, timeout=1) as response:
                        if json.load(response)["data"]["tracking"]["ready"]:
                            break
                except (OSError, ValueError):
                    pass
                assert time.monotonic() < deadline, (root / "daemon.log").read_text()
                time.sleep(.1)
            invalid = migration_args.copy()
            invalid[3] = "0" * 64
            command("--activate-runtime", manifest, "--api-port", str(port), *invalid, error="known service recipe")
            for drift, message in [("pid", "PID does not own"), ("environment", "environment differs"), ("drop_in", "drop-ins preserved")]:
                overrides = root / "inspection-overrides.json"
                overrides.write_text(json.dumps({drift: True}))
                command("--activate-runtime", manifest, "--api-port", str(port), *migration_args, error=message)
                overrides.unlink()
                assert not (control / "standalone-activation.json").exists()
            assert json.loads((root / "manager-counts.json").read_text()) == dict(start=0, stop=0, reload=0)
        if options.change_unit_after_stop:
            command("--activate-runtime", manifest, "--api-port", str(port), *migration_args, error="source user unit changed")
            assert unit_path.read_text() == "# externally edited during stop\n"
            assert json.loads((root / "manager-counts.json").read_text()) == dict(start=0, stop=1, reload=0)
            pending = json.loads((control / "standalone-activation.json").read_text())
            assert pending["phase"] == "prepared" and pending["migration"]["original_unit"] == original_unit
            # Explicit fixture repair; the application must never undo the edit.
            unit_path.write_text(original_unit)
        if options.deactivate_after_failed_start:
            (root / "control-faults.json").write_text(json.dumps({"fail_next_start": True}))
            command("--activate-runtime", manifest, "--api-port", str(port), *migration_args, error="injected start failure")
            pending = json.loads((control / "standalone-activation.json").read_text())
            assert pending["phase"] == "starting"
            stopped = command("--deactivate-runtime", manifest)
            assert stopped["phase"] == "deactivated" and not stopped["runtime_start_allowed"]
            first = command("--activate-runtime", manifest, "--api-port", str(port))
        elif options.fail_first_reload:
            command("--activate-runtime", manifest, "--api-port", str(port), *migration_args, error="injected reload failure")
            pending = json.loads((control / "standalone-activation.json").read_text())
            assert pending["phase"] == "prepared" and pending["migration"]["original_unit"] == original_unit
            assert unit_path.read_text() == plan["unit_text"]
            first = command("--activate-runtime", manifest, "--api-port", str(port))
        else:
            first = command("--activate-runtime", manifest, "--api-port", str(port), *migration_args)
        assert first["phase"] == "completed" and first["binary_sha256"] == selected["binary_sha256"]
        counts = json.loads((root / "manager-counts.json").read_text())
        assert command("--activate-runtime", manifest, "--api-port", str(port)) == first
        assert json.loads((root / "manager-counts.json").read_text()) == counts
        # Recovery does not send another StopUnit for an already inactive source.
        expected_counts = dict(start=1, stop=int(bool(options.migrate_from)), reload=1 + int(options.fail_first_reload) + 2 * int(options.deactivate_after_failed_start))
        if options.reload_test_binary:
            expected_counts["controlled_restart"] = 0
        assert counts == expected_counts, counts
        if options.migrate_from:
            assert first["migration"]["original_unit"] == original_unit
            assert first["migration"]["request"]["kind"] == options.migrate_from
            assert packaged_path.read_text() == Path(__file__).resolve().parents[2].joinpath("packaging/systemd/patinad.service").read_text()
        switched = False
        if second:
            source_b, manifest_b = second
            staged_b = command("--stage-runtime", str(Path(source_b).resolve(strict=True)), "--manifest-sha256", manifest_b)
            assert staged_b["binary_sha256"] != selected["binary_sha256"]
            command("--select-runtime", manifest_b, "--expected-current", manifest)
            if options.reload_test_binary:
                reload_env = dict(env, PATINA_RELOAD_ACCEPTANCE_ROOT=str(root), PATINA_RELOAD_ACCEPTANCE_PORT=str(port))
                with (root / "reload-test.log").open("w") as log:
                    result = subprocess.run([str(options.reload_test_binary.resolve(strict=True)), "--ignored", "--exact", "app::daemon_service::upgrade::tests::native_reload_selected_backend", "--nocapture"], env=reload_env, stdout=log, stderr=subprocess.STDOUT, timeout=100)
                assert result.returncode == 0, (root / "reload-test.log").read_text()
                result = json.loads((root / "reload-result.json").read_text())
                assert result["passed"] and result["binary_sha256"] == staged_b["binary_sha256"]
                # API reload owns its lifecycle ticket; the installer reconciles
                # its previous receipt from the verified running target, without
                # sending another stop/start or rewriting the unit.
                updated = command("--activate-runtime", manifest_b, "--api-port", str(port))
                assert updated["manifest_sha256"] == manifest_b and updated["binary_sha256"] == staged_b["binary_sha256"]
            else:
                updated = command("--activate-runtime", manifest_b, "--api-port", str(port))
                assert updated["phase"] == "completed" and updated["binary_sha256"] == staged_b["binary_sha256"]
            counts = json.loads((root / "manager-counts.json").read_text())
            if options.reload_test_binary:
                assert counts == dict(start=1, stop=0, reload=1, controlled_restart=1)
            else:
                assert counts == {key: value + 1 for key, value in expected_counts.items()}
            switched = True
        if options.deactivate_roundtrip:
            import dbus
            activation_path = control / "standalone-activation.json"
            baseline_counts = dict(counts)
            overrides = root / "inspection-overrides.json"
            for fault in ("pid", "environment", "drop_in"):
                overrides.write_text(json.dumps({fault: True}))
                command("--deactivate-runtime", manifest, error="")
                assert json.loads((root / "manager-counts.json").read_text()) == baseline_counts
                assert json.loads(activation_path.read_text())["runtime_start_allowed"]
            overrides.unlink()
            # An externally installed mask is never claimed merely because its
            # target is also /dev/null. Preserve it and the active service state.
            unit_path.unlink()
            unit_path.symlink_to("/dev/null")
            foreign_inode = unit_path.lstat().st_ino
            command("--deactivate-runtime", manifest, error="")
            assert unit_path.lstat().st_ino == foreign_inode
            assert json.loads((root / "manager-counts.json").read_text()) == baseline_counts
            unit_path.unlink()
            unit_path.write_text(plan["unit_text"])
            faults = root / "control-faults.json"
            faults.write_text(json.dumps({"fail_reload_numbers": [baseline_counts["reload"] + 1]}))
            command("--deactivate-runtime", manifest, error="injected reload failure")
            pending = json.loads(activation_path.read_text())
            assert pending["phase"] == "deactivating" and not pending["runtime_start_allowed"]
            assert unit_path.is_symlink() and os.readlink(unit_path) == "/dev/null"
            mask = pending["deactivation_mask"]
            stopped = command("--deactivate-runtime", manifest)
            assert stopped["phase"] == "deactivated" and stopped["deactivation_mask"] == mask
            assert not stopped["runtime_start_allowed"]
            stopped_counts = json.loads((root / "manager-counts.json").read_text())
            assert stopped_counts["stop"] == baseline_counts["stop"] + 1
            assert stopped_counts["start"] == baseline_counts["start"]
            command("--deactivate-runtime", manifest)
            assert json.loads((root / "manager-counts.json").read_text()) == stopped_counts
            manager_proxy = dbus.Interface(dbus.SessionBus().get_object("org.freedesktop.systemd1", "/org/freedesktop/systemd1"), "org.freedesktop.systemd1.Manager")
            try:
                manager_proxy.StartUnit("patinad.service", "replace")
                raise AssertionError("masked service started")
            except dbus.exceptions.DBusException as error:
                assert error.get_dbus_name() == "org.freedesktop.systemd1.UnitMasked"
            before_db = hashlib.sha256(db.read_bytes()).hexdigest()
            denied = subprocess.run([controller, "--profile", "production"], env=env, capture_output=True, timeout=15)
            assert denied.returncode != 0 and b"startup is disabled" in denied.stderr
            assert hashlib.sha256(db.read_bytes()).hexdigest() == before_db
            # Replacing a saved mask with a new external mask must block resume.
            parked = root / "saved-mask"
            unit_path.rename(parked)
            unit_path.symlink_to("/dev/null")
            foreign_inode = unit_path.lstat().st_ino
            command("--activate-runtime", manifest, "--api-port", str(port), error="")
            assert unit_path.lstat().st_ino == foreign_inode
            unit_path.unlink()
            parked.rename(unit_path)
            faults.write_text(json.dumps({"fail_reload_numbers": [stopped_counts["reload"] + 1]}))
            command("--activate-runtime", manifest, "--api-port", str(port), error="injected reload failure")
            pending = json.loads(activation_path.read_text())
            assert pending["phase"] == "prepared" and not pending["runtime_start_allowed"]
            assert unit_path.is_symlink() and unit_path.lstat().st_ino == mask["inode"]
            assert json.loads((root / "manager-counts.json").read_text())["start"] == baseline_counts["start"]
            # Model interruption between atomic unit restoration and Reload:
            # disk contains our exact unit while the manager still holds a mask.
            restored_unit = root / "interrupted-restored-unit"
            restored_unit.write_text(plan["unit_text"])
            restored_unit.replace(unit_path)
            resumed = command("--activate-runtime", manifest, "--api-port", str(port))
            assert resumed["phase"] == "completed" and resumed["runtime_start_allowed"]
            assert "deactivation_mask" not in resumed
            assert resumed["minimum_runtime_version"] == selected["build"]["package_version"]
            assert unit_path.read_text() == plan["unit_text"]
            counts = json.loads((root / "manager-counts.json").read_text())
            assert counts["start"] == baseline_counts["start"] + 1 and counts["stop"] == baseline_counts["stop"] + 1
            command("--activate-runtime", manifest, "--api-port", str(port))
            assert json.loads((root / "manager-counts.json").read_text()) == counts
        cutover_after = json.loads((control / "runtime-owner-cutover.json").read_text())
        assert cutover_after["status"] == "completed" and cutover_after["request_id"] == cutover_id
        assert cutover_after["background_tracking_at_login"] == options.login_enabled and cutover_after["desktop_launch_at_login"] is False
        assert wants.readlink() == wants_before if options.login_enabled else not wants.is_symlink()
        with sqlite3.connect(db) as connection:
            assert connection.execute("SELECT value FROM settings WHERE key='background_tracking_at_login'").fetchone() == ("1" if options.login_enabled else "0",)
            assert connection.execute("SELECT duration FROM sessions WHERE exe_name='fixture'").fetchone() == (1000,)
        result = dict(passed=True, evidence=str(root), binary_sha256=selected["binary_sha256"], real_daemon=True,
                      private_dbus_manager=True, actual_systemd=False, production_state_used=False,
                      resumed_cutover=not bool(options.migrate_from), repeated_activation_no_restart=True, login_preference_preserved=True,
                      migration_source=options.migrate_from, synthetic_appimage_layout=options.migrate_from == "appimage",
                      resumed_after_unit_replacement=options.fail_first_reload,
                      post_stop_external_edit_preserved=options.change_unit_after_stop,
                      desktop_native_reload=bool(options.reload_test_binary),
                      deactivation_roundtrip=options.deactivate_roundtrip,
                      deactivation_after_failed_start=options.deactivate_after_failed_start,
                      activation_adopted_reloaded_target=bool(options.reload_test_binary),
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
