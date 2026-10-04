import type { ToolsRuntimeSnapshot } from "../../../shared/types/tools.ts";
import { ToolsRuntimeService } from "./toolsRuntimeService.ts";

type ToolsRuntimeSnapshotListener = (snapshot: ToolsRuntimeSnapshot) => void;

interface ToolsRuntimeSnapshotStoreDeps {
  getSnapshot: () => Promise<ToolsRuntimeSnapshot>;
  onChanged: (listener: ToolsRuntimeSnapshotListener) => Promise<() => void>;
  warn: (message: string, error: unknown) => void;
}

export interface ToolsRuntimeSnapshotStore {
  getCurrentSnapshot: () => ToolsRuntimeSnapshot | null;
  runAction: (action: () => Promise<ToolsRuntimeSnapshot>) => Promise<void>;
  refreshSnapshot: () => Promise<ToolsRuntimeSnapshot>;
  subscribe: (listener: ToolsRuntimeSnapshotListener) => () => void;
}

export function createToolsRuntimeSnapshotStore(
  deps: ToolsRuntimeSnapshotStoreDeps,
): ToolsRuntimeSnapshotStore {
  const listeners = new Set<ToolsRuntimeSnapshotListener>();
  let currentSnapshot: ToolsRuntimeSnapshot | null = null;
  let revision = 0;
  let publishedRevision = 0;
  let subscription: object | null = null;
  let runtimeUnlisten: (() => void) | null = null;
  let pendingRuntimeListen: Promise<void> | null = null;
  let pendingRefresh: Promise<ToolsRuntimeSnapshot> | null = null;
  let pendingAction: Promise<void> | null = null;

  function publish(snapshot: ToolsRuntimeSnapshot) {
    currentSnapshot = snapshot;
    publishedRevision = ++revision;
    for (const listener of listeners) listener(snapshot);
  }

  function detachIfUnused() {
    if (listeners.size || pendingRefresh || pendingAction) return;
    subscription = null;
    pendingRuntimeListen = null;
    const dispose = runtimeUnlisten;
    runtimeUnlisten = null;
    dispose?.();
  }

  function ensureRuntimeListener(): Promise<void> {
    if (runtimeUnlisten) return Promise.resolve();
    if (pendingRuntimeListen) return pendingRuntimeListen;
    const token = {};
    subscription = token;
    const request = Promise.resolve().then(() => deps.onChanged(snapshot => {
      if (subscription === token) publish(snapshot);
    })).then(dispose => {
      if (subscription !== token) { dispose(); return; }
      runtimeUnlisten = dispose;
      detachIfUnused();
    }).catch(error => {
      if (subscription === token) deps.warn("listen tools runtime snapshot failed", error);
    }).finally(() => {
      if (pendingRuntimeListen === request) pendingRuntimeListen = null;
    });
    pendingRuntimeListen = request;
    return request;
  }

  async function loadSnapshot(): Promise<ToolsRuntimeSnapshot> {
    for (;;) {
      // A refresh begun before a write must not resurrect its pre-write state.
      if (pendingAction) { await pendingAction.catch(() => {}); continue; }
      await ensureRuntimeListener();
      if (pendingAction) continue;
      const expected = revision;
      try {
        const snapshot = await deps.getSnapshot();
        if (revision === expected) { publish(snapshot); return snapshot; }
      } catch (error) {
        if (revision === expected) throw error;
      }
      // Event snapshots are already ordered by the runtime adapter. Wall-clock
      // timestamps cannot order them against HTTP/IPC responses.
      if (currentSnapshot && publishedRevision > expected) return currentSnapshot;
      // Only a write invalidated this read: wait for it, then read current facts.
    }
  }

  function refreshSnapshot(): Promise<ToolsRuntimeSnapshot> {
    if (pendingRefresh) return pendingRefresh;
    const request = Promise.resolve().then(loadSnapshot).finally(() => {
      if (pendingRefresh === request) pendingRefresh = null;
      detachIfUnused();
    });
    pendingRefresh = request;
    return request;
  }

  function runAction(action: () => Promise<ToolsRuntimeSnapshot>): Promise<void> {
    if (pendingAction) return pendingAction;
    const expected = ++revision;
    let needsRefresh = false;
    const request = Promise.resolve().then(async () => {
      await ensureRuntimeListener();
      const snapshot = await action();
      if (revision === expected) publish(snapshot);
      else needsRefresh = true;
    }).catch(error => {
      needsRefresh = true;
      throw error;
    }).finally(() => {
      if (pendingAction === request) pendingAction = null;
      if (needsRefresh) {
        // Re-read after uncertain results; never repeat a mutation automatically.
        void refreshSnapshot().catch(error => deps.warn("reload Tools after action failed", error));
      }
      detachIfUnused();
    });
    pendingAction = request;
    return request;
  }

  return {
    getCurrentSnapshot: () => currentSnapshot,
    runAction,
    refreshSnapshot,
    subscribe(listener) {
      listeners.add(listener);
      if (currentSnapshot) listener(currentSnapshot);
      void ensureRuntimeListener();
      return () => { listeners.delete(listener); detachIfUnused(); };
    },
  };
}

export const toolsRuntimeSnapshotStore = createToolsRuntimeSnapshotStore({
  getSnapshot: ToolsRuntimeService.getToolsSnapshot,
  onChanged: ToolsRuntimeService.onToolsRuntimeChanged,
  warn: (message, error) => console.warn(message, error),
});

export function prewarmToolsRuntimeSnapshot(): Promise<ToolsRuntimeSnapshot> {
  return toolsRuntimeSnapshotStore.refreshSnapshot();
}
