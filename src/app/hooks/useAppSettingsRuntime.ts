import { useCallback, useEffect, useRef, useState, type SetStateAction } from "react";
import { DEFAULT_SETTINGS, PRODUCT_POLICY_SETTING_KEYS, RESOURCE_SETTING_KEYS, type AppSettings } from "../../shared/settings/appSettings.ts";
import { SnapshotReadController, SNAPSHOT_READ_RETRY_DELAYS_MS } from "../../shared/lib/snapshotReadController.ts";
import {
  loadCurrentAppSettingsSnapshot,
  saveMinSessionSecsSetting,
  subscribeAppSettingsChanged,
  type AppSettingsReadSnapshot,
} from "../services/appSettingsRuntimeService.ts";

interface SettingsState {
  settings: AppSettings;
  baseline: AppSettingsReadSnapshot | null;
}

/** Own the displayed settings and the exact baseline used by inline policy writes. */
export function useAppSettingsRuntime() {
  const [state, setState] = useState<SettingsState>({ settings: DEFAULT_SETTINGS, baseline: null });
  const ownerRef = useRef<SnapshotReadController<AppSettingsReadSnapshot> | null>(null);
  const activeWriteRef = useRef<object | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unsubscribe: (() => void) | undefined;
    const owner = new SnapshotReadController(
      loadCurrentAppSettingsSnapshot,
      snapshot => setState({ settings: snapshot.settings, baseline: snapshot }),
      error => console.warn("Failed to reload app settings", error),
      () => 0,
      { retryDelaysMs: SNAPSHOT_READ_RETRY_DELAYS_MS },
    );
    ownerRef.current = owner;
    activeWriteRef.current = null;
    setSaving(false);
    setState(current => ({ ...current, baseline: null }));
    const refreshOnForeground = () => {
      if (document.visibilityState !== "hidden") owner.refresh(true);
    };
    window.addEventListener("focus", refreshOnForeground);
    document.addEventListener("visibilitychange", refreshOnForeground);
    void subscribeAppSettingsChanged(() => owner.refresh(true)).then(off => {
      if (cancelled) {
        off();
        return;
      }
      unsubscribe = off;
      owner.refresh();
    }).catch(error => {
      if (!cancelled) {
        console.warn("Settings subscription failed", error);
        owner.refresh();
      }
    });
    return () => {
      cancelled = true;
      owner.dispose();
      unsubscribe?.();
      if (ownerRef.current === owner) ownerRef.current = null;
      window.removeEventListener("focus", refreshOnForeground);
      document.removeEventListener("visibilitychange", refreshOnForeground);
    };
  }, []);

  const setAppSettings = useCallback((action: SetStateAction<AppSettings>) => {
    setState(current => {
      const settings = typeof action === "function" ? action(current.settings) : action;
      const changesProduct = [...PRODUCT_POLICY_SETTING_KEYS, ...RESOURCE_SETTING_KEYS]
        .some(key => settings[key] !== current.settings[key]);
      return { settings, baseline: changesProduct ? null : current.baseline };
    });
  }, []);
  const refreshSettings = useCallback(() => ownerRef.current?.refresh(true), []);

  const updateMinSessionSecs = useCallback(async (nextValue: number) => {
    const owner = ownerRef.current;
    const baseline = state.baseline;
    if (activeWriteRef.current) return;
    if (!owner || !baseline || baseline.settings.minSessionSecs !== state.settings.minSessionSecs) {
      throw new Error("Product settings baseline is unavailable; reload before saving");
    }
    if (nextValue === state.settings.minSessionSecs) return;
    const ticket = {};
    activeWriteRef.current = ticket;
    setSaving(true);
    const current = () => ownerRef.current === owner;
    try {
      const confirmed = await saveMinSessionSecsSetting(nextValue, baseline.productRevision);
      if (!current()) return;
      setState(latest => {
        // An intervening read (even of the same revision) cannot be ordered
        // against this response. Keep its display and require a fresh baseline.
        if (latest.baseline !== baseline || latest.settings.minSessionSecs !== baseline.settings.minSessionSecs) {
          return { ...latest, baseline: null };
        }
        const minSessionSecs = confirmed.settings.minSessionSecs;
        return {
          settings: { ...latest.settings, minSessionSecs },
          baseline: {
            ...baseline,
            settings: { ...baseline.settings, minSessionSecs },
            productRevision: confirmed.revision,
          },
        };
      });
    } catch (error) {
      if (current()) {
        setState(latest => ({ ...latest, baseline: null }));
        throw error;
      }
    } finally {
      if (activeWriteRef.current === ticket) activeWriteRef.current = null;
      if (current()) {
        setSaving(false);
        owner.refresh(true);
      }
    }
  }, [state.baseline, state.settings.minSessionSecs]);

  return {
    appSettings: state.settings,
    setAppSettings,
    refreshSettings,
    updateMinSessionSecs,
    minSessionUpdatePending: saving,
    canUpdateMinSession: !!state.baseline && state.baseline.settings.minSessionSecs === state.settings.minSessionSecs,
  };
}
