"""Pure archive/metadata regressions; never execute a fixture as a daemon."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

source = Path(__file__).resolve().parents[1] / "package-daemon.py"
spec = importlib.util.spec_from_file_location("daemon_package", source)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def info():
    return {"format_version": 1, "package_version": "1.9.2", "target": "x86_64-unknown-linux-gnu",
            "desktop_feature": False, "debug_assertions": False,
            "protocol": {"current": 2, "min_supported_client": 1, "max_supported_client": 2}}


class DaemonPackageTests(unittest.TestCase):
    def test_metadata_rejects_wrong_projection_format_target_and_debug(self):
        for field, value in [("format_version", True), ("format_version", 2), ("desktop_feature", True),
                             ("desktop_feature", 0), ("debug_assertions", True), ("debug_assertions", 0),
                             ("target", "x86_64-pc-windows-msvc"), ("target", "../linux-gnu"),
                             ("target", "x86_64-unknown-notlinux-gnu")]:
            fixture = info()
            fixture[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                package.validate_build_info(fixture)
        fixture = info()
        fixture["debug_assertions"] = True
        package.validate_build_info(fixture, allow_debug=True)

    def test_semver_and_protocol_are_validated_without_coercion(self):
        for version in ["", "../1.9.2", "01.9.2", "1.9.2-01", "1.9.2-a..b", "1.9.2+", "1.9.2\n"]:
            fixture = info()
            fixture["package_version"] = version
            with self.subTest(version=version), self.assertRaises(ValueError):
                package.validate_build_info(fixture)
        for protocol in [{"current": True, "min_supported_client": 1, "max_supported_client": 2},
                         {"current": 2, "min_supported_client": 3, "max_supported_client": 4},
                         {"current": 2, "min_supported_client": 1, "max_supported_client": 0}]:
            fixture = info()
            fixture["protocol"] = protocol
            with self.assertRaises(ValueError):
                package.validate_build_info(fixture)
        for version in ["1.9.2-beta.1", "1.10.0+fixture.001", "2.0.0-rc.1+source"]:
            fixture = info()
            fixture["package_version"] = version
            package.validate_build_info(fixture)

    def test_archive_is_reproducible_and_binds_payload_content_and_modes(self):
        with tempfile.TemporaryDirectory(prefix="patina-package-test-") as temporary:
            root = Path(temporary)
            binary = root / "fixture"
            binary.write_bytes(b"\x7fELFsynthetic-test-payload-not-an-executable")
            unit = (package.ROOT / "packaging/systemd/patinad.service").read_text()
            first = package._assemble(binary, info(), root / "first", unit)
            second = package._assemble(binary, copy.deepcopy(info()), root / "second", unit)
            self.assertEqual(first["sha256"], second["sha256"])
            with tarfile.open(first["archive"], "r:gz") as archive:
                self.assertEqual(sorted(archive.getnames()), ["patinad/LICENSE", "patinad/README.txt", "patinad/bin/patinad",
                    "patinad/manifest.json", "patinad/systemd/patinad.service.in"])
                manifest_bytes = archive.extractfile("patinad/manifest.json").read()
                self.assertEqual(hashlib.sha256(manifest_bytes).hexdigest(), first["manifest_sha256"])
                manifest = json.loads(manifest_bytes)
                self.assertEqual(manifest["build"], info())
                self.assertEqual(manifest["distribution"], "standalone")
                self.assertEqual(archive.extractfile("patinad/LICENSE").read(), (package.ROOT / "LICENSE").read_bytes())
                for name, record in manifest["files"].items():
                    entry = archive.getmember("patinad/" + name)
                    body = archive.extractfile(entry).read()
                    self.assertTrue(entry.isfile())
                    self.assertEqual((entry.uid, entry.gid, entry.mtime), (0, 0, 0))
                    self.assertEqual(len(body), record["size"])
                    self.assertEqual(entry.mode, record["mode"])
                    self.assertEqual(hashlib.sha256(body).hexdigest(), record["sha256"])
                template = archive.extractfile("patinad/systemd/patinad.service.in").read().decode()
                self.assertIn("ExecStart=@PATINAD_EXECUTABLE@ --profile production --serve-api --track", template)
            with self.assertRaises(FileExistsError):
                package._assemble(binary, info(), root / "first", unit)
            self.assertEqual(package.digest(Path(first["archive"])), first["sha256"])
            with self.assertRaises(ValueError):
                package._assemble(binary, info(), root / "bad", "ExecStart=/wrong")
            self.assertFalse((root / "bad").exists())


if __name__ == "__main__":
    unittest.main()
