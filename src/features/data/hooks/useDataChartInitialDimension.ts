import { useEffect, useLayoutEffect, useRef, useState } from "react";

export type DataChartDimensionKey = "overviewTrend" | "destinationTrend";

interface DataChartDimension {
  width: number;
  height: number;
}

const dimensionCache: Partial<Record<DataChartDimensionKey, DataChartDimension>> = {};
const useIsomorphicLayoutEffect = typeof window === "undefined" ? useEffect : useLayoutEffect;

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function getViewportSize() {
  if (typeof window === "undefined") {
    return { width: 1366, height: 768 };
  }
  return { width: window.innerWidth, height: window.innerHeight };
}

function getFallbackDimension(key: DataChartDimensionKey): DataChartDimension {
  const viewport = getViewportSize();
  if (key === "destinationTrend") {
    return {
      width: viewport.width >= 1900 ? 852 : clampNumber(viewport.width - 520, 420, 860),
      height: viewport.width >= 1900 ? 200 : viewport.width <= 900 ? 172 : 210,
    };
  }

  const isWideReferenceLayout = viewport.width >= 1900;
  return {
    width: isWideReferenceLayout ? 852 : clampNumber(viewport.width - 296, 560, 1280),
    height: viewport.width >= 1536 && viewport.height >= 900 ? 214 : viewport.width <= 900 ? 140 : 168,
  };
}

export function useDataChartInitialDimension(key: DataChartDimensionKey) {
  const chartRef = useRef<HTMLDivElement | null>(null);
  const [initialDimension, setInitialDimension] = useState<DataChartDimension>(
    () => dimensionCache[key] ?? getFallbackDimension(key),
  );

  useIsomorphicLayoutEffect(() => {
    const element = chartRef.current;
    if (!element) return undefined;

    const syncDimension = () => {
      const rect = element.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      if (width <= 0 || height <= 0) return;

      const next = { width, height };
      dimensionCache[key] = next;
      setInitialDimension((previous) => (
        previous.width === width && previous.height === height ? previous : next
      ));
    };

    syncDimension();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", syncDimension);
      return () => window.removeEventListener("resize", syncDimension);
    }

    const observer = new ResizeObserver(syncDimension);
    observer.observe(element);
    return () => observer.disconnect();
  }, [key]);

  return { chartRef, initialDimension };
}
