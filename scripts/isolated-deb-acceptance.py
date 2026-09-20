#!/usr/bin/env python3
"""Opt-in dpkg lifecycle acceptance, without installing Patina on the host.

Usage: python3 scripts/isolated-deb-acceptance.py --candidate /absolute/new.deb \
           --baseline /absolute/old.deb

Requires Linux amd64, dpkg, unshare, bubblewrap and Python 3.10+. Retains the
private installation and evidence on success or failure. Dependency versions
are checked by normal dpkg against a copy of host installed-package metadata;
dependency payload installation, GUI, services and login are NOT validated.
Only packages with control/md5sums and regular files under usr/ are accepted.
No maintainer scripts, triggers, package executables or service commands run.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import resource
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time


MAX_ARCHIVE = 512 * 1024 * 1024
REQUIRED = (
    "usr/bin/Patina", "usr/bin/patinad", "usr/lib/systemd/user/patinad.service",
    "usr/share/gnome-shell/extensions/patina-window-tracker@patina/extension.js",
    "usr/share/gnome-shell/extensions/patina-window-tracker@patina/metadata.json",
)
SCOPE = (
    "Private dpkg baseline install, candidate upgrade, remove and reinstall. "
    "Normal dependency metadata checks against copied host installed versions; "
    "dependency payload installation is not verified. No GUI, production data, "
    "service control, login, database migration or release acceptance."
)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(".pending")
    with temporary.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    os.replace(temporary, path)


def limits():
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_ARCHIVE, MAX_ARCHIVE))


def run(args, output, timeout=45):
    """Log bounded commands, killing their entire process group on timeout."""
    with output.open("wb") as log:
        process = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True, preexec_fn=limits)
        try:
            code = process.wait(timeout=timeout)
        except BaseException:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise
    require(code == 0, f"Command failed ({code}); see {output}: {args[0]}")


def archive(deb, target, flag):
    # Keep stderr out of the tar stream. RLIMIT_FSIZE also bounds decompression.
    with target.open("wb") as output, target.with_suffix(".log").open("wb") as errors:
        process = subprocess.Popen(["dpkg-deb", flag, str(deb)], stdout=output,
                                   stderr=errors, start_new_session=True,
                                   preexec_fn=limits)
        try:
            code = process.wait(timeout=45)
        except BaseException:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise
    require(code == 0, f"Cannot inspect {deb.name}; see {target.with_suffix('.log')}")


def safe_name(member):
    path = PurePosixPath(member.name)
    require(not path.is_absolute() and ".." not in path.parts,
            f"Unsafe archive path: {member.name}")
    return str(path)


def fields(text):
    result = {}
    for line in text.splitlines():
        if not line or line[0].isspace():
            continue
        key, separator, value = line.partition(":")
        require(separator and key not in result, "Invalid or duplicate control field")
        result[key] = value.strip()
    return result


def inspect(deb, directory, candidate):
    control = directory / "control.tar"
    archive(deb, control, "--ctrl-tarfile")
    control_names = set()
    metadata = None
    with tarfile.open(control) as contents:
        for member in contents:
            name = safe_name(member)
            if name == "." and member.isdir():
                continue
            require(member.isfile() and name in {"control", "md5sums"}
                    and name not in control_names and member.size <= 1024 * 1024,
                    f"Unexpected control entry {name}; scripts/triggers are forbidden")
            control_names.add(name)
            if name == "control":
                metadata = fields(contents.extractfile(member).read().decode("utf-8"))
    require(metadata and metadata.get("Package") == "patina", "Expected package patina")
    require(metadata.get("Architecture") == "amd64", "Only amd64 packages are supported")
    require(bool(metadata.get("Version")), "Missing package version")
    payload = directory / "payload.tar"
    archive(deb, payload, "--fsys-tarfile")
    manifest = {}
    names = set()
    unit = None
    extension = None
    with tarfile.open(payload) as contents:
        for member in contents:
            name = safe_name(member)
            if name == "." and member.isdir():
                continue
            require(name == "usr" or name.startswith("usr/"), f"Unexpected payload path: {name}")
            require(name not in names, f"Duplicate payload path: {name}")
            names.add(name)
            require(member.isdir() or member.isfile(), f"Links/devices forbidden: {name}")
            require(not member.mode & 0o7022, f"Unsafe payload permissions: {name}")
            require(not any(part.endswith((".wants", ".requires"))
                            for part in PurePosixPath(name).parts), "Automatic enablement forbidden")
            if member.isdir():
                continue
            require(member.size <= MAX_ARCHIVE, f"Oversized payload: {name}")
            source = contents.extractfile(member)
            value = hashlib.sha256()
            text = bytearray()
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                value.update(chunk)
                if name == REQUIRED[2] or name == REQUIRED[4]:
                    require(len(text) + len(chunk) <= 65536, f"Oversized metadata: {name}")
                    text.extend(chunk)
            manifest[name] = {"sha256": value.hexdigest(), "size": member.size,
                              "mode": member.mode & 0o777}
            if name == REQUIRED[2]:
                unit = text.decode("utf-8")
            if name == REQUIRED[4]:
                extension = json.loads(text)
    for name in REQUIRED if candidate else (REQUIRED[0],):
        require(name in manifest and manifest[name]["size"] > 0, f"Missing payload: {name}")
    for name in REQUIRED[:2]:
        if name in manifest:
            require(manifest[name]["mode"] & 0o111, f"Binary is not executable: {name}")
    if candidate:
        lines = [line.strip() for line in unit.splitlines()]
        require([line for line in lines if line.startswith("Exec")] == [
            "ExecStart=/usr/bin/patinad --profile production --serve-api --track"
        ], "Unexpected service Exec directives")
        for line in ("Environment=PATINA_SYSTEMD_SERVICE=patinad.service", "Restart=on-failure",
                     "KillSignal=SIGINT", "UMask=0077", "NoNewPrivileges=true", "ProtectSystem=strict",
                     "WantedBy=default.target"):
            require(line in lines, f"Missing service setting: {line}")
        require("systemctl" not in unit, "Unit must not control services")
        require(extension.get("uuid") == "patina-window-tracker@patina", "Incorrect extension UUID")
    result = {"sha256": digest(deb), "metadata": metadata, "manifest": manifest,
              "control_entries": sorted(control_names), "maintainer_scripts": []}
    write_json(directory / "manifest.json", result)
    control.unlink()
    payload.unlink()
    return result


def verify_install(root, package, removed=()):
    for name, expected in package["manifest"].items():
        path = root / name
        actual = path.lstat()
        require(stat.S_ISREG(actual.st_mode), f"Not a regular installed file: {name}")
        require(actual.st_size == expected["size"] and digest(path) == expected["sha256"],
                f"Installed payload differs: {name}")
        require(stat.S_IMODE(actual.st_mode) == expected["mode"], f"Installed mode differs: {name}")
    for name in removed:
        require(not (root / name).exists(), f"Obsolete payload retained: {name}")


def verify_private_data(root, sentinels):
    for name, expected in sentinels.items():
        require(digest(root / name) == expected, f"Synthetic user data changed: {name}")
    for path in root.rglob("*"):
        require(not path.is_symlink(), f"Unexpected installation symlink: {path}")
        require(not path.name.endswith((".wants", ".requires")), f"Unexpected enablement: {path}")


def worker():
    base = Path("/evidence")
    plan = json.loads((base / "plan.json").read_text())
    root = base / "root"
    require(os.geteuid() == 0 and Path.cwd() == base
            and not any(Path("/home").iterdir())
            and not any(Path("/run").iterdir()), "Isolation is missing")
    previous = plan["baseline"]
    current = plan["candidate"]
    stages = [
        ("baseline-install", ["--install", "/evidence/baseline/package.deb"], previous),
        ("candidate-upgrade", ["--install", "/evidence/candidate/package.deb"], current),
        ("remove", ["--remove", "patina"], None),
        ("candidate-reinstall", ["--install", "/evidence/candidate/package.deb"], current),
    ]
    evidence = {"scope": SCOPE, "passed": False, "stages": []}
    try:
        for name, operation, package in stages:
            evidence["active_stage"] = name
            write_json(base / "lifecycle.json", evidence)
            run(["dpkg", f"--root={root}", f"--log={base / 'dpkg.log'}", *operation],
                base / f"{name}.log")
            installed = {}
            for paragraph in (root / "var/lib/dpkg/status").read_text().split("\n\n"):
                row = fields(paragraph)
                if row.get("Package") == "patina":
                    installed = row
            if package:
                require(installed.get("Status") == "install ok installed"
                        and installed.get("Version") == package["metadata"]["Version"],
                        f"Unexpected package status after {name}")
                obsolete = set(previous["manifest"]) - set(current["manifest"]) if name == "candidate-upgrade" else ()
                verify_install(root, package, obsolete)
            else:
                require(installed.get("Status", "not-installed").endswith("not-installed"),
                        "Package remains installed after remove")
                for path in set(previous["manifest"]) | set(current["manifest"]):
                    require(not (root / path).exists(), f"Payload survives removal: {path}")
            verify_private_data(root, plan["sentinels"])
            evidence["stages"].append({"stage": name, "passed": True,
                                       "package_status": installed, "user_data_preserved": True,
                                       "auto_enablement_absent": True, "payload_verified": True})
            write_json(base / "lifecycle.json", evidence)
        evidence["passed"] = True
        evidence["active_stage"] = None
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        write_json(base / "lifecycle.json", evidence)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    args = parser.parse_args()
    require(os.geteuid() != 0, "Run as an ordinary user, without sudo")
    for path in (args.candidate, args.baseline):
        require(path.is_absolute() and path.is_file() and not path.is_symlink(),
                "candidate and baseline must be absolute regular non-symlink paths")
        require(path.stat().st_size <= MAX_ARCHIVE, "DEB exceeds size limit")
    base = Path(tempfile.mkdtemp(prefix="patina-deb-acceptance-", dir="/tmp"))
    base.chmod(0o700)
    print(f"Private evidence: {base}", flush=True)
    evidence = {"format": "patina.isolated-deb-acceptance.v1", "scope": SCOPE,
                "passed": False, "root": str(base / "root"), "started_at": time.time()}
    try:
        packages = {}
        for label in ("baseline", "candidate"):
            directory = base / label
            directory.mkdir()
            source = getattr(args, label)
            package = directory / "package.deb"
            shutil.copyfile(source, package)
            package.chmod(0o400)
            packages[label] = inspect(package, directory, label == "candidate")
            packages[label]["source_path"] = str(source)
        evidence["packages"] = packages
        run(["dpkg", "--compare-versions", packages["candidate"]["metadata"]["Version"],
             "gt", packages["baseline"]["metadata"]["Version"]], base / "versions.log")
        root = base / "root"
        admin = root / "var/lib/dpkg"
        admin.mkdir(parents=True)
        (root / "tmp").mkdir()
        # Metadata only: never copy host dpkg info scripts, triggers or file lists.
        status = Path("/var/lib/dpkg/status").read_text(encoding="utf-8")
        rows = [paragraph for paragraph in status.split("\n\n") if paragraph.strip()
                and fields(paragraph).get("Package") != "patina"
                and fields(paragraph).get("Status", "").endswith("ok installed")]
        (admin / "status").write_text("\n\n".join(rows) + "\n", encoding="utf-8")
        evidence["dependency_check"] = {
            "mode": "normal-dpkg-against-host-metadata-copy", "copied_entries": len(rows),
            "status_sha256": digest(admin / "status"), "force_depends": False,
            "dependency_payload_installation_verified": False,
        }
        sentinels = {}
        for relative in ("home/synthetic/.local/share/Patina/synthetic-records.json",
                         "home/synthetic/.config/Patina/synthetic-settings.json"):
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('{"synthetic":true,"preserve":"user-owned data"}\n', encoding="utf-8")
            sentinels[relative] = digest(target)
        write_json(base / "plan.json", {**packages, "sentinels": sentinels})
        shutil.copyfile(Path(__file__), base / "runner.py")
        command = ["unshare", "--user", "--map-root-user", "--mount", "--pid", "--fork",
                   "--kill-child", "--net", "bwrap", "--die-with-parent",
                   "--ro-bind", "/usr", "/usr"]
        for name in ("bin", "sbin", "lib", "lib64"):
            command.extend(["--symlink", f"usr/{name}", f"/{name}"])
        command.extend(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                        "--tmpfs", "/run", "--dir", "/etc", "--dir", "/home",
                        "--bind", str(base), "/evidence", "--chdir", "/evidence",
                        "--clearenv", "--setenv", "PATH", "/usr/sbin:/usr/bin:/sbin:/bin",
                        "--setenv", "HOME", "/home", "--setenv", "LC_ALL", "C",
                        "--setenv", "PYTHONDONTWRITEBYTECODE", "1", "--",
                        "/usr/bin/python3", "/evidence/runner.py", "--internal-worker"])
        run(command, base / "namespace.log", timeout=210)
        evidence["lifecycle"] = json.loads((base / "lifecycle.json").read_text())
        require(evidence["lifecycle"]["passed"], "Lifecycle did not complete")
        evidence["passed"] = True
        evidence["retained_daemon"] = str(root / "usr/bin/patinad")
        print(f"Passed. Retained candidate daemon: {evidence['retained_daemon']}")
        print(SCOPE)
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        evidence["finished_at"] = time.time()
        write_json(base / "evidence.json", evidence)
        print(f"Evidence retained: {base / 'evidence.json'}", flush=True)


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["--internal-worker"]:
            worker()
        else:
            main()
    except Exception as error:
        print(f"Isolated DEB acceptance failed: {error}", file=sys.stderr)
        sys.exit(1)
