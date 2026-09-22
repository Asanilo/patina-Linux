"""Opt-in minisign interoperability tests with disposable keys, never release keys."""
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class CandidateSignatureTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='patina-test-signature-')
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.image = self.root / 'Patina_1.0.0_amd64.AppImage'
        self.image.write_bytes(b'\x7fELF\x02\x01\x01\x00AI\x02' + bytes(100))
        self.tool('minisign', '-G', '-W', '-p', 'test.pub', '-s', 'test.key')
        self.tool('minisign', '-S', '-s', 'test.key', '-m', self.image.name, '-t', 'test only')
        self.image.with_suffix('.AppImage.sig').write_bytes(
            base64.b64encode(self.image.with_suffix('.AppImage.minisig').read_bytes()))
        self.config = self.root / 'tauri.conf.json'
        self.config.write_text(json.dumps({'version': '1.0.0', 'plugins': {'updater': {
            'pubkey': base64.b64encode((self.root / 'test.pub').read_bytes()).decode()}}}))

    def tool(self, *args):
        result = subprocess.run(args, cwd=self.root, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        return result

    def verify(self, success):
        output = self.root / 'verified'
        result = subprocess.run(['python3', str(Path(__file__).with_name('signed-appimage.py').resolve()),
                                 str(self.root), str(self.config), str(output)], capture_output=True,
                                env={**os.environ, 'CANDIDATE_SHA': 'a' * 40, 'CANDIDATE_RUN': 'test-only'})
        self.assertEqual(result.returncode == 0, success, result.stderr.decode())
        self.assertEqual(output.exists(), success)
        if success:
            self.assertEqual((output / self.image.name).read_bytes(), self.image.read_bytes())
            self.assertTrue(json.loads((output / 'candidate.json').read_text())['signature_verified'])

    def test_valid_signature_is_staged(self):
        self.verify(True)

    def test_tauri_cli_signature_is_staged(self):
        key = self.root / 'tauri-test.key'
        cli = Path(__file__).resolve().parents[2] / 'node_modules/.bin/tauri'
        self.tool(str(cli), 'signer', 'generate', '--ci', '--password', '', '--write-keys', str(key))
        key.chmod(0o600)
        config = json.loads(self.config.read_text())
        config['plugins']['updater']['pubkey'] = key.with_suffix('.key.pub').read_text().strip()
        self.config.write_text(json.dumps(config))
        self.tool(str(cli), 'signer', 'sign', '--private-key-path', str(key),
                  '--password', '', str(self.image))
        self.verify(True)

    def test_tampered_image_is_not_staged(self):
        self.image.write_bytes(self.image.read_bytes()[:-1] + b'x')
        self.verify(False)

    def test_other_key_is_not_trusted(self):
        self.tool('minisign', '-G', '-W', '-p', 'other.pub', '-s', 'other.key')
        config = json.loads(self.config.read_text())
        config['plugins']['updater']['pubkey'] = base64.b64encode((self.root / 'other.pub').read_bytes()).decode()
        self.config.write_text(json.dumps(config))
        self.verify(False)

    def test_invalid_signature_is_not_staged(self):
        self.image.with_suffix('.AppImage.sig').write_text('invalid signature')
        self.verify(False)


if __name__ == '__main__':
    unittest.main()
