"""Guest-only daemon crash recovery with one synthetic Wayland window."""

import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import time
import urllib.request
import uuid

import dbus


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'
os.environ['WAYLAND_DISPLAY'] = 'wayland-0'
os.environ['GDK_BACKEND'] = 'wayland'

data = Path.home() / '.local/share/Patina'
output = Path.home() / 'acceptance'
run_id = uuid.uuid4().hex[:8]
title = f'Patina isolated daemon restart witness {run_id}'
tracker = dbus.SessionBus().get_object('org.patina.WindowTracker1', '/org/patina/WindowTracker1')


def wait_for(check, seconds=40):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.5)
    raise RuntimeError('Timed out: ' + check.__name__)


def service():
    lines = subprocess.check_output(
        ['systemctl', '--user', 'show', 'patinad.service', '--no-pager', '-p',
         'MainPID,InvocationID,NRestarts,ActiveState'], text=True).splitlines()
    return dict(line.split('=', 1) for line in lines)


def capabilities():
    token = (data / 'api_token').read_text().strip()
    request = urllib.request.Request('http://127.0.0.1:14840/api/v1/capabilities',
                                     headers={'Authorization': 'Bearer ' + token})
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3) as response:
        return json.load(response)['data']


def ready_after(previous):
    current = service()
    if current['ActiveState'] != 'active' or current['InvocationID'] == previous['InvocationID']:
        return None
    try:
        caps = capabilities()
    except (OSError, ValueError):
        return None
    if caps['tracking']['owned'] and caps['tracking']['ready']:
        return current
    return None


def read_state():
    with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
        assert db.execute('PRAGMA foreign_key_check').fetchone() is None
        last = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('__tracker_last_successful_sample_ms','__tracker_last_heartbeat_ms')"))
        rows = db.execute('SELECT start_time,end_time FROM sessions WHERE window_title=? ORDER BY start_time,id', (title,)).fetchall()
    return last, rows


code = f"""import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk
Gtk.init([])
w=Gtk.Window(title={title!r})
w.set_default_size(400,200)
w.show_all();w.present();Gtk.main()
"""
assert not read_state()[1], 'fault witness already exists in this guest'
window_started_ms = int(time.time() * 1000)
with (output / f'fault-window-{run_id}.log').open('x') as log:
    child = subprocess.Popen(['python3', '-c', code], stdout=log, stderr=subprocess.STDOUT)
    try:
        wait_for(lambda: (value if (value := tracker.GetSnapshot(
            dbus_interface='org.patina.WindowTracker1', timeout=3))[1] == 1 and
            value[2] == title and value[5] == child.pid else None))
        initial = service()
        before_sample, before_rows = wait_for(lambda: (state if (state := read_state())[1] else None))
        assert before_rows
        os.kill(int(initial['MainPID']), signal.SIGKILL)
        recovered = wait_for(lambda: ready_after(initial), 60)
        time.sleep(15)
        after_sample, rows = read_state()
        assert int(after_sample['__tracker_last_successful_sample_ms']) > int(
            before_sample['__tracker_last_successful_sample_ms'])
        pids = subprocess.check_output(['pgrep', '-u', '1000', '-x', 'patinad'], text=True).splitlines()
        assert len(pids) == 1 and pids[0] == recovered['MainPID']
        now_ms = int(time.time() * 1000)
        assert len(rows) >= 2, rows
        intervals = [(start, end if end is not None else now_ms) for start, end in rows]
        assert all(start <= end for start, end in intervals)
        assert all(left[1] <= right[0] for left, right in zip(intervals, intervals[1:]))
        duration = sum(end - start for start, end in intervals)
        assert duration <= now_ms - window_started_ms + 5000
        result = {'passed': True, 'run_id': run_id,
                  'old_invocation': initial['InvocationID'],
                  'new_invocation': recovered['InvocationID'], 'old_pid': initial['MainPID'],
                  'new_pid': recovered['MainPID'], 'single_daemon_owner': True,
                  'synthetic_session_count': len(rows), 'non_overlapping_sessions': True,
                  'duration_bounded_by_window': True, 'last_successful_sample_advanced': True,
                  'database_integrity': 'ok', 'scope': 'isolated GNOME VM; no real suspend or public updater'}
        (output / f'fault-{run_id}.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps({key: result[key] for key in ('passed','single_daemon_owner',
            'synthetic_session_count','non_overlapping_sessions','last_successful_sample_advanced')}))
    finally:
        if child.poll() is None:
            child.terminate()
        child.wait(timeout=5)
