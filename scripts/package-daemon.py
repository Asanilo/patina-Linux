#!/usr/bin/env python3
"""Build a standalone runtime archive from a probed executable; never install it."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def validate_build_info(info, allow_debug=False):
    if not isinstance(info, dict) or set(info) != {
        "format_version", "package_version", "protocol", "target", "desktop_feature", "debug_assertions"
    }:
        raise ValueError("invalid daemon build metadata fields")
    if type(info["format_version"]) is not int or info["format_version"] != 1:
        raise ValueError("unsupported daemon build metadata format")
    number = r"(?:0|[1-9][0-9]*)"
    identifier = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
    version_pattern = rf"{number}\.{number}\.{number}(?:-{identifier}(?:\.{identifier})*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    if not isinstance(info["package_version"], str) or not re.fullmatch(version_pattern, info["package_version"]):
        raise ValueError("invalid daemon package version")
    if not isinstance(info["target"], str) or not re.fullmatch(r"[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)?-linux-(?:gnu|musl)[A-Za-z0-9_]*", info["target"]):
        raise ValueError("standalone archives require a Linux build target")
    if info["desktop_feature"] is not False:
        raise ValueError("standalone archive requires --no-default-features")
    if type(info["debug_assertions"]) is not bool or (info["debug_assertions"] and not allow_debug):
        raise ValueError("debug builds require explicit --allow-debug")
    protocol = info["protocol"]
    if not isinstance(protocol, dict) or set(protocol) != {"current", "min_supported_client", "max_supported_client"}:
        raise ValueError("invalid protocol metadata")
    if any(type(value) is not int or not 1 <= value <= 0xFFFFFFFF for value in protocol.values()):
        raise ValueError("invalid protocol version")
    if not protocol["min_supported_client"] <= protocol["current"] <= protocol["max_supported_client"]:
        raise ValueError("inconsistent protocol compatibility range")


def probe_binary(binary):
    import resource  # Linux executable probing; pure archive checks stay portable.

    with binary.open("rb") as source:
        if source.read(4) != b"\x7fELF":
            raise ValueError("daemon must be an ELF executable")
    # A mistaken Desktop executable must not bootstrap the user's real profile.
    # File-backed output has an OS size limit, not a check after buffering stdout.
    with tempfile.TemporaryDirectory(prefix="patina-metadata-") as temporary:
        root = Path(temporary)
        env = dict(os.environ)
        for key in ("DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "APPIMAGE", "APPDIR", "LD_PRELOAD", "LD_LIBRARY_PATH"):
            env.pop(key, None)
        for key, name in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime")):
            env[key] = str(root / name)
        env.update(DBUS_SESSION_BUS_ADDRESS=f"unix:path={root}/no-session-bus", DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={root}/no-system-bus", PULSE_SERVER=f"unix:{root}/no-pulse")
        with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
            def limit_output():
                resource.setrlimit(resource.RLIMIT_FSIZE, (16 * 1024 + 1, 16 * 1024 + 1))
            child = subprocess.Popen([str(binary), "--build-info"], cwd=root, env=env,
                                     stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                                     start_new_session=True, preexec_fn=limit_output)
            try:
                code = child.wait(timeout=5)
            except BaseException:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                child.wait()
                raise
            output.seek(0)
            errors.seek(0)
            body = output.read(16 * 1024 + 1)
            if code or errors.read(1) or len(body) > 16 * 1024:
                raise ValueError("daemon metadata probe failed or exceeded its output budget")
            if list(root.iterdir()):
                raise ValueError("metadata probe unexpectedly created profile files")
            return json.loads(body)


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def payloads(binary, info, unit, allow_debug=False):
    """Canonical payload shared by archive and distro-package delivery."""
    validate_build_info(info, allow_debug)
    expected = "ExecStart=/usr/bin/patinad --profile production --serve-api --track"
    if unit.count(expected) != 1:
        raise ValueError("unexpected packaged service command")
    template = unit.replace(expected, "ExecStart=@PATINAD_EXECUTABLE@ --profile production --serve-api --track")
    readme = (
        "Patina standalone runtime candidate\n\n"
        "This archive does not install files, enable services or select a runtime owner.\n"
        "Inspect bin/patinad with --build-info. The systemd file is a template, not an installed unit.\n"
        "Installation and migration tooling are separate work; do not overwrite an existing patinad.service.\n"
        "This is a dynamically linked Linux executable, not a promise of support for every distribution.\n"
        "Manifest SHA256 values provide content integrity, not publisher authentication or a signature.\n"
    )
    payloads = {"systemd/patinad.service.in": template.encode(), "README.txt": readme.encode(),
                "LICENSE": (ROOT / "LICENSE").read_bytes()}
    files = {"bin/patinad": {"sha256": digest(binary), "size": binary.stat().st_size, "mode": 0o755}}
    for name, content in payloads.items():
        files[name] = {"sha256": hashlib.sha256(content).hexdigest(), "size": len(content), "mode": 0o644}
    manifest = {"format_version": 1, "distribution": "standalone", "build": info, "files": files}
    payloads["manifest.json"] = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    return manifest, payloads


def _assemble(binary, info, output, unit, allow_debug=False):
    """The caller must probe this exact staged binary before assembly."""
    manifest, content = payloads(binary, info, unit, allow_debug)
    files = manifest["files"]
    suffix = "-debug" if info["debug_assertions"] else ""
    name = f"patinad-{info['package_version']}-{info['target']}{suffix}.tar.gz"
    destination = output / name
    output.mkdir(parents=True, exist_ok=True)
    # Exclusive creation prevents an accidental replacement of previous evidence.
    with destination.open("xb") as raw:
        try:
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                    for path in sorted(["bin/patinad", *content]):
                        entry = tarfile.TarInfo("patinad/" + path)
                        entry.mode = 0o755 if path == "bin/patinad" else 0o644
                        entry.size = files[path]["size"] if path == "bin/patinad" else len(content[path])
                        if path == "bin/patinad":
                            with binary.open("rb") as source:
                                archive.addfile(entry, source)
                        else:
                            archive.addfile(entry, io.BytesIO(content[path]))
        except BaseException:
            destination.unlink(missing_ok=True)
            raise
    return {"archive": str(destination), "sha256": digest(destination),
            "manifest_sha256": hashlib.sha256(content["manifest.json"]).hexdigest(), "build": info}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--allow-debug", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not binary.is_file():
        parser.error("binary must be a regular file")
    with tempfile.TemporaryDirectory(prefix="patina-daemon-package-") as temporary:
        staged = Path(temporary) / "patinad"
        shutil.copyfile(binary, staged)
        staged.chmod(0o755)
        info = probe_binary(staged)
        result = _assemble(staged, info, args.output.resolve(),
                           (ROOT / "packaging/systemd/patinad.service").read_text(encoding="utf-8"), args.allow_debug)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
