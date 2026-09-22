# Linux Development Setup

This page records Linux development and verification procedures. `main` is the
primary Linux product branch and includes the daemon separation developed on
`feature/patinad-daemon`; subsequent product work continues on `main`.
Daemon-specific procedures apply to this baseline and its selected candidates.
The source merge does not change beta or AppImage release gates.

## Build Storage

Rust artifacts are under `src-tauri/target`, not the installed application's data directory. Repeated builds, tests, compiler versions and feature combinations can leave multiple artifact variants. Incremental compilation stores additional compiler state; full debug information also increases dependency and binary size. See the [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html).

The repository's dev profile uses `debug = 1` and `incremental = false`; tests inherit these defaults. This retains limited debug information for backtraces while avoiding the largest incremental caches. Recompiling local Rust code can take longer, and inspecting variables/types in a debugger requires full debug information. Release settings are unchanged. Changing profiles does not delete artifacts from previous builds.

Inspect before cleaning, from the repository root:

```bash
du -h --max-depth=2 src-tauri/target
df -h .
```

Stop Cargo/rustc builds and source-launched development processes before cleaning. For a complete debug/test cache reset, leaving release bundles intact:

```bash
cargo clean --manifest-path src-tauri/Cargo.toml --profile dev
```

For a narrower reset, only `src-tauri/target/debug/incremental` is disposable incremental compiler state; inspect the exact path before deleting it. Do not delete application profiles, databases, backup archives or signing keys. Avoid an unrestricted `cargo clean` when local DEBs under `target/release/bundle` are needed for installation or rollback. Cleaning build artifacts does not uninstall the DEB or erase installed application data, but the next development build must recreate them.

When full debugger information is needed temporarily, use `CARGO_PROFILE_DEV_DEBUG=2` (and `CARGO_PROFILE_TEST_DEBUG=2` for tests). `CARGO_INCREMENTAL=1` temporarily restores incremental compilation. These overrides can create additional cached variants; they are not the normal disk-conscious workflow. Prefer `cargo check` while iterating and run the required tests/build gates before delivery, rather than repackaging DEBs after every edit.

## Reloading An Installed Daemon

Installing a DEB replaces files on disk, not necessarily the running daemon. Opening a compatible Desktop also does not automatically restart it. Check the running version, not just the package version.

Daemon-backed Desktop exposes Desktop/Daemon versions in Settings -> Diagnostics and an explicitly confirmed reload action for a completed Production managed-client cutover. The action appears only for a known version difference and an available systemd service-lifecycle capability. It briefly interrupts tracking, uses the existing graceful restart API, and does not download packages or change login preferences. Candidate implementation and installed-version evidence live in the [current daemon checklist](./working/2026-07-10-patinad-runtime-design.md).

Success requires a completed matching restart ticket, a new daemon instance, the Desktop version, and tracking readiness. Rejected, ambiguous or timed-out requests are not automatically resubmitted; inspect service state before trying again. If the disk still contains a different daemon version, reloading cannot install the missing version. Do not substitute a forced process kill for this flow.

## GNOME Wayland Window Tracking

Patina uses a GNOME Shell extension on GNOME Wayland to read the focused window through session D-Bus.

Source:

```text
extensions/gnome-shell/patina-window-tracker@patina/
```

Check and build:

```bash
npm run extension:gnome:check
npm run extension:gnome:build
```

Install into the current user's GNOME Shell extension directory:

```bash
npm run extension:gnome:install
gnome-extensions enable patina-window-tracker@patina
```

If GNOME Shell has cached an older copy, log out and back in.

Verify the D-Bus endpoint:

```bash
gdbus call --session \
  --dest org.patina.WindowTracker \
  --object-path /org/patina/WindowTracker \
  --method org.patina.WindowTracker.GetFocusedWindow
```

Expected behavior:

- On GNOME Wayland, Patina should use `org.patina.WindowTracker`.
- On X11, Patina should use the X11 fallback path. This path exists in code but has less release verification than GNOME Wayland.
- KDE and wlroots Wayland are not currently promised.

Settings -> Diagnostics shows the current window-tracking provider and distinguishes unavailable GNOME extension D-Bus from unsupported non-GNOME Wayland compositors.

## Local API

Patina starts a localhost API for external tools:

```text
http://127.0.0.1:14840
```

Token path:

```text
${XDG_DATA_HOME:-~/.local/share}/Patina/api_token
```

This local API token/port is separate from the browser Web Sync port/token shown in Settings -> Interface. The local API defaults to port `14840`, and Settings can update the local API port/token.

Example:

```bash
export PATINA_API_BASE="http://127.0.0.1:14840"
export PATINA_API_TOKEN="$(cat "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token")"

curl -s "$PATINA_API_BASE/api/v1/diagnostics" \
  -H "Authorization: Bearer $PATINA_API_TOKEN"
```

Settings -> Diagnostics also shows whether the API is listening and where the token file lives.

## Local Storage Paths And Migration

Production builds keep non-movable storage control metadata under:

```text
${XDG_CONFIG_HOME:-~/.config}/Patina
```

The default activity data and WebView profile root is:

```text
${XDG_DATA_HOME:-~/.local/share}/Patina
```

The stable control directory contains versioned data/WebView anchors, pending migration metadata, and maintenance state. The default data root contains `patina.db`, local backups, and the local API token. Moving activity data relocates the database and managed backups; temporary downloads are not migrated, and the API token remains at the stable default product data root. Activity data and WebView data can be moved independently from Settings -> Data Safety -> Local storage.

Storage changes use a restart boundary:

1. Settings previews the target and available space without mutating it.
2. Confirmation writes a pending operation and backup metadata.
3. Patina exits only when the user chooses to restart.
4. On the next launch, migration runs before SQLite or either WebView is opened.
5. The copied database must pass SQLite integrity, schema, and row-count checks before the target is promoted.

In managed daemon mode, Desktop performs offline maintenance as a local host operation; it never starts an embedded tracker. All Desktop instances hold a shared storage-access lock, and startup maintenance takes it exclusively before opening SQLite or creating WebViews. On Linux, bounded checks of same-user process/handle metadata also wait for older Desktop versions and WebKit processes that do not know this lock. A conflicting process produces an error rather than being killed; close the other instance before retrying.

Restoring either storage location may rejoin the shared default directory while the other location remains there. This exception applies only to that exact default root. Custom shared locations and parent/child overlap remain rejected; migration uses separate managed file lists so restoring one location preserves the other location's files.

For an activity-data move, the host stops the managed service, takes a temporary maintenance runtime lease, executes the verified migration, releases that lease, and starts the daemon again. A WebView-only move or cache clear does not stop tracking. The appointment itself leaves the daemon on its original paths until maintenance begins, including when the user chooses to restart later. Unmanaged daemon preview has no service-control contract and rejects these requests.

A private journal in the stable control directory records file promotion and anchor changes. Interrupted migration must recover or finish its recorded operation before a daemon can open the database. Do not manually delete the journal or pending request to bypass a recovery error. Ordinary failures retain the original source and report the concrete reason; current automated and installed evidence is recorded in the [daemon checklist](./working/2026-07-10-patinad-runtime-design.md).

Custom activity storage is fail-closed. If its anchor is invalid, its root is unavailable, or `patina.db` is missing, startup reports the storage error and does not create a database in the default directory. This prevents a missing mount from looking like an empty Patina installation.

After a successful migration, previous source directories are retained and shown in Settings. Patina does not automatically delete them. Verify the active database and backups before removing an old directory manually.

WebView maintenance deletes only the exact `WebKitCache` directory below the active WebView root, and only during restart maintenance. It does not expose a general recursive-delete command and does not remove cookies, local storage, configuration, or the whole WebView profile.

## Browser Web Sync

Browser activity sync is configured from Settings -> Interface. The page can copy the extension configuration and shows separate Firefox/Zen and Chromium installation paths.

Supported development paths:

- Firefox / Zen: `extensions/firefox`
- Chromium / Chrome / Edge: `extensions/chromium`

Persistent Firefox/Zen installation requires a signed XPI. Temporary development loading through `about:debugging#/runtime/this-firefox` is not persistent across browser restarts.

## Linux Autostart Diagnostics

Settings -> Diagnostics shows the current `~/.config/autostart/Patina.desktop` state. When the `Exec` path points to a stale launcher, such as a terminal executable used during development, the Desktop Integration row exposes a repair action that rewrites it to the current Patina executable with `--autostart`.

## patinad Managed Service

The Debian bundle now includes both:

```text
/usr/bin/patinad
/usr/lib/systemd/user/patinad.service
```

Package maintainer scripts only install the unit; they do not enable or start it. Before owner cutover, Desktop remains the Production tracking owner. A daemon-backed DEB starts the transition from the current user's Desktop session, persists an owner-only reservation, releases the embedded runtime, and only then enables and starts the user service. After the reservation reaches `completed`, `patinad` is the Production tracking owner and Desktop is its client. Do not manually start the Production unit while an embedded Desktop still owns the same profile.

Validate the source packaging contract without installing it:

```bash
npm run test:release
```

After building a Debian package, validate the actual package payload without installing it:

```bash
npm run release:verify-daemon-deb -- \
  src-tauri/target/release/bundle/deb/Patina_<version>_amd64.deb \
  <version>
```

This checks the package identity, both executables, the default-disabled user unit, its required safety settings, the GNOME extension UUID, and maintainer scripts that could otherwise enable the service outside the first-launch handoff.

After installing a daemon-backed DEB, systemd can also validate the real installed executable and unit paths:

```bash
systemd-analyze verify --user /usr/lib/systemd/user/patinad.service
```

For an isolated manual preview, use a non-production profile:

```bash
src-tauri/target/debug/patinad --profile dev --serve-api --track --port 0
```

A manual preview reports startup stage `tracking-preview`, exposes service state, and rejects controlled restart because no supervisor can bring it back. A systemd-managed instance reports `managed-service` and advertises `service-lifecycle`; restart persists a ticket, responds before shutdown, exits through the graceful runtime path, and the next instance confirms the same ticket. A daemon without `--track` reports `read-only`.

## Linux Release Bundles

### Daemon-Backed AppImage Runtime (Development)

Daemon-backed AppImage support on `main` is implemented but remains behind the
independent packaging/acceptance gate; prerelease publishing is still DEB-only.
The AppImage includes both Desktop and `usr/bin/patinad`. `AppRun --patinad`
executes the bundled daemon before Desktop initialization. First launch stages
the entire AppDir, including its libraries, under the stable product data root:

```text
~/.local/share/Patina/runtime-appimage/versions/<package-sha256>/
~/.local/share/Patina/runtime-appimage/current -> versions/<package-sha256>
~/.config/systemd/user/patinad.service
```

The store follows XDG roots, not a relocated activity database. Copying is bounded
to 2 GiB, 30,000 entries and depth 64; external/dangling links and special files
are rejected. The root `.DirIcon` launcher alias is omitted without resolving it:
Tauri may encode it as an absolute link to the build machine. The real icon and
runtime files remain subject to normal validation. A private installation lock
serializes staging. Only a complete,
synced runtime that passes `--patinad --version` can become `current` via atomic
symlink replacement. Older packages do not downgrade it; different content with
the same version is rejected. Use a new candidate version for rebuilt packages.

Production and isolated tests share the same preflight: it replaces inherited
AppImage/GTK loader paths with the staged AppDir environment, allows 10 seconds,
and accepts only the exact version line (at most 128 bytes) and a successful exit.
Wrong versions, excessive output, nonzero exits and timeouts leave `current`
unchanged. The installation guard explicitly unlocks after cleanup, including
when another concurrent spawn briefly inherits its file descriptor. The generated
unit is also checked with the real systemd parser using paths containing spaces,
percent signs and dollar signs; this check does not install or start a service.

There is only one `patinad.service` and one profile lease. An existing managed
AppImage user unit keeps ownership; otherwise a packaged DEB unit is reused if
available. In that case upgrade the DEB to upgrade its daemon, rather than
expecting the AppImage to replace `/usr/bin/patinad`. Masks/custom user units are
preserved and setup fails explicitly. Systemd and Desktop must agree on the
configuration root; DEB reuse additionally requires matching data roots. AppImage
portable HOME/config directories are not supported for this shared user service.
Creating/reloading the unit does not enable or start tracking: existing owner
handoff, startup preferences and explicit reload confirmation remain authoritative.

AppImage updates still use Tauri's verified download and configured public key.
Only the successful verified buffer enters the replacement helper. It writes and
syncs a private same-directory file, retains the previous image as
`.patina-previous-<sha256>.AppImage`, then atomically renames the new file. The
target and directory must not be writable by other users. Filesystems without
hard-link/rename support fail instead of falling back to an in-place overwrite.
A final directory sync failure can report an error after the complete new image
has already been installed; retry is idempotent. Neither recovery images nor old
runtime versions are automatically deleted, so reserve disk space for both.

Before pointer activation, copy or version-preflight failure leaves the previous
runtime selected. Retained files are recovery material, not a promise of automatic
database downgrade: after a new daemon has opened/migrated data, do not point an
older binary at that data. Use a compatible repaired package or the existing
validated backup/restore flow. A first-launch failure must be corrected before
claiming AppImage acceptance; unit tests alone do not satisfy the release gate.

For an unsigned **local test artifact**, without changing release signing policy:

```bash
npm run tauri -- build --bundles appimage --config '{"bundle":{"createUpdaterArtifacts":false}}' --ci
PATINA_APPIMAGE_TEST_SOURCE=/absolute/path/Patina.AppDir \
PATINA_APPIMAGE_TEST_IMAGE=/absolute/path/Patina.AppImage \
  cargo test --manifest-path src-tauri/Cargo.toml --lib \
  built_appdir_runs_from_durable_store -- --ignored --nocapture
```

The opt-in test uses only a new private temporary runtime and prints its retained
`PERSISTED_APPDIR`. To run the existing isolated local/remote systemd restore
tests against it, set `PATINA_SYSTEMD_TEST_BINARY=<PERSISTED_APPDIR>/AppRun` and
`PATINA_SYSTEMD_TEST_APPIMAGE_LAUNCHER=1`. Never supply the production unit or data
directory. This does not replace real first-launch, login or signed-release tests.

### Packaged Desktop Startup Validation

Before accepting a newly built AppImage, exercise the packaged Desktop startup
path (not only the build-directory AppDir):

```bash
/usr/bin/python3 scripts/appimage-startup-acceptance.py /absolute/Patina.AppImage
# Also exercise the daemon/unit installed by isolated-deb-acceptance.py:
/usr/bin/python3 scripts/appimage-startup-acceptance.py /absolute/Patina.AppImage \
  --installed-deb-root /absolute/private-dpkg-root
```

The opt-in runner requires bubblewrap, Xvfb, dbus-run-session and Python GI.
It uses a private filesystem/PID/network namespace, X11 and a non-activating
D-Bus with a fixture systemd manager. It checks standalone runtime staging,
packaged DEB unit selection, preservation of custom units and mismatched profile
root rejection. It also runs the actual Desktop owner-cutover restart and packaged
daemon, checks managed API readiness after Desktop exit/reopen, and verifies clean
daemon shutdown and SQLite integrity. The optional private dpkg root supplies the
real installed DEB daemon and unit for a second handoff case; AppImage must reuse
them without staging a second runtime. The manager simulates systemd control and
INVOCATION_ID; these cases do not prove installed systemd takeover, UI behavior,
login or formal signed upgrades. Evidence and failures are
retained under the printed `/tmp/patina-appimage-startup-*` directory.

### AppImage With A Real Isolated User Manager

To check the standalone path without uninstalling a working host DEB:

```bash
python3 scripts/appimage-systemd-acceptance.py /absolute/Patina.AppImage
```

This opt-in runner requires an amd64 Linux Docker host with cgroup v2 and builds
an Ubuntu 22.04 test image. It creates a disposable user with a real systemd user
manager and no Patina DEB. The actual Desktop stages its runtime and unit and
performs owner cutover. The runner checks managed API readiness, Desktop
exit/reopen without daemon replacement, SIGKILL recovery, clean stop and database
integrity. It then restarts the container and verifies the enabled daemon starts
without opening Desktop or manually starting the service.
An isolated test-only unit drop-in delays daemon startup by eleven seconds to
cover recovery beyond the former ten-second credential deadline. The runner also
reopens Desktop five times using the generated autostart command after closing its original
process, rejecting commands that point into temporary extraction directories.
Standalone autostart uses the original AppImage package; moving or deleting that
package requires setting up autostart again. DEB coexistence uses the installed
Desktop executable.
The headless display uses software rendering, synchronous GDK X11 calls, and
disables Xvfb reset between clients. This isolates launcher/service lifecycle checks from GPU availability
and display regeneration; it does not validate graphical-session rendering.

Nested systemd requires SYS_ADMIN and relaxed container seccomp/AppArmor; the
container has a private cgroup namespace, no network, no host mounts, no extra
devices and no Docker socket. Only the candidate and test script are copied in.
The container is removed in cleanup; its local build image and private evidence
under the printed `/tmp/patina-appimage-systemd-*` directory are retained.
This checks real service ownership and container/user-manager restart, not GNOME
login, real window sampling, FUSE mounting or formal signed updater delivery.
These remaining release gates must retain their own evidence.

For actual graphical login, FUSE and foreground-window recording, use the
[independent GNOME VM procedure](../scripts/acceptance/appimage-gnome/README.md).
It verifies first installation and a real GDM login after a guest cold boot in a
private user profile with no Patina DEB. It records the Wayland login session
separately from the AppImage's XWayland client backend. The same procedure defines
production-key upgrade acceptance using the non-publishing Actions candidate;
preparing that workflow does not constitute signed-upgrade acceptance.

### Published Bundles

Stable tagged releases build on Ubuntu 22.04 and publish:

- x86_64 AppImage for portable execution and package-aware Tauri updates
- amd64 Debian package for Ubuntu / Debian installation and package-aware Tauri updates
- GNOME Shell extension zip
- signed Firefox / Zen XPI

The Debian package installs `patinad`, the default-disabled systemd user unit, and the GNOME extension source under:

```text
/usr/share/gnome-shell/extensions/patina-window-tracker@patina/
```

Enable it for the current user after package installation:

```bash
gnome-extensions enable patina-window-tracker@patina
```

For AppImage installations, install the separately published extension archive:

```bash
gnome-extensions install --force patina-gnome-shell-extension-v<version>.zip
gnome-extensions enable patina-window-tracker@patina
```

For stable tags, the release workflow publishes a Linux-only `latest.json` with `linux-x86_64-appimage` and `linux-x86_64-deb` package-specific targets. It also keeps an AppImage-based `linux-x86_64` fallback for older clients. AppImage installations download the signed AppImage; Debian installations download the signed `.deb` and may request system authorization before installation.

Daemon-backed prerelease tags use the narrow DEB-only beta contract. Their workflow builds only `--bundles deb`, uploads no AppImage, and writes `latest.json` with only `linux-x86_64-deb`. This manifest remains attached to the prerelease; it does not replace the stable `/releases/latest/download/latest.json` endpoint. Beta installation and later beta upgrades therefore remain an explicit acceptance flow until a dedicated prerelease updater channel is designed.

Before publishing a Linux tag, run the release-focused local checks:

```bash
npm run release:validate-version-files -- <version>
npm run release:validate-changelog -- <version>
npm run test:release
npm run test:storage
npm run test:storage-docs
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
```

`npm run test:release` covers both Linux package release contracts. Stable tags request `--bundles appimage,deb`, require both signatures, and route each installation type to its matching artifact. Daemon-backed prerelease tags request `--bundles deb`, reject missing or empty DEB signatures, omit AppImage assets, and expose only the DEB updater target.

When debugging Debian packaging locally, run a focused Tauri release build:

```bash
npm run tauri build -- --bundles deb --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

This checks the `.deb` bundler without requiring the updater signing secret. The real tagged release still uses GitHub Actions with `createUpdaterArtifacts: true`, so both signed package artifacts and the package-aware `latest.json` remain part of the release workflow.

### Installed Daemon Package Acceptance

The installed-package collector is read-only and can be run from the repository against the current production profile:

```bash
npm run release:inspect-installed-patinad -- --phase baseline --expected-version 1.8.3
npm run release:inspect-installed-patinad -- --phase managed --expected-version "$BETA_VERSION"
```

Use `--output /absolute/new-file.json` to retain evidence. The collector creates that file as `0600` and refuses to overwrite an existing path. It never prints the API Token, window titles, or visited URLs; API output is reduced to protocol and capability readiness. It also never installs a package, enables or stops a service, changes owner state, or deletes data.

Supported phases are `baseline`, `installed`, `managed`, `rolled-back`, and `uninstalled`. The authoritative action order and safety gates live in [`working/2026-07-10-patinad-runtime-design.md`](./working/2026-07-10-patinad-runtime-design.md); do not use a passing snapshot as a substitute for the before/after checks around UI exit, service crash, lock/suspend, upgrade, rollback, and uninstall.

### Isolated Systemd Restore Acceptance

This opt-in Rust test launches random `patina-restore-systemd_test_*.service` transient units under the current user's systemd manager. It never controls `patinad.service`, enables login startup, uses existing app profiles, or restores a user backup. It requires Linux, `systemd-run`, `systemctl`, `timeout`, and an explicitly selected daemon binary matching the source version:

```bash
PATINA_SYSTEMD_TEST_BINARY=/usr/bin/patinad cargo test \
  --manifest-path src-tauri/Cargo.toml --lib \
  real_systemd_restore_crosses_process_boundary -- --ignored --nocapture
```

Each case creates a private temporary HOME/XDG tree, a synthetic archive and database, pauses tracking, disables audio/web/remote-status integration, and disconnects the child from desktop D-Bus and the real system bus. The latter prevents the user's actual lock/suspend state from changing a restore test's readiness; power lifecycle has separate tests. It sends an authenticated restore request on a random loopback port, verifies a new systemd PID and terminal restore status, then checks data, receipts, integrity and exact staging cleanup. The cases cover Replace, Merge and an injected INSERT failure with transaction rollback. Test services have bounded runtime/restart limits and are stopped on completion or unwinding; a cleanup failure must be investigated using the exact printed test unit name. Never stop the product service to clean up a test.

Successful cases retain small synthetic fixtures and owner-only `evidence.json` files under their printed temporary directories. No Token, real window title or URL is printed. Normal `cargo test` ignores this test. This is a real cross-process restore check, not a second installed package, separate user account, WebDAV test, power-loss test or validation of every security property of the packaged unit. The fixed service environment marker is reused for protocol negotiation while the actual transient unit name is intentionally distinct.

The remote variant additionally requires `dbus-daemon`, `gdbus` and `gnome-keyring-daemon`:

```bash
PATINA_SYSTEMD_TEST_BINARY=/usr/bin/patinad cargo test \
  --manifest-path src-tauri/Cargo.toml --lib \
  real_webdav_restore_crosses_private_credentials_and_systemd -- --ignored --nocapture
```

It creates a private D-Bus and Secret Service with synthetic credentials under the temporary tree. A guarded child test seeds that keyring; never invoke `seed_private_webdav_credential` manually or run all ignored tests indiscriminately. Only the fixture and test daemon receive that bus address; the parent keeps the user-manager connection. The daemon lists/downloads a synthetic archive from an authenticated loopback HTTP fixture and completes Replace/Merge/rollback across a real systemd restart. It does not read the login keyring, connect to a real WebDAV account, test upload/TLS interoperability, or change production settings. Owned fixture processes and transient units are stopped; synthetic evidence remains private under `/tmp`.

## Opt-in Validation Routing

`check:full` does not run Rust `#[ignore]` tests. Select a named test only when
the changed behavior and its environment match; never run all ignored tests in
the normal user session. Candidate status, versions, and evidence belong in the
[daemon checklist](./working/2026-07-10-patinad-runtime-design.md), not this procedure.

| Changed behavior | Named test / entry point | Preconditions and limits |
| --- | --- | --- |
| Desktop window disposal / background delay | `native_window_lifecycle`, `native_background_delay`; `node scripts/native-window-lifecycle.mjs` with optional `--background-delay` | Private profile and D-Bus, real Wayland, timed waits; native lifecycle only. See [native lifecycle procedure](#native-window-lifecycle-regression) |
| React heatmap / daemon reads / disposal | `heatmap_desktop_worker`; `npm run perf:heatmap-desktop` | Use the runner's private fixture and actual frontend; see [heatmap procedure](#real-frontend-heatmap-acceptance) |
| Managed Desktop storage maintenance | `storage_desktop_worker`; `npm run test:storage-native` | Private Production-shaped profile, real WebKit/IPC, UI restart and independent daemon; optional real temporary systemd unit via `--systemd`. See [native storage procedure](#native-storage-maintenance-acceptance) |
| Query or backup resource bounds | `query_worker`, `worker`; `node scripts/perf/daily-activity-benchmark.mjs`, `node scripts/perf/backup-benchmark.mjs` | Runner-owned synthetic data; do not invoke workers without their fixture. See [benchmark contracts](./engineering-quality.md#5-默认验证门槛) and the runners' options |
| Restore across service restart / WebDAV credentials | `real_systemd_restore_crosses_process_boundary`, `real_webdav_restore_crosses_private_credentials_and_systemd` | Explicit matching binary, temporary service and private credentials; use [isolated systemd restore](#isolated-systemd-restore-acceptance) |
| Private credential seeding | `seed_private_webdav_credential` | Internal child of the WebDAV restore test only; never select directly |
| Generated AppImage service unit | `generated_unit_passes_real_systemd_parser` | Requires `systemd-analyze`; parses a private unit, does not install or start it |
| Durable AppDir and atomic update | `built_appdir_runs_from_durable_store`, `tauri_download_verifies_before_atomic_install` | Explicit private AppDir/AppImage or `PATINA_UPDATER_TEST_ROOT` signed synthetic fixture; never use a release private key. See [AppImage procedure](#daemon-backed-appimage-runtime-development) and [updater test preconditions](../src-tauri/src/platform/linux/appimage_update.rs) |
| Graphical session discovery / lock subscription rebinding | `private_logind_late_login_logout_and_rebind_preserve_global_power_events`, `host_logind_resolves_graphical_session_without_environment` | Private D-Bus lifecycle fixture or read-only host query, respectively; see [session validation](#graphical-session-validation) |
| Service diagnostics | `live_user_manager_snapshot_is_classified_without_mutation` | Requires live user systemd manager; reads service state only, not an isolated lifecycle test |
| Audio or media provider | `live_pulseaudio_query_completes_when_compat_server_is_available`, `live_mpris_query_completes_when_session_bus_is_available` | Requires real PulseAudio/pipewire-pulse or D-Bus session; provider availability is not proof of full tracking correctness |

## Desktop Memory Evidence

```bash
npm run perf:memory-snapshot -- --label foreground --output /tmp/patina-memory-foreground.json
```

This read-only Linux collector reads `/proc` metadata and `smaps_rollup`, not command lines, environment variables, databases or credentials. Output is owner-only and refuses overwrite. It groups current-user `/usr/bin/Patina`, `/usr/bin/patinad` and attributable live descendants; custom build paths and reparented processes are outside its scope. Missing metrics remain null. Compare PSS and USS rather than summed RSS; samples are non-atomic and have no pass/fail memory budget.

The development-only in-process resource command follows the same Linux memory
definitions: RSS and PSS come from `smaps_rollup`, USS is
`Private_Clean + Private_Dirty + Private_Hugetlb`, and Swap is reported
separately. Its compatibility `private_usage_bytes` field equals USS on Linux;
it is no longer derived from `VmData`. Unavailable fields remain null.

Capture comparable foreground, tray-hidden, low-resource-background after its delay, and reopened states with different output filenames. The current low-resource setting defaults off; when enabled, main-window close schedules destruction after five minutes and rechecks visibility/generation. Hiding is not immediate destruction. Do not change the setting or close the user's window automatically for a measurement. Keep tracking enabled and verify it continues across UI reclamation.

## Native Window Lifecycle Regression

For the shorter widget startup regression (20 creations without a Main window):

```bash
node scripts/native-window-lifecycle.mjs --autostart-only
node scripts/native-window-lifecycle.mjs --autostart-only --x11
```

The first uses the current Wayland display with private application roots; the
second creates its own Xvfb and enables synchronous GDK errors. Both start the
widget from an async worker and exercise monitor discovery on the UI thread.
They do not replace packaged AppImage or graphical login acceptance.

From a GNOME Wayland session, explicitly run:

```bash
node scripts/native-window-lifecycle.mjs
```

The runner builds the ignored Rust test using the normal Cargo cache, then starts
only that test under private HOME/XDG roots and a private D-Bus session. It opens
real GTK/WebKit windows with inert content and uses a synthetic SQLite database.
It does not initialize the product tracker, systemd integration or production API.
Expect about eleven minutes: the two five-minute lifecycle timers are not shortened.

The test first applies the default autostart plan and verifies that it creates a
Widget without a hidden Main window. It then injects cancellation after Widget
creation starts but before native registration, checks its hidden-window cleanup,
checks that reopening cancels the old Main timer, and checks Main teardown while a visible Widget survives. The
first reopen deliberately avoids the normal focus/Widget-close callbacks, which
would otherwise schedule another cleanup and mask the missing-timer regression.
Finally it destroys Widget and Main and verifies that the last WebView schedules
one app-level heap reclaim attempt. This is a native lifecycle test, not full
React/IPC, tracking continuity, allocator-budget or long-running memory acceptance.

Evidence remains in the printed `/tmp/patina-window-test-*` directory as
`native.log` and `result.json`. The test is skipped by default; do not run all
ignored tests against the normal user environment. No installed app is replaced.

## Real Frontend Heatmap Acceptance

From a Wayland session, explicitly run:

```bash
npm run perf:heatmap-desktop
```

This builds the real frontend and an opt-in Rust test executable, then runs the
production Desktop bootstrap in daemon-client preview mode and the real daemon
runtime in separate test processes. Both use the same synthetic Local profile
under a private `0700` `/tmp/patina-heatmap-test-*` tree. A private D-Bus serves as
both session and system bus; only the Wayland display is borrowed. Tracking is
paused, audio/web bridges and login preferences are disabled. No installed app,
production database, user service, or production credential is used.

The runner creates 50,000 native sessions near the current date, serves the built
React assets on an ephemeral loopback port, and drives the real WebKit page.
It observes daily IPC responses without mocking them, checks heatmap data and
History navigation, closes Main through the normal lifecycle, waits 310 seconds,
and reopens it. It then stops the daemon with the service's normal `SIGINT` signal
and checks fixture counts, total duration and SQLite integrity. Expect about six
minutes plus compilation; do not run every ignored test indiscriminately.

Evidence includes `build.json`, `ui-1.json`, `ui-2.json`, `closed.json`,
`destroyed.json`, `integrity.json`, logs, `evidence.json` and `result.json`.
The collector samples spawned Desktop/daemon processes and attributable live
descendants about every 250ms. Missing memory values remain null; PSS/USS are more
useful than summed RSS. The UI probe only retains compact synthetic daily totals,
not authentication headers or complete activity records.

This is a debug-runtime functional gate and memory observation, not a fixed
whole-product memory-budget benchmark or release-package acceptance. It excludes
unattributable/reparented helpers, production data, continuous real tracking,
widget appearance, screenshot-based visual review and multi-year/import-heavy
scalability. Failed or interrupted runs retain their private evidence rather than
deleting directories recursively. Do not infer a memory improvement merely from
`passed: true`; compare the recorded phases and query-level budgets separately.

## Native Storage Maintenance Acceptance

From a Wayland session, run:

```bash
npm run test:storage-native
```

This opt-in runner builds the frontend, a Rust test worker and a separate
`patinad` binary. It creates a private `0700` `/tmp/patina-storage-test-*` tree,
HOME/XDG roots and a D-Bus socket inside that tree. Both session and system bus
addresses point there. The Production application identifier is used only within
those synthetic roots, with a completed owner reservation, paused tracking and
disabled login/audio/web integrations. The installed profile and production
`patinad.service` are not used.

A test-only `org.freedesktop.systemd1` fixture accepts start/stop for its own
daemon child. The real Desktop bootstrap and service adapter call it over the
private bus. Five Desktop processes exercise data migration, restoring the
default data directory, WebView migration and cache clearing. Only the first
Desktop is launched by the runner; four real Settings button clicks invoke the
production IPC and Tauri `app.restart`, with PID/start-time and restart-event
evidence for every successor. Migration appointments use real IPC directly;
native folder pickers and migration confirmation dialogs are not automated.
Checks cover cancellation, pending paths, persistent localStorage, daemon
PID/counters and database integrity. A synthetic cache marker must disappear
while persistent state survives.

To use a candidate daemon with a real temporary systemd user unit:

```bash
npm run test:storage-native -- --systemd --daemon /absolute/private-install/usr/bin/patinad
```

The private D-Bus adapter maps its fixed product name to exactly one random
`patina-storage-test-*.service` under the real user manager. The installed
`patinad.service` is never controlled. `systemd-run --wait` preserves the daemon
exit status, and both the worker and outer runner check cleanup of that exact
temporary unit. This mode requires access to `/run/user/<uid>/bus`. The optional
daemon path must be a canonical regular executable; its hash is recorded.
Desktop still uses the test host compiled from the current source.

Each run retains owner-only stage reports, native snapshots, daemon logs, build
hashes and final results in its printed private directory. All waits are bounded;
cleanup only terminates runner-owned processes. Legacy-client occupancy checks
can refuse maintenance while another Patina Desktop is running. A refusal is
not permission for the runner to close that application.

The default D-Bus service fixture is not a real systemd manager. The opt-in
systemd mode verifies real service start/stop and exit status, with a private name
adapter and test unit properties (`Type=exec`, `Restart=no`, `PrivateTmp=no`). It
does not verify all packaged hardening, login, ongoing real activity collection
or production installation. Power-loss and release acceptance remain separate.
Never point this worker at an existing profile or run the ignored worker directly
without the runner.

## Isolated Debian Candidate Acceptance

When candidate installation validation is requested, an unsigned local candidate
may be inspected separately from public release artifacts. Record its source
manifest and SHA256; a local build retaining the source version is not the
published release with that version. Public builds and signing still follow the
[release policy](./versioning-and-release-policy.md).

```bash
npm run test:deb-isolated -- --candidate /absolute/candidate.deb --baseline /absolute/baseline.deb
```

This opt-in runner requires Python 3.10+, Debian tools, user namespaces and
Bubblewrap. It installs the baseline, upgrades to the candidate, removes it and
reinstalls it using a private root, dpkg database and log. Payload hashes and
synthetic user-data sentinels are checked at each stage; the final installation
is retained for the native daemon test above. Packages with maintainer scripts,
triggers, unexpected payload locations or automatic service enablement are
rejected before installation. No package GUI or service is executed by this
runner.

Normal dpkg dependency checks use a copy of installed host package metadata.
This proves compatibility with those recorded dependency versions, not fresh
dependency installation on a clean distribution. It does not exercise database
upgrade, production service takeover, login, purge or signed updater delivery.
Use a baseline with verified provenance; its filename alone is not evidence of
an official release. Results, input hashes and the private installed daemon path
are retained under the printed `/tmp/patina-deb-acceptance-*` directory.

## MCP Wrapper

The MCP wrapper is a stdio server that maps MCP tool calls to the local API:

```bash
npm run mcp:patina
```

It reads:

- `PATINA_API_BASE`
- `PATINA_API_TOKEN`
- `PATINA_API_TOKEN_FILE`

The wrapper exposes read tools for activity, diagnostics, settings, Tools state, and service state, plus explicitly confirmed writes for app mapping, runtime/API settings, Tools actions, and managed daemon restart. The canonical tool list and client setup live in [`mcp-wrapper.md`](./mcp-wrapper.md).

## Current Validation Commands

Choose the gate for the change; broader gates already include the narrower ones.

| Scope | Command |
| --- | --- |
| Ordinary automated tests, including GNOME script and MCP contracts | `npm test` (discovers `tests/**/*.test.ts`) |
| Frontend delivery, boundaries, build and bundle budget | `npm run check` |
| Architecture or Rust runtime changes | `npm run check:full` |
| Release preparation | `npm run release:check` |
| Focused tracking lifecycle iteration | `npm run test:tracking-lifecycle` |
| Extension source validation | `npm run extension:gnome:check` |

Other `test:*` commands remain available for focused iteration. Building or
installing an extension/package is a separate action, not part of ordinary test
discovery. See [validation policy](./engineering-quality.md#5-默认验证门槛) and
[opt-in routing](#opt-in-validation-routing) for additional risks.

## Isolated GNOME Extension Acceptance

After `npm run extension:gnome:build`, run:

```bash
python3 scripts/gnome-shell-acceptance.py
```

The opt-in runner requires GNOME Shell 42, GJS, GTK 3, `gdbus` and
`dbus-run-session`. It copies the built extension into a private temporary HOME,
starts a separate headless Wayland Shell, and checks both D-Bus protocols against
a synthetic GTK window. It covers overview, the real Shell screen shield, unlock
recovery and three disable/enable cycles. No existing extension is installed or
replaced. Its private bus has no service activation and also substitutes for the
system bus; a GDM Version fixture permits screen-shield construction without
connecting to the host GDM/logind. This is not password-authentication, production
login or suspend acceptance. The printed evidence directory retains the Shell
log and JSON result; missing desktop services can produce expected warnings.
The runner terminates only its own private process group.

## Graphical Session Validation

Managed foreground sampling uses live logind facts, not the service's inherited
desktop variables. The resolver reads only the current user's graphical session
metadata. Missing, inactive, remote, closing or mismatched sessions fail closed;
no old positive session is cached. The power watcher retains global sleep/shutdown
subscriptions while graphical sessions disappear or change, and reconciles the
new session's lock state after subscribing.

Run the lifecycle fixture on a new private bus, never the host bus:

```bash
dbus-run-session -- env PATINA_PRIVATE_LOGIND_TEST=1 \
  cargo test --manifest-path src-tauri/Cargo.toml --lib \
  private_logind_late_login_logout_and_rebind_preserve_global_power_events \
  -- --ignored --nocapture
```

The fixture covers late login, no-session power events, logout/relogin, stale
session signals and clearing the previous login's lock. It does not lock or
suspend the machine and uses no production profile.

To validate the real host's session metadata without inherited desktop labels:

```bash
env -u XDG_SESSION_TYPE -u XDG_CURRENT_DESKTOP -u DESKTOP_SESSION \
  -u DISPLAY -u WAYLAND_DISPLAY PATINA_SYSTEMD_SERVICE=patinad.service \
  cargo test --manifest-path src-tauri/Cargo.toml --lib \
  host_logind_resolves_graphical_session_without_environment \
  -- --ignored --nocapture
```

This second test needs an active local graphical login. It reads logind metadata
and provider-name availability only; it does not query foreground content, modify
the manager environment, restart the installed daemon or prove tracking-duration
correctness. Environment removal applies only to the test process.
