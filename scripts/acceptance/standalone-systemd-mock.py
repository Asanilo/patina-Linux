#!/usr/bin/env python3
"""Private D-Bus systemd adapter fixture; never connect this to the host user bus."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

fixture_path = Path(sys.argv[1]).resolve(strict=True)
fixture = json.loads(fixture_path.read_text())
assert os.environ.get("DBUS_SESSION_BUS_ADDRESS") == fixture["private_bus"]
assert fixture["private_bus"] != os.environ.get("PATINA_ACCEPTANCE_PARENT_BUS", "")
root = fixture_path.parent
counts = {"start": 0, "stop": 0, "reload": 0}
child = None
log = (root / "daemon.log").open("w")


def save():
    (root / "manager-counts.json").write_text(json.dumps(counts))


def stop_child():
    global child
    if child is not None and child.poll() is None:
        child.send_signal(signal.SIGINT)
        try:
            child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
    child = None


DBusGMainLoop(set_as_default=True)
bus = dbus.bus.BusConnection(fixture["private_bus"])
name = dbus.service.BusName("org.freedesktop.systemd1", bus=bus, do_not_queue=True)
UNIT_PATH = "/org/freedesktop/systemd1/unit/patinad_2eservice"


class Properties(dbus.service.Object):
    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        return self.values()[str(prop)]

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return self.values()


class Manager(Properties):
    def values(self):
        return {"Environment": dbus.Array(["HOME=" + os.environ["HOME"],
                "XDG_CONFIG_HOME=" + fixture["config"], "XDG_DATA_HOME=" + fixture["data"]], signature="s")}

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="s", out_signature="s")
    def GetUnitFileState(self, unit):
        assert unit == "patinad.service"
        return "disabled"

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="s", out_signature="o")
    def LoadUnit(self, unit):
        assert unit == "patinad.service"
        return UNIT_PATH

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="s", out_signature="o")
    def GetUnit(self, unit):
        return self.LoadUnit(unit)

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="", out_signature="")
    def Reload(self):
        counts["reload"] += 1
        save()

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StartUnit(self, unit, mode):
        global child
        assert unit == "patinad.service" and mode == "replace"
        assert Path(fixture["unit_path"]).read_text() == fixture["unit_text"]
        assert child is None or child.poll() is not None
        env = dict(os.environ, PATINA_SYSTEMD_SERVICE="patinad.service", INVOCATION_ID="a" * 32)
        child = subprocess.Popen([fixture["binary"], "--profile", "production", "--serve-api", "--track"],
                                 env=env, stdout=log, stderr=subprocess.STDOUT)
        counts["start"] += 1
        save()
        return "/org/freedesktop/systemd1/job/1"

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StopUnit(self, unit, mode):
        assert unit == "patinad.service" and mode == "replace"
        counts["stop"] += 1
        stop_child()
        save()
        return "/org/freedesktop/systemd1/job/2"


class Unit(Properties):
    def values(self):
        active = child is not None and child.poll() is None
        return {"FragmentPath": dbus.String(fixture["unit_path"]), "DropInPaths": dbus.Array([], signature="s"),
                "ActiveState": dbus.String("active" if active else "inactive"),
                "SubState": dbus.String("running" if active else "dead")}


manager = Manager(bus, "/org/freedesktop/systemd1")
unit = Unit(bus, UNIT_PATH)
loop = GLib.MainLoop()
signal.signal(signal.SIGTERM, lambda *_: loop.quit())
signal.signal(signal.SIGINT, lambda *_: loop.quit())
save()
(root / "manager-ready").write_text("ready")
try:
    loop.run()
finally:
    stop_child()
    log.close()
