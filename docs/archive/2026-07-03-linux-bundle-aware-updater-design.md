# Linux Bundle-Aware Updater Design

## Status

- Date: 2026-07-03
- Scope: Linux updater artifact selection for DEB and AppImage installations
- Decision: use Tauri's built-in bundle-specific target lookup with a static multi-target manifest

## Problem

Patina currently publishes both an AppImage and a Debian package, but `latest.json` exposes only `linux-x86_64` and points that target to the AppImage. Tauri selects the installer from the bundle type embedded in the running binary:

- an AppImage installation expects AppImage bytes;
- a DEB installation expects Debian package bytes and installs them through the system package flow.

As a result, a DEB-installed Patina downloads the AppImage and rejects it during installation. The update is correctly signed, but it is the wrong artifact type.

## Goals

- AppImage installations download and install the signed AppImage.
- DEB installations download and install the signed Debian package.
- DEB installation uses Tauri's existing privileged `dpkg -i` path and displays the system authorization prompt.
- Existing 1.8.3 DEB and AppImage installations both select the correct artifact without a manual transition.
- Release validation fails if either Linux artifact or its matching signature is missing.

## Non-Goals

- No dynamic update server.
- No change to the Tauri signing key or public key.
- No package-manager repository, Flatpak, Snap, RPM, or AUR support in this change.
- No Windows updater or Windows-source cleanup in this change.
- No delta updates.

## Considered Approaches

### Static multi-target manifest

Publish bundle-specific targets in the existing GitHub Release `latest.json`. Tauri already reads the bundle identity embedded in the packaged binary and searches `{os}-{arch}-{installer}` before the generic `{os}-{arch}` target.

This is the selected approach because it keeps GitHub Release as the sole update source, preserves signature verification, and adds no service infrastructure.

### Manual DEB updates

Keep AppImage auto-update and direct DEB users to download releases manually. This is simpler but leaves the primary integrated package with a permanently incomplete update experience.

### Dynamic update endpoint

Serve a package-specific response from an update service. This can solve artifact routing but introduces network infrastructure and maintenance that are unnecessary for two static Linux artifacts.

## Target Contract

Tauri uses these targets automatically when no custom target override is configured:

| Running bundle | Updater target | Artifact | Signature |
| --- | --- | --- | --- |
| DEB | `linux-x86_64-deb` | `Patina_X.Y.Z_amd64.deb` | matching `.deb.sig` content |
| AppImage | `linux-x86_64-appimage` | `Patina_X.Y.Z_amd64.AppImage` | matching `.AppImage.sig` content |

The release manifest retains the generic `linux-x86_64` target and keeps it pointed at the AppImage. It is a compatibility fallback for clients without a recognized installer type.

Patina 1.8.3 already contains Tauri updater 2.10.1. That updater searches `linux-x86_64-deb` or `linux-x86_64-appimage` before falling back to `linux-x86_64`. Publishing the missing entries therefore fixes both existing package types without changing runtime code or requiring a one-time manual install.

The generic target must not point to the DEB. Tauri's AppImage installation path can replace the current executable with downloaded bytes without first proving that those bytes are an AppImage. Serving DEB bytes through the generic target could therefore corrupt an older AppImage installation.

## Runtime Design

No application runtime change is required. `engine/updater.rs` continues to use `app.updater()`, allowing Tauri to inspect its patched bundle identity and perform installer-specific target lookup.

The returned Tauri `Update` remains the single pending update object. Existing download progress, signature verification, install state, retry behavior, authorization prompt, and release-page fallback remain intact.

## Release Design

`scripts/release.ts` discovers four linked files from the Tauri bundle output:

- AppImage;
- AppImage signature;
- DEB;
- DEB signature.

Artifact discovery validates that each signature belongs to an existing artifact path. Empty signatures are rejected.

`latest.json` contains three platform entries:

```json
{
  "platforms": {
    "linux-x86_64": {
      "url": ".../Patina_X.Y.Z_amd64.AppImage",
      "signature": "<AppImage signature>"
    },
    "linux-x86_64-deb": {
      "url": ".../Patina_X.Y.Z_amd64.deb",
      "signature": "<deb signature>"
    },
    "linux-x86_64-appimage": {
      "url": ".../Patina_X.Y.Z_amd64.AppImage",
      "signature": "<AppImage signature>"
    }
  }
}
```

The Release asset names do not change. Signature files remain represented by their contents in `latest.json`; they do not need to become public Release attachments.

## Error Handling

- Missing artifact or signature during release preparation: fail the release workflow before publication.
- Invalid or mismatched signature: Tauri rejects the update before installation.
- User cancels authorization or `dpkg` fails: retain the downloaded update for retry and surface an install-stage error.
- AppImage replacement failure: preserve Tauri's existing rollback behavior.

Bundle-specific entries must never share signatures across package types. The generic fallback remains AppImage-compatible for backward compatibility.

## Security And Data Safety

- Both package types use the existing Tauri signing key and embedded public key.
- The updater remains HTTPS-only in production.
- The package URL is generated from the fixed repository, version, and release asset contract, not user input.
- DEB installation delegates privilege escalation to Tauri's `pkexec`/package installation path; Patina does not collect or store an administrator password.
- This change does not modify activity data, storage migration state, or SQLite files.

## Validation

Release policy tests cover:

- all three manifest targets and their exact URLs;
- DEB targets using the DEB signature;
- AppImage target using the AppImage signature;
- failure when `.deb.sig` is missing or empty;
- failure when `.AppImage.sig` is missing or empty;
- required DEB and AppImage Release assets;
- the generic fallback remaining AppImage-based;
- documentation of Tauri's bundle-specific lookup contract.

The final validation bar is `npm run release:check` plus a test fixture that prepares `latest.json` from fake signed DEB and AppImage bundle outputs.

## Rollout

1. Implement and validate the bundle-aware updater without changing product identity.
2. Publish it as the next patch release.
3. Verify update installation from a real 1.8.3 DEB with the authorization prompt.
4. Verify a real 1.8.3 AppImage selects `linux-x86_64-appimage` and updates in place.
5. Keep the GitHub Release page available as the fallback for both package types.

Fork detachment, product rebranding, Windows-source cleanup, and binary-size optimization remain separate follow-up projects.
