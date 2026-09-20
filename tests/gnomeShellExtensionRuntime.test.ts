import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";

const extensionRoot = new URL("../extensions/gnome-shell/patina-window-tracker@patina/", import.meta.url);
const extensionSource = readFileSync(new URL("extension.js", extensionRoot), "utf8");
const metadata = JSON.parse(readFileSync(new URL("metadata.json", extensionRoot), "utf8"));

const PRIVATE_TITLE = "Synthetic private document — 中文";
const PRIVATE_APP_ID = "org.example.SyntheticPrivate.desktop";
const PRIVATE_CLASS = "SyntheticPrivateClass";
const PID = 1234;
const WINDOW_ID = 4_294_967_298;
const EXPECTED = [PRIVATE_TITLE, "org.example.SyntheticPrivate", PRIVATE_CLASS, PID, WINDOW_ID];

function createHarness() {
  const logs: string[] = [];
  const signals: { name: string; signature: string; values: unknown[] }[] = [];
  let xml = "";
  let methods: { GetFocusedWindow(): unknown[] } | undefined;
  let focusChanged: (() => void) | undefined;
  let firstSnapshot: (() => unknown) | undefined;
  const display = {
    focus_window: {
      get_title: () => PRIVATE_TITLE,
      get_wm_class: () => PRIVATE_CLASS,
      get_pid: () => PID,
      get_id: () => WINDOW_ID,
    } as object | null,
    connect(name: string, callback: () => void) {
      assert.equal(name, "notify::focus-window");
      focusChanged = callback;
      return 10;
    },
    disconnect() {},
  };

  class Variant {
    signature: string;
    values: unknown[];

    constructor(signature: string, values: unknown[]) {
      this.signature = signature;
      this.values = Array.from(values);
      const types = [...signature.slice(1, -1)];
      assert.equal(types.length, values.length);
      for (const [index, type] of types.entries()) {
        assert.equal(typeof values[index], type === "s" ? "string" : "number",
          `D-Bus field ${index} must match ${type}`);
      }
    }
  }

  const exports = runInNewContext(`${extensionSource}\n({enable, disable});`, {
    log: (...values: unknown[]) => logs.push(values.map(String).join(" ")),
    global: { display },
    imports: { gi: {
      Gio: {
        BusType: { SESSION: 0 },
        BusNameOwnerFlags: { NONE: 0 },
        DBusNodeInfo: {
          new_for_xml(value: string) { xml = value; return { interfaces: [value] }; },
        },
        bus_own_name(_type: number, name: string, _flags: number,
          acquired: (connection: object, name: string) => void) {
          assert.equal(name, "org.patina.WindowTracker");
          acquired({}, name);
          return 20;
        },
        bus_unown_name() {},
        DBusExportedObject: {
          wrapJSObject(_xml: string, implementation: typeof methods) {
            methods = implementation;
            return {
              export(_connection: object, path: string) {
                assert.equal(path, "/org/patina/WindowTracker");
              },
              unexport() {},
              emit_signal(name: string, variant: Variant) {
                signals.push({ name, signature: variant.signature, values: variant.values });
              },
            };
          },
        },
      },
      GLib: {
        PRIORITY_DEFAULT: 0, SOURCE_REMOVE: false, Variant,
        timeout_add(_priority: number, _delay: number, callback: () => unknown) {
          firstSnapshot = callback;
          return 30;
        },
        source_remove() {},
      },
      Shell: {
        WindowTracker: { get_default: () => ({
          get_window_app: () => ({ get_id: () => PRIVATE_APP_ID }),
        }) },
      },
    } },
  }, { filename: "synthetic-gnome-extension.js" });

  exports.enable();
  return {
    logs, signals, display, xml,
    query: () => Array.from(methods!.GetFocusedWindow()),
    initialSignal: () => firstSnapshot!(),
    focusChanged: () => focusChanged!(),
    stop: () => exports.disable(),
  };
}

function contract(xml: string, kind: "method" | "signal", name: string) {
  const content = xml.match(new RegExp(`<${kind} name="${name}">([\\s\\S]*?)</${kind}>`));
  assert.ok(content, `Missing ${kind} ${name}`);
  return [...content[1].matchAll(/<arg name="([^"]+)" type="([a-z])"/g)]
    .map((match) => ({ name: match[1], type: match[2] }));
}

let passed = 0;
function runTest(name: string, test: () => void) {
  try {
    test();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`, error);
    process.exitCode = 1;
  }
}

runTest("focus queries and notifications never log synthetic window activity", () => {
  const harness = createHarness();
  const startupLogs = harness.logs.length;
  assert.deepEqual(harness.query(), EXPECTED);
  assert.deepEqual(harness.query(), EXPECTED);
  harness.initialSignal();
  harness.focusChanged();
  harness.stop();
  assert.equal(harness.logs.length, startupLogs, "Polling must not emit per-window logs");
  for (const privateValue of [PRIVATE_TITLE, PRIVATE_APP_ID, EXPECTED[1], PRIVATE_CLASS]) {
    assert.ok(harness.logs.every((line) => !line.includes(String(privateValue))));
  }
});

runTest("method and emitted signal preserve the existing typed window contract", () => {
  const harness = createHarness();
  const method = contract(harness.xml, "method", "GetFocusedWindow");
  const signal = contract(harness.xml, "signal", "FocusedWindowChanged");
  assert.deepEqual(method, [
    { name: "title", type: "s" }, { name: "app_id", type: "s" },
    { name: "wm_class", type: "s" }, { name: "pid", type: "u" },
    { name: "window_id", type: "t" },
  ]);
  assert.deepEqual(signal, method);
  harness.initialSignal();
  assert.deepEqual(harness.signals, [{
    name: "FocusedWindowChanged", signature: `(${signal.map((arg) => arg.type).join("")})`,
    values: EXPECTED,
  }]);
  assert.deepEqual(harness.query(), harness.signals[0].values);
  harness.stop();
});

runTest("no-focus results retain the empty legacy tuple and GNOME 42 identity", () => {
  const harness = createHarness();
  harness.initialSignal();
  harness.display.focus_window = null;
  harness.focusChanged();
  assert.deepEqual(harness.query(), ["", "", "", 0, 0]);
  assert.deepEqual(harness.signals[1], {
    name: "FocusedWindowChanged", signature: "(sssut)", values: ["", "", "", 0, 0],
  });
  assert.equal(metadata.uuid, "patina-window-tracker@patina");
  assert.deepEqual(metadata["shell-version"], ["42"]);
  assert.equal(metadata.version, 3);
  harness.stop();
});

console.log(`Passed ${passed} synthetic GNOME extension runtime tests`);
