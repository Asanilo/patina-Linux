import { invoke } from "@tauri-apps/api/core";

interface CachedIcon {source_key: string; keys: string[]; data_url: string}
interface IconPage {entries: CachedIcon[]; next_after: string | null}
const encoder = new TextEncoder();
const keyBytes = (value: unknown): value is string => typeof value === "string" && encoder.encode(value).length <= 1024;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
function afterKey(key: string, previous: string): boolean {
  const a = encoder.encode(key), b = encoder.encode(previous);
  for (let index = 0; index < Math.min(a.length, b.length); index++) {
    if (a[index] !== b[index]) return a[index] > b[index];
  }
  return a.length > b.length;
}
function parsePage(raw: unknown, after: string | null): IconPage {
  if (!record(raw) || !Array.isArray(raw.entries) || raw.entries.length > 64
    || !(raw.next_after === null || keyBytes(raw.next_after))) throw new Error("Invalid cached icon page");
  let previous = after;
  const entries = raw.entries.map((entry: unknown): CachedIcon => {
    if (!record(entry) || !keyBytes(entry.source_key) || !entry.source_key
      || (previous !== null && !afterKey(entry.source_key, previous))
      || !Array.isArray(entry.keys) || entry.keys.length > 3
      || !entry.keys.every(key => keyBytes(key) && key.length > 0) || new Set(entry.keys).size !== entry.keys.length
      || typeof entry.data_url !== "string" || encoder.encode(entry.data_url).length > 512 * 1024
      || !/^data:image\/(?:png|svg\+xml);base64,[A-Za-z0-9+/]+={0,2}$/.test(entry.data_url)
      || entry.data_url.slice(entry.data_url.indexOf(',')+1).length % 4 !== 0) throw new Error("Invalid cached icon entry");
    previous = entry.source_key;
    return {source_key: entry.source_key, keys: entry.keys as string[], data_url: entry.data_url};
  });
  if (raw.next_after !== null && (!entries.length || raw.next_after !== entries[entries.length - 1].source_key)) throw new Error("Icon cursor did not advance");
  return {entries, next_after: raw.next_after};
}

/** Presentation cache pages may span concurrent writes. A failed page never
 * publishes a partial map, and activity facts never depend on icon availability. */
export async function getIconMap(request: (after: string | null) => Promise<unknown> = after => invoke("cmd_get_cached_icon_page", {after, limit: 64}), signal?: AbortSignal): Promise<Record<string,string>> {
  const result: Record<string,string> = Object.create(null);
  let after: string | null = null, bytes = 0, count = 0;
  for (let pageCount = 0; pageCount < 128; pageCount++) {
    if (signal?.aborted) throw new Error("Cached icon read cancelled");
    const raw = await request(after);
    if (signal?.aborted) throw new Error("Cached icon read cancelled");
    const size = encoder.encode(JSON.stringify(raw)).length;
    bytes += size;
    if (size > 2 * 1024 * 1024 || bytes > 32 * 1024 * 1024) throw new Error("Cached icon response budget exceeded");
    const page = parsePage(raw, after);
    count += page.entries.length;
    if (count > 4096) throw new Error("Cached icon entry budget exceeded");
    for (const entry of page.entries) for (const key of entry.keys) result[key] = entry.data_url;
    if (page.next_after === null) return result;
    after = page.next_after;
  }
  throw new Error("Cached icon pagination budget exceeded");
}
