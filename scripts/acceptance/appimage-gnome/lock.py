"""Guest-only GDM Wayland lock/unlock and tracking recovery acceptance."""

import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time

import dbus


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'

data = Path.home() / '.local/share/Patina'
output = Path.home() / 'acceptance/lock.json'
assert not output.exists()
session = subprocess.check_output(['loginctl', 'show-user', 'tester', '-p', 'Display', '--value'], text=True).strip()
properties = subprocess.check_output(['loginctl', 'show-session', session,
                                      '-p', 'Type', '-p', 'Service', '-p', 'Active'], text=True)
assert 'Type=wayland' in properties and 'Service=gdm-autologin' in properties and 'Active=yes' in properties
tracker = dbus.SessionBus().get_object('org.patina.WindowTracker1', '/org/patina/WindowTracker1')


def wait_for(check, seconds=30):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.5)
    raise RuntimeError('Timed out: ' + check.__name__)


def snapshot_state():
    return int(tracker.GetSnapshot(dbus_interface='org.patina.WindowTracker1', timeout=3)[1])


def last_successful_sample():
    with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
        row = db.execute("SELECT value FROM settings WHERE key='__tracker_last_successful_sample_ms'").fetchone()
    return int(row[0])


def invocation():
    return subprocess.check_output(['systemctl', '--user', 'show', 'patinad.service',
                                    '-p', 'InvocationID', '--value'], text=True).strip()


before = invocation()
subprocess.run(['loginctl', 'lock-session', session], check=True)
try:
    wait_for(lambda: snapshot_state() == 2)
    time.sleep(2)
    locked_sample = last_successful_sample()
    time.sleep(10)
    assert last_successful_sample() == locked_sample
finally:
    subprocess.run(['loginctl', 'unlock-session', session], check=True)

after_state = int(wait_for(lambda: (str(state) if (state := snapshot_state()) != 2 else None)))
wait_for(lambda: last_successful_sample() > locked_sample)
assert invocation() == before
result = {'passed': True, 'session_type': 'wayland', 'service': 'gdm-autologin',
          'locked_snapshot_state': 2, 'successful_sample_stopped_during_lock': True,
          'unlocked_snapshot_state': after_state,
          'successful_sample_resumed': True, 'daemon_invocation_unchanged': True,
          'database_integrity': 'ok', 'scope': 'isolated VM lock; not hardware suspend'}
with output.open('x') as stream:
    json.dump(result, stream, indent=2)
    stream.write('\n')
print(json.dumps({key: result[key] for key in ('passed','locked_snapshot_state',
    'successful_sample_stopped_during_lock','successful_sample_resumed')}))
