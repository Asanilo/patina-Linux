#!/usr/bin/env python3
"""Verify signed Linux bundles against the configured updater public key.

Usage: verify-release-bundles.py BUNDLE_DIRECTORY TAURI_CONFIG deb|appimage,deb
Requires minisign. Reads no private signing key and writes only temporary files.
"""

import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def regular_file(path: Path) -> None:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"missing regular bundle or signature: {path}")


def main() -> None:
    if len(sys.argv) != 4 or sys.argv[3] not in ("deb", "appimage,deb"):
        raise SystemExit(__doc__)

    bundle_root = Path(sys.argv[1])
    config = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
    version = config["version"]
    public_key = base64.b64decode(config["plugins"]["updater"]["pubkey"], validate=True)
    targets = ["deb"] if sys.argv[3] == "deb" else ["appimage", "deb"]

    with tempfile.TemporaryDirectory(prefix="patina-release-signatures-") as temp:
        root = Path(temp)
        key = root / "public.key"
        key.write_bytes(public_key)

        for target in targets:
            suffix = "AppImage" if target == "appimage" else "deb"
            bundle = bundle_root / target / f"Patina_{version}_amd64.{suffix}"
            signature = Path(f"{bundle}.sig")
            regular_file(bundle)
            regular_file(signature)
            raw_signature = base64.b64decode(signature.read_text().strip(), validate=True)
            signature_file = root / f"{target}.sig"
            signature_file.write_bytes(raw_signature)
            result = subprocess.run(
                ["minisign", "-V", "-m", str(bundle), "-p", str(key), "-x", str(signature_file)],
                capture_output=True,
            )
            if result.returncode:
                raise SystemExit(f"signature verification failed for {bundle.name}")
            digest = hashlib.sha256()
            with bundle.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            print(f"verified {bundle.name} sha256={digest.hexdigest()}")


if __name__ == "__main__":
    main()
