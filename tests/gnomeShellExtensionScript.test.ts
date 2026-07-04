import assert from "node:assert/strict";
import { join } from "node:path";
import {
  gnomeShellExtensionInstallDir,
  validateGnomeShellExtensionSourceText,
} from "../scripts/gnome-shell-extension.ts";

const metadata = JSON.stringify({
  uuid: "patina-window-tracker@patina",
  name: "Patina Window Tracker",
  description: "Exposes focused window info via D-Bus for Patina time tracking",
  version: 2,
  "shell-version": ["42"],
});

const extensionJs = [
  "const BUS_NAME = 'org.patina.WindowTracker';",
  "const OBJECT_PATH = '/org/patina/WindowTracker';",
  "const PatinaIface = `<node><interface name=\"org.patina.WindowTracker\">",
  "<method name=\"GetFocusedWindow\"></method>",
  "<signal name=\"FocusedWindowChanged\"></signal>",
  "</interface></node>`;",
].join("\n");

let passed = 0;

async function runTest(name: string, fn: () => void | Promise<void>) {
  try {
    await fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
  }
}

await runTest("GNOME extension check accepts the Patina D-Bus contract", () => {
  assert.deepEqual(validateGnomeShellExtensionSourceText(metadata, extensionJs), []);
});

await runTest("GNOME extension check rejects missing D-Bus methods", () => {
  assert.deepEqual(validateGnomeShellExtensionSourceText(metadata, ""), [
    "GNOME Shell extension check failed. extension.js must define org.patina.WindowTracker.",
    "GNOME Shell extension check failed. extension.js must export GetFocusedWindow.",
    "GNOME Shell extension check failed. extension.js must emit FocusedWindowChanged.",
  ]);
});

await runTest("GNOME extension install dir uses XDG data home when present", () => {
  assert.equal(
    gnomeShellExtensionInstallDir({
      home: "/home/user",
      xdgDataHome: "/tmp/data",
    }),
    join("/tmp/data", "gnome-shell", "extensions", "patina-window-tracker@patina"),
  );
});

await runTest("GNOME extension install dir falls back to local share", () => {
  assert.equal(
    gnomeShellExtensionInstallDir({
      home: "/home/user",
      xdgDataHome: undefined,
    }),
    join("/home/user", ".local", "share", "gnome-shell", "extensions", "patina-window-tracker@patina"),
  );
});

console.log(`Passed ${passed} GNOME Shell extension script tests`);
