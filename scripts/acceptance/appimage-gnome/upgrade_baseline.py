"""Guest-only snapshot of synthetic history before a signed AppImage upgrade."""

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
output = home / 'acceptance/upgrade-baseline.json'
inputs = json.loads((root / 'input.json').read_text())
assert hashlib.sha256((root / 'installed.AppImage').read_bytes()).hexdigest() == inputs['old_sha256']
assert not output.exists()

token = (data / 'api_token').read_text().strip()
request = urllib.request.Request('http://127.0.0.1:14840/api/v1/system/service',
                                 headers={'Authorization': 'Bearer ' + token})
with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=3) as response:
    service = json.load(response)['data']

with sqlite3.connect(f'file:{data / "patina.db"}?mode=ro', uri=True) as db:
    db.row_factory = sqlite3.Row
    witness_rows = [dict(row) for row in db.execute(
        "SELECT * FROM sessions WHERE window_title='Patina isolated GNOME sampling witness' ORDER BY id")]
    assert witness_rows
    preferences = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('launch_at_login','background_tracking_at_login')"))
    migrations = [dict(row) for row in db.execute(
        'SELECT version,success,hex(checksum) AS checksum FROM _sqlx_migrations ORDER BY version')]
    assert db.execute('PRAGMA quick_check').fetchone()[0] == 'ok'
    assert db.execute('PRAGMA foreign_key_check').fetchone() is None

unit = home / '.config/systemd/user/patinad.service'
autostart = (home / '.config/autostart/Patina.desktop').read_text()
baseline = {'witness_rows': witness_rows, 'preferences': preferences,
            'migrations': migrations, 'service': service,
            'runtime_pointer': os.readlink(data / 'runtime-appimage/current'),
            'unit_sha256': hashlib.sha256(unit.read_bytes()).hexdigest(),
            'autostart': autostart}
with output.open('x') as stream:
    json.dump(baseline, stream, indent=2)
    stream.write('\n')
print('OLD_APPIMAGE_SYNTHETIC_BASELINE_RECORDED')
