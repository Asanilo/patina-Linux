import type { WebActivityTrendSegment } from "../../shared/types/webActivity.ts";
import { getDB } from "./sqlite.ts";

interface RawWebActivityTrendSegmentRow {
  id: number;
  domain: string;
  normalized_domain: string;
  favicon_url: string | null;
  start_time: number;
  end_time: number | null;
}

export async function getWebActivityTrendSegmentsInRange(
  startMs: number,
  endMs: number,
): Promise<WebActivityTrendSegment[]> {
  const db = await getDB();
  const now = Date.now();
  const rows = await db.select<RawWebActivityTrendSegmentRow[]>(
    `SELECT id,
            domain,
            normalized_domain,
            favicon_url,
            start_time,
            end_time
     FROM web_activity_segments
     WHERE start_time < ?
       AND COALESCE(end_time, ?) > ?
     ORDER BY start_time ASC, id ASC`,
    [endMs, now, startMs],
  );

  return rows.map((row) => ({
    id: row.id,
    domain: row.domain,
    normalizedDomain: row.normalized_domain,
    faviconUrl: row.favicon_url,
    startTime: row.start_time,
    endTime: row.end_time,
  }));
}
