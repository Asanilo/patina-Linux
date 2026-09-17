import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { aggregateHeatmapDays, getHeatmapRange } from "../src/features/data/services/dataReadModel.ts";
import { getDailyActivity } from "../src/platform/persistence/dailyActivityRepository.ts";

const cases: Record<string, Array<[number, number, number]>> = {
  "UTC": [[2, 8, 24], [10, 1, 24]],
  "Asia/Singapore": [[2, 8, 24], [10, 1, 24]],
  "Asia/Pyongyang": [[2, 8, 24], [10, 1, 24]],
  "America/New_York": [[2, 8, 23], [10, 1, 25]],
  "Australia/Lord_Howe": [[3, 5, 24.5], [9, 4, 23.5]],
};

if (process.env.PATINA_HEATMAP_CALENDAR_CHILD === "1") {
  const timezone = process.env.TZ!;
  for (const [month, day, expectedHours] of cases[timezone]) {
    const start = new Date(2026, month, day);
    const end = new Date(2026, month, day + 1);
    assert.equal((end.getTime() - start.getTime()) / 3_600_000, expectedHours, timezone);
    const daily = await getDailyActivity(start.getTime(), end.getTime(), async () => ({
      sampled_at_ms: end.getTime(), earliest_start_ms: start.getTime(),
      days: [{ start_ms: start.getTime(), end_ms: end.getTime(), active_ms: expectedHours * 3_600_000 }],
    }));
    assert.equal(daily.days[0].duration, expectedHours * 3_600_000);
    const days = aggregateHeatmapDays([{
      appName: "App", exeName: "app", startTime: start.getTime(), endTime: end.getTime(),
    }], 2026, end.getTime());
    const key = `2026-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    assert.equal(days.find((entry) => entry.date === key)?.duration, expectedHours * 3_600_000);
    assert.equal(days.reduce((total, entry) => total + entry.duration, 0), expectedHours * 3_600_000);
    // A session crossing the next midnight must neither overlap nor leave a gap.
    const extended = aggregateHeatmapDays([{
      appName: "App", exeName: "app", startTime: start.getTime(), endTime: end.getTime() + 3_600_000,
    }], 2026, end.getTime() + 3_600_000);
    assert.equal(extended.find((entry) => entry.date === key)?.duration, expectedHours * 3_600_000);
    assert.equal(extended.reduce((total, entry) => total + entry.duration, 0), (expectedHours + 1) * 3_600_000);
  }
  const range = getHeatmapRange(2026, Date.now());
  assert.equal(range.start.getDay(), 1);
  assert.equal(range.end.getDay(), 1);
  assert.ok(range.weekCount <= 54);
  // Pyongyang's 2015 offset rollback must not add a phantom calendar week.
  assert.equal(getHeatmapRange(2015, Date.now()).weekCount, 53);
  console.log(`PASS heatmap local calendar ${timezone}`);
} else {
  for (const timezone of Object.keys(cases)) {
    const output = execFileSync(process.execPath, ["--experimental-strip-types", fileURLToPath(import.meta.url)], {
      env: { ...process.env, TZ: timezone, PATINA_HEATMAP_CALENDAR_CHILD: "1" },
      encoding: "utf8", timeout: 30_000,
    });
    process.stdout.write(output);
  }
}
