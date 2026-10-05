#!/usr/bin/env python3
"""Opt-in independent installer/legacy Desktop coexistence in a private dpkg root.

No host install, package program, maintainer script or service command runs.
Dependency checks use copied host package metadata, not dependency payloads.
User data and a copied runtime are checked; this is not live service acceptance.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


WORKER = sys.argv[1:] == ["--internal-worker"]
HERE = Path(__file__).resolve().parent
support = module("deb_support", HERE / "support.py" if WORKER else HERE.parent / "isolated-deb-acceptance.py")
require = support.require
NAME = "patina-backend-installer"
PREFIX = "usr/lib/patina-backend-installer/candidate/"
SCOPE = "Private dpkg coexistence, upgrade, removal in both orders and reinstall; user data and copied runtime preserved. Dependency metadata only. No live service, GNOME, login, migration or public release acceptance."


def worker():
    base = Path("/evidence")
    require(os.geteuid() == 0 and Path.cwd() == base
            and not any(Path("/home").iterdir()) and not any(Path("/run").iterdir()), "Isolation is missing")
    plan = json.loads((base / "plan.json").read_text())
    root = base / "root"
    packages = plan["packages"]
    installed = {}
    sentinels = plan["sentinels"]
    stages = [
        ("desktop-install", ["--install", "/evidence/desktop/package.deb"], "desktop", "desktop"),
        ("installer-install", ["--install", "/evidence/baseline/package.deb"], "installer", "baseline"),
        ("installer-upgrade", ["--install", "/evidence/candidate/package.deb"], "installer", "candidate"),
        ("desktop-remove", ["--remove", "patina"], "desktop", None),
        ("installer-remove", ["--remove", NAME], "installer", None),
        ("desktop-reinstall", ["--install", "/evidence/desktop/package.deb"], "desktop", "desktop"),
        ("installer-reinstall", ["--install", "/evidence/candidate/package.deb"], "installer", "candidate"),
        ("installer-remove-before-desktop", ["--remove", NAME], "installer", None),
        ("desktop-remove-after-installer", ["--remove", "patina"], "desktop", None),
        ("installer-final-install", ["--install", "/evidence/candidate/package.deb"], "installer", "candidate"),
    ]
    evidence = {"passed": False, "scope": SCOPE, "stages": []}
    copied_runtime = None
    try:
        for name, operation, owner, label in stages:
            evidence["active_stage"] = name
            support.write_json(base / "lifecycle.json", evidence)
            support.run(["dpkg", f"--root={root}", f"--log={base / 'dpkg.log'}", *operation], base / f"{name}.log")
            installed[owner] = label
            status = {}
            for paragraph in (root / "var/lib/dpkg/status").read_text().split("\n\n"):
                row = support.fields(paragraph)
                if row.get("Package") in {"patina", NAME}:
                    status[row["Package"]] = row
            for key, package_name, default in (("desktop", "patina", "desktop"), ("installer", NAME, "baseline")):
                current = installed.get(key)
                row = status.get(package_name, {})
                if current:
                    expected = packages[current]
                    require(row.get("Status") == "install ok installed" and row.get("Version") == expected["metadata"]["Version"],
                            f"Wrong installed status: {package_name}")
                    support.verify_install(root, expected)
                else:
                    require(not row.get("Status", "").endswith("ok installed"), f"Unexpected installed package: {package_name}")
                    for path in packages[default]["manifest"]:
                        require(not (root / path).exists(), f"Removed package left a payload: {path}")
            if name == "installer-install":
                # Model the copy boundary using the actual delivered runtime.
                # Activation and its service semantics are tested separately.
                relative = "home/synthetic/.local/share/patina-runtime/versions/" + packages["baseline"]["runtime_manifest_sha256"]
                shutil.copytree(root / PREFIX, root / relative)
                copied_runtime = {"manifest": {
                    relative + "/" + path.removeprefix(PREFIX): value
                    for path, value in packages["baseline"]["manifest"].items() if path.startswith(PREFIX)
                }}
                evidence["copied_runtime"] = copied_runtime
            if copied_runtime:
                support.verify_install(root, copied_runtime)
            support.verify_private_data(root, sentinels)
            evidence["stages"].append({"name": name, "package_status": status, "payload_verified": True,
                "user_data_preserved": True, "copied_runtime_preserved": copied_runtime is not None,
                "auto_enablement_absent": True})
            support.write_json(base / "lifecycle.json", evidence)
        evidence["passed"] = True
        evidence["active_stage"] = None
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        support.write_json(base / "lifecycle.json", evidence)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("desktop", "baseline", "candidate"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--baseline-manifest-sha256", required=True)
    parser.add_argument("--candidate-manifest-sha256", required=True)
    parser.add_argument("--allow-debug", action="store_true")
    args = parser.parse_args()
    require(os.geteuid() != 0, "Run as an ordinary user without sudo")
    verify = module("daemon_deb", HERE / "daemon-deb.py")
    base = Path(tempfile.mkdtemp(prefix="patina-backend-deb-acceptance-", dir="/tmp"))
    base.chmod(0o700)
    print(f"Private evidence: {base}", flush=True)
    evidence = {"format": "patina.backend-deb-lifecycle.v1", "scope": SCOPE, "passed": False}
    try:
        packages = {}
        for label in ("desktop", "baseline", "candidate"):
            source = getattr(args, label)
            require(source.is_absolute() and source.is_file() and not source.is_symlink()
                    and source.stat().st_size <= support.MAX_ARCHIVE, "Expected bounded absolute regular packages")
            directory = base / label
            directory.mkdir()
            deb = directory / "package.deb"
            shutil.copyfile(source, deb)
            deb.chmod(0o400)
            packages[label] = (support.inspect(deb, directory, candidate=True) if label == "desktop" else
                               verify.inspect(deb, directory, getattr(args, label + "_manifest_sha256"), args.allow_debug))
            packages[label]["source_path"] = str(source)
        evidence["packages"] = packages
        for label in ("baseline", "candidate"):
            require(not set(packages["desktop"]["manifest"]) & set(packages[label]["manifest"]), "Package file ownership overlaps")
        require(set(packages["baseline"]["manifest"]) == set(packages["candidate"]["manifest"]), "Installer inventory changed")
        support.run(["dpkg", "--compare-versions", packages["candidate"]["metadata"]["Version"],
                     "gt", packages["baseline"]["metadata"]["Version"]], base / "versions.log")
        root = base / "root"
        admin = root / "var/lib/dpkg"
        admin.mkdir(parents=True)
        (root / "tmp").mkdir()
        rows = [paragraph for paragraph in Path("/var/lib/dpkg/status").read_text().split("\n\n")
                if paragraph.strip() and support.fields(paragraph).get("Package") not in {"patina", NAME}
                and support.fields(paragraph).get("Status", "").endswith("ok installed")]
        (admin / "status").write_text("\n\n".join(rows) + "\n", encoding="utf-8")
        evidence["dependency_check"] = {"mode": "normal-dpkg-against-host-metadata-copy", "copied_entries": len(rows),
            "status_sha256": support.digest(admin / "status"), "force_depends": False, "dependency_payload_installation_verified": False}
        sentinels = {}
        for path in ("home/synthetic/.local/share/Patina/records.json", "home/synthetic/.config/Patina/settings.json",
                     "home/synthetic/.config/systemd/user/patinad.service"):
            target = root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("synthetic user-owned fixture; preserve exactly\n", encoding="utf-8")
            sentinels[path] = support.digest(target)
        support.write_json(base / "plan.json", {"packages": packages, "sentinels": sentinels})
        shutil.copyfile(Path(__file__), base / "runner.py")
        shutil.copyfile(HERE.parent / "isolated-deb-acceptance.py", base / "support.py")
        command = ["unshare", "--user", "--map-root-user", "--mount", "--pid", "--fork", "--kill-child", "--net",
                   "bwrap", "--die-with-parent", "--ro-bind", "/usr", "/usr"]
        for name in ("bin", "sbin", "lib", "lib64"):
            command.extend(["--symlink", f"usr/{name}", f"/{name}"])
        command.extend(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/run",
                        "--dir", "/etc", "--dir", "/home", "--bind", str(base), "/evidence", "--chdir", "/evidence",
                        "--clearenv", "--setenv", "PATH", "/usr/sbin:/usr/bin:/sbin:/bin", "--setenv", "LC_ALL", "C",
                        "--setenv", "PYTHONDONTWRITEBYTECODE", "1", "--", "/usr/bin/python3", "/evidence/runner.py", "--internal-worker"])
        support.run(command, base / "namespace.log", timeout=240)
        evidence["lifecycle"] = json.loads((base / "lifecycle.json").read_text())
        require(evidence["lifecycle"]["passed"], "Lifecycle did not complete")
        evidence["passed"] = True
        evidence["retained_candidate"] = str(root / PREFIX)
        print(f"Passed. Retained candidate: {evidence['retained_candidate']}")
    except BaseException as error:
        evidence["error"] = str(error)
        raise
    finally:
        support.write_json(base / "evidence.json", evidence)
        print(f"Evidence retained: {base / 'evidence.json'}", flush=True)


if __name__ == "__main__":
    worker() if WORKER else main()
