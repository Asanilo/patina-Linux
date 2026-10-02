import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { loadClassificationSnapshot } from "../src/platform/persistence/classificationSnapshot.ts";
import { loadSettingValue, loadSettingRowsByKeyPrefix } from "../src/platform/persistence/classificationPersistence.ts";

const valid = { revision: "a".repeat(64), sampled_at_ms: 1000, entries: [
  { key: "__app_override::editor", value: '{"displayName":"编辑器","track":true}' },
  { key: "__deleted_category::music", value: "1" },
] };
assert.deepEqual(await loadClassificationSnapshot(async () => valid), { revision: valid.revision, sampledAtMs: 1000, entries: valid.entries });
for (const raw of [
  { ...valid, revision: "unknown" }, { ...valid, sampled_at_ms: Number.MAX_SAFE_INTEGER + 1 },
  { ...valid, entries: [{ key: "local_api_token", value: "must-not-be-exposed" }] },
  { ...valid, entries: [valid.entries[0], valid.entries[0]] },
  { ...valid, entries: [{ key: "__app_override::", value: "{}" }] },
  { ...valid, entries: [{ key: "__category_label_override::other", value: "中".repeat(2000) }] },
  { ...valid, entries: Array.from({ length: 20_001 }, (_, i) => ({ key: `__custom_category::${i}`, value: "1" })) },
]) { await assert.rejects(loadClassificationSnapshot(async () => raw)); }

// Escaped controls cost more bytes on the wire than in the stored strings.
await assert.rejects(loadClassificationSnapshot(async () => ({ ...valid,
  entries: Array.from({ length: 180 }, (_, i) => ({ key: `__category_label_override::custom:${i}`, value: "\u0000".repeat(4000) })),
})), /response budget/);

const descriptor = Object.getOwnPropertyDescriptor(globalThis, "window");
Object.defineProperty(globalThis, "window", { configurable: true, value: {} });
try {
  const calls: string[] = [];
  mockIPC((command) => { calls.push(command); if (command === "cmd_get_classification_snapshot") return valid; throw new Error("SQL fallback forbidden"); });
  assert.equal(await loadSettingValue("__app_override::editor"), valid.entries[0].value);
  assert.deepEqual(await loadSettingRowsByKeyPrefix("__deleted_category::"), [valid.entries[1]]);
  assert.deepEqual(calls, ["cmd_get_classification_snapshot", "cmd_get_classification_snapshot"]);
  await assert.rejects(loadSettingValue("local_api_token"), /Unsupported/);
  await assert.rejects(loadSettingRowsByKeyPrefix("__"), /Unsupported/);
  assert.equal(calls.length, 2);
  const failure = new Error("daemon does not support classification snapshots");
  calls.length = 0;
  mockIPC((command) => { calls.push(command); throw failure; });
  await assert.rejects(loadSettingRowsByKeyPrefix("__app_override::"), (error) => error === failure);
  assert.deepEqual(calls, ["cmd_get_classification_snapshot"]);
} finally {
  clearMocks();
  if (descriptor) Object.defineProperty(globalThis, "window", descriptor); else Reflect.deleteProperty(globalThis, "window");
}
console.log("Classification snapshot validation and owner-only reads passed");
