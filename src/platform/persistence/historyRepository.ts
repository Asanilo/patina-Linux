import { invoke } from "@tauri-apps/api/core";
import type { HistorySession } from "../../shared/types/sessions.ts";
import { isAppCategory } from "../../shared/classification/categoryTokens.ts";
import { getUiTextLanguage, type UiLanguage } from "../../shared/copy/uiText.ts";
import { parseActivityReadHealth, type ActivityReadHealth } from "./activityReadHealth.ts";
export interface ExactHistoryRead {
    fromMs: number;
    toMs: number;
    sampledAtMs: number;
    configurationRevision: string;
    trackingHealth: ActivityReadHealth;
    sessions: HistorySession[];
}
let requestQueue: Promise<unknown> = Promise.resolve();
function record(value: unknown): value is Record<string, unknown> { return !!value && typeof value === "object" && !Array.isArray(value); }
function integer(value: unknown): value is number { return typeof value === "number" && Number.isSafeInteger(value); }
const encoder = new TextEncoder();
function text(value: unknown, limit: number): value is string { return typeof value === "string" && encoder.encode(value).length <= limit; }
export async function getExactHistorySnapshot(fromMs: number, toMs: number, request?: (from: number, to: number) => Promise<unknown>, language: UiLanguage = getUiTextLanguage()): Promise<ExactHistoryRead> {
    if (!integer(fromMs) || !integer(toMs) || toMs <= fromMs || toMs - fromMs > 32 * 86400000)
        throw new Error("Invalid exact history range");
    const read = request ?? ((from: number, to: number) => invoke("cmd_get_exact_history", { fromMs: from, toMs: to, language }));
    const pending = requestQueue.then(() => read(fromMs, toMs));
    requestQueue = pending.then(() => undefined, () => undefined);
    const raw = await pending;
    if (!record(raw) || raw.from_ms !== fromMs || raw.to_ms !== toMs || !integer(raw.sampled_at_ms) || raw.sampled_at_ms < 0
        || typeof raw.configuration_revision !== "string" || !/^[0-9a-f]{64}$/.test(raw.configuration_revision)
        || !Array.isArray(raw.records) || raw.records.length > 40000)
        throw new Error("Invalid exact history snapshot");
    if (encoder.encode(JSON.stringify(raw)).length > 8 * 1024 * 1024)
        throw new Error("Exact history response exceeds budget");
    const health = parseActivityReadHealth(raw.tracking_health, raw.sampled_at_ms);
    let sampleCount = 0, importedId = 0;
    let previous: number[] | null = null;
    const seen = new Set<string>();
    const sessions = raw.records.map((value: unknown): HistorySession => {
        if (!record(value) || (value.origin !== 'native' && value.origin !== 'import_exact') || !integer(value.record_id) || value.record_id <= 0
            || !text(value.app_key, 1024) || !value.app_key || !text(value.exe_name, 1024) || !value.exe_name || !text(value.app_name, 1024)
            || !text(value.window_title, 16384) || !text(value.category, 1024) || !isAppCategory(value.category) || value.category === 'system'
            || !(value.display_name_override === null || (text(value.display_name_override, 4096) && value.display_name_override.length > 0))
            || !integer(value.start_ms) || !integer(value.end_ms) || value.start_ms < fromMs || value.end_ms > toMs || value.end_ms <= value.start_ms
            || !integer(value.continuity_start_ms) || value.continuity_start_ms > value.start_ms || typeof value.is_open !== "boolean" || !Array.isArray(value.title_samples))
            throw new Error("Invalid exact history record");
        const native = value.origin === 'native';
        if (!native && (value.is_open || value.title_samples.length))
            throw new Error("Imported caption cannot be a native sample");
        if (value.is_open && (health.status === 'unavailable' || value.end_ms > health.liveCutoffMs))
            throw new Error("Unconfirmed open activity");
        const order = [value.start_ms, native ? 0 : 1, value.record_id, value.end_ms];
        const key = order.join(':');
        if (seen.has(key))
            throw new Error("Duplicate exact history fragment");
        if (previous) {
            const mismatch = order.findIndex((value, index) => value !== previous![index]);
            if (mismatch >= 0 && order[mismatch] < previous[mismatch])
                throw new Error("Unordered exact history");
        }
        seen.add(key);
        previous = order;
        let lastSampleStart = -Infinity;
        const titleSampleDetails = value.title_samples.map((sample: unknown) => {
            sampleCount++;
            if (sampleCount > 50000 || !record(sample) || !text(sample.title, 16384) || !integer(sample.start_ms) || !integer(sample.end_ms)
                || sample.start_ms < (value.start_ms as number) || sample.end_ms > (value.end_ms as number) || sample.end_ms <= sample.start_ms || sample.start_ms < lastSampleStart)
                throw new Error("Invalid exact title sample");
            lastSampleStart = sample.start_ms;
            return { title: sample.title, startTime: sample.start_ms, endTime: sample.end_ms };
        });
        return { id: native ? value.record_id : -(++importedId), appName: value.app_name, exeName: value.exe_name, windowTitle: value.window_title,
            startTime: value.start_ms, endTime: value.end_ms, duration: value.end_ms - value.start_ms, continuityGroupStartTime: value.continuity_start_ms, titleSampleDetails,
            confirmed: { appKey: value.app_key, category: value.category, displayNameOverride: value.display_name_override, origin: native ? 'native' : 'import_exact', recordId: value.record_id,
                isOpen: value.is_open, isLive: value.is_open && health.status === 'healthy' && value.end_ms === health.liveCutoffMs } };
    });
    return { fromMs, toMs, sampledAtMs: raw.sampled_at_ms, configurationRevision: raw.configuration_revision, trackingHealth: health, sessions };
}
