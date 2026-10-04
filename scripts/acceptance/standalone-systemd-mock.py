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
loaded_source = fixture.get("source")
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


def command_definition():
    if loaded_source:
        return loaded_source["argv"], loaded_source["environment"], loaded_source["unit_path"]
    return ([fixture["binary"], "--profile", "production", "--serve-api", "--track"],
            ["PATINA_SYSTEMD_SERVICE=patinad.service", "XDG_CONFIG_HOME=" + fixture["config"],
             "XDG_DATA_HOME=" + fixture["data"]], fixture["unit_path"])


def start_child():
    global child
    assert child is None or child.poll() is not None
    argv, environment, _ = command_definition()
    env = dict(os.environ, INVOCATION_ID="a" * 32)
    env.update(entry.split("=", 1) for entry in environment)
    child = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT)


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
        return "enabled" if fixture.get("enabled", False) else "disabled"

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="s", out_signature="o")
    def LoadUnit(self, unit):
        assert unit == "patinad.service"
        return UNIT_PATH

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="s", out_signature="o")
    def GetUnit(self, unit):
        return self.LoadUnit(unit)

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="", out_signature="")
    def Reload(self):
        global loaded_source
        counts["reload"] += 1
        save()
        if fixture.get("fail_first_reload") and counts["reload"] == 1:
            raise dbus.exceptions.DBusException("injected reload failure", name="org.freedesktop.systemd1.Error.Failed")
        assert Path(fixture["unit_path"]).read_text() == fixture["unit_text"]
        loaded_source = None

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StartUnit(self, unit, mode):
        assert unit == "patinad.service" and mode == "replace"
        assert Path(fixture["unit_path"]).read_text() == fixture["unit_text"]
        start_child()
        counts["start"] += 1
        save()
        return "/org/freedesktop/systemd1/job/1"

    @dbus.service.method("org.freedesktop.systemd1.Manager", in_signature="ss", out_signature="o")
    def StopUnit(self, unit, mode):
        assert unit == "patinad.service" and mode == "replace"
        counts["stop"] += 1
        stop_child()
        if fixture.get("change_unit_after_stop") and counts["stop"] == 1:
            Path(fixture["unit_path"]).write_text("# externally edited during stop\n")
        save()
        return "/org/freedesktop/systemd1/job/2"


class Unit(Properties):
    def values(self):
        active = child is not None and child.poll() is None
        argv, environment, fragment = command_definition()
        overrides_path = root / "inspection-overrides.json"
        overrides = json.loads(overrides_path.read_text()) if overrides_path.exists() else {}
        if overrides.get("environment"):
            environment = [*environment, "OTHER_PROFILE=fixture"]
        entry = dbus.Struct([dbus.String(argv[0]), dbus.Array(argv, signature="s"), dbus.Boolean(False),
                             *[dbus.UInt64(0) for _ in range(4)], dbus.UInt32(0), dbus.Int32(0), dbus.Int32(0)], signature="sasbttttuii")
        return {"FragmentPath": dbus.String(fragment), "DropInPaths": dbus.Array(["/custom.conf"] if overrides.get("drop_in") else [], signature="s"),
                "MainPID": dbus.UInt32(1 if overrides.get("pid") else child.pid if active else 0),
                "ExecStart": dbus.Array([entry], signature="(sasbttttuii)"),
                "Environment": dbus.Array(environment, signature="s"),
                "EnvironmentFiles": dbus.Array([], signature="(sb)"),
                "ActiveState": dbus.String("active" if active else "inactive"),
                "SubState": dbus.String("running" if active else "dead")}


manager = Manager(bus, "/org/freedesktop/systemd1")
unit = Unit(bus, UNIT_PATH)
loop = GLib.MainLoop()
signal.signal(signal.SIGTERM, lambda *_: loop.quit())
signal.signal(signal.SIGINT, lambda *_: loop.quit())
if loaded_source:
    start_child()
save()
(root / "manager-ready").write_text("ready")
try:
    loop.run()
finally:
    stop_child()
    log.close()
