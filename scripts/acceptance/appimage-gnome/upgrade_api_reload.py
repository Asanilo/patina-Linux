"""Guest-only backend upgrade check when the Settings WebView cannot be used.

This exercises the supported local service API, not the Settings confirmation UI.
Do not count its result as a complete Desktop upgrade acceptance.
"""

import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
import urllib.error
import urllib.request


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)

home = Path.home()
data = home / '.local/share/Patina'
root = home / 'upgrade'
output = home / 'acceptance/upgrade-api-reload.json'
assert not output.exists()
inputs = json.loads((root / 'input.json').read_text())
baseline = json.loads((home / 'acceptance/upgrade-baseline.json').read_text())
verified = json.loads((root / 'verified-install.json').read_text())
assert verified['production_key_verified'] and verified['tampered_rejected_before_install']
assert hashlib.sha256((root / 'installed.AppImage').read_bytes()).hexdigest() == inputs['new_sha256']
assert hashlib.sha256(Path(verified['previous']).read_bytes()).hexdigest() == inputs['old_sha256']
assert os.readlink(data / 'runtime-appimage/current') == 'versions/' + inputs['new_sha256']

token = (data / 'api_token').read_text().strip()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def api(path, body=None):
    request = urllib.request.Request(
        'http://127.0.0.1:14840/api/v1/' + path,
        data=json.dumps(body).encode() if body is not None else None,
        headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
        method='POST' if body is not None else 'GET')
    with opener.open(request, timeout=3) as response:
        return json.load(response)['data']


before = api('system/service')
assert before['managed_by_systemd'] and before['service_name'] == 'patinad.service'
assert before['instance_id'] == baseline['service']['instance_id']
assert api('capabilities')['server_version'] == inputs['old_version']

# Send this request exactly once: a lost response can still schedule a restart.
accepted = api('system/service/restart', {'confirmed': True})
ticket = accepted['service']['restart']
assert accepted['reconnect_required'] and ticket['status'] == 'pending'
assert ticket['requested_instance_id'] == before['instance_id']

deadline = time.monotonic() + 45
after = None
while time.monotonic() < deadline:
    try:
        service = api('system/service')
        restart = service.get('restart') or {}
        if (service['instance_id'] != before['instance_id']
                and restart.get('request_id') == ticket['request_id']
                and restart.get('status') == 'completed'
                and restart.get('completed_instance_id') == service['instance_id']
                and api('capabilities')['server_version'] == inputs['new_version']):
            after = service
            break
    except (OSError, ValueError, KeyError, urllib.error.HTTPError):
        pass
    time.sleep(.5)
assert after is not None, 'daemon restart did not complete with the signed candidate'

assert hashlib.sha256((home / '.config/systemd/user/patinad.service').read_bytes()).hexdigest() == baseline['unit_sha256']
autostart = (home / '.config/autostart/Patina.desktop').read_text()
assert 'Exec=' + str(root / 'installed.AppImage') + ' --autostart' in autostart
assert '/.mount_' not in autostart
with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
    db.row_factory = sqlite3.Row
    for row in baseline['witness_rows']:
        assert dict(db.execute('SELECT * FROM sessions WHERE id=?', (row['id'],)).fetchone()) == row
    preferences = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('launch_at_login','background_tracking_at_login')"))
    assert preferences == baseline['preferences']
    migrations = [dict(row) for row in db.execute(
        'SELECT version,success,hex(checksum) AS checksum FROM _sqlx_migrations ORDER BY version')]
    assert migrations == baseline['migrations']
    assert db.execute('PRAGMA quick_check').fetchone()[0] == 'ok'
    assert db.execute('PRAGMA foreign_key_check').fetchone() is None

result = {'passed': True, 'scope': 'backend API reload only; Settings WebView not verified',
          'old_version': inputs['old_version'], 'new_version': inputs['new_version'],
          'signed_package_sha256': inputs['new_sha256'], 'old_package_retained': True,
          'production_key_verified': True, 'tampered_rejected_before_install': True,
          'service_instance_changed': True, 'restart_ticket_completed': True,
          'old_history_unchanged': True, 'preferences_unchanged': True,
          'schema_unchanged': True, 'unit_unchanged': True,
          'database_integrity': 'ok', 'settings_ui_tested': False,
          'public_updater_channel_tested': False}
with output.open('x') as stream:
    json.dump(result, stream, indent=2)
    stream.write('\n')
print('ISOLATED_BACKEND_API_RELOAD_PASS')
