import { Activity, Clipboard, Globe2, MonitorCheck, Power } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import QuietSwitch from "../../../shared/components/QuietSwitch";
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
  getDesktopIntegrationDiagnostics,
  type DesktopIntegrationDiagnosticsSnapshot,
} from "../../../platform/runtime/desktopIntegrationDiagnosticsGateway.ts";
import {
  buildSettingsDiagnosticsViewModel,
  type SettingsDiagnosticItem,
} from "../services/settingsDiagnosticsViewModel.ts";

type SettingsDiagnosticsPanelProps = {
  trackerHealth: TrackerHealthSnapshot;
  webActivityEnabled: boolean;
  webActivityPort: number;
  webActivityToken: string;
  launchAtLoginChecked: boolean;
  onLaunchAtLoginChange: (nextChecked: boolean) => void;
  startMinimizedChecked: boolean;
  startMinimizedDisabled: boolean;
  onStartMinimizedChange: (nextChecked: boolean) => void;
};

const BRIDGE_DIAGNOSTICS_REFRESH_MS = 5_000;

const DIAGNOSTIC_ICONS = {
  "window-tracking": MonitorCheck,
  "local-api": Activity,
  "desktop-integration": Power,
  "browser-bridge": Globe2,
};

export default function SettingsDiagnosticsPanel({
  trackerHealth,
  webActivityEnabled,
  webActivityPort,
  webActivityToken,
  launchAtLoginChecked,
  onLaunchAtLoginChange,
  startMinimizedChecked,
  startMinimizedDisabled,
  onStartMinimizedChange,
}: SettingsDiagnosticsPanelProps) {
  const [bridgeSnapshot, setBridgeSnapshot] = useState<WebActivityBridgeSnapshot | null>(null);
  const [localApiSnapshot, setLocalApiSnapshot] = useState<LocalApiDiagnosticsSnapshot | null>(null);
  const [desktopIntegrationSnapshot, setDesktopIntegrationSnapshot] =
    useState<DesktopIntegrationDiagnosticsSnapshot | null>(null);

  useEffect(() => {
    let disposed = false;

    const refresh = async () => {
      try {
        const [bridge, localApi, desktopIntegration] = await Promise.allSettled([
          getWebActivityBridgeSnapshot(),
          getLocalApiDiagnostics(),
          getDesktopIntegrationDiagnostics(),
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

        if (desktopIntegration.status === "fulfilled") {
          setDesktopIntegrationSnapshot(desktopIntegration.value);
        } else {
          setDesktopIntegrationSnapshot(null);
          console.warn("load desktop integration diagnostics failed", desktopIntegration.reason);
        }
      } catch (error) {
        if (!disposed) {
          setBridgeSnapshot(null);
          setLocalApiSnapshot(null);
          setDesktopIntegrationSnapshot(null);
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
    desktopIntegration: desktopIntegrationSnapshot,
  }), [
    bridgeSnapshot,
    desktopIntegrationSnapshot,
    localApiSnapshot,
    trackerHealth,
    webActivityEnabled,
    webActivityPort,
    webActivityToken,
  ]);

  const handleCopyApiCurl = async () => {
    const baseUrl = localApiSnapshot?.baseUrl ?? "http://127.0.0.1:14840";
    const tokenPath = localApiSnapshot?.tokenPath ?? "${XDG_DATA_HOME:-$HOME/.local/share}/Patina/api_token";
    const quotedTokenPath = tokenPath.replace(/'/g, "'\\''");
    await navigator.clipboard.writeText(
      `curl -H "Authorization: Bearer $(cat '${quotedTokenPath}')" "${baseUrl}/api/v1/summary/today"`,
    );
  };

  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="mb-5 flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
        <Activity size={16} className="text-[var(--qp-accent-default)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">
          {UI_TEXT.settings.diagnosticsTitle}
        </h2>
      </div>

      <div className="divide-y divide-[var(--qp-border-subtle)]">
        {diagnostics.map((item) => (
          <DiagnosticItem
            key={item.id}
            item={item}
            actions={item.id === "desktop-integration" ? (
              <div className="grid min-w-[220px] gap-3 sm:grid-cols-2 lg:grid-cols-1 xl:grid-cols-2">
                <DiagnosticSwitch
                  label={UI_TEXT.settings.launchAtLoginLabel}
                  checked={launchAtLoginChecked}
                  onChange={onLaunchAtLoginChange}
                  ariaLabel={UI_TEXT.accessibility.settings.toggleLaunchAtLogin}
                />
                <DiagnosticSwitch
                  label={UI_TEXT.settings.startMinimizedLabel}
                  checked={startMinimizedChecked}
                  disabled={startMinimizedDisabled}
                  onChange={onStartMinimizedChange}
                  ariaLabel={UI_TEXT.accessibility.settings.toggleStartMinimized}
                />
              </div>
            ) : item.id === "local-api" ? (
              <button
                type="button"
                className="qp-button-secondary inline-flex h-8 items-center gap-2 px-3 text-xs font-semibold"
                onClick={() => void handleCopyApiCurl()}
                aria-label={UI_TEXT.accessibility.settings.copyApiCurl}
              >
                <Clipboard size={13} />
                <span>{UI_TEXT.settings.copyApiCurlLabel}</span>
              </button>
            ) : null}
          />
        ))}
      </div>
    </section>
  );
}

function DiagnosticItem({
  item,
  actions,
}: {
  item: SettingsDiagnosticItem;
  actions?: ReactNode;
}) {
  const Icon = DIAGNOSTIC_ICONS[item.id as keyof typeof DIAGNOSTIC_ICONS] ?? Activity;
  const toneClassName = item.tone === "ok"
    ? "text-[var(--qp-success)]"
    : item.tone === "danger"
      ? "text-[var(--qp-danger)]"
    : item.tone === "warning"
      ? "text-[var(--qp-warning)]"
      : "text-[var(--qp-text-tertiary)]";
  const statusClassName = item.tone === "ok"
    ? "qp-status-ok"
    : item.tone === "danger"
      ? "qp-status-danger"
    : item.tone === "warning"
      ? "qp-status-warning"
      : "qp-status-muted";

  return (
    <div className="grid grid-cols-1 gap-3 py-4 first:pt-0 last:pb-0 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-start">
      <div className="flex min-w-0 items-start gap-3">
        <div className={`mt-1 shrink-0 ${toneClassName}`}>
          <Icon size={15} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{item.label}</p>
            <span className={`qp-status px-2 py-0.5 text-[11px] font-semibold ${statusClassName}`}>
              {item.value}
            </span>
          </div>
          <p className="mt-1 break-words text-xs leading-relaxed text-[var(--qp-text-secondary)]">{item.detail}</p>
          {item.metadata && item.metadata.length > 0 ? (
            <dl className="mt-2 grid grid-cols-1 gap-x-4 gap-y-1 text-[11px] leading-relaxed text-[var(--qp-text-tertiary)] md:grid-cols-[max-content_minmax(0,1fr)]">
              {item.metadata.map((entry) => (
                <div key={`${item.id}-${entry.label}`} className="contents">
                  <dt className="font-semibold">{entry.label}</dt>
                  <dd className="min-w-0 break-all font-mono">{entry.value}</dd>
                </div>
              ))}
            </dl>
          ) : null}
        </div>
      </div>
      {actions ? (
        <div className="flex justify-start lg:justify-end">
          {actions}
        </div>
      ) : null}
    </div>
  );
}

function DiagnosticSwitch({
  label,
  checked,
  disabled = false,
  onChange,
  ariaLabel,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (nextChecked: boolean) => void;
  ariaLabel: string;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-[6px] border border-[var(--qp-border-subtle)] px-3 py-2">
      <span className="text-xs font-semibold text-[var(--qp-text-secondary)]">{label}</span>
      <QuietSwitch checked={checked} disabled={disabled} onChange={onChange} ariaLabel={ariaLabel} />
    </div>
  );
}
