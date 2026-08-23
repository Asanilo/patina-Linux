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

## patinad Service Preview

The Debian bundle now includes both:

```text
/usr/bin/patinad
/usr/lib/systemd/user/patinad.service
```

The package does not enable or start the unit. Desktop remains the default production tracking owner until the client cutover stage migrates the current user's XDG autostart and enables the daemon from the user session. Do not manually start the production unit while Patina Desktop is tracking the same profile.

Validate the source packaging contract without installing it:

```bash
npm run test:release
```

After installing a daemon-backed DEB, systemd can also validate the real installed executable and unit paths:

```bash
systemd-analyze verify --user /usr/lib/systemd/user/patinad.service
```

For an isolated manual preview, use a non-production profile:

```bash
src-tauri/target/debug/patinad --profile dev --serve-api --track --port 0
```

A manual preview exposes service state but rejects controlled restart because no supervisor can bring it back. A systemd-managed instance advertises `service-lifecycle`; restart persists a ticket, responds before shutdown, exits through the graceful runtime path, and the next instance confirms the same ticket.

## Linux Release Bundles

Tagged releases build on Ubuntu 22.04 and publish:

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

The release workflow publishes a Linux-only `latest.json` with `linux-x86_64-appimage` and `linux-x86_64-deb` package-specific targets. It also keeps an AppImage-based `linux-x86_64` fallback for older clients. AppImage installations download the signed AppImage; Debian installations download the signed `.deb` and may request system authorization before installation.

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

`npm run test:release` covers the Linux package release contract: the GitHub Actions workflow must request `--bundles appimage,deb`, `prepare-linux-release-assets` must reject missing or empty signatures, and `latest.json` must route AppImage and Debian installations to their matching signed artifacts.

When debugging Debian packaging locally, run a focused Tauri release build:

```bash
npm run tauri build -- --bundles deb --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

This checks the `.deb` bundler without requiring the updater signing secret. The real tagged release still uses GitHub Actions with `createUpdaterArtifacts: true`, so both signed package artifacts and the package-aware `latest.json` remain part of the release workflow.

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
