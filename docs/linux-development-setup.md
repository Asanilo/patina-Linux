# Linux Development Setup

This page records the current Linux prototype setup path.

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

The stable control directory contains versioned data/WebView anchors, pending migration metadata, and maintenance state. The default data root contains `patina.db`, local backups, and the local API token. Moving activity data relocates the database and managed backup/temp directories; the API token remains at the stable default product data root. Activity data and WebView data can be moved independently from Settings -> Data Safety -> Local storage.

Storage changes use a restart boundary:

1. Settings previews the target and available space without mutating it.
2. Confirmation writes a pending operation and backup metadata.
3. Patina exits only when the user chooses to restart.
4. On the next launch, migration runs before SQLite or either WebView is opened.
5. The copied database must pass SQLite integrity, schema, and row-count checks before the target is promoted.

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

```bash
node --experimental-strip-types --experimental-specifier-resolution=node tests/gnomeShellExtensionScript.test.ts
node --experimental-strip-types --experimental-specifier-resolution=node tests/patinaMcpScript.test.ts
npm run test:release
npm run extension:gnome:check
npm run extension:gnome:build
npm run build
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```
