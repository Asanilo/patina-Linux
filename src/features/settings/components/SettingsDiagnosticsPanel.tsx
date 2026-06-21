import { Activity, Globe2, MonitorCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";
import {
  getWebActivityBridgeSnapshot,
  type WebActivityBridgeSnapshot,
} from "../../../platform/runtime/webActivityBridgeGateway.ts";
import {
  getLocalApiDiagnostics,
  type LocalApiDiagnosticsSnapshot,
} from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import {
  buildSettingsDiagnosticsViewModel,
  type SettingsDiagnosticItem,
} from "../services/settingsDiagnosticsViewModel.ts";

type SettingsDiagnosticsPanelProps = {
  trackerHealth: TrackerHealthSnapshot;
  webActivityEnabled: boolean;
  webActivityPort: number;
  webActivityToken: string;
};

const BRIDGE_DIAGNOSTICS_REFRESH_MS = 5_000;

const DIAGNOSTIC_ICONS = {
  "window-tracking": MonitorCheck,
  "local-api": Activity,
  "browser-bridge": Globe2,
};

export default function SettingsDiagnosticsPanel({
  trackerHealth,
  webActivityEnabled,
  webActivityPort,
  webActivityToken,
}: SettingsDiagnosticsPanelProps) {
  const [bridgeSnapshot, setBridgeSnapshot] = useState<WebActivityBridgeSnapshot | null>(null);
  const [localApiSnapshot, setLocalApiSnapshot] = useState<LocalApiDiagnosticsSnapshot | null>(null);

  useEffect(() => {
    let disposed = false;

    const refresh = async () => {
      try {
        const [bridge, localApi] = await Promise.allSettled([
          getWebActivityBridgeSnapshot(),
          getLocalApiDiagnostics(),
        ]);
        if (disposed) return;

        if (bridge.status === "fulfilled") {
          setBridgeSnapshot(bridge.value);
        } else {
          setBridgeSnapshot(null);
          console.warn("load web activity bridge snapshot failed", bridge.reason);
        }

        if (localApi.status === "fulfilled") {
          setLocalApiSnapshot(localApi.value);
        } else {
          setLocalApiSnapshot(null);
          console.warn("load local API diagnostics failed", localApi.reason);
        }
      } catch (error) {
        if (!disposed) {
          setBridgeSnapshot(null);
          setLocalApiSnapshot(null);
          console.warn("load settings diagnostics failed", error);
        }
      }
    };

    void refresh();
    const timerId = window.setInterval(() => {
      void refresh();
    }, BRIDGE_DIAGNOSTICS_REFRESH_MS);

    return () => {
      disposed = true;
      window.clearInterval(timerId);
    };
  }, []);

  const diagnostics = useMemo(() => buildSettingsDiagnosticsViewModel({
    trackerHealth,
    webActivityEnabled,
    webActivityPort,
    webActivityToken,
    webActivityBridge: bridgeSnapshot,
    localApi: localApiSnapshot,
  }), [bridgeSnapshot, localApiSnapshot, trackerHealth, webActivityEnabled, webActivityPort, webActivityToken]);

  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="mb-5 flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
        <Activity size={16} className="text-[var(--qp-accent-default)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">
          {UI_TEXT.settings.diagnosticsTitle}
        </h2>
      </div>

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
        {diagnostics.map((item) => (
          <DiagnosticItem key={item.id} item={item} />
        ))}
      </div>
    </section>
  );
}

function DiagnosticItem({ item }: { item: SettingsDiagnosticItem }) {
  const Icon = DIAGNOSTIC_ICONS[item.id as keyof typeof DIAGNOSTIC_ICONS] ?? Activity;
  const toneClassName = item.tone === "ok"
    ? "text-[var(--qp-success)]"
    : item.tone === "warning"
      ? "text-[var(--qp-warning)]"
      : "text-[var(--qp-text-tertiary)]";

  return (
    <QuietSubpanel className="min-w-0">
      <div className="flex items-start gap-3">
        <div className={`mt-0.5 shrink-0 ${toneClassName}`}>
          <Icon size={16} />
        </div>
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <p className="truncate text-sm font-semibold text-[var(--qp-text-primary)]">{item.label}</p>
            <span className={`qp-status px-2 py-0.5 text-[11px] font-semibold ${toneClassName}`}>
              {item.value}
            </span>
          </div>
          <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-secondary)]">{item.detail}</p>
        </div>
      </div>
    </QuietSubpanel>
  );
}
