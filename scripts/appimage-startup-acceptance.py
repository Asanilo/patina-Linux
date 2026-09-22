#!/usr/bin/env python3
"""Opt-in packaged Desktop startup routing, with private X11/D-Bus and mount namespace.

Usage: /usr/bin/python3 scripts/appimage-startup-acceptance.py /absolute/Patina.AppImage
       [--installed-deb-root /absolute/private-dpkg-root]
Requires bubblewrap, xvfb-run, dbus-run-session, Python GI. Tests runtime staging,
DEB-unit reuse, rejection and real Desktop/daemon process handoff. The systemd
manager is a fixture: this is NOT installed systemd, login, UI or signed-upgrade acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request


def inner(root, image, case):
    from gi.repository import Gio, GLib

    events = []
    handoff = case in ["handoff", "deb-handoff"]
    daemon = None
    enabled = False
    daemon_log = None
    env = os.environ.copy()
    env.update({"HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
                "XDG_RUNTIME_DIR": str(root / "runtime"), "XDG_SESSION_TYPE": "x11",
                "GDK_BACKEND": "x11", "DBUS_SYSTEM_BUS_ADDRESS": env["DBUS_SESSION_BUS_ADDRESS"]})
    # Preserve extract mode across the real Desktop's automatic owner-cutover restart.
    env["APPIMAGE_EXTRACT_AND_RUN"] = "1"
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"]:
        Path(env[name]).mkdir(mode=0o700, exist_ok=True)
    for name in ["APPIMAGE", "APPDIR", "PATINA_SYSTEMD_SERVICE", "WAYLAND_DISPLAY", "LD_LIBRARY_PATH", "GTK_MODULES"]:
        env.pop(name, None)
    subprocess.run(["xwininfo", "-root"], env=env, check=True, stdout=subprocess.DEVNULL)
    unit = root / "config/systemd/user/patinad.service"
    if case == "custom":
        unit.parent.mkdir(parents=True)
        unit.write_text("# User-owned unit; must survive unchanged\n")
        unit.chmod(0o600)
    xml = '''<node><interface name="org.freedesktop.systemd1.Manager">
      <property name="Environment" type="as" access="read"/>
      <method name="Reload"/>
      <method name="GetUnitFileState"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
      <method name="GetUnit"><arg type="s" direction="in"/><arg type="o" direction="out"/></method>
      <method name="StartUnit"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="o" direction="out"/></method>
      <method name="StopUnit"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="o" direction="out"/></method>
      <method name="EnableUnitFiles"><arg type="as" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="out"/><arg type="a(sss)" direction="out"/></method>
      <method name="DisableUnitFiles"><arg type="as" direction="in"/><arg type="b" direction="in"/><arg type="a(sss)" direction="out"/></method>
    </interface></node>'''
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def method(connection, sender, path, interface, name, parameters, invocation):
        nonlocal daemon, enabled, daemon_log
        events.append(name)
        if name == "Reload":
            invocation.return_value(GLib.Variant("()", ()))
        elif handoff and (unit.exists() or case == "deb-handoff"):
            values = parameters.unpack()
            requested = values[0] if values else None
            assert requested == "patinad.service" or requested == ["patinad.service"]
            if name == "GetUnitFileState":
                invocation.return_value(GLib.Variant("(s)", ("enabled" if enabled else "disabled",)))
            elif name == "GetUnit":
                invocation.return_value(GLib.Variant("(o)", ("/org/freedesktop/systemd1/unit/patinad",)))
            elif name in ["EnableUnitFiles", "DisableUnitFiles"]:
                enabled = name == "EnableUnitFiles"
                invocation.return_value(GLib.Variant("(ba(sss))", (True, [])) if enabled else GLib.Variant("(a(sss))", ([],)))
            elif name == "StartUnit":
                assert daemon is None, "unexpected duplicate daemon start"
                command = (["/usr/bin/patinad"] if case == "deb-handoff" else
                           [str(root / "data/Patina/runtime-appimage/current/AppRun"), "--patinad"])
                daemon_log = (root / "daemon.log").open("w")
                daemon = subprocess.Popen(command + ["--profile", "production", "--serve-api", "--track"],
                                          env={**env, "PATINA_SYSTEMD_SERVICE": "patinad.service",
                                               "INVOCATION_ID": "isolated-appimage-handoff-fixture"},
                                          stdout=daemon_log, stderr=subprocess.STDOUT, start_new_session=True)
                invocation.return_value(GLib.Variant("(o)", ("/org/freedesktop/systemd1/job/1",)))
            else:
                invocation.return_dbus_error("org.freedesktop.DBus.Error.Failed", "Unexpected stop during handoff")
        else:
            invocation.return_dbus_error("org.freedesktop.systemd1.NoSuchUnit", "Acceptance stops before service handoff")

    def prop(connection, sender, path, interface, name):
        events.append("Environment")
        config = str(root / ("other-config" if case == "mismatch" else "config"))
        return GLib.Variant("as", ["HOME=" + env["HOME"], "XDG_CONFIG_HOME=" + config,
                                   "XDG_DATA_HOME=" + env["XDG_DATA_HOME"]])

    bus.register_object("/org/freedesktop/systemd1", Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0], method, prop, None)
    unit_xml = '<node><interface name="org.freedesktop.systemd1.Unit"><property name="ActiveState" type="s" access="read"/><property name="SubState" type="s" access="read"/></interface></node>'
    def unit_prop(connection, sender, path, interface, name):
        active = daemon is not None and daemon.poll() is None
        return GLib.Variant("s", ("active" if active else "inactive") if name == "ActiveState" else ("running" if active else "dead"))
    bus.register_object("/org/freedesktop/systemd1/unit/patinad", Gio.DBusNodeInfo.new_for_xml(unit_xml).interfaces[0], None, unit_prop, None)
    bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                  "RequestName", GLib.Variant("(su)", ("org.freedesktop.systemd1", 0)), None,
                  Gio.DBusCallFlags.NONE, 3000, None)
    log_path = root / "desktop.log"
    with log_path.open("w") as log:
        child = subprocess.Popen([str(image), "--appimage-extract-and-run"], env=env,
                                 stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            end = time.monotonic() + (90 if handoff else 60)
            context = GLib.MainContext.default()
            while time.monotonic() < end:
                while context.pending():
                    context.iteration(False)
                text = log_path.read_text(errors="replace")
                reservation = root / "config/Patina/runtime-owner-cutover.json"
                if handoff and reservation.exists() and json.loads(reservation.read_text())["status"] == "completed":
                    break
                if case == "standalone" and "Reload" in events:
                    break
                if (case == "deb" and "Environment" in events
                        and "GetUnitFileState" in events[events.index("Environment") + 1:]):
                    break
                if case == "custom" and "custom patinad user unit preserved" in text:
                    break
                if case == "mismatch" and "different profile roots" in text:
                    break
                if child.poll() is not None and not handoff:
                    raise RuntimeError("Desktop exited before expected checkpoint; see " + str(log_path))
                time.sleep(0.05)
            else:
                raise RuntimeError("Desktop checkpoint timed out; see " + str(log_path))
            store = root / "data/Patina/runtime-appimage"
            if case in ["standalone", "handoff"]:
                assert (store / "current/AppRun").is_file()
                assert unit.read_text().startswith("# Managed by Patina AppImage runtime v1\n")
                assert str(store / "current/AppRun") in unit.read_text()
                assert "--patinad" in unit.read_text()
            else:
                assert not store.exists()
                if not handoff:
                    assert "Reload" not in events
                if case == "custom":
                    assert unit.read_text() == "# User-owned unit; must survive unchanged\n"
                else:
                    assert not unit.exists()
            details = {}
            if handoff:
                assert daemon is not None and daemon.poll() is None
                token = (root / "data/Patina/api_token").read_text().strip()
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                def capabilities():
                    request = urllib.request.Request("http://127.0.0.1:14840/api/v1/capabilities", headers={"Authorization": "Bearer " + token})
                    with opener.open(request, timeout=2) as response:
                        data = json.load(response)["data"]
                    assert data["runtime_host"] == "daemon"
                    assert data["tracking"]["owned"] and data["tracking"]["ready"]
                    assert data["daemon_service"]["owned"] and data["daemon_service"]["ready"]
                    return data
                capabilities()
                autostart = (root / "config/autostart/Patina.desktop").read_text()
                expected = "/usr/bin/Patina" if case == "deb-handoff" else str(image)
                assert f"Exec={expected} --autostart\n" in autostart, autostart
                os.killpg(child.pid, signal.SIGTERM)
                # Desktop restart descendants remain in its process group; daemon has its own.
                for _ in range(20):
                    while context.pending():
                        context.iteration(False)
                    time.sleep(0.1)
                capabilities()
                assert daemon.poll() is None
                child = subprocess.Popen([str(image), "--appimage-extract-and-run"], env=env,
                                         stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                for _ in range(50):
                    while context.pending():
                        context.iteration(False)
                    time.sleep(0.1)
                assert child.poll() is None
                capabilities()
                assert events.count("StartUnit") == 1
                details = {"cutover": "completed", "daemon_pid": daemon.pid,
                           "survived_desktop_exit_and_reopen": True, "service_manager": "fixture"}
            (root / "result.json").write_text(json.dumps({"passed": True, "case": case, "events": events, **details}, indent=2) + "\n")
        finally:
            # The whole namespace exits with this runner, including helper descendants.
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            if daemon is not None:
                daemon.send_signal(signal.SIGINT)
                try:
                    daemon.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()
                if daemon_log:
                    daemon_log.close()
                assert daemon.returncode == 0, "daemon did not shut down cleanly"
                with sqlite3.connect(f"file:{root / 'data/Patina/patina.db'}?mode=ro", uri=True) as database:
                    assert database.execute("PRAGMA quick_check").fetchone() == ("ok",)
                (root / "cleanup.json").write_text(json.dumps({"daemon_exit": 0, "database_quick_check": "ok"}) + "\n")


def main():
    if len(sys.argv) == 5 and sys.argv[1] == "--inner":
        inner(Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4])
        return
    if len(sys.argv) not in [2, 4] or (len(sys.argv) == 4 and sys.argv[2] != "--installed-deb-root"):
        raise SystemExit(__doc__)
    installed = Path(sys.argv[3]).resolve(strict=True) if len(sys.argv) == 4 else None
    image = Path(sys.argv[1]).resolve(strict=True)
    script = Path(__file__).resolve()
    root = Path(tempfile.mkdtemp(prefix="patina-appimage-startup-"))
    print("EVIDENCE=" + str(root), flush=True)
    # A private copy remains visible after /tmp is replaced in each namespace.
    import shutil
    shutil.copy2(image, root / "candidate.AppImage")
    shutil.copy2(script, root / "runner.py")
    manifest = {"passed": False, "sha256": hashlib.sha256(image.read_bytes()).hexdigest(),
                "scope": "Packaged Desktop startup and real process handoff; fixture systemd; no installed systemd or login", "cases": []}
    if installed:
        for source, target in [(installed / "usr/bin/patinad", root / "deb-patinad"),
                               (installed / "usr/bin/Patina", root / "deb-desktop"),
                               (installed / "usr/lib/systemd/user/patinad.service", root / "deb-unit")]:
            if source.is_symlink() or not source.is_file():
                raise RuntimeError("Expected a regular private dpkg payload: " + str(source))
            shutil.copy2(source, target)
        manifest["deb_daemon_sha256"] = hashlib.sha256((root / "deb-patinad").read_bytes()).hexdigest()
        manifest["installed_deb_root"] = str(installed)
    try:
        for case in ["standalone", "deb", "custom", "mismatch", "handoff"] + (["deb-handoff"] if installed else []):
            directory = root / case
            directory.mkdir(mode=0o700)
            units = directory / "packaged-units"
            units.mkdir()
            bus_config = directory / "bus.conf"
            bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>'
                                  '<policy context="default"><allow send_destination="*"/>'
                                  '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
            if case == "deb-handoff":
                shutil.copy2(root / "deb-unit", units / "patinad.service")
            elif case == "deb":
                (units / "patinad.service").write_text("# Packaged-unit presence fixture\n")
            args = ["bwrap", "--die-with-parent", "--unshare-all", "--ro-bind", "/", "/",
                    "--tmpfs", "/tmp", "--dir", "/tmp/.X11-unix", "--bind", str(root), str(root), "--proc", "/proc", "--dev", "/dev",
                    "--ro-bind", str(units), "/usr/lib/systemd/user"]
            if Path("/lib/systemd/user").resolve() != Path("/usr/lib/systemd/user").resolve():
                args += ["--ro-bind", str(units), "/lib/systemd/user"]
            if case == "deb-handoff":
                args += ["--ro-bind", str(root / "deb-patinad"), "/usr/bin/patinad"]
                args += ["--ro-bind", str(root / "deb-desktop"), "/usr/bin/Patina"]
            args += ["--", "dbus-run-session", "--config-file=" + str(bus_config), "--", "xvfb-run", "-a", "-s", "-screen 0 1280x720x24 -extension GLX", "-e", str(directory / "xvfb.log"), "/usr/bin/python3",
                     str(root / "runner.py"), "--inner", str(directory), str(root / "candidate.AppImage"), case]
            with (directory / "runner.log").open("w") as log:
                subprocess.run(args, env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"},
                               stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)
            manifest["cases"].append(json.loads((directory / "result.json").read_text()))
            print(case + " passed", flush=True)
        manifest["passed"] = True
    finally:
        (root / "result.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
