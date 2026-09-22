#!/usr/bin/env python3
"""Run the packaged GNOME 42 extension in a private headless Shell, never the host Shell."""
import ast
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

UUID = "patina-window-tracker@patina"
DRIVER = "patina-acceptance@patina"
REPO = Path(__file__).resolve().parents[1]

DRIVER_JS = r"""
const {Gio} = imports.gi;
const Main = imports.ui.main;
let owner, object;
function init() {}
function enable() {
    owner = Gio.bus_own_name(Gio.BusType.SESSION, 'org.patina.Acceptance', 0, null,
        connection => {
            object = Gio.DBusExportedObject.wrapJSObject(`<node>
              <interface name="org.patina.Acceptance">
                <method name="Act"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
              </interface></node>`, {
                Act(action) {
                    switch (action) {
                    case 'overview': Main.overview.show(); break;
                    case 'desktop': Main.overview.hide(); break;
                    case 'lock': Main.screenShield.lock(false); break;
                    case 'unlock': Main.screenShield.deactivate(false); break;
                    case 'disable': Main.extensionManager.disableExtension('patina-window-tracker@patina'); break;
                    case 'enable': Main.extensionManager.enableExtension('patina-window-tracker@patina'); break;
                    }
                    return JSON.stringify({locked: Main.screenShield.locked,
                        active: Main.screenShield.active, mode: Main.sessionMode.currentMode});
                }
            });
            object.export(connection, '/org/patina/Acceptance');
        }, null);
}
function disable() {
    if (object) object.unexport();
    if (owner) Gio.bus_unown_name(owner);
}
"""


def rpc(name, method, *args):
    result = subprocess.run(["gdbus", "call", "--session", "--timeout", "3",
        "--dest", name, "--object-path", "/" + name.replace(".", "/"),
        "--method", name + "." + method, *args], capture_output=True, text=True, timeout=5)
    if result.returncode:
        raise RuntimeError(result.stderr.strip())
    return ast.literal_eval(re.sub(r"\buint(?:32|64) ", "", result.stdout))


def wait_for(check, description, seconds=15):
    end = time.monotonic() + seconds
    last = None
    while time.monotonic() < end:
        try:
            value = check()
            if value:
                return value
            last = value
        except Exception as error:
            last = error
        time.sleep(0.2)
    raise RuntimeError(f"Timed out: {description}; {last}")


def inner(root):
    # The session and 'system' buses are the same private bus without activation.
    os.environ["DBUS_SYSTEM_BUS_ADDRESS"] = os.environ["DBUS_SESSION_BUS_ADDRESS"]
    os.environ["WAYLAND_DISPLAY"] = "patina-acceptance"
    processes = []
    with (root / "shell.log").open("w") as log:
        try:
            # GDM capability fixture only; no authentication or host logind calls.
            gdm = root / "gdm.js"
            gdm.write_text("""const {Gio, GLib} = imports.gi;
const loop = GLib.MainLoop.new(null, false);
let object;
Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnome.DisplayManager', 0, null, connection => {
  object = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="org.gnome.DisplayManager.Manager">
    <property name="Version" type="s" access="read"/>
    </interface></node>`, {Version: '42.0'});
  object.export(connection, '/org/gnome/DisplayManager/Manager');
}, null);
loop.run();
""")
            processes.append(subprocess.Popen(["gjs", str(gdm)], stdout=log, stderr=log))
            time.sleep(0.5)
            shell = subprocess.Popen(["gnome-shell", "--headless", "--wayland",
                "--virtual-monitor", "1280x720", "--no-x11", "--sm-disable",
                "--wayland-display", "patina-acceptance"], stdout=log, stderr=log)
            processes.append(shell)
            act = lambda action: rpc("org.patina.Acceptance", "Act", action)
            snapshot = lambda: rpc("org.patina.WindowTracker1", "GetSnapshot")
            legacy = lambda: rpc("org.patina.WindowTracker", "GetFocusedWindow")
            wait_for(lambda: act("desktop"), "driver ready", 30)
            win = root / "window.js"
            win.write_text("""imports.gi.versions.Gtk = '3.0';
const {Gtk, GLib} = imports.gi;
GLib.set_prgname('patina-synthetic'); Gtk.init(null);
const win = new Gtk.Window({title: 'Patina synthetic acceptance'});
win.set_default_size(500, 300); win.show_all(); win.present(); Gtk.main();
""")
            processes.append(subprocess.Popen(["gjs", str(win)], stdout=log, stderr=log))
            def focused():
                act("desktop")
                value = snapshot()
                (root / "last-snapshot.json").write_text(json.dumps(value))
                return value if value[1] == 1 else None
            current = wait_for(focused, "synthetic focus")
            assert current[2] == "Patina synthetic acceptance", current
            old = legacy()
            assert old[0] == current[2] and str(old[4]) == current[6], (old, current)
            act("overview")
            wait_for(lambda: snapshot() == (1, 0, "", "", "", 0, ""), "overview empty")
            assert legacy() == ("", "", "", 0, 0)
            act("desktop")
            wait_for(lambda: snapshot()[1] == 1, "overview recovery")
            lock_state = act("lock")
            wait_for(lambda: snapshot() == (1, 2, "", "", "", 0, ""), "locked empty")
            assert legacy() == ("", "", "", 0, 0)
            act("unlock")
            wait_for(lambda: snapshot()[1] == 1, "unlock recovery")
            for _ in range(3):
                act("disable")
                for name in ["org.patina.WindowTracker", "org.patina.WindowTracker1"]:
                    result = subprocess.run(["gdbus", "call", "--session", "--dest",
                        "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
                        "--method", "org.freedesktop.DBus.NameHasOwner", name],
                        capture_output=True, text=True, check=True)
                    assert result.stdout.strip() == "(false,)", result.stdout
                act("enable")
                wait_for(lambda: snapshot()[1] == 1, "re-enable")
                assert legacy()[0] == "Patina synthetic acceptance"
            report = {"shell": subprocess.check_output(["gnome-shell", "--version"], text=True).strip(),
                "extension_version": json.loads((root / "data/gnome-shell/extensions" / UUID / "metadata.json").read_text())["version"],
                "extension_sha256": hashlib.sha256((root / "data/gnome-shell/extensions" / UUID / "extension.js").read_bytes()).hexdigest(),
                "dual_protocol": True, "overview_recovery": True,
                "lock_recovery": True, "lock_state": lock_state, "disable_enable_cycles": 3,
                "isolated_session": True, "gdm_capability_fixture": True, "production_login_or_suspend": False}
            (root / "result.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report), flush=True)
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
            for process in reversed(processes):
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def main():
    root = Path(tempfile.mkdtemp(prefix="patina-gnome-acceptance-"))
    print(f"Evidence: {root}", flush=True)
    for directory in ["home", "run", "data", "config", "cache"]:
        (root / directory).mkdir(mode=0o700)
    extensions = root / "data/gnome-shell/extensions"
    shutil.copytree(REPO / "dist/extensions/gnome-shell" / UUID, extensions / UUID)
    driver = extensions / DRIVER
    driver.mkdir()
    (driver / "metadata.json").write_text(json.dumps({"uuid": DRIVER,
        "name": "Private acceptance driver", "description": "Private test only",
        "shell-version": ["42"], "session-modes": ["user", "unlock-dialog"], "version": 1}))
    (driver / "extension.js").write_text(DRIVER_JS)
    # Keyfile settings avoid dconf activation and never read host settings.
    settings = root / "config/glib-2.0/settings"
    settings.mkdir(parents=True)
    (settings / "keyfile").write_text(f"[org/gnome/shell]\nenabled-extensions=['{UUID}', '{DRIVER}']\ndisable-user-extensions=false\n[org/gnome/desktop/interface]\nenable-animations=false\n")
    config = root / "bus.conf"
    config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + str(root) +
        '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/>' +
        '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": str(root / "home"),
        "XDG_RUNTIME_DIR": str(root / "run"), "XDG_DATA_HOME": str(root / "data"),
        "XDG_CONFIG_HOME": str(root / "config"), "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_SESSION_TYPE": "wayland", "XDG_CURRENT_DESKTOP": "GNOME",
        "XDG_SESSION_ID": "patina-private-test",
        "GSETTINGS_BACKEND": "keyfile", "NO_AT_BRIDGE": "1", "LIBGL_ALWAYS_SOFTWARE": "1"}
    child = subprocess.Popen(["dbus-run-session", "--config-file", str(config), "--",
        sys.executable, str(Path(__file__).resolve()), "--inner", str(root)],
        env=env, start_new_session=True)
    try:
        return child.wait(timeout=120)
    finally:
        # All children are confined to the new process group, including bus helpers.
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--inner":
        root = Path(sys.argv[2]).resolve()
        assert root.name.startswith("patina-gnome-acceptance-")
        assert os.environ.get("HOME") == str(root / "home")
        assert str(root) in os.environ.get("DBUS_SESSION_BUS_ADDRESS", "")
        inner(root)
    else:
        sys.exit(main())
