import assert from "node:assert/strict";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { getExactHistorySnapshot } from "../src/platform/persistence/historyRepository.ts";
import { getHistoryByDate } from "../src/platform/persistence/sessionReadRepository.ts";
import { compileSessions, getSessionCategory } from "../src/shared/lib/sessionReadCompiler.ts";
import { materializeLiveSessions } from "../src/shared/lib/readModelCore.ts";
import { buildHourlyActivity } from "../src/shared/lib/hourlyActivityCompiler.ts";
import { buildHistoryReadModel } from "../src/features/history/services/historyReadModel.ts";
import { buildHistoryTimelineViewModel } from "../src/features/history/services/historyTimelineViewModel.ts";
import { loadDestinationDetailDay, getDestinationDetailTitleRecords } from "../src/features/destination/services/destinationDetailReadModel.ts";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";
import { getHistorySnapshotCache, setHistorySnapshotCache, clearHistorySnapshotCache } from "../src/features/history/services/historySnapshotCache.ts";
import { loadHistoryRuntimeSnapshotWithDeps } from "../src/app/services/readModelRuntimeService.ts";
import { setUiTextLanguage } from "../src/shared/copy/uiText.ts";
const date = new Date(2026, 3, 18);
const from = date.getTime(), to = new Date(2026, 3, 19).getTime();
const start = from + 3600000, end = start + 120000, sampled = end + 10000;
const source = { origin: "native", record_id: 1, app_key: "editor", app_name: "Editor", exe_name: "editor",
    category: "development", display_name_override: "Research", window_title: "Untimed caption",
    start_ms: start, end_ms: end, continuity_start_ms: start, is_open: false,
    title_samples: [{ title: "Document", start_ms: start, end_ms: start + 20000 },
        { title: "Document", start_ms: start + 21000, end_ms: start + 40000 }] };
const wire = { from_ms: from, to_ms: to, sampled_at_ms: sampled, configuration_revision: "a".repeat(64),
    tracking_health: { status: "healthy", last_heartbeat_ms: sampled, live_cutoff_ms: sampled, stale_after_ms: 8000 },
    records: [source] };
const read = await getExactHistorySnapshot(from, to, async () => wire);
const health = { status: "healthy" as const, lastHeartbeatMs: sampled, checkedAtMs: sampled, staleAfterMs: 8000 };
const build = () => buildHistoryReadModel({ daySessions: read.sessions, trackerHealth: health,
    selectedDate: date, nowMs: sampled, minSessionSecs: 0, mergeThresholdSecs: 180 });
const before = build();
ProcessMapper.setUserOverrides({ editor: { category: "music", displayName: "New local name", track: false } });
assert.deepEqual(build(), before);
assert.equal(before.appSummary[0].duration, 120000);
assert.equal(before.compiledSessions[0].displayName, "Research");
assert.equal(getSessionCategory(before.compiledSessions[0]), "development");
assert.equal(before.compiledSessions[0].titleSampleDetails.length, 2, "equal captions cannot fill a sample gap");
ProcessMapper.clearUserOverrides();
assert.deepEqual(materializeLiveSessions(read.sessions, health, sampled + 86400000), read.sessions);
const captionWire = structuredClone(wire);
captionWire.records[0].title_samples = [];
const caption = (await getExactHistorySnapshot(from, to, async () => captionWire)).sessions;
const captionCompiled = compileSessions(caption, { startMs: from, endMs: to, minSessionSecs: 0 });
assert.equal(captionCompiled[0].displayTitle, "Untimed caption");
assert.deepEqual(captionCompiled[0].titleSampleDetails, []);
const timeline = buildHistoryTimelineViewModel({ sessions: captionCompiled, selectedDate: date, nowMs: sampled, mode: "app" });
assert.ok(timeline.segments.length);
assert.ok(timeline.segments.every(segment => segment.titleSampleDetails.length === 0));
const target = { mode: "app" as const, key: "editor", identityKeys: ["editor"], displayName: "Research", secondaryText: "editor", iconUrl: null, color: "#123456" };
const details = await loadDestinationDetailDay(target, "2026-04-18", sampled, 180, { getAppSessions: async () => caption, getWebSegments: async () => { throw new Error("unexpected web read"); } });
assert.equal(details.totalDuration, 120000);
assert.equal(details.activities.length, 1);
assert.deepEqual(getDestinationDetailTitleRecords(details.activities[0]), []);
const overlapWire = structuredClone(wire);
overlapWire.records.push({ ...structuredClone(source), record_id: 2, start_ms: start + 10000,
    end_ms: end - 10000, title_samples: [{ title: "Independent sample", start_ms: start + 10000, end_ms: start + 30000 }] });
const overlap = (await getExactHistorySnapshot(from, to, async () => overlapWire)).sessions;
const overlapDetails = await loadDestinationDetailDay(target, "2026-04-18", start + 1000, 180, { getAppSessions: async () => overlap, getWebSegments: async () => [] });
assert.equal(overlapDetails.totalDuration, 220000, "client clock and display overlap must not trim confirmed facts");
assert.equal(overlapDetails.lastEndTime, end);
assert.equal(overlapDetails.activities.reduce((sum, activity) => sum + activity.duration, 0), 220000);
assert.ok(overlapDetails.activities.flatMap(getDestinationDetailTitleRecords).some(record => record.title === "Independent sample"));
const gappedDetails = await loadDestinationDetailDay(target, "2026-04-18", sampled, 180, { getAppSessions: async () => read.sessions, getWebSegments: async () => [] });
assert.equal(getDestinationDetailTitleRecords(gappedDetails.activities[0]).length, 2);
const liveWire = structuredClone(wire);
liveWire.records[0].is_open = true;
liveWire.records[0].end_ms = sampled;
const live = (await getExactHistorySnapshot(from, to, async () => liveWire)).sessions;
assert.equal(live[0].confirmed?.isLive, true);
assert.equal(materializeLiveSessions(live, health, sampled + 86400000)[0].endTime, sampled);
liveWire.sampled_at_ms = sampled + 20000;
liveWire.tracking_health.status = "stale";
const stale = await getExactHistorySnapshot(from, to, async () => liveWire);
assert.equal(stale.sessions[0].confirmed?.isLive, false);
assert.equal(stale.sessions[0].duration, sampled - start);
const importedWire = structuredClone(captionWire);
importedWire.records.push({ ...structuredClone(captionWire.records[0]), origin: "import_exact" });
const imported = await getExactHistorySnapshot(from, to, async () => importedWire);
assert.equal(imported.sessions[0].id, 1);
assert.equal(imported.sessions[1].id, -1);
assert.equal(imported.sessions[1].confirmed?.recordId, 1);
assert.equal(buildHourlyActivity([{ ...caption[0], startTime: -60000, endTime: 0, duration: 60000 }]).reduce((sum, point) => sum + point.minutes, 0), 1);
for (const mutate of [
    (v: typeof wire) => { v.from_ms++; },
    (v: typeof wire) => { v.records[0].record_id = 0; },
    (v: typeof wire) => { v.records[0].origin = "bucket"; },
    (v: typeof wire) => { v.records[0].origin = "import_exact"; },
    (v: typeof wire) => { v.records[0].start_ms = from - 1; },
    (v: typeof wire) => { v.records[0].end_ms = to + 1; },
    (v: typeof wire) => { v.records[0].category = "system"; },
    (v: typeof wire) => { v.records[0].title_samples[0].start_ms = start - 1; },
    (v: typeof wire) => { v.records[0].title_samples.reverse(); },
    (v: typeof wire) => { v.records[0].window_title = "中".repeat(6000); },
    (v: typeof wire) => { v.records.push(v.records[0]); },
    (v: typeof wire) => { v.records.push({ ...v.records[0], record_id: 2, start_ms: start - 1 }); },
    (v: typeof wire) => { v.tracking_health.live_cutoff_ms--; },
    (v: typeof wire) => { v.records[0].is_open = true; v.records[0].end_ms = sampled + 1; },
]) {
    const invalid = structuredClone(wire);
    mutate(invalid);
    await assert.rejects(getExactHistorySnapshot(from, to, async () => invalid));
}
await assert.rejects(getExactHistorySnapshot(from, from + 33 * 86400000, async () => { throw new Error("must not request"); }), /Invalid exact history range/);
await assert.rejects(getExactHistorySnapshot(from, to, async () => ({ ...wire, extra: "x".repeat(8 * 1024 * 1024) })), /budget/);
const descriptor = Object.getOwnPropertyDescriptor(globalThis, "window");
Object.defineProperty(globalThis, "window", { configurable: true, value: {} });
try {
    const calls: string[] = [];
    mockIPC((command, payload) => { calls.push(command); assert.equal(command, "cmd_get_exact_history"); assert.equal(payload.fromMs, from); return wire; });
    assert.deepEqual(await getHistoryByDate(date), read.sessions);
    assert.deepEqual(calls, ["cmd_get_exact_history"]);
    mockIPC(() => { throw new Error("backend unavailable"); });
    await assert.rejects(getHistoryByDate(date), /backend unavailable/);
}
finally {
    clearMocks();
    if (descriptor)
        Object.defineProperty(globalThis, "window", descriptor);
    else
        Reflect.deleteProperty(globalThis, "window");
}
clearHistorySnapshotCache();
setUiTextLanguage("zh-CN");
const snapshot = { language: "zh-CN" as const, fetchedAtMs: sampled, liveCutoffMs: sampled, trackerHealth: health,
    daySessions: read.sessions, dayWebSegments: [], webDomainOverrides: {} };
setHistorySnapshotCache(snapshot, date);
assert.equal(getHistorySnapshotCache(date), snapshot);
setUiTextLanguage("en-US");
assert.equal(getHistorySnapshotCache(date), null);
setUiTextLanguage("zh-CN");
let release!: () => void;
let cached = false;
const pending = loadHistoryRuntimeSnapshotWithDeps(date, {
    ensureProcessMapperRuntimeReady: () => new Promise<void>(resolve => { release = resolve; }),
    loadHistorySnapshot: async () => snapshot,
    setHistorySnapshotCache: () => { cached = true; },
});
clearHistorySnapshotCache();
release();
await pending;
assert.equal(cached, false);
console.log("Exact History owner transport, validation, confirmed presentation, samples, overlap and cache guards passed");
