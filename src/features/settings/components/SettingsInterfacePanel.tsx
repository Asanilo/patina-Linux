import { Copy, Dices, EthernetPort, ExternalLink, Eye, EyeOff, Fingerprint, KeyRound, Link2, Puzzle, RefreshCw, Server } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import QuietActionRow from "../../../shared/components/QuietActionRow";
import QuietSelect from "../../../shared/components/QuietSelect";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import QuietSwitch from "../../../shared/components/QuietSwitch";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { WebActivityUrlPrivacy } from "../../../shared/settings/appSettings.ts";
import { buildBrowserExtensionConfigText } from "../services/settingsBrowserExtensionService.ts";
import { createSettingsToken } from "../services/settingsTokenService.ts";

type SettingsInterfacePanelProps = {
  webActivityEnabled: boolean;
  localApiPort: number;
  localApiToken: string;
  port: number;
  webActivityToken: string;
  webActivityUrlPrivacy: WebActivityUrlPrivacy;
  remoteStatusBridgeEnabled: boolean;
  remoteStatusBridgeUrl: string;
  remoteStatusBridgeToken: string;
  remoteStatusBridgeMachineId: string;
  localApiActionStatus: "idle" | "applying-port" | "rotating-token";
  onWebActivityEnabledChange: (nextChecked: boolean) => void;
  onApplyLocalApiPort: (nextPort: number) => Promise<boolean>;
  onRotateLocalApiToken: () => Promise<boolean>;
  onPortChange: (nextPort: number) => void;
  onWebActivityTokenChange: (nextToken: string) => void;
  onWebActivityUrlPrivacyChange: (nextMode: WebActivityUrlPrivacy) => void;
  onRemoteStatusBridgeEnabledChange: (nextChecked: boolean) => void;
  onRemoteStatusBridgeUrlChange: (nextUrl: string) => void;
  onRemoteStatusBridgeTokenChange: (nextToken: string) => void;
};

type TokenFieldProps = {
  id: string;
  value: string;
  visible: boolean;
  disabled: boolean;
  onChange: (nextToken: string) => void;
  onGenerate: () => void;
  onCopy: () => void;
  onToggleVisible: () => void;
  showLabel: string;
  hideLabel: string;
};

type PortFieldProps = {
  id: string;
  value: string;
  disabled: boolean;
  onChange: (nextValue: string) => void;
  onCommit: () => void;
};

type TextFieldProps = {
  id: string;
  value: string;
  disabled: boolean;
  readOnly?: boolean;
  spellCheck?: boolean;
  onChange?: (nextValue: string) => void;
  onCommit?: () => void;
};

type RevealableTextFieldProps = {
  id: string;
  value: string;
  visible: boolean;
  disabled: boolean;
  readOnly?: boolean;
  showLabel: string;
  hideLabel: string;
  onToggleVisible: () => void;
};

type InterfaceInlineFieldProps = {
  htmlFor: string;
  icon: ReactNode;
  title: string;
  children: ReactNode;
  className?: string;
};

const WEB_ACTIVITY_PORT_MIN = 1024;
const WEB_ACTIVITY_PORT_MAX = 65535;
const PORT_DRAFT_PATTERN = /^\d{0,5}$/;
const INTERFACE_FIELD_GRID_CLASS = "mt-4 grid grid-cols-1 gap-x-4 gap-y-3 lg:grid-cols-[minmax(0,4fr)_minmax(0,6fr)]";
const BROWSER_EXTENSION_GUIDES = [
  {
    id: "firefox",
    label: "Firefox / Zen",
    path: "extensions/firefox",
    setupUrl: "about:debugging#/runtime/this-firefox",
  },
  {
    id: "chromium",
    label: "Chromium / Chrome / Edge",
    path: "extensions/chromium",
    setupUrl: "chrome://extensions",
  },
] as const;

function normalizePort(value: string) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) return "";
  if (parsed < WEB_ACTIVITY_PORT_MIN || parsed > WEB_ACTIVITY_PORT_MAX) return "";
  return String(parsed);
}

function TokenField({
  id,
  value,
  visible,
  disabled,
  onChange,
  onGenerate,
  onCopy,
  onToggleVisible,
  showLabel,
  hideLabel,
}: TokenFieldProps) {
  const inputClassName = [
    "qp-input settings-token-input-with-actions h-[34px] w-full",
    visible ? null : "settings-token-input-hidden",
  ].filter(Boolean).join(" ");

  return (
    <div className="relative w-full">
      <input
        id={id}
        type="text"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={inputClassName}
        disabled={disabled}
        autoComplete="off"
        spellCheck={false}
      />
      <button
        type="button"
        className="settings-token-action-button settings-token-random-button"
        disabled={disabled}
        aria-label={UI_TEXT.accessibility.settings.generateServiceToken}
        onClick={onGenerate}
      >
        <Dices size={14} />
      </button>
      <button
        type="button"
        className="settings-token-action-button settings-token-copy-button"
        disabled={disabled || value.trim().length === 0}
        aria-label={`${UI_TEXT.settings.copyBrowserExtensionConfigLabel} ${UI_TEXT.settings.webActivityTokenLabel}`}
        onClick={onCopy}
      >
        <Copy size={14} />
      </button>
      <button
        type="button"
        className="settings-token-action-button settings-token-visibility-button"
        disabled={disabled}
        aria-label={visible ? hideLabel : showLabel}
        onClick={onToggleVisible}
      >
        {visible ? <EyeOff size={14} /> : <Eye size={14} />}
      </button>
    </div>
  );
}

function PortField({
  id,
  value,
  disabled,
  onChange,
  onCommit,
}: PortFieldProps) {
  return (
    <input
      id={id}
      type="text"
      inputMode="numeric"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onBlur={onCommit}
      className="qp-input h-[34px] w-full"
      disabled={disabled}
      autoComplete="off"
      spellCheck={false}
    />
  );
}

function TextField({
  id,
  value,
  disabled,
  readOnly = false,
  spellCheck = false,
  onChange,
  onCommit,
}: TextFieldProps) {
  return (
    <input
      id={id}
      type="text"
      value={value}
      onChange={onChange ? (event) => onChange(event.target.value) : undefined}
      onBlur={onCommit}
      className="qp-input h-[34px] w-full"
      disabled={disabled}
      readOnly={readOnly}
      autoComplete="off"
      spellCheck={spellCheck}
    />
  );
}

function RevealableTextField({
  id,
  value,
  visible,
  disabled,
  readOnly = false,
  showLabel,
  hideLabel,
  onToggleVisible,
}: RevealableTextFieldProps) {
  const inputClassName = [
    "qp-input h-[34px] w-full pr-10",
    visible ? null : "settings-token-input-hidden",
  ].filter(Boolean).join(" ");

  return (
    <div className="relative w-full">
      <input
        id={id}
        type="text"
        value={value}
        className={inputClassName}
        disabled={disabled}
        readOnly={readOnly}
        autoComplete="off"
        spellCheck={false}
      />
      <button
        type="button"
        className="settings-token-action-button settings-token-visibility-button"
        disabled={disabled}
        aria-label={visible ? hideLabel : showLabel}
        onClick={onToggleVisible}
      >
        {visible ? <EyeOff size={14} /> : <Eye size={14} />}
      </button>
    </div>
  );
}

function InterfaceInlineField({
  htmlFor,
  icon,
  title,
  children,
  className,
}: InterfaceInlineFieldProps) {
  const rowClassName = [
    "settings-interface-field grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2.5 gap-y-2",
    className,
  ].filter(Boolean).join(" ");

  return (
    <QuietActionRow className={rowClassName}>
      <label
        htmlFor={htmlFor}
        className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-sm font-semibold text-[var(--qp-text-primary)]"
      >
        {icon}
        <span>{title}</span>
      </label>
      {children}
    </QuietActionRow>
  );
}

function BrowserExtensionInstallGuide({
  port,
  token,
}: {
  port: number;
  token: string;
}) {
  const copyText = async (value: string) => {
    await navigator.clipboard.writeText(value);
  };
  const extensionConfigText = buildBrowserExtensionConfigText({ port, token });

  return (
    <div className="mt-4 border-t border-[var(--qp-border-subtle)] pt-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 items-start gap-2.5">
          <Puzzle size={14} className="mt-0.5 shrink-0 text-[var(--qp-text-tertiary)]" />
          <div className="min-w-0">
            <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
              {UI_TEXT.settings.browserExtensionInstallTitle}
            </p>
            <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
              {UI_TEXT.settings.browserExtensionInstallHint}
            </p>
          </div>
        </div>
        <button
          type="button"
          className="qp-button-secondary inline-flex h-8 shrink-0 items-center justify-center gap-2 px-3 text-xs font-semibold"
          onClick={() => void copyText(extensionConfigText)}
          aria-label={UI_TEXT.settings.copyBrowserExtensionConfigLabel}
        >
          <Copy size={13} />
          <span>{UI_TEXT.settings.copyBrowserExtensionConfigLabel}</span>
        </button>
      </div>

      <div className="mt-3 grid grid-cols-1 gap-3 xl:grid-cols-2">
        {BROWSER_EXTENSION_GUIDES.map((guide) => (
          <div
            key={guide.id}
            className="rounded-[6px] border border-[var(--qp-border-subtle)] px-3 py-3"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="text-xs font-semibold text-[var(--qp-text-primary)]">{guide.label}</p>
                <p className="mt-1 break-all font-mono text-[11px] text-[var(--qp-text-tertiary)]">{guide.path}</p>
              </div>
            </div>
            <div className="mt-3 flex flex-wrap gap-2">
              <button
                type="button"
                className="qp-button-secondary inline-flex h-8 items-center gap-2 px-3 text-xs font-semibold"
                onClick={() => void copyText(guide.path)}
                aria-label={UI_TEXT.accessibility.settings.copyBrowserExtensionPath(guide.label)}
              >
                <Copy size={13} />
                <span>{UI_TEXT.settings.copyPathLabel}</span>
              </button>
              <button
                type="button"
                className="qp-button-secondary inline-flex h-8 items-center gap-2 px-3 text-xs font-semibold"
                onClick={() => void copyText(guide.setupUrl)}
                aria-label={UI_TEXT.accessibility.settings.copyBrowserExtensionSetupUrl(guide.label)}
              >
                <ExternalLink size={13} />
                <span>{UI_TEXT.settings.copySetupUrlLabel}</span>
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function SettingsInterfacePanel({
  webActivityEnabled,
  localApiPort,
  localApiToken,
  port,
  webActivityToken,
  webActivityUrlPrivacy,
  remoteStatusBridgeEnabled,
  remoteStatusBridgeUrl,
  remoteStatusBridgeToken,
  remoteStatusBridgeMachineId,
  localApiActionStatus,
  onWebActivityEnabledChange,
  onApplyLocalApiPort,
  onRotateLocalApiToken,
  onPortChange,
  onWebActivityTokenChange,
  onWebActivityUrlPrivacyChange,
  onRemoteStatusBridgeEnabledChange,
  onRemoteStatusBridgeUrlChange,
  onRemoteStatusBridgeTokenChange,
}: SettingsInterfacePanelProps) {
  const [webActivityPortDraft, setWebActivityPortDraft] = useState(String(port));
  const [localApiPortDraft, setLocalApiPortDraft] = useState(String(localApiPort));
  const [localApiTokenVisible, setLocalApiTokenVisible] = useState(false);
  const [webActivityTokenVisible, setWebActivityTokenVisible] = useState(false);
  const [remoteStatusBridgeTokenVisible, setRemoteStatusBridgeTokenVisible] = useState(false);
  const [remoteStatusBridgeMachineIdVisible, setRemoteStatusBridgeMachineIdVisible] = useState(false);
  const normalizedLocalApiPort = normalizePort(localApiPortDraft);
  const localApiBusy = localApiActionStatus !== "idle";
  const localApiPortChanged = normalizedLocalApiPort !== ""
    && Number(normalizedLocalApiPort) !== localApiPort;
  const webActivityUrlPrivacyOptions: Array<{ value: WebActivityUrlPrivacy; label: string }> = [
    { value: "full", label: UI_TEXT.settings.webActivityUrlPrivacyOptions.full },
    { value: "strip_query", label: UI_TEXT.settings.webActivityUrlPrivacyOptions.stripQuery },
    { value: "domain_only", label: UI_TEXT.settings.webActivityUrlPrivacyOptions.domainOnly },
  ];

  const copyText = async (value: string) => {
    await navigator.clipboard.writeText(value);
  };

  useEffect(() => {
    setWebActivityPortDraft(String(port));
  }, [port]);

  useEffect(() => {
    setLocalApiPortDraft(String(localApiPort));
  }, [localApiPort]);

  const handleWebActivityEnabledChange = (nextChecked: boolean) => {
    if (nextChecked && webActivityToken.trim().length === 0) {
      onWebActivityTokenChange(createSettingsToken());
    }
    onWebActivityEnabledChange(nextChecked);
  };
  const handleRemoteStatusBridgeEnabledChange = (nextChecked: boolean) => {
    if (nextChecked && remoteStatusBridgeToken.trim().length === 0) {
      onRemoteStatusBridgeTokenChange(createSettingsToken());
    }
    onRemoteStatusBridgeEnabledChange(nextChecked);
  };
  const commitPortDraft = (
    draft: string,
    currentPort: number,
    setDraft: (nextDraft: string) => void,
    onChange: (nextPort: number) => void,
  ) => {
    const normalized = normalizePort(draft);
    if (normalized) {
      setDraft(normalized);
      const nextPort = Number(normalized);
      if (nextPort !== currentPort) onChange(nextPort);
    } else {
      setDraft(String(currentPort));
    }
  };
  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="mb-5 flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
        <Server size={16} className="text-[var(--qp-accent-default)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.servicesTitle}</h2>
      </div>

      <div className="space-y-5">
        <QuietSubpanel>
          <div className="min-w-0">
            <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
              {UI_TEXT.settings.localApiTitle}
            </p>
            <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
              {UI_TEXT.settings.localApiHint}
            </p>
          </div>

          <div className={INTERFACE_FIELD_GRID_CLASS}>
            <InterfaceInlineField
              htmlFor="settings-local-api-port"
              icon={<EthernetPort size={14} className="text-[var(--qp-text-tertiary)]" />}
              title={UI_TEXT.settings.localApiPortLabel}
            >
              <div className="flex min-w-0 items-center gap-2">
                <PortField
                  id="settings-local-api-port"
                  value={localApiPortDraft}
                  disabled={localApiBusy}
                  onChange={(nextValue) => {
                    if (PORT_DRAFT_PATTERN.test(nextValue)) setLocalApiPortDraft(nextValue);
                  }}
                  onCommit={() => {
                    if (!normalizePort(localApiPortDraft)) {
                      setLocalApiPortDraft(String(localApiPort));
                    }
                  }}
                />
                <button
                  type="button"
                  className="qp-button-secondary inline-flex min-w-[92px] shrink-0 items-center justify-center gap-2 rounded-[8px] px-3 py-2 text-xs font-semibold"
                  disabled={!localApiPortChanged || localApiBusy}
                  onClick={() => {
                    if (!normalizedLocalApiPort) return;
                    void onApplyLocalApiPort(Number(normalizedLocalApiPort));
                  }}
                >
                  {localApiActionStatus === "applying-port" && (
                    <RefreshCw size={14} className="animate-spin" />
                  )}
                  <span>{UI_TEXT.settings.localApiApplyPortLabel}</span>
                </button>
              </div>
            </InterfaceInlineField>

            <InterfaceInlineField
              htmlFor="settings-local-api-token"
              icon={<KeyRound size={14} className="text-[var(--qp-text-tertiary)]" />}
              title={UI_TEXT.settings.localApiTokenLabel}
            >
              <RevealableTextField
                id="settings-local-api-token"
                value={localApiToken}
                visible={localApiTokenVisible}
                disabled={localApiBusy}
                readOnly
                onToggleVisible={() => setLocalApiTokenVisible((current) => !current)}
                showLabel={UI_TEXT.accessibility.settings.showServiceToken}
                hideLabel={UI_TEXT.accessibility.settings.hideServiceToken}
              />
            </InterfaceInlineField>
          </div>

          <div className="mt-3 flex flex-wrap justify-end gap-2">
            <button
              type="button"
              className="qp-button-secondary inline-flex items-center gap-2 rounded-[8px] px-3 py-2 text-xs font-semibold"
              disabled={localApiBusy || localApiToken.trim().length === 0}
              onClick={() => void copyText(localApiToken.trim())}
            >
              <Copy size={14} />
              <span>{UI_TEXT.settings.localApiCopyTokenLabel}</span>
            </button>
            <button
              type="button"
              className="qp-button-secondary inline-flex items-center gap-2 rounded-[8px] px-3 py-2 text-xs font-semibold"
              disabled={localApiBusy}
              onClick={() => void onRotateLocalApiToken()}
            >
              <RefreshCw
                size={14}
                className={localApiActionStatus === "rotating-token" ? "animate-spin" : undefined}
              />
              <span>{UI_TEXT.settings.localApiRotateTokenLabel}</span>
            </button>
          </div>
        </QuietSubpanel>

        <QuietSubpanel>
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                {UI_TEXT.settings.webActivityTitle}
              </p>
              <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
                {UI_TEXT.settings.webActivityEnabledHint}
              </p>
            </div>
            <QuietSwitch
              checked={webActivityEnabled}
              onChange={handleWebActivityEnabledChange}
              ariaLabel={UI_TEXT.accessibility.settings.toggleWebActivity}
            />
          </div>

          {webActivityEnabled ? (
            <>
              <div className={INTERFACE_FIELD_GRID_CLASS}>
                <InterfaceInlineField
                  htmlFor="settings-web-activity-address"
                  icon={<EthernetPort size={14} className="text-[var(--qp-text-tertiary)]" />}
                  title={UI_TEXT.settings.webActivityAddressLabel}
                >
                  <PortField
                    id="settings-web-activity-address"
                    value={webActivityPortDraft}
                    disabled={!webActivityEnabled}
                    onChange={(nextValue) => {
                      if (PORT_DRAFT_PATTERN.test(nextValue)) setWebActivityPortDraft(nextValue);
                    }}
                    onCommit={() => commitPortDraft(
                      webActivityPortDraft,
                      port,
                      setWebActivityPortDraft,
                      onPortChange,
                    )}
                  />
                </InterfaceInlineField>

                <InterfaceInlineField
                  htmlFor="settings-web-activity-token"
                  icon={<KeyRound size={14} className="text-[var(--qp-text-tertiary)]" />}
                  title={UI_TEXT.settings.webActivityTokenLabel}
                >
                  <TokenField
                    id="settings-web-activity-token"
                    value={webActivityToken}
                    visible={webActivityTokenVisible}
                    disabled={!webActivityEnabled}
                    onChange={onWebActivityTokenChange}
                    onGenerate={() => {
                      onWebActivityTokenChange(createSettingsToken());
                      setWebActivityTokenVisible(true);
                    }}
                    onCopy={() => void copyText(webActivityToken.trim())}
                    onToggleVisible={() => setWebActivityTokenVisible((current) => !current)}
                    showLabel={UI_TEXT.accessibility.settings.showServiceToken}
                    hideLabel={UI_TEXT.accessibility.settings.hideServiceToken}
                  />
                </InterfaceInlineField>

                <InterfaceInlineField
                  htmlFor="settings-web-activity-url-privacy"
                  icon={<Link2 size={14} className="text-[var(--qp-text-tertiary)]" />}
                  title={UI_TEXT.settings.webActivityUrlPrivacyLabel}
                  className="lg:col-span-2"
                >
                  <div className="grid gap-1.5">
                    <QuietSelect
                      value={webActivityUrlPrivacy}
                      options={webActivityUrlPrivacyOptions}
                      onChange={onWebActivityUrlPrivacyChange}
                      ariaLabel={UI_TEXT.accessibility.settings.webActivityUrlPrivacy}
                      disabled={!webActivityEnabled}
                    />
                    <p className="text-xs leading-relaxed text-[var(--qp-text-tertiary)]">
                      {UI_TEXT.settings.webActivityUrlPrivacyHint}
                    </p>
                  </div>
                </InterfaceInlineField>
              </div>
              <BrowserExtensionInstallGuide port={port} token={webActivityToken} />
            </>
          ) : null}
        </QuietSubpanel>

        <QuietSubpanel>
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                {UI_TEXT.settings.remoteStatusBridgeTitle}
              </p>
              <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
                {UI_TEXT.settings.remoteStatusBridgeEnabledHint}
              </p>
            </div>
            <QuietSwitch
              checked={remoteStatusBridgeEnabled}
              onChange={handleRemoteStatusBridgeEnabledChange}
              ariaLabel={UI_TEXT.accessibility.settings.toggleRemoteStatusBridge}
            />
          </div>

          {remoteStatusBridgeEnabled ? (
            <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,4fr)_minmax(0,6fr)]">
              <InterfaceInlineField
                htmlFor="settings-remote-status-bridge-url"
                icon={<Link2 size={14} className="text-[var(--qp-text-tertiary)]" />}
                title={UI_TEXT.settings.remoteStatusBridgeUrlLabel}
                className="lg:col-span-2"
              >
                <TextField
                  id="settings-remote-status-bridge-url"
                  value={remoteStatusBridgeUrl}
                  disabled={!remoteStatusBridgeEnabled}
                  spellCheck={false}
                  onChange={onRemoteStatusBridgeUrlChange}
                />
              </InterfaceInlineField>

              <InterfaceInlineField
                htmlFor="settings-remote-status-bridge-machine-id"
                icon={<Fingerprint size={14} className="text-[var(--qp-text-tertiary)]" />}
                title={UI_TEXT.settings.remoteStatusBridgeMachineIdLabel}
              >
                <RevealableTextField
                  id="settings-remote-status-bridge-machine-id"
                  value={remoteStatusBridgeMachineId}
                  visible={remoteStatusBridgeMachineIdVisible}
                  disabled={!remoteStatusBridgeEnabled}
                  readOnly
                  onToggleVisible={() => setRemoteStatusBridgeMachineIdVisible((current) => !current)}
                  showLabel={UI_TEXT.accessibility.settings.showRemoteMachineId}
                  hideLabel={UI_TEXT.accessibility.settings.hideRemoteMachineId}
                />
              </InterfaceInlineField>

              <InterfaceInlineField
                htmlFor="settings-remote-status-bridge-token"
                icon={<KeyRound size={14} className="text-[var(--qp-text-tertiary)]" />}
                title={UI_TEXT.settings.remoteStatusBridgeTokenLabel}
              >
                <TokenField
                  id="settings-remote-status-bridge-token"
                  value={remoteStatusBridgeToken}
                  visible={remoteStatusBridgeTokenVisible}
                  disabled={!remoteStatusBridgeEnabled}
                  onChange={onRemoteStatusBridgeTokenChange}
                  onGenerate={() => {
                    onRemoteStatusBridgeTokenChange(createSettingsToken());
                    setRemoteStatusBridgeTokenVisible(true);
                  }}
                  onCopy={() => void copyText(remoteStatusBridgeToken.trim())}
                  onToggleVisible={() => setRemoteStatusBridgeTokenVisible((current) => !current)}
                  showLabel={UI_TEXT.accessibility.settings.showServiceToken}
                  hideLabel={UI_TEXT.accessibility.settings.hideServiceToken}
                />
              </InterfaceInlineField>
            </div>
          ) : null}
        </QuietSubpanel>
      </div>
    </section>
  );
}
