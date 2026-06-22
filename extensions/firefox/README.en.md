# Patina Web Sync for Firefox / Zen

This is the Firefox/Zen browser extension for syncing the active webpage to local Patina.

## Before using

- Install and run the Patina desktop app.
- Enable Web Sync in Patina settings.
- Copy the Web Sync port and token. The default port is `12345`.

## Temporary loading in Zen / Firefox

1. Open `about:debugging#/runtime/this-firefox`.
2. Click "Load Temporary Add-on".
3. Select `manifest.json` in this directory.
4. Open the extension options page, then fill in the Patina port and token.
5. Open a regular website and click "Sync current page" in the extension popup.

## Synced data

The extension only syncs the active webpage URL, title, and favicon URL.

It does not read page contents, form contents, screenshots, clipboard data, or browser history.
