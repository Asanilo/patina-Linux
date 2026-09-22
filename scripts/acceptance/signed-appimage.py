#!/usr/bin/env python3
"""Verify a Tauri AppImage against the configured public key; stage no release.

Usage: signed-appimage.py BUNDLE_DIRECTORY TAURI_CONFIG NEW_OUTPUT_DIRECTORY
Requires minisign. No signing private key is read, created or exported here.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    bundle, config_path, output = map(Path, sys.argv[1:])
    config = json.loads(config_path.read_text())
    version = config['version']
    assert re.fullmatch(r'\d+\.\d+\.\d+(?:[-.][0-9A-Za-z.-]+)?', version)
    image = bundle / f'Patina_{version}_amd64.AppImage'
    signature = image.with_suffix('.AppImage.sig')
    assert image.is_file() and not image.is_symlink()
    assert signature.is_file() and not signature.is_symlink()
    with image.open('rb') as stream:
        header = stream.read(11)
        assert header[:4] == b'\x7fELF' and header[8:11] == b'AI\x02'
    key = base64.b64decode(config['plugins']['updater']['pubkey'], validate=True)
    sig = base64.b64decode(signature.read_text().strip(), validate=True)
    with tempfile.TemporaryDirectory(prefix='patina-signature-') as folder:
        root = Path(folder)
        (root / 'public.key').write_bytes(key)
        (root / 'image.sig').write_bytes(sig)
        subprocess.run(['minisign', '-V', '-m', str(image), '-p', str(root / 'public.key'),
                        '-x', str(root / 'image.sig')], check=True)
    commit = os.environ['CANDIDATE_SHA']
    assert re.fullmatch(r'[0-9a-f]{40}', commit)
    manifest = {'format': 'patina.signed-appimage-acceptance.v1', 'version': version,
                'commit': commit, 'run': os.environ['CANDIDATE_RUN'],
                'production_public_key_sha256': hashlib.sha256(key).hexdigest(),
                'image': image.name, 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(),
                'signature_verified': True, 'public_release': False}
    output.mkdir(mode=0o700, exist_ok=False)
    shutil.copy2(image, output / image.name)
    shutil.copy2(signature, output / signature.name)
    (output / 'candidate.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest))


if __name__ == '__main__':
    main()
