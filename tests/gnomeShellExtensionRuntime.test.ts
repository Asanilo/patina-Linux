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
  const objects = new Map<string, any>();
  const owners: any[] = [];
  const callbacks = new Map<number, () => void>();
  let nextId = 1;
  let firstSnapshot: (() => unknown) | undefined;
  function emitter(properties = {}) {
    return Object.assign(properties, {
      connect(_name: string, callback: () => void) {
        const id = nextId++; callbacks.set(id, callback); return id;
      },
      disconnect(id: number) { assert.ok(callbacks.delete(id)); },
    });
  }
  const main = {
    sessionMode: emitter({ isLocked: false }),
    screenShield: emitter({ locked: false, active: false }),
    overview: emitter({ visible: false, visibleTarget: false }),
  };
  const display = emitter({
    focus_window: {
      get_title: () => PRIVATE_TITLE, get_wm_class: () => PRIVATE_CLASS,
      get_pid: () => PID, get_id: () => WINDOW_ID,
    } as any,
  });
  const exports = runInNewContext(`${extensionSource}\n({enable, disable});`, {
    log: (...values: unknown[]) => logs.push(values.map(String).join(" ")),
    global: { display },
    imports: { ui: { main }, gi: {
      Gio: {
        BusType: { SESSION: 0 }, BusNameOwnerFlags: { NONE: 0 },
        bus_own_name(_type: number, name: string, _flags: number, _bus: unknown,
          acquired: (connection: object) => void, lost: () => void) {
          const owner = { name, acquired, lost, active: true };
          owners.push(owner); acquired({}); return owners.length;
        },
        bus_unown_name(id: number) { owners[id - 1].active = false; },
        DBusExportedObject: {
          wrapJSObject(xml: string, methods: any) {
            const object = { xml, methods, path: "", exported: false,
              export(_connection: object, path: string) {
                object.path = path; object.exported = true; objects.set(path, object);
              },
              unexport() { object.exported = false; },
              emit_signal(name: string, variant: any) {
                assert.ok(object.exported);
                signals.push({ name, signature: variant.signature, values: variant.values });
              },
            };
            return object;
          },
        },
      },
      GLib: {
        PRIORITY_DEFAULT: 0, SOURCE_REMOVE: false,
        Variant: class {
          signature: string; values: unknown[];
          constructor(signature: string, values: unknown[]) {
            this.signature = signature; this.values = Array.from(values);
            const types = [...signature.slice(1, -1)];
            assert.equal(types.length, values.length);
            types.forEach((type, i) => assert.equal(typeof values[i], type === "s" ? "string" : "number"));
          }
        },
        timeout_add(_priority: number, _delay: number, callback: () => unknown) {
          firstSnapshot = callback; return 30;
        },
        source_remove() {},
      },
      Shell: { WindowTracker: { get_default: () => ({
        get_window_app: () => ({ get_id: () => PRIVATE_APP_ID }),
      }) } },
    } },
  }, { filename: "synthetic-gnome-extension.js" });
  exports.enable();
  const legacy = () => objects.get("/org/patina/WindowTracker");
  return {
    logs, signals, display, main, owners, objects, callbacks,
    xml: legacy().xml,
    query: () => Array.from(legacy().methods.GetFocusedWindow()),
    snapshot: () => Array.from(objects.get("/org/patina/WindowTracker1").methods.GetSnapshot()),
    initialSignal: () => firstSnapshot!(),
    focusChanged: () => [...callbacks.values()].forEach(callback => callback()),
    start: () => exports.enable(), stop: () => exports.disable(),
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
  assert.equal(metadata.version, 4);
  harness.stop();
});

runTest("v1 preserves desktop suffix and 64-bit window identity", () => {
  const h = createHarness();
  assert.deepEqual(h.snapshot(), [1, 1, PRIVATE_TITLE, PRIVATE_APP_ID, PRIVATE_CLASS, PID, String(WINDOW_ID)]);
  assert.equal(contract(h.objects.get("/org/patina/WindowTracker1").xml, "method", "GetSnapshot").map(x => x.type).join(""), "uusssus");
  h.stop();
});

runTest("lock and overview hide focus before any window access and recover", () => {
  const h = createHarness();
  const win = h.display.focus_window;
  Object.defineProperty(h.display, "focus_window", { configurable: true, get() { throw new Error(PRIVATE_TITLE); } });
  for (const [target, key, state] of [
    [h.main.sessionMode, "isLocked", 2], [h.main.screenShield, "locked", 2],
    [h.main.screenShield, "active", 2], [h.main.overview, "visible", 0],
    [h.main.overview, "visibleTarget", 0],
  ] as const) {
    (target as any)[key] = true;
    assert.deepEqual(h.snapshot(), [1, state, "", "", "", 0, ""]);
    assert.deepEqual(h.query(), ["", "", "", 0, 0]);
    h.focusChanged();
    (target as any)[key] = false;
  }
  assert.deepEqual(h.snapshot(), [1, 3, "", "", "", 0, ""]);
  Object.defineProperty(h.display, "focus_window", { value: win, writable: true });
  assert.deepEqual(h.query(), EXPECTED);
  assert.ok(h.logs.every(line => !line.includes(PRIVATE_TITLE)));
  h.stop();
});

runTest("UTF-8 fields are bounded; malformed IDs fail closed", () => {
  const h = createHarness();
  h.display.focus_window.get_title = () => "\0\ud800" + "中文😀".repeat(1000);
  const title = h.snapshot()[2] as string;
  assert.ok(Buffer.byteLength(title) <= 4096);
  assert.equal(title.includes("\0"), false);
  assert.equal(title.includes("\ud800"), false);
  for (const id of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    h.display.focus_window.get_id = () => id;
    assert.equal(h.snapshot()[1], 3);
  }
  h.display.focus_window.get_id = () => 0;
  assert.equal(h.snapshot()[1], 1, "zero ID is valid when application identity is present");
  assert.equal(h.query()[4], 0);
  h.stop();
});

runTest("name loss cleans only its endpoint, reacquisition and enable are idempotent", () => {
  const h = createHarness();
  h.start(); assert.equal(h.owners.length, 2); assert.equal(h.callbacks.size, 6);
  const old = h.objects.get("/org/patina/WindowTracker1");
  h.owners[1].lost(); assert.equal(old.exported, false);
  assert.equal(h.objects.get("/org/patina/WindowTracker").exported, true);
  h.owners[1].acquired({}); assert.equal(h.objects.get("/org/patina/WindowTracker1").exported, true);
  h.stop(); h.stop();
  assert.equal(h.callbacks.size, 0);
  assert.ok(h.owners.every(owner => !owner.active));
  assert.ok([...h.objects.values()].every(object => !object.exported));
});

runTest("late callbacks after disable cannot resurrect or unexport a new generation", () => {
  const h = createHarness();
  const oldCallbacks = [...h.callbacks.values()];
  h.stop(); h.start();
  const objects = [...h.objects.values()];
  h.owners.slice(0, 2).forEach(owner => { owner.acquired({}); owner.lost(); });
  oldCallbacks.forEach(callback => callback());
  assert.deepEqual([...h.objects.values()], objects);
  assert.ok(objects.every(object => object.exported));
  assert.equal(h.callbacks.size, 6);
  h.stop();
});

runTest("sessions without a screen shield still support foreground observation", () => {
  const h = createHarness();
  h.stop();
  (h.main as any).screenShield = null;
  h.start();
  assert.deepEqual(h.query(), EXPECTED);
  assert.equal(h.callbacks.size, 4);
  h.stop();
});

console.log(`Passed ${passed} synthetic GNOME extension runtime tests`);
