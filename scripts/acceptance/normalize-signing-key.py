"""Canonicalize a base64 signing secret from stdin; capture stdout, never log it.

Existing Secrets may contain whitespace or redundant trailing padding accepted
by older tooling. Reject non-base64 data and internal padding instead of using
a decoder's partial output after an error. No private key file is created.
"""
import base64
import binascii
import sys


def main():
    compact = b''.join(sys.stdin.buffer.read().split()).rstrip(b'=')
    if not compact:
        raise SystemExit('Signing secret is empty')
    try:
        decoded = base64.b64decode(compact + b'=' * (-len(compact) % 4), validate=True)
    except (ValueError, binascii.Error):
        raise SystemExit('Signing secret is not valid base64') from None
    sys.stdout.buffer.write(base64.b64encode(decoded))


if __name__ == '__main__':
    main()
