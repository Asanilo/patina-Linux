"""Guest-only final data and runtime check after a signed AppImage cold login."""

import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import urllib.request


assert subprocess.check_output(['systemd-detect-virt'], text=True).strip() == 'kvm'
assert Path.home() == Path('/home/tester') and os.getuid() == 1000
assert Path('/etc/patina-acceptance-vm').read_text().strip() == 'isolated-appimage-acceptance'
assert not Path('/usr/bin/patinad').exists()
os.umask(0o077)

home = Path.home()
data = home / '.local/share/Patina'
root = home / 'upgrade'
output = home / 'acceptance'
inputs = json.loads((root / 'input.json').read_text())
baseline = json.loads((output / 'upgrade-baseline.json').read_text())
verified = json.loads((root / 'verified-install.json').read_text())
first = json.loads((output / 'first.json').read_text())
login = json.loads((output / 'login.json').read_text())
signed_witness = json.loads((output / 'sample-signed-cold.json').read_text())
assert first['passed'] and login['passed'] and signed_witness['passed']
assert first['session']['boot_id'] != login['session']['boot_id']
assert verified['production_key_verified'] and verified['tampered_rejected_before_install']
assert hashlib.sha256((root / 'installed.AppImage').read_bytes()).hexdigest() == inputs['new_sha256']
assert hashlib.sha256(Path(verified['previous']).read_bytes()).hexdigest() == inputs['old_sha256']

token = (data / 'api_token').read_text().strip()
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def api(name):
    request = urllib.request.Request('http://127.0.0.1:14840/api/v1/' + name,
                                     headers={'Authorization': 'Bearer ' + token})
    with opener.open(request, timeout=3) as response:
        return json.load(response)['data']


assert api('capabilities')['server_version'] == inputs['new_version']
service = api('system/service')
assert service['instance_id'] != baseline['service']['instance_id']
assert os.readlink(data / 'runtime-appimage/current') == 'versions/' + inputs['new_sha256']
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

result = {'passed': True, 'signed_package_sha256': inputs['new_sha256'],
          'production_key_verified': True, 'tampered_rejected_before_install': True,
          'old_package_retained': True, 'old_history_unchanged': True,
          'preferences_unchanged': True, 'schema_unchanged': True, 'unit_unchanged': True,
          'autostart_points_to_installed_package': True,
          'cold_login_new_boot': True, 'post_upgrade_window_recorded': True,
          'daemon_version': inputs['new_version'], 'database_integrity': 'ok',
          'public_updater_channel_tested': False}
path = output / 'signed-cold-final.json'
with path.open('x') as stream:
    json.dump(result, stream, indent=2)
    stream.write('\n')
print('SIGNED_GNOME46_UPGRADE_COLD_FINAL_PASS')
