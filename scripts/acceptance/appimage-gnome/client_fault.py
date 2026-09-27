"""Guest-only Desktop crash/reopen acceptance; the daemon must keep recording."""

import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import time


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'

home = Path.home()
image = home / 'Applications/Patina.AppImage'
database = home / '.local/share/Patina/patina.db'
output = home / 'acceptance/client-fault.json'
assert image.is_file() and database.is_file() and not output.exists()


def wait_for(check, seconds=40):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.5)
    raise RuntimeError('Timed out: ' + check.__name__)


def desktop():
    matches = []
    for p in Path('/proc').glob('[0-9]*/exe'):
        try:
            exe = os.readlink(p)
            if p.parent.stat().st_uid == os.getuid() and exe.endswith('/usr/bin/Patina'):
                matches.append(int(p.parent.name))
        except OSError:
            pass
    assert len(matches) <= 1
    return matches


def service():
    lines = subprocess.check_output(['systemctl', '--user', 'show', 'patinad.service',
                                     '-p', 'ActiveState,MainPID,InvocationID,NRestarts'], text=True).splitlines()
    return dict(line.split('=', 1) for line in lines)


def sample_ms():
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
        row = db.execute("SELECT value FROM settings WHERE key='__tracker_last_successful_sample_ms'").fetchone()
    return int(row[0])


initial = service()
assert initial['ActiveState'] == 'active'
original = wait_for(desktop)[0]
before = sample_ms()
os.kill(original, signal.SIGKILL)
wait_for(lambda: not desktop())
time.sleep(15)
assert sample_ms() > before
assert service()['InvocationID'] == initial['InvocationID']

manager = subprocess.check_output(['systemctl', '--user', 'show-environment'], text=True)
environment = os.environ.copy()
for line in manager.splitlines():
    key, _, value = line.partition('=')
    if key in ('DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY', 'XDG_CURRENT_DESKTOP', 'XDG_SESSION_TYPE'):
        environment[key] = value
with (home / 'acceptance/client-reopen.log').open('x') as log:
    subprocess.Popen([str(image)], env=environment, stdout=log, stderr=subprocess.STDOUT,
                     start_new_session=True)
    reopened = wait_for(lambda: (matches[0] if (matches := desktop()) and matches[0] != original else None))
    assert service()['InvocationID'] == initial['InvocationID']
    os.kill(reopened, signal.SIGTERM)
    wait_for(lambda: not desktop())

assert service()['InvocationID'] == initial['InvocationID']
result = {'passed': True, 'old_desktop_pid': original, 'reopened_desktop_pid': reopened,
          'daemon_pid_unchanged': initial['MainPID'] == service()['MainPID'],
          'daemon_invocation_unchanged': True, 'no_desktop_sample_advanced': True,
          'database_integrity': 'ok', 'scope': 'isolated GNOME VM; no production UI'}
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: result[key] for key in ('passed', 'daemon_pid_unchanged',
    'daemon_invocation_unchanged', 'no_desktop_sample_advanced')}))
