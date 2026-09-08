import { useCallback, useState } from "react";
import type { DestinationDetailOpenRequest } from "../types.ts";

export function useDestinationDetailLauncher() {
  const [request, setRequest] = useState<DestinationDetailOpenRequest | null>(null);

  const open = useCallback((nextRequest: DestinationDetailOpenRequest) => {
    setRequest(nextRequest);
  }, []);

  const close = useCallback(() => {
    setRequest(null);
  }, []);

  return {
    request,
    open,
    close,
  };
}
