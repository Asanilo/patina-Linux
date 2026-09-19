import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolveNativeSessionPrecedence, type TimeRecordOrigin } from "../src/platform/persistence/nativeSessionPrecedence.ts";
import { loadRecentObservedSessionStats, loadMigrationObservedSessionStats } from "../src/platform/persistence/observedAppsRepository.ts";

const cases = JSON.parse(readFileSync(new URL("./fixtures/observed-apps.json", import.meta.url), "utf8"));
for (const test of cases) {
  const records = test.facts.map((fact: { origin: TimeRecordOrigin; start: number; end: number | null; exe: string; app: string }, index: number) => ({
    key: String(index), origin: fact.origin, startTime: fact.start,
    endTime: fact.end ?? Math.min(test.sampled, test.to),
    capacityEndTime: fact.origin === "import_bucket" ? fact.start + 3600000 : fact.end ?? Math.min(test.sampled, test.to), value: fact,
  }));
  const resolved = resolveNativeSessionPrecedence(records, { startTime: test.from, endTime: test.to });
  const byExe = new Map<string, { exe_name: string; app_name: string; total_duration_ms: number; last_seen_ms: number }>();
  for (const item of resolved) {
    const fact = item.value!;
    const previous = byExe.get(fact.exe);
    const last = Math.max(previous?.last_seen_ms ?? 0, item.startTime);
    byExe.set(fact.exe, { exe_name: fact.exe, app_name: last === item.startTime ? fact.app : previous!.app_name,
      total_duration_ms: (previous?.total_duration_ms ?? 0) + item.endTime - item.startTime, last_seen_ms: last });
  }
  assert.deepEqual([...byExe.values()], test.expected, test.name);
  const mapped = await loadRecentObservedSessionStats(test.from, test.to, async () => test.expected);
  assert.equal(mapped.length, test.expected.length);
  console.log(`PASS observed apps shared fixture: ${test.name}`);
}

let calls = 0;
await assert.rejects(loadRecentObservedSessionStats(0, 0, async () => { calls++; return []; }), /range/);
assert.equal(calls,0);
const valid = { exe_name: "zen", app_name: "Zen", total_duration_ms: 1000, last_seen_ms: 1000 };
for (const response of [null, {}, [null], [valid, valid], [{...valid, total_duration_ms: -1}], [{...valid, last_seen_ms: 3000}], [{...valid, app_name: "x".repeat(1025)}], Array(4097).fill(valid)]) {
  await assert.rejects(loadRecentObservedSessionStats(0,3000,async () => response), /Invalid/);
}
await assert.rejects(loadRecentObservedSessionStats(0,3000,async () => { throw new Error("unsupported daemon"); }), /unsupported daemon/);
console.log("PASS observed apps adapter validates bounded responses and propagates failures without fallback");
const cutoff = new Date(2026, 8, 1).getTime();
assert.equal((await loadMigrationObservedSessionStats(cutoff, async () => [valid]))[0].lastSeenMs, 1000);
await assert.rejects(loadMigrationObservedSessionStats(cutoff, async () => { throw new Error("overlapping facts exceed budget"); }), /budget/);
await assert.rejects(loadMigrationObservedSessionStats(cutoff, async () => [{ ...valid, last_seen_ms: cutoff }]), /Invalid/);
await assert.rejects(loadMigrationObservedSessionStats(0, async () => []), /cutoff/);
console.log("PASS full-history migration validates old evidence without a recent-history truncation or SQL fallback");
