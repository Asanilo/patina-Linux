"""DEB ownership/integrity regressions using inert ELF fixtures; no installation."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


HERE = Path(__file__).resolve().parent
builder = module("daemon_deb_builder", HERE.parent / "package-daemon-deb.py")
verify = module("daemon_deb_verify", HERE / "daemon-deb.py")


def info():
    return {"format_version": 1, "package_version": "1.9.2", "target": "x86_64-unknown-linux-gnu",
            "desktop_feature": False, "debug_assertions": False,
            "protocol": {"current": 2, "min_supported_client": 1, "max_supported_client": 2}}


class DaemonDebTests(unittest.TestCase):
    def test_metadata_probe_has_private_profile_and_bounded_output(self):
        # Exercise the actual ELF/Popen path without product data or services.
        with tempfile.TemporaryDirectory(prefix="patina-probe-test-") as temporary:
            root = Path(temporary)
            source = root / "probe.c"
            binary = root / "probe"
            source.write_text('''#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[1], "--build-info")) return 2;
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY") || getenv("APPIMAGE") || getenv("LD_PRELOAD")) return 3;
    const char *mode = getenv("PATINA_PACKAGE_PROBE_CASE");
    if (!strcmp(mode, "profile")) mkdir(getenv("XDG_DATA_HOME"), 0700);
    if (!strcmp(mode, "stderr")) fputs("unexpected", stderr);
    if (!strcmp(mode, "timeout")) sleep(15);
    if (!strcmp(mode, "overflow")) { for (int n = 0; n < 17000; ++n) putchar('x'); }
    puts(''' + json.dumps(json.dumps(info())) + ''');
    return 0;
}
''')
            subprocess.run(["cc", str(source), "-o", str(binary)], check=True, capture_output=True)
            for mode in ("success", "profile", "stderr", "overflow", "timeout"):
                with self.subTest(mode=mode), patch.dict(os.environ, {
                    "PATINA_PACKAGE_PROBE_CASE": mode, "DISPLAY": ":fixture", "WAYLAND_DISPLAY": "fixture",
                    "APPIMAGE": "fixture", "LD_PRELOAD": "/not-a-real-library", "XDG_DATA_HOME": str(root / "untouched")
                }):
                    if mode == "success":
                        self.assertEqual(builder.package.probe_binary(binary), info())
                    else:
                        with self.assertRaises(subprocess.TimeoutExpired if mode == "timeout" else ValueError):
                            builder.package.probe_binary(binary)
                    self.assertFalse((root / "untouched").exists())

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="patina-deb-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / "inert-fixture"
        self.binary.write_bytes(b"\x7fELF\x02\x01" + b"\0" * 12 + b"\x3e\0" + b"inert fixture, never execute")
        self.first = builder.assemble(self.binary, info(), self.root / "first", "libc6 (>= 2.34), coreutils")

    def inspect(self, result, label="inspection", allow_debug=False):
        folder = self.root / label
        folder.mkdir()
        return verify.inspect(Path(result["archive"]), folder, result["manifest_sha256"], allow_debug)

    def altered(self, label, mutate, refresh_md5=True):
        root = self.root / label
        subprocess.run(["dpkg-deb", "--raw-extract", self.first["archive"], str(root)], check=True, capture_output=True)
        mutate(root)
        if refresh_md5:
            paths = [path for path in root.rglob("*") if path.is_file() and "DEBIAN" not in path.relative_to(root).parts]
            (root / "DEBIAN/md5sums").write_text("".join(hashlib.md5(path.read_bytes(), usedforsecurity=False).hexdigest()
                + "  " + str(path.relative_to(root)) + "\n" for path in sorted(paths)))
        deb = self.root / (label + ".deb")
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(root), str(deb)], check=True, capture_output=True)
        return dict(self.first, archive=str(deb))

    def test_reproducible_inventory_and_same_tar_manifest(self):
        second = builder.assemble(self.binary, copy.deepcopy(info()), self.root / "second", "libc6 (>= 2.34), coreutils")
        self.assertEqual(self.first["sha256"], second["sha256"])
        inspected = self.inspect(self.first)
        self.assertEqual(set(inspected["manifest"]), set(verify.FILES))
        self.assertEqual(inspected["control_entries"], ["control", "md5sums"])
        self.assertFalse(inspected["publisher_verified"])
        archive = builder.package._assemble(self.binary, info(), self.root / "tar",
            (builder.ROOT / "packaging/systemd/patinad.service").read_text())
        self.assertEqual(archive["manifest_sha256"], inspected["runtime_manifest_sha256"])
        with self.assertRaises(FileExistsError):
            builder.assemble(self.binary, info(), self.root / "first", "coreutils")
        self.assertEqual(builder.package.digest(Path(self.first["archive"])), self.first["sha256"])

    def test_debug_opt_in_and_version_ordering(self):
        debug = dict(info(), debug_assertions=True)
        with self.assertRaises(ValueError):
            builder.assemble(self.binary, debug, self.root / "rejected", "coreutils")
        self.assertFalse((self.root / "rejected").exists())
        result = builder.assemble(self.binary, debug, self.root / "debug", "coreutils", allow_debug=True)
        with self.assertRaisesRegex(ValueError, "allow-debug"):
            self.inspect(result, "debug-rejected")
        self.inspect(result, "debug-accepted", allow_debug=True)
        for version in ("1.9.2", "1.9.2-beta.1", "2.0.0-rc.2+source.1"):
            with self.subTest(version=version):
                normal = builder.deb_version(version, False, "a" * 64)
                debug_version = builder.deb_version(version, True, "a" * 64)
                subprocess.run(["dpkg", "--compare-versions", debug_version, "lt", normal], check=True)
        subprocess.run(["dpkg", "--compare-versions", builder.deb_version("1.9.2-beta.2", False, "a" * 64),
                        "lt", builder.deb_version("1.9.2", False, "a" * 64)], check=True)

    def test_rejects_ownership_takeover_scripts_links_and_modes(self):
        def extra(root):
            (root / "usr/bin/patinad").write_bytes(b"forbidden Desktop path")
        def script(root):
            (root / "DEBIAN/postinst").write_text("#!/bin/sh\nexit 0\n")
            (root / "DEBIAN/postinst").chmod(0o755)
        def link(root):
            path = root / "usr/bin/patina-backend"
            path.unlink()
            path.symlink_to("/usr/bin/patinad")
        def hardlink(root):
            path = root / verify.PREFIX / "LICENSE"
            path.unlink()
            os.link(root / f"usr/share/doc/{verify.NAME}/copyright", path)
        for label, mutate, refresh in (("extra", extra, True), ("script", script, True),
                                       ("link", link, False), ("hardlink", hardlink, True),
                                       ("permissions", lambda root: (root / "usr/bin/patina-backend").chmod(0o777), True)):
            with self.subTest(label=label):
                result = self.altered(label, mutate, refresh)
                with self.assertRaises(RuntimeError):
                    self.inspect(result, label + "-inspection")

    def test_rejects_content_even_when_deb_md5sums_are_updated(self):
        result = self.altered("binary-tampered", lambda root: (root / verify.PREFIX / "bin/patinad").write_bytes(
            self.binary.read_bytes() + b"tampered"))
        with self.assertRaisesRegex(RuntimeError, "Runtime content mismatch"):
            self.inspect(result)
        launcher = self.altered("launcher-tampered", lambda root: (root / "usr/bin/patina-backend").write_bytes(
            verify.LAUNCHER + b"echo unexpected\n"))
        with self.assertRaisesRegex(RuntimeError, "Unexpected launcher"):
            self.inspect(launcher, "launcher-inspection")
        with self.assertRaisesRegex(RuntimeError, "Manifest digest mismatch"):
            self.inspect(dict(self.first, manifest_sha256="0" * 64), "wrong-manifest")

    def test_rejects_control_takeover_and_identity_mismatch(self):
        def change(root, before, after):
            path = root / "DEBIAN/control"
            path.write_text(path.read_text().replace(before, after))
        for index, (before, after) in enumerate((
            ("Description:", "Replaces: patina\nDescription:"),
            ("Description:", "Conflicts: patina\nDescription:"),
            ("Depends: libc6 (>= 2.34), coreutils", "Depends: libgtk-3-0, coreutils"),
            ("Version: 1.9.2\n", "Version: 1.9.3\n"),
            ("X-Patina-Debug-Build: no", "X-Patina-Debug-Build: yes"),
        )):
            with self.subTest(after=after):
                label = "control-" + str(index)
                result = self.altered(label, lambda root: change(root, before, after))
                with self.assertRaises(RuntimeError):
                    self.inspect(result, label + "-inspection")

    def test_rejects_duplicate_manifest_keys_even_with_matching_outer_digest(self):
        expected = None
        def change(root):
            nonlocal expected
            path = root / verify.PREFIX / "manifest.json"
            path.write_text(path.read_text().replace('"distribution": "standalone",',
                '"distribution": "standalone", "distribution": "standalone",'))
            expected = hashlib.sha256(path.read_bytes()).hexdigest()
            control = root / "DEBIAN/control"
            control.write_text(control.read_text().replace(self.first["manifest_sha256"], expected))
        result = self.altered("duplicate-key", change)
        with self.assertRaisesRegex(RuntimeError, "Duplicate JSON key"):
            self.inspect(dict(result, manifest_sha256=expected))

    def test_rejects_cross_target_and_non_amd64_input(self):
        builder.require_native_amd64(self.binary, info())
        with self.assertRaises(ValueError):
            builder.require_native_amd64(self.binary, dict(info(), target="aarch64-unknown-linux-gnu"))
        self.binary.write_bytes(b"\x7fELFnot-an-amd64-executable")
        with self.assertRaises(ValueError):
            builder.require_native_amd64(self.binary, info())
        result = subprocess.run([sys.executable, "-B", str(HERE.parent / "package-daemon-deb.py"),
            "--binary", "/dev/null", "--output", str(self.root / "device-input")], capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"binary must be a regular file", result.stderr)
        self.assertFalse((self.root / "device-input").exists())


if __name__ == "__main__":
    unittest.main()
