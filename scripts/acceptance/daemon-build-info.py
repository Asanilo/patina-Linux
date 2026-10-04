#!/usr/bin/env python3
"""Verify static metadata without creating any profile, lease or database files."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve(strict=True)
with tempfile.TemporaryDirectory(prefix="patina-build-info-") as temporary:
    root = Path(temporary)
    env = os.environ.copy()
    for key, directory in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                           ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "runtime")]:
        env[key] = str(root / directory)
    metadata = subprocess.run([str(binary), "--build-info"], env=env, capture_output=True,
                              timeout=5, check=True)
    info = json.loads(metadata.stdout)
    assert info["format_version"] == 1 and info["desktop_feature"] is False
    version = subprocess.run([str(binary), "--version"], env=env, capture_output=True,
                             timeout=5, check=True)
    assert version.stdout.decode().strip() == "patinad " + info["package_version"]
    assert metadata.stderr == b"" and version.stderr == b""
    invalid = subprocess.run([str(binary), "--build-info", "--profile", "production"], env=env,
                             capture_output=True, timeout=5, check=False)
    assert invalid.returncode != 0 and b"unknown patinad option" in invalid.stderr
    assert list(root.iterdir()) == [], "metadata inspection touched a profile or runtime directory"
    print(json.dumps({"passed": True, "profile_files_created": False, "build": info}))
