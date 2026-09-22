"""Container-only real user-systemd acceptance; never run on a user's host."""
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.request

assert Path('/run/systemd/container').read_text().strip() == 'docker'
assert os.getuid() == 1000 and Path.home() == Path('/home/tester')
assert not Path('/usr/lib/systemd/user/patinad.service').exists()
assert not Path('/usr/bin/patinad').exists()
home = Path.home()
data = home / '.local/share/Patina'
output = home / 'acceptance'
output.mkdir(exist_ok=True)
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

def systemctl(*args):
    return subprocess.check_output(['systemctl', '--user', *args], text=True).strip()

def status():
    return dict(line.split('=', 1) for line in systemctl('show', 'patinad.service',
        '-p', 'MainPID', '-p', 'ActiveState', '-p', 'NRestarts', '-p', 'InvocationID',
        '-p', 'FragmentPath', '-p', 'ExecMainStatus').splitlines())

def wait_for(check, timeout=90):
    end = time.monotonic() + timeout
    error = None
    while time.monotonic() < end:
        try:
            value = check()
            if value:
                return value
        except (OSError, ValueError, AssertionError) as exc:
            error = exc
        time.sleep(.3)
    raise RuntimeError(f'Checkpoint timed out: {check.__name__}: {error}')

def ready():
    token = (data / 'api_token').read_text().strip()
    request = urllib.request.Request('http://127.0.0.1:14840/api/v1/capabilities',
                                    headers={'Authorization': 'Bearer ' + token})
    with opener.open(request, timeout=2) as response:
        capabilities = json.load(response)['data']
    assert capabilities['runtime_host'] == 'daemon'
    assert capabilities['tracking']['owned'] and capabilities['tracking']['ready']
    assert capabilities['daemon_service']['owned'] and capabilities['daemon_service']['ready']
    return status()

def integrity():
    with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
        assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
        assert db.execute('PRAGMA foreign_key_check').fetchone() is None

def desktop_running():
    found = []
    for path in Path('/proc').glob('[0-9]*/exe'):
        try:
            if os.readlink(path).endswith('/usr/bin/Patina'):
                found.append(int(path.parent.name))
        except OSError:
            pass
    return found

def close_desktop(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=5)
    wait_for(lambda: not desktop_running(), 15)

phase = sys.argv[1]
if phase == 'first':
    assert not data.exists(), 'First-install test requires a fresh container user'
    # Real systemd deliberately delays the daemon so Desktop must wait for its
    # first credential. This is a test-only drop-in, not a production unit change.
    delay = home / '.config/systemd/user/patinad.service.d/acceptance-delay.conf'
    delay.parent.mkdir(parents=True)
    delay.write_text('[Service]\nExecStartPre=/bin/sleep 11\n')
    image = Path('/candidate.AppImage')
    # Xvfb has neither a GPU nor a compositor. Exercise launcher/lifecycle
    # behavior with software rendering, including the autostart widget path.
    env = {**os.environ, 'APPIMAGE_EXTRACT_AND_RUN': '1', 'GDK_BACKEND': 'x11',
           'WEBKIT_DISABLE_COMPOSITING_MODE': '1', 'GDK_SYNCHRONIZE': '1'}
    log = (output / 'desktop.log').open('w')
    child = subprocess.Popen([str(image)], env=env, stdout=log,
                             stderr=subprocess.STDOUT, start_new_session=True)
    try:
        def cutover():
            path = home / '.config/Patina/runtime-owner-cutover.json'
            return path.exists() and json.loads(path.read_text())['status'] == 'completed'
        wait_for(cutover)
        initial = wait_for(ready)
        unit = home / '.config/systemd/user/patinad.service'
        assert unit.read_text().startswith('# Managed by Patina AppImage runtime v1\n')
        assert initial['FragmentPath'] == str(unit)
        assert systemctl('is-enabled', 'patinad.service') == 'enabled'
        assert (data / 'runtime-appimage/current/AppRun').is_file()
        autostart = home / '.config/autostart/Patina.desktop'
        command = next(line[5:] for line in autostart.read_text().splitlines() if line.startswith('Exec='))
        assert command == '/candidate.AppImage --autostart', command
        extracted = {Path(os.readlink(f'/proc/{pid}/exe')).parents[2] for pid in desktop_running()}
        close_desktop(child)
        for path in extracted:
            assert path.parent == Path('/tmp') and path.name.startswith('appimage_extracted_'), path
            if path.exists():
                shutil.rmtree(path)
        assert wait_for(ready)['InvocationID'] == initial['InvocationID']
        # Exercise the default widget path on every fresh Desktop. A single
        # successful reopen previously hid an intermittent Xlib failure.
        reopen_cycles = []
        for cycle in range(5):
            child = subprocess.Popen(command.split(), env=env, stdout=log,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            def reopened():
                assert child.poll() is None, f'Desktop autostart exited: {child.returncode}'
                return desktop_running()
            pids = wait_for(reopened)
            time.sleep(5)
            assert child.poll() is None, f'Desktop autostart exited in cycle {cycle}: {child.returncode}'
            assert ready()['InvocationID'] == initial['InvocationID']
            reopen_cycles.append({'cycle': cycle + 1, 'desktop_pids': pids})
            close_desktop(child)
        # Kill only this disposable user's actual systemd-owned daemon.
        os.kill(int(initial['MainPID']), signal.SIGKILL)
        def recovered():
            state = ready()
            return state if state['InvocationID'] != initial['InvocationID'] else None
        recovered_state = wait_for(recovered)
        assert int(recovered_state['NRestarts']) == 1
        systemctl('stop', 'patinad.service')
        assert status()['ExecMainStatus'] == '0'
        integrity()
        result = {'passed': True, 'phase': phase,
                  'sha256': hashlib.sha256(image.read_bytes()).hexdigest(),
                  'initial': initial, 'recovered': recovered_state,
                  'desktop_exit_reopen_same_invocation': True,
                  'desktop_reopened_via_persisted_autostart': True,
                  'autostart_reopen_cycles': reopen_cycles,
                  'headless_software_rendering': True,
                  'synchronous_x11_errors': True,
                  'test_only_daemon_start_delay_seconds': 11,
                  'database_quick_check': 'ok', 'foreign_key_check': 'ok',
                  'scope': 'Real container user manager; no GNOME/login or formal signed upgrade'}
        (output / 'first.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result))
    finally:
        close_desktop(child)
        log.close()
elif phase == 'cold':
    previous = json.loads((output / 'first.json').read_text())
    assert not desktop_running()
    state = wait_for(ready)
    assert state['InvocationID'] not in [previous['initial']['InvocationID'], previous['recovered']['InvocationID']]
    assert systemctl('is-enabled', 'patinad.service') == 'enabled'
    assert state['NRestarts'] == '0'
    integrity()
    result = {'passed': True, 'phase': phase, 'state': state,
              'desktop_running': False, 'database_quick_check': 'ok',
              'scope': 'Container PID1 and lingering user manager restart, not graphical login'}
    (output / 'cold.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
else:
    raise SystemExit('Expected first or cold')
