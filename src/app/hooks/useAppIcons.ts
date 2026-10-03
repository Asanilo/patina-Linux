import { useEffect, useState } from "react";
import { loadAppIcons, subscribeAppIconInvalidation } from "../services/appIconService.ts";
import { SnapshotReadController } from "../../shared/lib/snapshotReadController.ts";

/** Shared main-window presentation assets do not block any activity snapshot. */
export function useAppIcons(ready: boolean, foreground: boolean): Record<string,string> {
  const [icons, setIcons] = useState<Record<string,string>>({});
  useEffect(() => {
    if (!ready || !foreground) return;
    const abort = new AbortController();
    const owner = new SnapshotReadController(() => loadAppIcons(abort.signal), setIcons,
      error => {console.warn("Cached application icons unavailable", error);}, () => 0);
    owner.refresh();
    const timer = window.setInterval(() => owner.refresh(), 30000);
    let unlisten: (() => void) | undefined;
    void subscribeAppIconInvalidation(() => owner.refresh(true)).then(stop => {
      if (abort.signal.aborted) stop(); else unlisten = stop;
    }).catch(error => {if (!abort.signal.aborted) console.warn("Icon invalidation subscription unavailable", error);});
    return () => {
      owner.dispose(); abort.abort(); unlisten?.();
      window.clearInterval(timer);
    };
  }, [ready, foreground]);
  return icons;
}
