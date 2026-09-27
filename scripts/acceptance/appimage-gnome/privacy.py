"""Guest-only real GNOME window check for a saved captureTitle=false override."""

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
os.umask(0o077)
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'
os.environ['WAYLAND_DISPLAY'] = 'wayland-0'
os.environ['GDK_BACKEND'] = 'wayland'

database = Path.home() / '.local/share/Patina/patina.db'
run_id = sys.argv[1] if len(sys.argv) == 2 else ''
assert len(sys.argv) in (1, 2) and all(c.isalnum() or c in '-_' for c in run_id)
prefix = 'privacy' + (f'-{run_id}' if run_id else '')
output = Path.home() / f'acceptance/{prefix}.json'
assert not output.exists()
with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as db:
    raw = db.execute("SELECT value FROM settings WHERE key='__app_override::python3'").fetchone()
    assert raw and json.loads(raw[0]).get('captureTitle') is False

title = 'Patina isolated privacy witness'
code = f"""import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk
Gtk.init([])
w=Gtk.Window(title={title!r})
w.set_default_size(400,200)
w.add(Gtk.Label(label='Synthetic privacy witness'))
w.connect('destroy',Gtk.main_quit)
w.show_all();w.present();Gtk.main()
"""
tracker = dbus.SessionBus().get_object('org.patina.WindowTracker1', '/org/patina/WindowTracker1')
started_at_ms = int(time.time() * 1000) - 1000
with (Path.home() / f'acceptance/{prefix}-window.log').open('x') as log:
    child = subprocess.Popen(['python3', '-c', code], stdout=log, stderr=subprocess.STDOUT)
    try:
        for _ in range(40):
            snapshot = tracker.GetSnapshot(dbus_interface='org.patina.WindowTracker1', timeout=3)
            if snapshot[1] == 1 and snapshot[2] == title and snapshot[5] == child.pid:
                break
            time.sleep(.5)
        else:
            raise RuntimeError('synthetic native window did not become the foreground')

        recorded_id = None
        for _ in range(30):
            with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as db:
                row = db.execute(
                    "SELECT id, window_title FROM sessions WHERE exe_name='python3' "
                    "AND start_time>=? ORDER BY id DESC LIMIT 1", (started_at_ms,)
                ).fetchone()
                if row:
                    assert not row[1], 'window title was stored despite the privacy override'
                    recorded_id = row[0]
                    break
            time.sleep(1)
        assert recorded_id is not None, 'no synthetic native window session was recorded'
    finally:
        child.terminate()
        child.wait(timeout=5)

with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as db:
    assert db.execute('SELECT COUNT(*) FROM session_title_samples WHERE session_id=?',
                      (recorded_id,)).fetchone()[0] == 0
    assert db.execute('PRAGMA quick_check').fetchone()[0] == 'ok'

with output.open('x') as stream:
    json.dump({'passed': True, 'real_wayland_window_pid_matched': True,
               'capture_title_setting': False, 'session_recorded_without_title': True,
               'title_samples_for_session': 0, 'database_integrity': 'ok'}, stream, indent=2)
    stream.write('\n')
print('REAL_GNOME_TITLE_PRIVACY_PASS')
