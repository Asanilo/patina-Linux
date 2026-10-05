#!/usr/bin/env python3
"""Build a different standalone version from a private source copy, never modify the repo.

The returned binary is a local debug fixture, not a release or an installation.
Reuse its output with package-daemon.py and the isolated lifecycle/SDK acceptances.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
VERSION_FILES = ["package.json", "package-lock.json", "src-tauri/Cargo.toml",
                 "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json", "packaging/daemon/VERSION"]


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="temporary fixture version, distinct from Desktop")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory")
    parser.add_argument("--target-dir", type=Path, required=True, help="dedicated version-fixture cache, not the development target")
    args = parser.parse_args()
    desktop_version = json.loads((ROOT / "package.json").read_text())["version"]
    assert args.version != desktop_version, "fixture version must differ from Desktop"
    target_dir = args.target_dir.resolve()
    cache_marker = target_dir / ".patina-version-acceptance"
    if target_dir.exists():
        if cache_marker.exists():
            assert cache_marker.read_text() == "Patina version acceptance cache v1\n"
        else:
            assert not any(target_dir.iterdir()), "use a dedicated empty version-fixture cache; shared development targets can retain a different workspace's executable"
    else:
        target_dir.mkdir(parents=True)
    if not cache_marker.exists():
        cache_marker.write_text("Patina version acceptance cache v1\n")
    original = {name: digest(ROOT / name) for name in VERSION_FILES}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = Path(tempfile.mkdtemp(prefix="patina-version-source-"))
    inventory = subprocess.check_output(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=ROOT).split(b"\0")
    identity = hashlib.sha256()
    for raw in sorted(set(inventory) - {b""}):
        name = os.fsdecode(raw)
        path = ROOT / name
        if not path.exists() and not path.is_symlink():
            continue  # A tracked deletion is part of the working source snapshot.
        destination = source / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        if path.is_symlink():
            target = path.resolve(strict=True)
            assert target.is_relative_to(ROOT), f"source symlink leaves the workspace: {name}"
            destination.symlink_to(os.path.relpath(source / target.relative_to(ROOT), destination.parent))
            value = os.fsencode(os.readlink(path))
        else:
            assert path.is_file(), f"nonregular source input: {name}"
            shutil.copy2(path, destination)
            value = digest(destination).encode()
        identity.update(raw + b"\0" + value + b"\0")
    (source / "packaging/daemon/VERSION").write_text(args.version + "\n", encoding="utf-8")
    proof = dict(source_directory=str(source), source_head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                 source_inventory_sha256=identity.hexdigest(), override={"path":"packaging/daemon/VERSION", "version":args.version},
                 desktop_version=desktop_version, original_version_file_sha256=original)
    proof["cargo_target_directory"] = str(target_dir)
    (output / "source.json").write_text(json.dumps(proof, indent=2) + "\n")
    print(json.dumps({"source_directory":str(source), "fixture_version":args.version}), flush=True)
    env = dict(os.environ, CARGO_TARGET_DIR=str(target_dir), CARGO_NET_OFFLINE="true")
    with (output / "build.log").open("w") as log:
        result = subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(source / "src-tauri/Cargo.toml"),
                                 "--no-default-features", "--bin", "patinad"], cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=600)
    assert result.returncode == 0, (output / "build.log").read_text()[-6000:]
    binary = output / "patinad"
    shutil.copy2(target_dir / "debug/patinad", binary)
    probe_env = dict(env, XDG_CONFIG_HOME=str(output / "probe/config"), XDG_DATA_HOME=str(output / "probe/data"),
                     XDG_CACHE_HOME=str(output / "probe/cache"), XDG_RUNTIME_DIR=str(output / "probe/runtime"))
    info = json.loads(subprocess.check_output([str(binary), "--build-info"], env=probe_env, timeout=5))
    assert info["package_version"] == args.version and info["desktop_feature"] is False
    assert info["debug_assertions"] is True
    assert subprocess.check_output([str(binary), "--version"], env=probe_env, timeout=5).decode().strip() == "patinad " + args.version
    assert not (output / "probe").exists(), "metadata inspection created a profile"
    assert original == {name: digest(ROOT / name) for name in VERSION_FILES}, "repository version files changed"
    result = dict(passed=True, binary=str(binary), binary_sha256=digest(binary), build=info,
                  desktop_version=desktop_version, repository_versions_unchanged=True, source_directory=str(source))
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
