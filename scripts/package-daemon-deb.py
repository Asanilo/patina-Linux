#!/usr/bin/env python3
"""Build an independent per-user backend installer DEB. Never install or activate it."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("daemon_package", ROOT / "scripts/package-daemon.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
NAME = "patina-backend-installer"
CANDIDATE = "usr/lib/patina-backend-installer/candidate"
LAUNCHER = b'#!/bin/sh\nset -eu\nself=$(readlink -f -- "$0")\nexec "$(dirname -- "$self")/../lib/patina-backend-installer/candidate/bin/patinad" "$@"\n'


def deb_version(version, debug, manifest):
    base, _, build = version.partition("+")
    base = base.replace("-", "~", 1)
    if debug:
        base += "~debug"
        build = ".".join(part for part in (build, "manifest." + manifest[:16]) if part)
    return base + ("+" + build if build else "")


def require_native_amd64(binary, info):
    if info["target"] != "x86_64-unknown-linux-gnu":
        raise ValueError("this DEB builder currently supports x86_64-unknown-linux-gnu only")
    with binary.open("rb") as source:
        header = source.read(20)
    if len(header) != 20 or header[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", header, 18)[0] != 62:
        raise ValueError("candidate must be an amd64 little-endian ELF64 executable")
    if subprocess.check_output(["dpkg", "--print-architecture"], text=True).strip() != "amd64":
        raise ValueError("dependency resolution requires a native amd64 Debian build environment")


def dependencies(binary, temporary):
    debian = temporary / "debian"
    debian.mkdir()
    (debian / "control").write_text(f"Source: {NAME}\nSection: utils\nPriority: optional\nMaintainer: Patina maintainers\n\nPackage: {NAME}\nArchitecture: amd64\nDescription: Patina backend installer\n")
    result = subprocess.run(["dpkg-shlibdeps", "-O", "-e" + str(binary)], cwd=temporary,
                            capture_output=True, text=True, check=True, timeout=30)
    lines = result.stdout.strip().splitlines()
    if len(lines) != 1 or not lines[0].startswith("shlibs:Depends="):
        raise ValueError("unexpected dpkg-shlibdeps output")
    value = lines[0].removeprefix("shlibs:Depends=")
    if not value or "\n" in value or re.search(r"(?:^|,\s*)(?:patina(?:\s|,|$)|libgtk|libwebkit)", value):
        raise ValueError("standalone dependencies must not require Desktop/GTK/WebKit")
    return value + ", coreutils"


def assemble(binary, info, output, depends, allow_debug=False):
    manifest, content = package.payloads(binary, info, (ROOT / "packaging/systemd/patinad.service").read_text(), allow_debug)
    identity = hashlib.sha256(content["manifest.json"]).hexdigest()
    version = deb_version(info["package_version"], info["debug_assertions"], identity)
    if not depends or any(character in depends for character in "\r\n\0"):
        raise ValueError("invalid dependency metadata")
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f"{NAME}_{version}_amd64.deb"
    # Reserve only our output; an existing candidate is never overwritten.
    with destination.open("xb") as reserved:
        try:
            with tempfile.TemporaryDirectory(prefix="patina-backend-deb-") as temporary:
                work = Path(temporary)
                root = work / "root"
                payload = root / CANDIDATE
                (payload / "bin").mkdir(parents=True)
                shutil.copyfile(binary, payload / "bin/patinad")
                (payload / "bin/patinad").chmod(0o755)
                for name, data in content.items():
                    path = payload / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                    path.chmod(0o644)
                launcher = root / "usr/bin/patina-backend"
                launcher.parent.mkdir(parents=True)
                launcher.write_bytes(LAUNCHER)
                launcher.chmod(0o755)
                documentation = root / "usr/share/doc" / NAME
                documentation.mkdir(parents=True)
                shutil.copyfile(ROOT / "packaging/daemon/README.Debian", documentation / "README.Debian")
                shutil.copyfile(ROOT / "LICENSE", documentation / "copyright")
                files = sorted(path for path in root.rglob("*") if path.is_file())
                control = root / "DEBIAN"
                control.mkdir()
                (control / "control").write_text(
                    f"Package: {NAME}\nVersion: {version}\nArchitecture: amd64\nMaintainer: Patina maintainers\n"
                    f"Section: utils\nPriority: optional\nDepends: {depends}\n"
                    f"Installed-Size: {sum((path.stat().st_size + 1023) // 1024 for path in files)}\n"
                    "Homepage: https://github.com/Asanilo/patina-Linux\n"
                    f"X-Patina-Artifact-Version: {info['package_version']}\nX-Patina-Manifest-Sha256: {identity}\n"
                    f"X-Patina-Debug-Build: {'yes' if info['debug_assertions'] else 'no'}\n"
                    "Description: per-user Patina backend installer and candidate\n"
                    " Installs a standalone payload and patina-backend command.\n"
                    " User runtime activation is explicit; no services are enabled by this package.\n"
                    " Removing the installer leaves separately activated user runtimes and data intact.\n")
                def md5(path):
                    value = hashlib.md5(usedforsecurity=False)
                    with path.open("rb") as source:
                        for chunk in iter(lambda: source.read(1024 * 1024), b""):
                            value.update(chunk)
                    return value.hexdigest()
                (control / "md5sums").write_text("".join(md5(path) + "  " + str(path.relative_to(root)) + "\n" for path in files))
                for path in [root, *root.rglob("*")]:
                    if path.is_dir():
                        path.chmod(0o755)
                    elif path.parent == control or path.parent == documentation:
                        path.chmod(0o644)
                    os.utime(path, (0, 0))
                built = work / "candidate.deb"
                subprocess.run(["dpkg-deb", "--root-owner-group", "--uniform-compression", "-Zgzip", "-z6", "--build", str(root), str(built)],
                               env=dict(os.environ, SOURCE_DATE_EPOCH="0"), stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=90)
                with built.open("rb") as source:
                    shutil.copyfileobj(source, reserved)
                reserved.flush()
                os.fsync(reserved.fileno())
        except BaseException:
            destination.unlink(missing_ok=True)
            raise
    return {"archive": str(destination), "sha256": package.digest(destination), "package": NAME,
            "debian_version": version, "artifact_version": info["package_version"], "architecture": "amd64",
            "depends": depends, "manifest_sha256": identity, "build": info}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--allow-debug", action="store_true")
    args = parser.parse_args()
    source = args.binary.resolve(strict=True)
    if not source.is_file():
        parser.error("binary must be a regular file")
    with tempfile.TemporaryDirectory(prefix="patina-backend-input-") as temporary:
        work = Path(temporary)
        binary = work / "patinad"
        shutil.copyfile(source, binary)
        binary.chmod(0o755)
        info = package.probe_binary(binary)
        package.validate_build_info(info, args.allow_debug)
        require_native_amd64(binary, info)
        result = assemble(binary, info, args.output.resolve(), dependencies(binary, work), args.allow_debug)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
