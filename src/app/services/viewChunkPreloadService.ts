export type PreloadableView = "history" | "settings" | "mapping" | "data";

export interface LazyViewChunkPreloadOptions {
  views?: PreloadableView[];
  initialDelayMs?: number;
  staggerMs?: number;
  idleTimeoutMs?: number;
}

type ViewChunkLoader = () => Promise<unknown>;
type ViewChunkLoaders = Record<PreloadableView, ViewChunkLoader>;
type SchedulePreloadTask = (
  callback: () => void,
  delayMs: number,
  idleTimeoutMs: number,
) => () => void;

interface LazyViewChunkPreloadDeps {
  loaders?: Partial<ViewChunkLoaders>;
  schedule?: SchedulePreloadTask;
  warn?: (message: string, error: unknown) => void;
}

type IdleWindow = Window & typeof globalThis & {
  requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
  cancelIdleCallback?: (handle: number) => void;
};

const DEFAULT_PRELOADABLE_VIEWS: PreloadableView[] = ["history", "data", "settings", "mapping"];
const DEFAULT_INITIAL_DELAY_MS = 1200;
const DEFAULT_STAGGER_MS = 200;
const DEFAULT_IDLE_TIMEOUT_MS = 1500;

const DEFAULT_VIEW_CHUNK_LOADERS: ViewChunkLoaders = {
  history: () => import("../../features/history/components/History"),
  settings: () => import("../../features/settings/components/Settings"),
  mapping: () => import("../../features/classification/components/AppMapping"),
  data: () => import("../../features/data/components/Data"),
};

function schedulePreloadTask(
  callback: () => void,
  delayMs: number,
  idleTimeoutMs: number,
): () => void {
  if (typeof window === "undefined") {
    const timer = globalThis.setTimeout(callback, delayMs);
    return () => globalThis.clearTimeout(timer);
  }

  let cancelIdle: (() => void) | null = null;
  const timer = window.setTimeout(() => {
    const idleWindow = window as IdleWindow;

    if (idleWindow.requestIdleCallback && idleWindow.cancelIdleCallback) {
      const handle = idleWindow.requestIdleCallback(callback, { timeout: idleTimeoutMs });
      cancelIdle = () => idleWindow.cancelIdleCallback?.(handle);
      return;
    }

    const handle = window.setTimeout(callback, 0);
    cancelIdle = () => window.clearTimeout(handle);
  }, delayMs);

  return () => {
    window.clearTimeout(timer);
    cancelIdle?.();
  };
}

export function scheduleLazyViewChunkPreload(
  options: LazyViewChunkPreloadOptions = {},
  deps: LazyViewChunkPreloadDeps = {},
): () => void {
  const views = options.views ?? DEFAULT_PRELOADABLE_VIEWS;
  const initialDelayMs = options.initialDelayMs ?? DEFAULT_INITIAL_DELAY_MS;
  const staggerMs = options.staggerMs ?? DEFAULT_STAGGER_MS;
  const idleTimeoutMs = options.idleTimeoutMs ?? DEFAULT_IDLE_TIMEOUT_MS;
  const loaders: ViewChunkLoaders = {
    ...DEFAULT_VIEW_CHUNK_LOADERS,
    ...deps.loaders,
  };
  const schedule = deps.schedule ?? schedulePreloadTask;
  const warn = deps.warn ?? console.warn;
  let cancelled = false;
  let cancelCurrentTask: (() => void) | null = null;

  const scheduleView = (index: number, delayMs: number) => {
    if (cancelled || index >= views.length) {
      return;
    }

    cancelCurrentTask = schedule(() => {
      cancelCurrentTask = null;
      void preloadView(index);
    }, delayMs, idleTimeoutMs);
  };

  const preloadView = async (index: number) => {
    if (cancelled) {
      return;
    }

    const view = views[index];
    const loader = loaders[view];

    try {
      await loader();
    } catch (error) {
      warn(`Failed to preload ${view} view chunk`, error);
    }

    if (!cancelled) {
      scheduleView(index + 1, staggerMs);
    }
  };

  scheduleView(0, initialDelayMs);

  return () => {
    cancelled = true;
    cancelCurrentTask?.();
    cancelCurrentTask = null;
  };
}
