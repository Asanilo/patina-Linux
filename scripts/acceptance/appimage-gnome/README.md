# Independent GNOME AppImage acceptance

These guest-only scripts exercise an actual GNOME login in a disposable KVM VM.
They must not be run in the host user's profile. The scripts require UID 1000,
`/home/tester`, no installed Patina DEB, and an explicit VM marker. Results do not
claim compatibility with every GNOME version, manual password authentication,
or public updater distribution.

The validated setup is Ubuntu 22.04 amd64, GNOME 42.9, a real GDM Wayland session,
4 virtual CPUs, 5 GiB RAM, virtio GPU/network and a private disk. QEMU runs in a
tool container with only `/dev/kvm` exposed, no host-directory mounts and no
published ports. SSH forwarding is bound to loopback inside that container.
Use Ubuntu's official cloud image and verify its SHA256 against the accompanying
HTTPS checksum file before creating a writable qcow2 overlay.

Prepare the guest with `gnome-shell`, `gnome-session`, `gdm3`, `xserver-xorg`,
`xwayland`, `libgl1-mesa-dri`, `libfuse2`, `libwebkit2gtk-4.1-0`,
`libayatana-appindicator3-1`, `libpulse0`, `libx11-xcb1`, `libxcb-randr0`,
`libxcb-screensaver0`, `python3-gi`, `python3-dbus`, `gir1.2-atspi-2.0`,
`at-spi2-core`, `libatk-adaptor` and `dbus-x11`.
Create the `tester` user, install this repository's GNOME extension in that user's
extension directory, enable it in GSettings, and enable accessibility. Disable
idle lock only in the test VM. Write `isolated-appimage-acceptance` followed by a
newline to `/etc/patina-acceptance-vm`.

For the tested login path, set GDM `AutomaticLoginEnable=true`,
`AutomaticLogin=tester`, `WaylandEnable=true` and select `gnome-wayland` in the
test user's AccountsService session. Cold-boot the guest and confirm logind
reports `Type=wayland`, `Class=user`, `Service=gdm-autologin`, local and active.
Merely restarting GDM can leave the old GNOME user session alive and cause an
Xorg fallback; the script rejects that condition. Save a complete pre-install
VM snapshot so fixture failures can be replayed from clean state.

Copy the candidate to `/home/tester/Applications/Patina.AppImage`, with executable
permissions and an owner-only parent directory. As `tester` in the guest:

```bash
python3 guest.py first 1.9.0-beta.21
```

This starts the actual FUSE AppImage, waits for the real systemd owner handoff,
verifies the managed unit and version, exits through its DBus tray menu and
checks continued sampling. It never substitutes a fake service manager.

Reboot **the VM**, leaving Patina's generated autostart entry and user unit intact.
After GDM logs the test user in, run only:

```bash
python3 guest.py login 1.9.0-beta.21
```

The login phase does not launch Desktop or start the daemon. It requires a new
boot/session pair and service invocation, a FUSE Desktop with `--autostart`, then
normal exit and continued background sampling. GNOME session numbers can repeat
across boots, so session ID alone is insufficient evidence. The current GTK
AppImage hook forces X11; the scripts record the XWayland client backend separately
from the real Wayland login session.

Exit GNOME overview through the VM console before running `python3 sample.py`.
It opens a synthetic native Wayland window, matches the extension's title/PID,
and checks that the daemon recorded that exact window. Overview correctly hides
window facts and therefore cannot be used as a normal foreground sampling case.

Evidence is written under `/home/tester/acceptance`. Export it before deleting or
restoring the VM. Keep failed runs and distinguish fixture failures from product
failures. Private bootstrap commands and cloud-image identity for the executed
run are retained with the working document's acceptance evidence.

For production-key upgrade acceptance, first obtain the reviewed Actions
candidate, signature and `candidate.json` from `appimage-acceptance.yml`.
The ignored Rust test `platform::linux::appimage_update::tests::production_signed_appimage_upgrade`
accepts an owner-only `PATINA_SIGNED_UPGRADE_TEST_ROOT` containing:

- `marker`: `isolated-signed-upgrade` followed by a newline;
- `installed.AppImage`: the actual older package at its installation path;
- `candidate.AppImage` and `candidate.AppImage.sig`: the signed candidate;
- `input.json`: `old_version`, `new_version`, `old_sha256`, `new_sha256`.

The test uses the application's configured production public key and real Tauri
download verification, rejects a tampered download before changing the old file,
then performs the production atomic install. This is isolated loopback delivery,
not a public-channel test. Afterward, separately launch the installed candidate,
use the supported background-version reload flow, and verify the new daemon
version, persistent runtime pointer, unchanged history, autostart and recovery
material. Retaining an old binary does not authorize database downgrade.
