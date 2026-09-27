"""Guest-only GNOME extension disable/re-enable recovery acceptance."""

import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
import urllib.request

import dbus


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'

data = Path.home() / '.local/share/Patina'
output = Path.home() / 'acceptance/extension-fault.json'
assert not output.exists()
bus = dbus.SessionBus()
names = ('org.patina.WindowTracker', 'org.patina.WindowTracker1')


def wait_for(check, seconds=30):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.5)
    raise RuntimeError('Timed out: ' + check.__name__)


def service():
    lines = subprocess.check_output(['systemctl', '--user', 'show', 'patinad.service',
                                     '-p', 'ActiveState,MainPID,InvocationID'], text=True).splitlines()
    return dict(line.split('=', 1) for line in lines)


def last_successful_sample():
    with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
        row = db.execute("SELECT value FROM settings WHERE key='__tracker_last_successful_sample_ms'").fetchone()
    return int(row[0])


def diagnostics():
    token = (data / 'api_token').read_text().strip()
    request = urllib.request.Request('http://127.0.0.1:14840/api/v1/diagnostics',
                                     headers={'Authorization': 'Bearer ' + token})
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3) as response:
        return json.load(response)['data']


def set_enabled(value):
    subprocess.run(['gsettings', 'set', 'org.gnome.shell', 'enabled-extensions', value], check=True)


initial = service()
assert initial['ActiveState'] == 'active'
assert all(bus.name_has_owner(name) for name in names)
original = subprocess.check_output(['gsettings', 'get', 'org.gnome.shell', 'enabled-extensions'], text=True).strip()
assert 'patina-window-tracker@patina' in original
try:
    set_enabled('[]')
    wait_for(lambda: all(not bus.name_has_owner(name) for name in names))
    time.sleep(2)
    unavailable_sample = last_successful_sample()
    time.sleep(10)
    assert last_successful_sample() == unavailable_sample
    unavailable = diagnostics()['window_tracking']
    assert unavailable['status'] != 'available'
finally:
    set_enabled(original)

wait_for(lambda: all(bus.name_has_owner(name) for name in names))
wait_for(lambda: last_successful_sample() > unavailable_sample)
recovered = diagnostics()['window_tracking']
assert recovered['status'] == 'available'
assert service()['InvocationID'] == initial['InvocationID']
result = {'passed': True, 'both_protocol_names_released': True,
          'success_timestamp_stopped_while_unavailable': True,
          'diagnostic_during_disable': {'status': unavailable['status'],
                                        'reason': unavailable.get('reason')},
          'both_protocol_names_recovered': True,
          'diagnostic_after_reenable': recovered['status'],
          'daemon_invocation_unchanged': True,
          'database_integrity': 'ok', 'scope': 'isolated GNOME VM; foreground witness checked separately'}
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: result[key] for key in ('passed','both_protocol_names_released',
    'success_timestamp_stopped_while_unavailable','both_protocol_names_recovered',
    'diagnostic_after_reenable')}))
