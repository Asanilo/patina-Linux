import {
  Activity,
  Clipboard,
  Globe2,
  MonitorCheck,
  Power,
  RefreshCw,
  Server,
  Undo2,
  Wrench,
} from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import QuietSwitch from "../../../shared/components/QuietSwitch";
import type { QuietToastTone } from "../../../shared/components/QuietToast.tsx";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import { useQuietDialogs } from "../../../shared/hooks/useQuietDialogs.tsx";
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
  repairAutostartDesktopFile,
  type DesktopIntegrationDiagnosticsSnapshot,
} from "../../../platform/runtime/desktopIntegrationDiagnosticsGateway.ts";
import {
  getDaemonServiceDiagnostics,
  reloadDaemonVersion,
  retryRuntimeOwnerCutover,
  rollbackRuntimeOwnerToEmbedded,
  setBackgroundTrackingAtLogin,
  type DaemonServiceDiagnosticsSnapshot,
} from "../../../platform/runtime/daemonServiceDiagnosticsGateway.ts";
import {
  buildSettingsDiagnosticsViewModel,
  type SettingsDiagnosticItem,
} from "../services/settingsDiagnosticsViewModel.ts";
import { canReloadDaemonVersion, resolveDaemonServiceControlAvailability } from "../services/settingsDaemonServiceControls.ts";

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
  onBackgroundTrackingAtLoginApplied: (enabled: boolean) => void;
  onToast?: (message: string, tone?: QuietToastTone) => void;
};

type DaemonServiceAction = "idle" | "retrying" | "updating-login" | "rolling-back" | "reloading";

const LIVE_DIAGNOSTICS_REFRESH_MS = 5_000;
const DAEMON_SERVICE_DIAGNOSTICS_REFRESH_MS = 30_000;

const DIAGNOSTIC_ICONS = {
  "window-tracking": MonitorCheck,
  "local-api": Activity,
  "desktop-integration": Power,
  "daemon-service": Server,
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
  onBackgroundTrackingAtLoginApplied,
  onToast,
}: SettingsDiagnosticsPanelProps) {
  const { confirm, dialogs } = useQuietDialogs();
  const [bridgeSnapshot, setBridgeSnapshot] = useState<WebActivityBridgeSnapshot | null>(null);
  const [localApiSnapshot, setLocalApiSnapshot] = useState<LocalApiDiagnosticsSnapshot | null>(null);
  const [desktopIntegrationSnapshot, setDesktopIntegrationSnapshot] =
    useState<DesktopIntegrationDiagnosticsSnapshot | null>(null);
  const [daemonServiceSnapshot, setDaemonServiceSnapshot] =
    useState<DaemonServiceDiagnosticsSnapshot | null>(null);
  const [isRepairingAutostart, setIsRepairingAutostart] = useState(false);
  const [daemonServiceAction, setDaemonServiceAction] = useState<DaemonServiceAction>("idle");
  const reloadInFlight = useRef(false);

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
    }, LIVE_DIAGNOSTICS_REFRESH_MS);

    return () => {
      disposed = true;
      window.clearInterval(timerId);
    };
  }, []);

  useEffect(() => {
    let disposed = false;

    const refreshDaemonService = async () => {
      try {
        const snapshot = await getDaemonServiceDiagnostics();
        if (!disposed) setDaemonServiceSnapshot(snapshot);
      } catch (error) {
        if (!disposed) {
          setDaemonServiceSnapshot(null);
          console.warn("load daemon service diagnostics failed", error);
        }
      }
    };

    void refreshDaemonService();
    const timerId = window.setInterval(() => {
      void refreshDaemonService();
    }, DAEMON_SERVICE_DIAGNOSTICS_REFRESH_MS);

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
    daemonService: daemonServiceSnapshot,
  }), [
    bridgeSnapshot,
    desktopIntegrationSnapshot,
    daemonServiceSnapshot,
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

  const canRepairAutostart = Boolean(
    desktopIntegrationSnapshot?.launchAtLogin
      && !desktopIntegrationSnapshot.autostart.valid,
  );

  const handleRepairAutostart = async () => {
    setIsRepairingAutostart(true);
    try {
      const snapshot = await repairAutostartDesktopFile();
      setDesktopIntegrationSnapshot(snapshot);
    } catch (error) {
      console.warn("repair autostart desktop file failed", error);
    } finally {
      setIsRepairingAutostart(false);
    }
  };

  const daemonControls = resolveDaemonServiceControlAvailability(daemonServiceSnapshot);
  const canReload = canReloadDaemonVersion(daemonServiceSnapshot);
  const daemonActionBusy = daemonServiceAction !== "idle";
  const backgroundTrackingAtLogin = daemonServiceSnapshot?.cutover.backgroundTrackingAtLogin
    ?? desktopIntegrationSnapshot?.backgroundTrackingAtLogin
    ?? false;

  const handleSetBackgroundTrackingAtLogin = async (enabled: boolean) => {
    if (daemonActionBusy) return;
    setDaemonServiceAction("updating-login");
    try {
      const snapshot = await setBackgroundTrackingAtLogin(enabled);
      setDaemonServiceSnapshot(snapshot);
      setDesktopIntegrationSnapshot((current) => current ? {
        ...current,
        backgroundTrackingAtLogin: enabled,
      } : current);
      onBackgroundTrackingAtLoginApplied(enabled);
      onToast?.(UI_TEXT.settings.daemonLoginPreferenceApplied, "success");
    } catch (error) {
      console.warn("set background tracking login preference failed", error);
      onToast?.(UI_TEXT.settings.daemonLoginPreferenceFailed, "warning");
    } finally {
      setDaemonServiceAction("idle");
    }
  };

  const handleRetryDaemonCutover = async () => {
    if (daemonActionBusy || !daemonControls.retry) return;
    const confirmed = await confirm({
      title: UI_TEXT.settings.daemonCutoverRetryTitle,
      description: UI_TEXT.settings.daemonCutoverRetryDetail,
      confirmLabel: UI_TEXT.settings.daemonCutoverRetryLabel,
    });
    if (!confirmed) return;

    setDaemonServiceAction("retrying");
    try {
      await retryRuntimeOwnerCutover();
    } catch (error) {
      console.warn("retry runtime owner cutover failed", error);
      onToast?.(UI_TEXT.settings.daemonCutoverRetryFailed, "warning");
      setDaemonServiceAction("idle");
    }
  };

  const handleRollbackDaemonOwner = async () => {
    if (daemonActionBusy || !daemonControls.rollback) return;
    const confirmed = await confirm({
      title: UI_TEXT.settings.daemonOwnerRollbackTitle,
      description: UI_TEXT.settings.daemonOwnerRollbackDetail,
      confirmLabel: UI_TEXT.settings.daemonOwnerRollbackLabel,
      danger: true,
    });
    if (!confirmed) return;

    setDaemonServiceAction("rolling-back");
    try {
      await rollbackRuntimeOwnerToEmbedded();
    } catch (error) {
      console.warn("rollback runtime owner to embedded failed", error);
      onToast?.(UI_TEXT.settings.daemonOwnerRollbackFailed, "warning");
      setDaemonServiceAction("idle");
    }
  };

  const handleReloadDaemon = async () => {
    const runningVersion = daemonServiceSnapshot?.version?.runningVersion;
    if (daemonActionBusy || reloadInFlight.current || !canReload || !runningVersion) return;
    reloadInFlight.current = true;
    setDaemonServiceAction("reloading");
    try {
      const accepted = await confirm({
        title: UI_TEXT.settings.daemonReloadTitle,
        description: UI_TEXT.settings.daemonReloadDetail,
        confirmLabel: UI_TEXT.settings.daemonReloadLabel,
      });
      if (!accepted) return;
      await reloadDaemonVersion(runningVersion);
      onToast?.(UI_TEXT.settings.daemonReloadSucceeded, "success");
    } catch (error) {
      console.warn("daemon reload verification failed", error);
      onToast?.(UI_TEXT.settings.daemonReloadFailed, "warning");
    } finally {
      try { setDaemonServiceSnapshot(await getDaemonServiceDiagnostics()); }
      catch { setDaemonServiceSnapshot(null); }
      reloadInFlight.current = false;
      setDaemonServiceAction("idle");
    }
  };

  const daemonServiceActions = canReload || daemonControls.backgroundLogin
    || daemonServiceAction === "reloading"
    || daemonControls.retry
    || daemonControls.rollback
    ? (
      <div className="grid min-w-[220px] gap-3">
        {canReload || daemonServiceAction === "reloading" ? (
          <button type="button"
            className="qp-button-secondary inline-flex min-h-8 items-center justify-center gap-2 px-3 py-1 text-xs font-semibold"
            disabled={daemonActionBusy} onClick={() => void handleReloadDaemon()}>
            <RefreshCw size={13} className={daemonServiceAction === "reloading" ? "animate-spin shrink-0" : "shrink-0"} />
            <span>{UI_TEXT.settings.daemonReloadLabel}</span>
          </button>
        ) : null}
        {daemonControls.backgroundLogin ? (
          <DiagnosticSwitch
            label={UI_TEXT.settings.backgroundTrackingAtLoginLabel}
            checked={backgroundTrackingAtLogin}
            disabled={daemonActionBusy}
            onChange={(enabled) => void handleSetBackgroundTrackingAtLogin(enabled)}
            ariaLabel={UI_TEXT.accessibility.settings.toggleBackgroundTrackingAtLogin}
          />
        ) : null}
        {daemonControls.retry || daemonControls.rollback ? (
          <div className="flex flex-wrap justify-start gap-2 lg:justify-end">
            {daemonControls.retry ? (
              <button
                type="button"
                className="qp-button-secondary inline-flex h-8 items-center justify-center gap-2 px-3 text-xs font-semibold"
                onClick={() => void handleRetryDaemonCutover()}
                disabled={daemonActionBusy}
                aria-label={UI_TEXT.accessibility.settings.retryDaemonCutover}
              >
                <RefreshCw
                  size={13}
                  className={daemonServiceAction === "retrying" ? "animate-spin" : undefined}
                />
                <span>{UI_TEXT.settings.daemonCutoverRetryLabel}</span>
              </button>
            ) : null}
            {daemonControls.rollback ? (
              <button
                type="button"
                className="qp-button-danger inline-flex h-8 items-center justify-center gap-2 px-3 text-xs font-semibold"
                onClick={() => void handleRollbackDaemonOwner()}
                disabled={daemonActionBusy}
                aria-label={UI_TEXT.accessibility.settings.rollbackDaemonOwner}
              >
                <Undo2 size={13} />
                <span>{UI_TEXT.settings.daemonOwnerRollbackLabel}</span>
              </button>
            ) : null}
          </div>
        ) : null}
      </div>
    )
    : null;

  return (
    <>
      {dialogs}
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
              actions={item.id === "daemon-service" ? daemonServiceActions : item.id === "desktop-integration" ? (
                <div className="grid min-w-[220px] gap-3">
                  {canRepairAutostart ? (
                    <button
                      type="button"
                      className="qp-button-secondary inline-flex h-8 items-center justify-center gap-2 px-3 text-xs font-semibold"
                      onClick={() => void handleRepairAutostart()}
                      disabled={isRepairingAutostart}
                      aria-label={UI_TEXT.accessibility.settings.repairAutostart}
                    >
                      <Wrench size={13} />
                      <span>{isRepairingAutostart ? UI_TEXT.settings.repairingAutostartLabel : UI_TEXT.settings.repairAutostartLabel}</span>
                    </button>
                  ) : null}
                  <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-1 xl:grid-cols-2">
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
    </>
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
