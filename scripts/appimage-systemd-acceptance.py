#!/usr/bin/env python3
"""Opt-in real systemd AppImage acceptance in a disposable Ubuntu Docker container.

Usage: python3 scripts/appimage-systemd-acceptance.py /absolute/Patina.AppImage
Requires an amd64 Linux Docker host with cgroup v2. Builds a local test image.
Container gets SYS_ADMIN and relaxed seccomp/AppArmor to run nested systemd,
but no host mounts, extra devices, network, Docker socket or production data.
Tests installation/handoff, UI exit/reopen, crash recovery and container restart.
Does not validate GNOME login, FUSE mounting or formally signed upgrades.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    image = Path(sys.argv[1]).resolve(strict=True)
    sources = Path(__file__).resolve().parent / 'acceptance/appimage-systemd'
    evidence = Path(tempfile.mkdtemp(prefix='patina-appimage-systemd-'))
    name = 'patina-acceptance-' + uuid.uuid4().hex[:12]
    tag = 'patina-acceptance-systemd:ubuntu22'
    result = {'passed': False, 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(),
              'scope': 'Real isolated systemd; no graphical login/FUSE/formal upgrade'}
    print('EVIDENCE=' + str(evidence), flush=True)

    def docker(*args, timeout=180, check=True):
        with (evidence / 'driver.log').open('a') as log:
            return subprocess.run(['docker', *args], stdout=log, stderr=subprocess.STDOUT,
                                  check=check, timeout=timeout)

    def user(*args):
        return docker('exec', '-u', '1000', '-e', 'HOME=/home/tester',
                      '-e', 'XDG_RUNTIME_DIR=/run/user/1000',
                      '-e', 'DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus',
                      name, *args)

    def wait_manager():
        end = time.monotonic() + 60
        while time.monotonic() < end:
            if docker('exec', name, 'test', '-S', '/run/user/1000/bus', check=False).returncode == 0:
                return
            time.sleep(1)
        raise RuntimeError('Real user manager failed to boot; see driver.log')

    created = False
    try:
        docker('build', '--platform', 'linux/amd64', '-t', tag, str(sources), timeout=1800)
        docker('create', '--name', name, '--platform', 'linux/amd64', '--network', 'none',
               '--cap-add', 'SYS_ADMIN', '--security-opt', 'apparmor=unconfined',
               '--security-opt', 'seccomp=unconfined', '--cgroupns', 'private',
               '--tmpfs', '/run', '--tmpfs', '/tmp:rw,exec,mode=1777', '--shm-size', '256m',
               tag, '/bin/bash', '-c', 'mount -o remount,rw /sys/fs/cgroup && exec /sbin/init')
        created = True
        docker('cp', str(image), name + ':/candidate.AppImage')
        docker('cp', str(sources / 'accept.py'), name + ':/accept.py')
        docker('start', name)
        wait_manager()
        user('xvfb-run', '-a', '-s', '-screen 0 1280x720x24 -extension GLX',
             'python3', '/accept.py', 'first')
        docker('restart', '--timeout', '25', name)
        wait_manager()
        user('python3', '/accept.py', 'cold')
        docker('cp', name + ':/home/tester/acceptance/.', str(evidence))
        for phase in ['first', 'cold']:
            result[phase] = json.loads((evidence / (phase + '.json')).read_text())
            assert result[phase]['passed']
        assert result['first']['sha256'] == result['sha256']
        result['passed'] = True
    finally:
        if created:
            docker('inspect', name, check=False)
            docker('exec', name, 'journalctl', '--no-pager', '-n', '100', check=False)
            docker('stop', '--timeout', '25', name, check=False)
            docker('cp', name + ':/home/tester/acceptance/.', str(evidence), check=False)
            docker('logs', name, check=False)
            docker('rm', name, check=False)
        (evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print('PASSED: real systemd handoff, recovery and container cold start', flush=True)


if __name__ == '__main__':
    main()
