"""Canonicalize a base64 signing secret from stdin; capture stdout, never log it.

Legacy Secrets may contain transport damage after a complete key. The existing
release workflow uses GNU base64's decoded output even on error. Accept that
compatibility path only for a complete supported minisign key box, and require
signing-preflight.py to prove its identity before building. No key file is made.
"""
import base64
import binascii
import subprocess
import sys


def main():
    raw = sys.stdin.buffer.read()
    compact = b''.join(raw.split()).rstrip(b'=')
    if not compact:
        raise SystemExit('Signing secret is empty')
    try:
        decoded = base64.b64decode(compact + b'=' * (-len(compact) % 4), validate=True)
    except (ValueError, binascii.Error):
        legacy = subprocess.run(['base64', '--decode'], input=raw, capture_output=True)
        decoded = legacy.stdout
        print('Legacy secret encoding detected; complete key-box and production-key proof required', file=sys.stderr)
    try:
        lines = decoded.splitlines()
        assert len(lines) >= 2 and lines[0].startswith(b'untrusted comment: ')
        assert len(base64.b64decode(lines[1], validate=True)) == 158
    except (AssertionError, ValueError, binascii.Error):
        raise SystemExit('Signing secret does not contain a complete supported minisign key') from None
    if len(lines) > 2:
        print('Ignoring transport trailer after a complete key box; production-key proof required', file=sys.stderr)
    sys.stdout.buffer.write(base64.b64encode(b'\n'.join(lines[:2]) + b'\n'))


if __name__ == '__main__':
    main()
