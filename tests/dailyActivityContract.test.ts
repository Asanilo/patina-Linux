import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolveNativeSessionPrecedence, type OwnedTimeRange } from "../src/platform/persistence/nativeSessionPrecedence.ts";
import { shouldTrackProcess, resolveCanonicalExecutable } from "../src/shared/classification/processNormalization.ts";

interface DailyFixture {
  name: string;
  boundaries: number[];
  sampledAt: number;
  records: Array<Omit<OwnedTimeRange, "endTime"> & { endTime: number | null }>;
  excludedKeys: string[];
  expectedDaily: number[];
  expectedLegacy: number[];
}

const fixtures: DailyFixture[] = JSON.parse(await readFile(new URL("./fixtures/daily-activity-cases.json", import.meta.url), "utf8"));
for (const fixture of fixtures) {
  const records = fixture.records.map((record) => ({ ...record, endTime: record.endTime ?? fixture.sampledAt }));
  const daily = fixture.boundaries.slice(0, -1).map((start, index) => {
    const scoped = resolveNativeSessionPrecedence(records, { startTime: start, endTime: fixture.boundaries[index + 1] });
    return scoped.filter((record) => !fixture.excludedKeys.includes(record.key))
      .reduce((total, record) => total + record.endTime - record.startTime, 0);
  });
  assert.deepEqual(daily, fixture.expectedDaily, fixture.name);
  // The current Desktop resolves the whole query first and does not apply app exclusions here.
  const whole = resolveNativeSessionPrecedence(records, {
    startTime: fixture.boundaries[0], endTime: fixture.boundaries.at(-1)!,
  });
  const legacy = fixture.boundaries.slice(0, -1).map((start, index) => whole.reduce((total, record) => (
    total + Math.max(0, Math.min(record.endTime, fixture.boundaries[index + 1]) - Math.max(record.startTime, start))
  ), 0));
  assert.deepEqual(legacy, fixture.expectedLegacy, `${fixture.name}: existing Desktop policy`);
}

const filterCases: Array<[string, string, string, boolean]> = [
  ["zen", "Zen", "Article", true],
  ["ghostty", "Ghostty", "cargo build", true],
  ["setup.exe", "Installer", "", false],
  ["cursor-updater.exe", "Cursor", "", false],
  ["abc.tmp", "Temporary", "", false],
  ["lockapp.exe", "Lock", "", false],
  ["app-1.0-x64.exe", "App", "Installing", false],
  ["app-1.0-x64.exe", "App", "Document", true],
  ["launcher.exe", "Wallpaper Engine Launcher", "", false],
  ["launcher.exe", "Game Launcher", "", true],
  ["geek-uninstaller.exe", "Geek Uninstaller", "", true],
  ["myupdate.exe", "MyUpdate", "", false],
  ["myupdate.exe", "Editor", "Document", true],
];
for (const [exeName, appName, windowTitle, expected] of filterCases) {
  assert.equal(shouldTrackProcess(exeName, { appName, windowTitle }), expected, `${exeName}: ${windowTitle}`);
}
const sharedFilterCases: Array<[string, string, string, boolean]> = JSON.parse(
  await readFile(new URL("./fixtures/activity-read-filter-cases.json", import.meta.url), "utf8"),
);
for (const [exeName, appName, windowTitle, expected] of sharedFilterCases) {
  assert.equal(shouldTrackProcess(exeName, { appName, windowTitle }), expected, JSON.stringify([exeName, appName, windowTitle]));
}
console.log(`PASS ${sharedFilterCases.length} cross-runtime historical filter cases`);
const aliasCases: Array<[string, string]> = JSON.parse(
  await readFile(new URL("./fixtures/activity-read-alias-cases.json", import.meta.url), "utf8"),
);
for (const [exe, expected] of aliasCases) {
  assert.equal(resolveCanonicalExecutable(exe), expected, exe);
}
console.log(`PASS ${aliasCases.length} cross-runtime executable aliases`);
console.log(`PASS ${fixtures.length} shared daily activity fixtures and ${filterCases.length} Desktop filter cases`);
