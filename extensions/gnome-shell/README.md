# Patina GNOME Shell Extension

This extension exposes the focused GNOME Wayland window through the local session D-Bus name used by Patina:

- bus: `org.patina.WindowTracker`
- path: `/org/patina/WindowTracker`
- method: `org.patina.WindowTracker.GetFocusedWindow`

Development commands:

```bash
npm run extension:gnome:check
npm run extension:gnome:build
npm run extension:gnome:install
```

After install, enable the extension:

```bash
gnome-extensions enable patina-window-tracker@patina
```

If GNOME Shell already cached an older extension copy, log out and back in before testing.
