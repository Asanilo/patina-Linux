#!/usr/bin/env python3
"""Exercise explicit selection of two verified local candidate directories.

Usage: standalone-selection.py controller source_a manifest_sha_a source_b manifest_sha_b [--allow-debug]
No service or profile is installed. Digests must come from the caller's verified candidates.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    args = sys.argv[1:]
    allow_debug = args[-1:] == ["--allow-debug"]
    if allow_debug:
        args.pop()
    if len(args) != 5:
        raise SystemExit(__doc__)
    controller, source_a, digest_a, source_b, digest_b = args
    controller = str(Path(controller).resolve(strict=True))
    source_a, source_b = (str(Path(source).resolve(strict=True)) for source in (source_a, source_b))
    assert digest_a != digest_b, "two distinct candidate identities are required"
    root = Path(tempfile.mkdtemp(prefix="patina-selection-"))
    runtime = root / "runtime"
    env = os.environ.copy()
    for key, name in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                      ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "xdg-runtime")]:
        env[key] = str(root / name)

    def invoke(*command, success=True):
        flags = ["--allow-debug"] if allow_debug else []
        result = subprocess.run([controller, *command, "--runtime-root", str(runtime), *flags],
                                env=env, capture_output=True, timeout=60)
        if success:
            assert result.returncode == 0, result.stderr.decode()
            return json.loads(result.stdout)
        assert result.returncode != 0, "expected refusal"
        return result.stderr.decode()

    invoke("--inspect-runtime", success=False)
    assert not runtime.exists(), "inspection initialized a runtime root"
    first = invoke("--stage-runtime", source_a, "--manifest-sha256", digest_a)
    second = invoke("--stage-runtime", source_b, "--manifest-sha256", digest_b)
    assert first["binary_sha256"] != second["binary_sha256"], "use two different real builds"
    assert invoke("--inspect-runtime") == {"selected": None}
    assert invoke("--select-runtime", digest_a, "--expected-current", "none") == first
    assert invoke("--inspect-runtime")["selected"] == first
    assert invoke("--select-runtime", digest_b, "--expected-current", digest_a) == second
    assert invoke("--inspect-runtime")["selected"] == second
    inode = (runtime / "current").lstat().st_ino
    assert invoke("--select-runtime", digest_b, "--expected-current", digest_b) == second
    assert (runtime / "current").lstat().st_ino == inode
    invoke("--select-runtime", digest_a, "--expected-current", digest_a, success=False)
    assert invoke("--inspect-runtime")["selected"] == second
    binary = runtime / "current/bin/patinad"
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == second["binary_sha256"]
    metadata = subprocess.run([str(binary), "--build-info"], env=env, capture_output=True,
                              check=True, timeout=10)
    assert json.loads(metadata.stdout) == second["build"]
    assert len(list((runtime / "versions").iterdir())) == 2
    assert list(root.iterdir()) == [runtime], "selection or metadata touched a profile"
    result = {"passed": True, "root": str(root), "selected_binary": str(binary),
              "previous": first, "selected": second, "stale_selection_rejected": True,
              "repeat_selection_idempotent": True, "profile_files_created": False,
              "service_activated": False}
    (root / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
