export interface ActivityReadHealth {
    status: "healthy" | "stale" | "unavailable";
    lastHeartbeatMs: number | null;
    liveCutoffMs: number;
    staleAfterMs: number;
}
export function parseActivityReadHealth(raw: unknown, sampledAtMs: number): ActivityReadHealth {
    if (!raw || typeof raw !== "object" || Array.isArray(raw))
        throw new Error("Missing activity read health");
    const health = raw as Record<string, unknown>;
    if (!Number.isSafeInteger(sampledAtMs) || sampledAtMs < 0
        || !Number.isSafeInteger(health.live_cutoff_ms) || !Number.isSafeInteger(health.stale_after_ms)
        || (health.live_cutoff_ms as number) < 0 || (health.live_cutoff_ms as number) > sampledAtMs || (health.stale_after_ms as number) <= 0) {
        throw new Error("Invalid activity read health");
    }
    const heartbeat = health.last_heartbeat_ms;
    const hasHeartbeat = typeof heartbeat === "number" && Number.isSafeInteger(heartbeat) && heartbeat > 0 && heartbeat <= sampledAtMs;
    const valid = health.status === "unavailable" ? heartbeat === null && health.live_cutoff_ms === 0
        : health.status === "healthy" ? hasHeartbeat && sampledAtMs - (heartbeat as number) <= (health.stale_after_ms as number) && health.live_cutoff_ms === sampledAtMs
            : health.status === "stale" && hasHeartbeat && sampledAtMs - (heartbeat as number) > (health.stale_after_ms as number) && health.live_cutoff_ms === heartbeat;
    if (!valid)
        throw new Error("Inconsistent activity read health");
    return { status: health.status as ActivityReadHealth["status"], lastHeartbeatMs: heartbeat as number | null,
        liveCutoffMs: health.live_cutoff_ms as number, staleAfterMs: health.stale_after_ms as number };
}
