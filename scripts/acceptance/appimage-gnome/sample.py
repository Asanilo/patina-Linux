"""Prove extension-to-daemon recording using one synthetic window in the test VM."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time
import dbus

assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'
os.environ['WAYLAND_DISPLAY'] = 'wayland-0'
os.environ['GDK_BACKEND'] = 'wayland'
bus = dbus.SessionBus()
tracker = bus.get_object('org.patina.WindowTracker1', '/org/patina/WindowTracker1')
run_id = sys.argv[1] if len(sys.argv) == 2 else ''
assert len(sys.argv) in (1, 2) and all(c.isalnum() or c in '-_' for c in run_id)
prefix = f'sample-{run_id}' if run_id else 'sample'
title = 'Patina isolated GNOME sampling witness' + (f' {run_id}' if run_id else '')
code = f"""import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk
Gtk.init([])
w=Gtk.Window(title={title!r})
w.set_default_size(400,200)
w.add(Gtk.Label(label='Synthetic acceptance window'))
w.connect('destroy',Gtk.main_quit)
w.show_all();w.present();Gtk.main()
"""
output = Path.home() / 'acceptance'
with (output / f'{prefix}-window.log').open('x') as log:
    child = subprocess.Popen(['python3', '-c', code], stdout=log, stderr=subprocess.STDOUT)
    try:
        snapshot = None
        for _ in range(40):
            snapshot = tracker.GetSnapshot(dbus_interface='org.patina.WindowTracker1', timeout=3)
            if snapshot[1] == 1 and snapshot[2] == title and snapshot[5] == child.pid:
                break
            time.sleep(.5)
        else:
            raise RuntimeError('Synthetic window did not become the GNOME foreground window')
        recorded = 0
        for _ in range(30):
            with sqlite3.connect(f'file:{Path.home()}/.local/share/Patina/patina.db?mode=ro', uri=True) as db:
                recorded = db.execute('SELECT count(*) FROM sessions WHERE window_title = ?', (title,)).fetchone()[0]
            if recorded:
                break
            time.sleep(1)
        assert recorded, 'Daemon did not record the real GNOME extension window'
        (output / f'{prefix}.json').write_text(json.dumps({'passed': True, 'witness_pid': child.pid,
            'extension_protocol': 1, 'window_pid_matched': True, 'synthetic_sessions': recorded}, indent=2))
        print('Real GNOME foreground window recorded by the standalone AppImage daemon')
    finally:
        child.terminate()
        child.wait(timeout=5)
