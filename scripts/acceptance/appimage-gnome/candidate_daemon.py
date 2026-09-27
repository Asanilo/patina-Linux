"""Temporarily test a candidate daemon in the isolated GNOME AppImage guest.

The original AppImage service command is restored even if the probe fails.
"""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.parse
import urllib.request


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert len(sys.argv) == 4
candidate = Path(sys.argv[1])
expected_sha = sys.argv[2]
run_id = sys.argv[3]
assert run_id and all(c.isalnum() or c in '-_' for c in run_id)
assert candidate == Path('/home/tester/acceptance/candidate-patinad')
assert candidate.is_file() and not candidate.is_symlink()
assert hashlib.sha256(candidate.read_bytes()).hexdigest() == expected_sha
os.umask(0o077)

home = Path.home()
unit = home / '.config/systemd/user/patinad.service'
unit_bytes = unit.read_bytes()
assert b'runtime-appimage/current/AppRun' in unit_bytes
dropin_dir = home / '.config/systemd/user/patinad.service.d'
dropin = dropin_dir / 'c1-candidate.conf'
assert not dropin.exists()
assert not subprocess.check_output(
    ['systemctl', '--user', 'show', 'patinad.service', '-p', 'DropInPaths'], text=True
).strip().removeprefix('DropInPaths=')

token = (home / '.local/share/Patina/api_token').read_text().strip()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def service():
    return dict(line.split('=', 1) for line in subprocess.check_output(
        ['systemctl', '--user', 'show', 'patinad.service', '-p',
         'ActiveState,MainPID,InvocationID,NRestarts'], text=True).splitlines())


def ready():
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            request = urllib.request.Request(
                'http://127.0.0.1:14840/api/v1/capabilities',
                headers={'Authorization': 'Bearer ' + token})
            with opener.open(request, timeout=3) as response:
                capabilities = json.load(response)['data']
            state = service()
            if (state['ActiveState'] == 'active' and int(state['MainPID']) > 0
                    and capabilities['server_version'] == '1.9.0-beta.21'
                    and capabilities['tracking']['ready']):
                return state
        except (OSError, ValueError, KeyError):
            pass
        time.sleep(.5)
    raise RuntimeError('isolated daemon did not become ready')


def tracking_ready():
    deadline = time.monotonic() + 45
    last = None
    while time.monotonic() < deadline:
        try:
            request = urllib.request.Request(
                'http://127.0.0.1:14840/api/v1/diagnostics',
                headers={'Authorization': 'Bearer ' + token})
            with opener.open(request, timeout=3) as response:
                last = json.load(response)['data']
            window = last['window_tracking']
            runtime = last['tracker_runtime']
            if window['status'] == 'available' and runtime['probe_status'] == 'ok':
                return last
        except (OSError, ValueError, KeyError):
            pass
        time.sleep(.5)
    raise RuntimeError(f'isolated candidate tracking did not become healthy: {last}')


def api(path, body=None):
    request = urllib.request.Request(
        'http://127.0.0.1:14840/api/v1/' + path,
        data=json.dumps(body).encode() if body is not None else None,
        headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
        method='POST' if body is not None else 'GET')
    with opener.open(request, timeout=3) as response:
        return json.load(response)['data']


def c1_summary_ms():
    summary = api('summary/range?from=1790380800000&to=1790467200000')
    return next((app['total_ms'] for app in summary['apps']
                 if app['exe_name'] == 'org.patina.c1synthetic'), 0)


before = ready()
dropin_dir.mkdir(mode=0o700, exist_ok=True)
with dropin.open('x') as stream:
    stream.write('[Service]\nExecStart=\n'
                 'ExecStart=/home/tester/acceptance/candidate-patinad '
                 '--profile production --serve-api --track\n')
try:
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', '--user', 'restart', 'patinad.service'], check=True)
    candidate_state = ready()
    assert candidate_state['InvocationID'] != before['InvocationID']
    assert os.readlink(f'/proc/{candidate_state["MainPID"]}/exe') == str(candidate)
    diagnostics = tracking_ready()
    with (home / f'acceptance/candidate-daemon-diagnostics-{run_id}.json').open('x') as stream:
        json.dump({'window_tracking': diagnostics['window_tracking'],
                   'tracker_runtime': diagnostics['tracker_runtime']}, stream, indent=2)
        stream.write('\n')
    subprocess.run([sys.executable, str(home / 'privacy.py'), run_id], check=True)
    assert c1_summary_ms() == 600000
    app_path = 'apps/' + urllib.parse.quote('org.patina.C1Synthetic', safe='') + '/exclude'
    try:
        assert api(app_path, {'excluded': True})['ok']
        assert c1_summary_ms() == 0, 'excluded imported activity remained in Summary'
    finally:
        assert api(app_path, {'excluded': False})['ok']
    assert c1_summary_ms() == 600000
finally:
    dropin.unlink()
    try:
        dropin_dir.rmdir()
    except OSError:
        pass
    subprocess.run(['systemctl', '--user', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', '--user', 'restart', 'patinad.service'], check=True)

restored = ready()
assert restored['InvocationID'] != candidate_state['InvocationID']
assert os.readlink(f'/proc/{restored["MainPID"]}/exe') != str(candidate)
assert unit.read_bytes() == unit_bytes
result = {'passed': True, 'scope': 'isolated temporary daemon executable, real Desktop and GNOME window',
          'candidate_daemon_sha256': expected_sha, 'privacy_probe_passed': True,
          'imported_summary_exclusion_passed': True,
          'original_unit_unchanged': True, 'original_appimage_daemon_restored': True,
          'service_instance_changed_for_candidate': True}
with (home / f'acceptance/candidate-daemon-{run_id}.json').open('x') as stream:
    json.dump(result, stream, indent=2)
    stream.write('\n')
print('ISOLATED_CANDIDATE_DAEMON_PRIVACY_PASS')
