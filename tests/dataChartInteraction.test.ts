import assert from "node:assert/strict";
import { resolveTrendDateFromChartEvent } from "../src/features/data/services/dataChartInteraction.ts";

let passed = 0;

async function runTest(name: string, fn: () => Promise<void> | void) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("chart interaction resolves date from active payload", () => {
  assert.equal(
    resolveTrendDateFromChartEvent({
      activePayload: [
        {
          payload: {
            date: "2026-05-29",
            hours: 1.5,
          },
        },
      ],
    }),
    "2026-05-29",
  );
});

await runTest("chart interaction falls back to direct payload", () => {
  assert.equal(
    resolveTrendDateFromChartEvent({
      payload: {
        date: "2026-05-28",
      },
    }),
    "2026-05-28",
  );
});

await runTest("chart interaction resolves date from active label and points", () => {
  assert.equal(
    resolveTrendDateFromChartEvent(
      {
        activeLabel: "05-28",
        activeTooltipIndex: "3",
      },
      [
        { date: "2026-05-25", label: "05-25" },
        { date: "2026-05-26", label: "05-26" },
        { date: "2026-05-27", label: "05-27" },
        { date: "2026-05-28", label: "05-28" },
      ],
    ),
    "2026-05-28",
  );
});

await runTest("chart interaction resolves date from active index when label is absent", () => {
  assert.equal(
    resolveTrendDateFromChartEvent(
      {
        activeTooltipIndex: 2,
      },
      [
        { date: "2026-05-25", label: "05-25" },
        { date: "2026-05-26", label: "05-26" },
        { date: "2026-05-27", label: "05-27" },
      ],
    ),
    "2026-05-27",
  );
});

await runTest("chart interaction ignores month keys and invalid dates", () => {
  assert.equal(
    resolveTrendDateFromChartEvent({
      activePayload: [{ payload: { date: "2026-05" } }],
    }),
    null,
  );
  assert.equal(
    resolveTrendDateFromChartEvent({
      activePayload: [{ payload: { date: "2026-02-31" } }],
    }),
    null,
  );
  assert.equal(
    resolveTrendDateFromChartEvent(
      { activeLabel: "5月" },
      [{ date: "2026-05", label: "5月" }],
    ),
    null,
  );
});

console.log(`Passed ${passed} data chart interaction tests`);
