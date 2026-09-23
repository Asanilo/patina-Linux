"""Prove the Actions signing key matches the configured public key before building.

Signs only a disposable challenge, not a release or updater manifest. The key
stays in the environment; stdout/stderr from the signer are captured, not logged.
"""
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    cli = Path(__file__).resolve().parents[2] / 'node_modules/.bin/tauri'
    with tempfile.TemporaryDirectory(prefix='patina-signing-proof-') as directory:
        root = Path(directory)
        challenge = root / 'challenge'
        challenge.write_bytes(b'Patina isolated signing-key proof\n' + os.urandom(32))
        result = subprocess.run([str(cli), 'signer', 'sign', str(challenge)], capture_output=True)
        if result.returncode:
            raise SystemExit('Signing preflight could not use the configured key/password')
        (root / 'public.key').write_bytes(base64.b64decode(config['plugins']['updater']['pubkey'], validate=True))
        (root / 'challenge.minisig').write_bytes(base64.b64decode(challenge.with_suffix('.sig').read_text().strip(), validate=True))
        result = subprocess.run(['minisign', '-V', '-m', str(challenge), '-p', str(root / 'public.key'),
                                 '-x', str(root / 'challenge.minisig')], capture_output=True)
        if result.returncode:
            raise SystemExit('Signing preflight does not match the configured production public key')
    print('Signing key identity verified against the configured public key')


if __name__ == '__main__':
    main()
