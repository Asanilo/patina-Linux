#!/usr/bin/env python3
"""Inspect an independent backend installer DEB without executing its payload.

An expected manifest binds content, not publisher identity. Release signatures
must be verified separately. This verifier does not accept Desktop bundles.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import struct
import tarfile
import tempfile


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


SCRIPTS = Path(__file__).resolve().parents[1]
support = module("deb_support", SCRIPTS / "isolated-deb-acceptance.py")
package = module("daemon_package", SCRIPTS / "package-daemon.py")
require = support.require
NAME = "patina-backend-installer"
PREFIX = "usr/lib/patina-backend-installer/candidate/"
PAYLOAD = {"bin/patinad": 0o755, "README.txt": 0o644, "LICENSE": 0o644,
           "systemd/patinad.service.in": 0o644}
FILES = {PREFIX + path: mode for path, mode in PAYLOAD.items()}
FILES.update({PREFIX + "manifest.json": 0o644, "usr/bin/patina-backend": 0o755,
              f"usr/share/doc/{NAME}/README.Debian": 0o644,
              f"usr/share/doc/{NAME}/copyright": 0o644})
LAUNCHER = ('#!/bin/sh\nset -eu\nself=$(readlink -f -- "$0")\n'
            'exec "$(dirname -- "$self")/../lib/patina-backend-installer/candidate/bin/patinad" "$@"\n').encode()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def inspect(deb, directory, expected_manifest, allow_debug=False):
    require(re.fullmatch(r"[0-9a-f]{64}", expected_manifest), "Expected manifest must be a SHA256 digest")
    require(deb.is_file() and not deb.is_symlink() and 0 < deb.stat().st_size <= support.MAX_ARCHIVE,
            "Expected a bounded regular DEB")
    control = directory / "control.tar"
    support.archive(deb, control, "--ctrl-tarfile")
    controls = {}
    seen = set()
    with tarfile.open(control) as contents:
        for member in contents:
            name = support.safe_name(member)
            require(name not in seen and (member.uid, member.gid) == (0, 0), "Duplicate or non-root control entry")
            seen.add(name)
            if name == "." and member.isdir() and member.mode == 0o755:
                continue
            require(member.isfile() and name in {"control", "md5sums"}
                    and member.mode == 0o644 and 0 < member.size <= 65536,
                    f"Unexpected control entry {name}; scripts/triggers are forbidden")
            controls[name] = contents.extractfile(member).read().decode("utf-8")
    require(set(controls) == {"control", "md5sums"}, "Incomplete control inventory")
    metadata = support.fields(controls["control"])
    require(set(metadata) == {"Package", "Version", "Architecture", "Maintainer", "Section", "Priority",
                             "Depends", "Installed-Size", "Homepage", "X-Patina-Artifact-Version",
                             "X-Patina-Manifest-Sha256", "X-Patina-Debug-Build", "Description"},
            "Unexpected control fields (including ownership overrides)")
    require(metadata["Package"] == NAME and metadata["Architecture"] == "amd64", "Wrong package or architecture")
    dependencies = set()
    for entry in metadata["Depends"].split(","):
        match = re.fullmatch(r"\s*([a-z0-9+.-]+)(?: \(>= [A-Za-z0-9.+:~\-]+\))?\s*", entry)
        require(match is not None, "Unexpected dependency relationship")
        dependencies.add(match[1])
    require("coreutils" in dependencies and dependencies <= {
        "libc6", "libgcc-s1", "libpulse0", "libx11-6", "libxcb1", "coreutils"
    }, "Unexpected dependencies; review the standalone native-library boundary")
    md5sums = {}
    for line in controls["md5sums"].splitlines():
        match = re.fullmatch(r"([0-9a-f]{32})  (.+)", line)
        require(match is not None and match[2] not in md5sums, "Invalid or duplicate md5sums entry")
        md5sums[match[2]] = match[1]
    require(set(md5sums) == set(FILES), "Incorrect md5sums inventory")

    payload = directory / "payload.tar"
    support.archive(deb, payload, "--fsys-tarfile")
    directories = {str(parent) for name in FILES for parent in PurePosixPath(name).parents}
    manifest = {}
    small = {}
    seen = set()
    with tarfile.open(payload) as contents:
        for member in contents:
            name = support.safe_name(member)
            require(name not in seen and (member.uid, member.gid) == (0, 0), f"Duplicate or non-root payload: {name}")
            seen.add(name)
            if member.isdir():
                require(name in directories and member.mode == 0o755, f"Unexpected directory: {name}")
                continue
            require(name in FILES and member.isfile() and member.mode == FILES[name],
                    f"Unexpected file/type/mode: {name}")
            binary = name == PREFIX + "bin/patinad"
            require(0 < member.size <= (support.MAX_ARCHIVE if binary else 65536), f"Oversized or empty payload: {name}")
            sha, md5, body = hashlib.sha256(), hashlib.md5(usedforsecurity=False), bytearray()
            source = contents.extractfile(member)
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                sha.update(chunk)
                md5.update(chunk)
                body.extend(chunk[:max(0, 20 - len(body))] if binary else chunk)
            require(md5.hexdigest() == md5sums[name], f"md5sums mismatch: {name}")
            manifest[name] = {"sha256": sha.hexdigest(), "size": member.size, "mode": member.mode}
            small[name] = bytes(body)
    require(set(manifest) == set(FILES), "Incomplete payload inventory")
    header = small[PREFIX + "bin/patinad"]
    require(len(header) == 20 and header[:6] == b"\x7fELF\x02\x01" and struct.unpack_from("<H", header, 18)[0] == 62,
            "Expected an amd64 little-endian ELF64 payload")
    require(small["usr/bin/patina-backend"] == LAUNCHER, "Unexpected launcher")
    manifest_sha = manifest[PREFIX + "manifest.json"]["sha256"]
    require(manifest_sha == expected_manifest == metadata["X-Patina-Manifest-Sha256"], "Manifest digest mismatch")
    runtime = json.loads(small[PREFIX + "manifest.json"], object_pairs_hook=unique_object)
    require(set(runtime) == {"format_version", "distribution", "build", "files"}
            and type(runtime["format_version"]) is int and runtime["format_version"] == 1
            and runtime["distribution"] == "standalone", "Unexpected runtime manifest format")
    package.validate_build_info(runtime["build"], allow_debug)
    build = runtime["build"]
    require(build["target"] == "x86_64-unknown-linux-gnu", "Incorrect runtime target")
    require(set(runtime["files"]) == set(PAYLOAD), "Unexpected runtime file inventory")
    for name in PAYLOAD:
        require(runtime["files"][name] == manifest[PREFIX + name], f"Runtime content mismatch: {name}")
    require(metadata["X-Patina-Artifact-Version"] == build["package_version"]
            and metadata["X-Patina-Debug-Build"] == ("yes" if build["debug_assertions"] else "no"),
            "DEB/runtime build identity mismatch")
    version, _, suffix = build["package_version"].partition("+")
    version = version.replace("-", "~", 1)
    if build["debug_assertions"]:
        version += "~debug"
        suffix = (suffix + "." if suffix else "") + "manifest." + manifest_sha[:16]
    require(metadata["Version"] == version + ("+" + suffix if suffix else ""), "Unexpected Debian version")
    require(metadata["Installed-Size"] == str(sum((item["size"] + 1023) // 1024 for item in manifest.values())),
            "Incorrect installed size")
    unit = small[PREFIX + "systemd/patinad.service.in"].decode("utf-8")
    require([line for line in unit.splitlines() if line.startswith("Exec")] == [
        "ExecStart=@PATINAD_EXECUTABLE@ --profile production --serve-api --track"
    ], "Unexpected template command")
    require("WantedBy=default.target" in unit and "systemctl" not in unit, "Unexpected service template")
    result = {"sha256": support.digest(deb), "metadata": metadata, "manifest": manifest,
              "runtime_manifest_sha256": manifest_sha, "build": build,
              "control_entries": sorted(controls), "maintainer_scripts": [], "publisher_verified": False}
    support.write_json(directory / "manifest.json", result)
    control.unlink()
    payload.unlink()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("deb", type=Path)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--allow-debug", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="patina-deb-inspect-") as temporary:
        result = inspect(args.deb, Path(temporary), args.manifest_sha256, args.allow_debug)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
