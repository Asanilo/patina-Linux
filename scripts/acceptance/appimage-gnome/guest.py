"""Guest-only AppImage acceptance: real GDM/logind, no Patina DEB.

Run as tester in the disposable KVM guest. `first` launches the candidate;
`login` only observes GNOME's generated autostart after a fresh guest login.
Both finish through the application's Quit menu and verify background sampling.
"""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
import dbus

assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert not Path('/usr/bin/patinad').exists()
assert not Path('/usr/lib/systemd/user/patinad.service').exists()
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
os.umask(0o077)
home = Path.home()
data = home / '.local/share/Patina'
output = home / 'acceptance'
output.mkdir(exist_ok=True)
image = home / 'Applications/Patina.AppImage'
phase, expected_version = sys.argv[1:]
assert phase in ['first', 'login']
os.environ['XDG_RUNTIME_DIR'] = '/run/user/1000'
os.environ['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=/run/user/1000/bus'
manager = subprocess.check_output(['systemctl', '--user', 'show-environment'], text=True)
for line in manager.splitlines():
    key, _, value = line.partition('=')
    if key in ['DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY', 'XDG_CURRENT_DESKTOP', 'XDG_SESSION_TYPE']:
        os.environ[key] = value
bus = dbus.SessionBus()
system_bus = dbus.SystemBus()
login_user = system_bus.get_object('org.freedesktop.login1', '/org/freedesktop/login1/user/_1000')
session_id, session_path = login_user.Get('org.freedesktop.login1.User', 'Display', dbus_interface='org.freedesktop.DBus.Properties')
session = system_bus.get_object('org.freedesktop.login1', session_path).GetAll(
    'org.freedesktop.login1.Session', dbus_interface='org.freedesktop.DBus.Properties')
assert session['Class'] == 'user' and session['Type'] == 'wayland'
assert session['Active'] and not session['Remote'] and session['Service'] == 'gdm-autologin'
session_facts = {key: str(session[key]) for key in ['Id', 'Type', 'Class', 'Service', 'Desktop', 'Timestamp']}
session_facts['boot_id'] = Path('/proc/sys/kernel/random/boot_id').read_text().strip()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def wait_for(check, seconds=90):
    end = time.monotonic() + seconds
    last = None
    while time.monotonic() < end:
        try:
            value = check()
            if value:
                return value
        except (OSError, ValueError, AssertionError) as error:
            last = str(error)
        time.sleep(.5)
    raise RuntimeError(f'{check.__name__} timed out: {last}')


def api(name):
    token = (data / 'api_token').read_text().strip()
    req = urllib.request.Request('http://127.0.0.1:14840/api/v1/' + name,
                                 headers={'Authorization': 'Bearer ' + token})
    with opener.open(req, timeout=3) as response:
        return json.load(response)['data']


def service():
    return dict(line.split('=', 1) for line in subprocess.check_output(
        ['systemctl', '--user', 'show', 'patinad.service', '-p',
         'MainPID,InvocationID,NRestarts,ActiveState,UnitFileState,FragmentPath'], text=True).splitlines())


def ready():
    caps = api('capabilities')
    assert caps['runtime_host'] == 'daemon' and caps['server_version'] == expected_version
    assert caps['tracking']['owned'] and caps['tracking']['ready']
    state = service()
    assert state['ActiveState'] == 'active' and state['UnitFileState'] == 'enabled'
    assert state['FragmentPath'] == str(home / '.config/systemd/user/patinad.service')
    return state


def desktop():
    matches = []
    for p in Path('/proc').glob('[0-9]*/exe'):
        try:
            exe = os.readlink(p)
            if p.parent.stat().st_uid == os.getuid() and exe.endswith('/usr/bin/Patina'):
                matches.append({'pid': int(p.parent.name), 'exe': exe})
        except OSError:
            pass
    assert len(matches) <= 1, 'multiple Desktop processes'
    return matches


def quit_desktop(pid):
    manager = bus.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus')
    matches = []
    for name in bus.list_names():
        if not name.startswith(':'):
            continue
        try:
            if int(manager.GetConnectionUnixProcessID(name)) != pid:
                continue
            pending, seen = ['/'], set()
            while pending and len(seen) < 80:
                path = pending.pop()
                if path in seen:
                    continue
                seen.add(path)
                obj = bus.get_object(name, path, introspect=False)
                tree = ET.fromstring(obj.Introspect(dbus_interface='org.freedesktop.DBus.Introspectable', timeout=3))
                if any(e.attrib['name'] == 'com.canonical.dbusmenu' for e in tree.findall('interface')):
                    _, layout = obj.GetLayout(0, -1, ['label', 'enabled', 'visible'], dbus_interface='com.canonical.dbusmenu', timeout=3)
                    rows = [layout]
                    while rows:
                        row = rows.pop()
                        if str(row[1].get('label', '')).replace('_', '') in ['Quit', 'Quit Patina', 'Exit', '退出应用']:
                            matches.append((name, path, int(row[0])))
                        rows.extend(row[2])
                pending.extend(path.rstrip('/') + '/' + e.attrib['name'] for e in tree.findall('node'))
        except dbus.DBusException:
            pass
    assert len(set(matches)) == 1, matches
    name, path, item = matches[0]
    bus.get_object(name, path, introspect=False).Event(dbus.Int32(item), 'clicked',
        dbus.String('', variant_level=1), dbus.UInt32(0), dbus_interface='com.canonical.dbusmenu', timeout=3)
    wait_for(lambda: not desktop(), 15)


if phase == 'first':
    assert not data.exists() and not desktop()
    with (output / 'first-desktop.log').open('x') as log:
        subprocess.Popen([str(image)], stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
else:
    previous = json.loads((output / 'first.json').read_text())
    assert (session_facts['boot_id'], session_facts['Id']) != (previous['session']['boot_id'], previous['session']['Id'])
    assert int(session_facts['Timestamp']) > int(previous['session']['Timestamp'])
initial = wait_for(ready)
running = wait_for(desktop)[0]
assert '/.mount_' in running['exe'], 'expected actual FUSE Desktop'
args = Path(f'/proc/{running["pid"]}/cmdline').read_bytes().split(b'\0')
if phase == 'login':
    assert b'--autostart' in args, 'Desktop was not launched through generated autostart'
    assert initial['InvocationID'] != previous['initial']['InvocationID'], 'expected guest cold start'
environment = dict(x.split('=', 1) for x in Path(f'/proc/{running["pid"]}/environ').read_bytes().decode().split('\0') if '=' in x)
assert environment['APPIMAGE'] == str(image)
assert environment['GDK_BACKEND'] == 'x11', 'record backend changes explicitly'
assert (home / '.config/autostart/Patina.desktop').is_file()
time.sleep(10)
assert desktop() == [running]
quit_desktop(running['pid'])
assert ready()['InvocationID'] == initial['InvocationID']
with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
    before = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('__tracker_last_heartbeat_ms','__tracker_last_successful_sample_ms')"))
time.sleep(15)
with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
    after = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('__tracker_last_heartbeat_ms','__tracker_last_successful_sample_ms')"))
    assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
    assert db.execute('PRAGMA foreign_key_check').fetchone() is None
    preferences = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('launch_at_login','background_tracking_at_login')"))
for key in before:
    assert int(after[key]) > int(before[key])
assert set(before) == {'__tracker_last_heartbeat_ms', '__tracker_last_successful_sample_ms'}
assert not desktop() and ready()['InvocationID'] == initial['InvocationID']
result = {'passed': True, 'phase': phase, 'version': expected_version, 'session': session_facts,
          'initial': initial, 'final': service(), 'desktop': running, 'backend': environment['GDK_BACKEND'],
          'fuse': True, 'preferences': preferences, 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(),
          'sampling_advance_ms': int(after['__tracker_last_successful_sample_ms']) - int(before['__tracker_last_successful_sample_ms']),
          'quick_check': 'ok', 'foreign_key_check': 'ok', 'diagnostics': api('diagnostics')}
with (output / (phase + '.json')).open('x') as stream:
    json.dump(result, stream, indent=2)
print(json.dumps({key: result[key] for key in ['passed', 'phase', 'session', 'backend', 'sampling_advance_ms']}))
