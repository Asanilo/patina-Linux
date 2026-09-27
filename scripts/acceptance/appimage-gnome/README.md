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

For an isolated old-to-new package upgrade fixture, an optional third argument
selects another regular AppImage path under `/home/tester`, for example
`python3 guest.py first 1.9.0-beta.20 /home/tester/upgrade/installed.AppImage`.
Use the same path with `guest.py login` after installing the newer verified
package; the default path above remains the normal first-install case.

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
across boots, so session ID alone is insufficient evidence. The script records
`GDK_BACKEND` as an override (`auto` when unset) and both display addresses;
these environment values alone do not establish the client's actual backend.
Tauri CLI 2.12 no longer forces X11 in its AppImage GTK hook. Record the actual
client connection separately when that distinction matters.

Exit GNOME overview through the VM console before running `python3 sample.py`.
It opens a synthetic native Wayland window, matches the extension's title/PID,
and checks that the daemon recorded that exact window. Overview correctly hides
window facts and therefore cannot be used as a normal foreground sampling case.

Evidence is written under `/home/tester/acceptance`. Export it before deleting or
restoring the VM. Keep failed runs and distinguish fixture failures from product
failures. Private bootstrap commands and cloud-image identity for the executed
run are retained with the working document's acceptance evidence.

For a guest-only daemon crash/restart check after the first install, exit GNOME
overview through the VM console and run `python3 fault.py` as `tester`. It starts
one synthetic native Wayland window, sends SIGKILL only to the current isolated
`patinad.service` MainPID, and checks the new invocation, single daemon process,
session non-overlap and bounded duration, resumed successful sampling and SQLite
integrity. It writes `fault-<run-id>.json` and a synthetic window log under the
guest acceptance directory. It does not suspend hardware, test arbitrary crash
timings, or replace the public update-channel gate.

To check Desktop crash/reopen independently, run `python3 client_fault.py` in the
same isolated guest after a cold GDM login has opened the AppImage Desktop. It
SIGKILLs only that Desktop, confirms the daemon invocation and successful
sampling continue without a UI, reopens the same AppImage, then checks that the
daemon was not replaced. It writes `client-fault.json` in the guest acceptance
directory and never operates on the host Desktop.

For extension loss/recovery, run `python3 extension_fault.py` in the guest after
the first install. It temporarily disables only the test user's GNOME extension,
checks both D-Bus names disappear, verifies successful sampling stops and the
daemon reports an unavailable window provider, then restores the original
enabled-extension preference and checks recovery without daemon replacement.
It writes `extension-fault.json`; exit overview and run
`python3 sample.py post-extension` for a separate real window record after
recovery. The optional sample suffix keeps earlier witness results intact.

For the real GDM Wayland session's lock boundary, run `python3 lock.py` in the
isolated guest. It uses logind to lock and unlock only that guest session, checks
the extension's locked snapshot, stopped/resumed successful sampling and the
unchanged daemon invocation, then writes `lock.json`. Exit overview and record a
fresh window with `sample.py post-lock` afterward. This does not validate host
hardware suspend or a password-authenticated unlock.

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

For an old-to-new guest lifecycle run, start the old AppImage using `guest.py first`
with the optional installation path, record a synthetic window with `sample.py`,
then run `upgrade_baseline.py`. After the ignored production-signed upgrade test
passes against the owner-only `/home/tester/upgrade` root, launch the newly
installed AppImage and use the actual Settings confirmation to reload the
background service. Check old history, preferences, schema, unit, retained old
package and no-UI sampling before reboot. After GDM cold login, run
`guest.py login` against the same installed path, record a separate window with
`sample.py signed-cold`, and run `upgrade_cold_final.py`. Preserve the pre-install
snapshot and all failures. This sequence still does not test a public updater
manifest or publicly served download.

If the VM's Settings WebView is blank, preserve its Desktop/WebKit log and do not
count the UI upgrade flow as passed. After the signed install has staged the new
runtime while the old daemon is still running, `python3 upgrade_api_reload.py`
can check the supported local API restart, ticket completion, old data and package
retention. Its output explicitly records `settings_ui_tested: false`; a successful
backend check does not satisfy the Settings, core-page or public distribution gates.

For a real GNOME title-privacy check, save `captureTitle=false` for the isolated
`python3` app in Classification and run `python3 privacy.py [run-id]` as `tester`.
It requires the VM marker, a real foreground Wayland window and a healthy
non-AFK tracking session. It verifies that a session is recorded without a
window title or title sample, and writes only aggregate results under the guest
acceptance directory. Give the VM a real virtual keyboard/mouse event before
this probe if the GDM session has been idle; opening a window alone does not
clear GNOME's idle state.

To check a rebuilt local daemon without replacing the signed AppImage, copy its
exact bytes to `/home/tester/acceptance/candidate-patinad`, record SHA256, then
run `python3 candidate_daemon.py /home/tester/acceptance/candidate-patinad
<sha256> <run-id>` in the same guest. The script temporarily selects that binary
through a user-service drop-in, checks title privacy and the synthetic imported
Summary exclusion, and restores the original AppImage service command even on
failure. This is an isolated source-candidate check; it does not certify a new
packaged or signed AppImage.
