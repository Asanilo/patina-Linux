"""Opt-in minisign interoperability tests with disposable keys, never release keys."""
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
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

    def test_workflow_normalizes_wrapped_environment_key(self):
        key = self.root / 'wrapped-test.key'
        repo = Path(__file__).resolve().parents[2]
        cli = repo / 'node_modules/.bin/tauri'
        self.tool(str(cli), 'signer', 'generate', '--ci', '--password', '', '--write-keys', str(key))
        key.chmod(0o600)
        config = json.loads(self.config.read_text())
        config['plugins']['updater']['pubkey'] = key.with_suffix('.key.pub').read_text().strip()
        self.config.write_text(json.dumps(config))
        # Both whitespace and redundant padding occur in legacy Secret values.
        wrapped = '\n'.join(textwrap.wrap(key.read_text().strip(), 76)) + '==\n'
        environment = {**os.environ, 'TAURI_SIGNING_PRIVATE_KEY': wrapped}
        command = [str(cli), 'signer', 'sign', '--password', '', str(self.image)]
        raw = subprocess.run(command, env=environment, capture_output=True)
        self.assertNotEqual(raw.returncode, 0, 'Fixture must reproduce strict base64 decoding failure')
        lines = (repo / '.github/workflows/appimage-acceptance.yml').read_text().splitlines()
        normalization = [line.strip() for line in lines if line.strip().startswith('TAURI_SIGNING_PRIVATE_KEY="$')]
        self.assertEqual(len(normalization), 1)
        script = 'set -euo pipefail\n' + normalization[0] + '\nexport TAURI_SIGNING_PRIVATE_KEY\nexec "$@"'
        normalized = subprocess.run(['bash', '-c', script, 'test-signing', *command],
                                    env=environment, capture_output=True)
        self.assertEqual(normalized.returncode, 0, normalized.stderr.decode())
        self.verify(True)
        invalid = subprocess.run(['bash', '-c', script, 'test-signing', 'true'],
                                 env={**environment, 'TAURI_SIGNING_PRIVATE_KEY': 'not-base64!'}, capture_output=True)
        self.assertNotEqual(invalid.returncode, 0, 'Invalid input must fail before building')

    def test_other_key_is_not_trusted(self):
        self.tool('minisign', '-G', '-W', '-p', 'other.pub', '-s', 'other.key')
        config = json.loads(self.config.read_text())
        config['plugins']['updater']['pubkey'] = base64.b64encode((self.root / 'other.pub').read_bytes()).decode()
        self.config.write_text(json.dumps(config))
        self.verify(False)

    def test_invalid_signature_is_not_staged(self):
        self.image.with_suffix('.AppImage.sig').write_text('invalid signature')
        self.verify(False)

    def test_key_normalization_rejects_partial_or_empty_input(self):
        script = Path(__file__).with_name('normalize-signing-key.py')
        for value in [b'', b' \n===', b'TQ==TQ==', b'TQ==!invalid', b'A']:
            with self.subTest(value=value):
                result = subprocess.run(['python3', str(script)], input=value, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b'')


if __name__ == '__main__':
    unittest.main()
