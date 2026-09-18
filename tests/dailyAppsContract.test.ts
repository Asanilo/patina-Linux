import assert from "node:assert/strict";
import { getDailyApps } from "../src/platform/persistence/dailyAppsRepository.ts";

const start = new Date(2026, 8, 1).getTime();
const middle = new Date(2026, 8, 2).getTime();
const end = new Date(2026, 8, 3).getTime();
const app = { app_key: "zen", active_ms: 37 };
const day = { start_ms: start, end_ms: middle, active_ms: 37, apps: [app] };
const empty = { start_ms: middle, end_ms: end, active_ms: 0, apps: [] };
const response = { sampled_at_ms: end, days: [day, empty] };
assert.deepEqual(await getDailyApps(start, end, async (from, to) => {
  assert.equal(from, "2026-09-01");
  assert.equal(to, "2026-09-03");
  return response;
}), {
  sampledAtMs: end,
  days: [
    { date: "2026-09-01", duration: 37, apps: [{ appKey: "zen", duration: 37 }] },
    { date: "2026-09-02", duration: 0, apps: [] },
  ],
});
for (const invalid of [null, {}, { ...response, days: [day] },
  { ...response, days: [empty, day] },
  { ...response, sampled_at_ms: Number.NaN },
  ...[
    { ...day, apps: [] },
    { ...day, apps: [app, app], active_ms: 74 },
    { ...day, start_ms: start + 1 },
    { ...day, apps: null },
    ...[-1, 0, 0.5, Number.MAX_SAFE_INTEGER + 1].map(active_ms => ({ ...day, apps: [{ ...app, active_ms }], active_ms })),
    ...["", "字".repeat(342)].map(app_key => ({ ...day, apps: [{ ...app, app_key }] })),
  ].map(invalidDay => ({ ...response, days: [invalidDay, empty] })),
]) {
  await assert.rejects(getDailyApps(start, end, async () => invalid));
}
await assert.rejects(getDailyApps(start, end, async () => { throw new Error("daily-apps-unsupported"); }), /daily-apps-unsupported/);
const many = (prefix: string, count: number) => Array.from({ length: count }, (_, index) => ({ app_key: `${prefix}-${index}`, active_ms: 1 }));
await assert.rejects(getDailyApps(start, end, async () => ({ ...response, days: [
  { ...day, apps: many("first", 2050), active_ms: 2050 },
  { ...empty, apps: many("second", 2050), active_ms: 2050 },
] })), /budget/);
await assert.rejects(getDailyApps(start, end, async () => ({ ...response, days: [
  { ...day, active_ms: 2 * Number.MAX_SAFE_INTEGER, apps: [
    { app_key: "a", active_ms: Number.MAX_SAFE_INTEGER },
    { app_key: "b", active_ms: Number.MAX_SAFE_INTEGER },
  ] }, empty,
] })), /budget/);
assert.equal((await getDailyApps(start, end, async () => response)).days[0].duration, 37);
console.log("PASS daily application contract, local dates, sum consistency, metadata, failure and retry");
