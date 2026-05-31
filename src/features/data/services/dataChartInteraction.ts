function isDateKey(value: unknown): value is string {
  if (typeof value !== "string") {
    return false;
  }

  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) {
    return false;
  }

  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);

  return date.getFullYear() === year
    && date.getMonth() === month - 1
    && date.getDate() === day;
}

function readDateFromPayload(payload: unknown): string | null {
  if (typeof payload !== "object" || payload === null || !("date" in payload)) {
    return null;
  }

  const date = (payload as { date?: unknown }).date;
  return isDateKey(date) ? date : null;
}

interface TrendDatePoint {
  date?: unknown;
  label?: unknown;
}

function readStringProperty(source: unknown, key: string) {
  if (typeof source !== "object" || source === null || !(key in source)) {
    return null;
  }

  const value = (source as Record<string, unknown>)[key];
  return typeof value === "string" ? value : null;
}

function readNumberProperty(source: unknown, key: string) {
  if (typeof source !== "object" || source === null || !(key in source)) {
    return null;
  }

  const value = (source as Record<string, unknown>)[key];
  if (typeof value === "number" && Number.isInteger(value)) {
    return value;
  }

  if (typeof value === "string" && /^\d+$/.test(value)) {
    return Number(value);
  }

  return null;
}

function findDateByLabel(points: readonly TrendDatePoint[], label: string) {
  const point = points.find((item) => item.label === label);
  return point && isDateKey(point.date) ? point.date : null;
}

function findDateByIndex(points: readonly TrendDatePoint[], index: number) {
  const point = points[index];
  return point && isDateKey(point.date) ? point.date : null;
}

export function resolveTrendDateFromChartEvent(
  event: unknown,
  points: readonly TrendDatePoint[] = [],
): string | null {
  if (typeof event !== "object" || event === null) {
    return null;
  }

  const activePayload = (event as { activePayload?: unknown }).activePayload;
  if (Array.isArray(activePayload)) {
    for (const item of activePayload) {
      if (typeof item !== "object" || item === null || !("payload" in item)) {
        continue;
      }

      const date = readDateFromPayload((item as { payload?: unknown }).payload);
      if (date) {
        return date;
      }
    }
  }

  const tooltipPayload = (event as { tooltipPayload?: unknown }).tooltipPayload;
  if (Array.isArray(tooltipPayload)) {
    for (const item of tooltipPayload) {
      if (typeof item !== "object" || item === null || !("payload" in item)) {
        continue;
      }

      const date = readDateFromPayload((item as { payload?: unknown }).payload);
      if (date) {
        return date;
      }
    }
  }

  const directPayloadDate = readDateFromPayload((event as { payload?: unknown }).payload);
  if (directPayloadDate) {
    return directPayloadDate;
  }

  const activeLabel = readStringProperty(event, "activeLabel") ?? readStringProperty(event, "label");
  if (activeLabel) {
    const date = findDateByLabel(points, activeLabel);
    if (date) {
      return date;
    }
  }

  const activeIndex = readNumberProperty(event, "activeTooltipIndex") ?? readNumberProperty(event, "activeIndex");
  return activeIndex === null ? null : findDateByIndex(points, activeIndex);
}
