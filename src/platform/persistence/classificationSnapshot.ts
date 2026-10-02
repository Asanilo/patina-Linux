import { invoke } from "@tauri-apps/api/core";

export const CLASSIFICATION_PREFIXES = [
  "__app_override::", "__web_domain_override::", "__category_color_override::",
  "__category_label_override::", "__category_default_color_assignment::",
  "__custom_category::", "__deleted_category::", "__classification_manual_confirmation_migration::",
] as const;

export interface ClassificationSnapshot {
  revision: string;
  sampledAtMs: number;
  entries: Array<{ key: string; value: string }>;
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export function isClassificationKey(key: string): boolean {
  return new TextEncoder().encode(key).length <= 256
    && CLASSIFICATION_PREFIXES.some((prefix) => key.startsWith(prefix) && key.length > prefix.length);
}

export async function loadClassificationSnapshot(
  request: () => Promise<unknown> = () => invoke("cmd_get_classification_snapshot"),
): Promise<ClassificationSnapshot> {
  const raw = await request();
  if (!record(raw) || typeof raw.revision !== "string" || !/^[0-9a-f]{64}$/.test(raw.revision)
    || !Number.isSafeInteger(raw.sampled_at_ms) || !Array.isArray(raw.entries) || raw.entries.length > 20_000) {
    throw new Error("Invalid classification snapshot");
  }
  const seen = new Set<string>();
  const encoder = new TextEncoder();
  let bytes = 512;
  const entries = raw.entries.map((entry: unknown) => {
    if (!record(entry) || typeof entry.key !== "string" || !isClassificationKey(entry.key)
      || typeof entry.value !== "string" || encoder.encode(entry.value).length > 4096 || seen.has(entry.key)) {
      throw new Error("Invalid classification configuration entry");
    }
    seen.add(entry.key);
    const result = { key: entry.key, value: entry.value };
    bytes += encoder.encode(JSON.stringify(result)).length + 1;
    if (bytes > 4 * 1024 * 1024) throw new Error("Classification snapshot exceeds its response budget");
    return result;
  });
  return { revision: raw.revision, sampledAtMs: raw.sampled_at_ms as number, entries };
}
