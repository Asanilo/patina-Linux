import { ChevronRight, Palette } from "lucide-react";
import { useState } from "react";
import QuietDialog from "../../../shared/components/QuietDialog";
import QuietSegmentedFilter from "../../../shared/components/QuietSegmentedFilter";
import type { AppLanguage, ColorScheme, ThemeMode } from "../../../shared/settings/appSettings.ts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";

type ThemeLibrary = "light" | "dark";

type SettingsAppearancePanelProps = {
  themeMode: ThemeMode;
  onThemeModeChange: (nextThemeMode: ThemeMode) => void;
  language: AppLanguage;
  onLanguageChange: (nextLanguage: AppLanguage) => void;
  colorSchemeLight: ColorScheme;
  onColorSchemeLightChange: (nextColorScheme: ColorScheme) => void;
  colorSchemeDark: ColorScheme;
  onColorSchemeDarkChange: (nextColorScheme: ColorScheme) => void;
  onConfirmColorSchemeChange: () => Promise<boolean>;
  colorSchemeConfirming: boolean;
};

const COLOR_SCHEME_OPTIONS: Record<ThemeLibrary, Array<{
  value: ColorScheme;
  label: string;
  swatches: [string, string, string, string, string, string];
}>> = {
  light: [
    { value: "default", label: UI_TEXT.common.default, swatches: ["#fbfbfb", "#ffffff", "#f5f5f5", "#315f9f", "#3f74c2", "#2f7d49"] },
    { value: "absolutely", label: "Absolutely", swatches: ["#f9f9f7", "#2d2d2b", "#cc7d5e", "#00c853", "#ff5f38", "#cc7d5e"] },
    { value: "catppuccin", label: "Catppuccin", swatches: ["#eff1f5", "#4c4f69", "#8839ef", "#40a02b", "#d20f39", "#8839ef"] },
    { value: "everforest", label: "Everforest", swatches: ["#fdf6e3", "#5c6a72", "#93b259", "#8da101", "#f85552", "#df69ba"] },
    { value: "github", label: "GitHub", swatches: ["#ffffff", "#1f2328", "#0969da", "#1a7f37", "#cf222e", "#8250df"] },
    { value: "gruvbox", label: "Gruvbox", swatches: ["#fbf1c7", "#3c3836", "#458588", "#3c3836", "#cc241d", "#b16286"] },
    { value: "linear", label: "Linear", swatches: ["#fcfcfd", "#1b1b1b", "#5e6ad2", "#52a450", "#c94446", "#8160d8"] },
    { value: "notion", label: "Notion", swatches: ["#ffffff", "#37352f", "#3183d8", "#008000", "#a31515", "#0000ff"] },
    { value: "one", label: "One", swatches: ["#fafafa", "#383a42", "#526fff", "#3bba54", "#e45649", "#526fff"] },
    { value: "proof", label: "Proof", swatches: ["#f5f3ed", "#2f312d", "#3d755d", "#3d755d", "#ba2623", "#5f6ac2"] },
    { value: "raycast", label: "Raycast", swatches: ["#ffffff", "#030303", "#ff6363", "#006b4f", "#b12424", "#9a1b6e"] },
    { value: "rose-pine", label: "Rose Pine", swatches: ["#faf4ed", "#575279", "#d7827e", "#56949f", "#797593", "#907aa9"] },
    { value: "solarized", label: "Solarized", swatches: ["#fdf6e3", "#657b83", "#b58900", "#859900", "#dc322f", "#d33682"] },
    { value: "vercel", label: "Vercel", swatches: ["#ffffff", "#171717", "#006aff", "#28a948", "#eb001d", "#a100f8"] },
    { value: "vscode-plus", label: "VS Code Plus", swatches: ["#ffffff", "#000000", "#007acc", "#008000", "#ee0000", "#0000ff"] },
    { value: "xcode", label: "Xcode", swatches: ["#ffffff", "#000000d9", "#0e0eff", "#00a240", "#c41a16", "#0e0eff"] },
  ],
  dark: [
    { value: "default", label: UI_TEXT.common.default, swatches: ["#212121", "#2d2d2d", "#404040", "#8ba1c0", "#93a9c8", "#82ad8b"] },
    { value: "absolutely", label: "Absolutely", swatches: ["#2d2d2b", "#f9f9f7", "#cc7d5e", "#00c853", "#ff5f38", "#cc7d5e"] },
    { value: "ayu", label: "Ayu", swatches: ["#0b0e14", "#bfbdb6", "#e6b450", "#7fd962", "#ea6c73", "#cda1fa"] },
    { value: "catppuccin", label: "Catppuccin", swatches: ["#1e1e2e", "#cdd6f4", "#cba6f7", "#a6e3a1", "#f38ba8", "#cba6f7"] },
    { value: "dracula", label: "Dracula", swatches: ["#282a36", "#f8f8f2", "#ff79c6", "#50fa7b", "#ff5555", "#ff79c6"] },
    { value: "everforest", label: "Everforest", swatches: ["#2d353b", "#d3c6aa", "#a7c080", "#a7c080", "#e67e80", "#d699b6"] },
    { value: "github", label: "GitHub", swatches: ["#0d1117", "#e6edf3", "#1f6feb", "#3fb950", "#f85149", "#bc8cff"] },
    { value: "gruvbox", label: "Gruvbox", swatches: ["#282828", "#ebdbb2", "#458588", "#ebdbb2", "#cc241d", "#b16286"] },
    { value: "linear", label: "Linear", swatches: ["#0f0f11", "#e3e4e6", "#606acc", "#69c967", "#ff7e78", "#c2a1ff"] },
    { value: "lobster", label: "Lobster", swatches: ["#111827", "#e4e4e7", "#ff5c5c", "#22c55e", "#ff5c5c", "#3b82f6"] },
    { value: "material", label: "Material", swatches: ["#212121", "#eeffff", "#80cbc4", "#c3e88d", "#f07178", "#c792ea"] },
    { value: "matrix", label: "Matrix", swatches: ["#040805", "#b8ffca", "#1eff5a", "#1eff5a", "#fa423e", "#1eff5a"] },
    { value: "monokai", label: "Monokai", swatches: ["#272822", "#f8f8f2", "#99947c", "#86b42b", "#c4265e", "#8c6bc8"] },
    { value: "night-owl", label: "Night Owl", swatches: ["#011627", "#d6deeb", "#44596b", "#c5e478", "#ef5350", "#c792ea"] },
    { value: "nord", label: "Nord", swatches: ["#2e3440", "#d8dee9", "#88c0d0", "#a3be8c", "#bf616a", "#b48ead"] },
    { value: "notion", label: "Notion", swatches: ["#191919", "#d9d9d8", "#3183d8", "#4ec9b0", "#fa423e", "#3183d8"] },
    { value: "one", label: "One", swatches: ["#282c34", "#abb2bf", "#4d78cc", "#8cc265", "#e05561", "#c162de"] },
    { value: "oscurange", label: "Oscurange", swatches: ["#0b0b0f", "#e6e6e6", "#f9b98c", "#40c977", "#fa423e", "#479ffa"] },
    { value: "raycast", label: "Raycast", swatches: ["#101010", "#fefefe", "#ff6363", "#59d499", "#ff6363", "#cf2f98"] },
    { value: "rose-pine", label: "Rose Pine", swatches: ["#232136", "#e0def4", "#ea9a97", "#9ccfd8", "#908caa", "#c4a7e7"] },
    { value: "sentry", label: "Sentry", swatches: ["#2d2935", "#e6dff9", "#7055f6", "#8ee6d7", "#fa423e", "#7055f6"] },
    { value: "solarized", label: "Solarized", swatches: ["#002b36", "#839496", "#d30102", "#859900", "#dc322f", "#d33682"] },
    { value: "temple", label: "Temple", swatches: ["#02120c", "#c7e6da", "#e4f222", "#40c977", "#fa423e", "#e4f222"] },
    { value: "tokyo-night", label: "Tokyo Night", swatches: ["#1a1b26", "#a9b1d6", "#3d59a1", "#449dab", "#914c54", "#9d7cd8"] },
    { value: "vercel", label: "Vercel", swatches: ["#000000", "#ededed", "#006efe", "#00ad3a", "#f13342", "#9540d5"] },
    { value: "vscode-plus", label: "VS Code Plus", swatches: ["#1e1e1e", "#d4d4d4", "#007acc", "#369432", "#f44747", "#000080"] },
    { value: "xcode", label: "Xcode", swatches: ["#1f1f24", "#ffffffd9", "#5482ff", "#67b7a4", "#fc6a5d", "#5482ff"] },
  ],
};

export default function SettingsAppearancePanel({
  themeMode,
  onThemeModeChange,
  language,
  onLanguageChange,
  colorSchemeLight,
  onColorSchemeLightChange,
  colorSchemeDark,
  onColorSchemeDarkChange,
  onConfirmColorSchemeChange,
  colorSchemeConfirming,
}: SettingsAppearancePanelProps) {
  const [activeLibrary, setActiveLibrary] = useState<ThemeLibrary | null>(null);
  const [dialogSnapshot, setDialogSnapshot] = useState<{
    library: ThemeLibrary;
    colorScheme: ColorScheme;
  } | null>(null);
  const themeModeOptions: Array<{ value: ThemeMode; label: string }> = [
    { value: "light", label: UI_TEXT.settings.themeModeOptions.light },
    { value: "dark", label: UI_TEXT.settings.themeModeOptions.dark },
    { value: "system", label: UI_TEXT.settings.themeModeOptions.system },
  ];
  const languageOptions: Array<{ value: AppLanguage; label: string }> = [
    { value: "zh-CN", label: UI_TEXT.settings.languageOptions.zhCN },
    { value: "en-US", label: UI_TEXT.settings.languageOptions.enUS },
  ];
  const themeLibraryOptions: Array<{
    value: ThemeLibrary;
    label: string;
  }> = [
    { value: "light", label: UI_TEXT.settings.themeLibraryOptions.light },
    { value: "dark", label: UI_TEXT.settings.themeLibraryOptions.dark },
  ];
  const activeLibraryOption = themeLibraryOptions.find((option) => option.value === activeLibrary);
  const activeColorScheme = activeLibrary === "dark" ? colorSchemeDark : colorSchemeLight;
  const changeActiveColorScheme = activeLibrary === "dark" ? onColorSchemeDarkChange : onColorSchemeLightChange;

  const openColorSchemeDialog = (library: ThemeLibrary) => {
    setDialogSnapshot({
      library,
      colorScheme: library === "dark" ? colorSchemeDark : colorSchemeLight,
    });
    setActiveLibrary(library);
  };

  const closeColorSchemeDialog = () => {
    if (dialogSnapshot) {
      if (dialogSnapshot.library === "dark") {
        onColorSchemeDarkChange(dialogSnapshot.colorScheme);
      } else {
        onColorSchemeLightChange(dialogSnapshot.colorScheme);
      }
    }

    setDialogSnapshot(null);
    setActiveLibrary(null);
  };

  const handleConfirmColorScheme = async () => {
    const accepted = await onConfirmColorSchemeChange();
    if (accepted) {
      setDialogSnapshot(null);
      setActiveLibrary(null);
    }
  };

  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
        <Palette size={16} className="text-[var(--qp-accent-default)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.appearanceTitle}</h2>
      </div>

      <div className="mt-5 grid grid-cols-1 items-start gap-3 md:grid-cols-[minmax(0,1fr)_236px] md:gap-4">
        <div>
          <label className="text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--qp-text-tertiary)]">
            {UI_TEXT.settings.themeModeLabel}
          </label>
          <p className="mt-2 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
            {UI_TEXT.settings.themeModeHint}
          </p>
        </div>

        <QuietSegmentedFilter
          value={themeMode}
          options={themeModeOptions}
          onChange={onThemeModeChange}
          className="md:self-end md:justify-self-end"
        />
      </div>

      <div className="mt-5 grid grid-cols-1 items-start gap-3 md:grid-cols-[minmax(0,1fr)_236px] md:gap-4">
        <div>
          <label className="text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--qp-text-tertiary)]">
            {UI_TEXT.settings.colorSchemeLabel}
          </label>
          <p className="mt-2 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
            {UI_TEXT.settings.colorSchemeHint}
          </p>
        </div>

        <div
          className="settings-theme-entry-list md:self-end md:justify-self-end"
          role="group"
          aria-label={UI_TEXT.accessibility.settings.colorScheme}
        >
          {themeLibraryOptions.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => openColorSchemeDialog(option.value)}
              className="settings-theme-entry"
            >
              <span className="settings-theme-entry-title">{option.label}</span>
              <ChevronRight size={14} aria-hidden="true" />
            </button>
          ))}
        </div>
      </div>

      <div className="mt-5 grid grid-cols-1 items-start gap-3 md:grid-cols-[minmax(0,1fr)_236px] md:gap-4">
        <div>
          <label className="text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--qp-text-tertiary)]">
            {UI_TEXT.settings.languageLabel}
          </label>
          <p className="mt-2 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
            {UI_TEXT.settings.languageHint}
          </p>
        </div>

        <QuietSegmentedFilter
          value={language}
          options={languageOptions}
          onChange={onLanguageChange}
          className="md:self-end md:justify-self-end"
        />
      </div>

      <QuietDialog
        open={activeLibrary !== null}
        title={activeLibraryOption?.label ?? UI_TEXT.settings.colorSchemeDialogFallbackTitle}
        description={UI_TEXT.settings.colorSchemeDialogDescription}
        onClose={closeColorSchemeDialog}
        surfaceClassName="qp-theme-dialog-surface"
        actions={(
          <>
            <button
              type="button"
              onClick={closeColorSchemeDialog}
              className="qp-button-secondary qp-dialog-action"
              disabled={colorSchemeConfirming}
            >
              {UI_TEXT.common.cancel}
            </button>
            <button
              type="button"
              onClick={() => void handleConfirmColorScheme()}
              className="qp-button-primary qp-dialog-action"
              disabled={colorSchemeConfirming}
            >
              {colorSchemeConfirming ? UI_TEXT.settings.colorSchemeSaving : UI_TEXT.common.confirm}
            </button>
          </>
        )}
      >
        {activeLibrary ? (
          <div className="qp-theme-dialog-body">
            <div className="settings-color-scheme-list" role="group" aria-label={activeLibraryOption?.label}>
              {COLOR_SCHEME_OPTIONS[activeLibrary].map((option) => {
                const selected = option.value === activeColorScheme;
                return (
                  <button
                    key={option.value}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => changeActiveColorScheme(option.value)}
                    className={`settings-color-scheme-option ${
                      selected ? "settings-color-scheme-option-selected" : ""
                    }`.trim()}
                  >
                    <span className="settings-color-scheme-swatches" aria-hidden="true">
                      {option.swatches.map((swatch) => (
                        <span
                          key={swatch}
                          className="settings-color-scheme-swatch"
                          style={{ backgroundColor: swatch }}
                        />
                      ))}
                    </span>
                    <span>{option.label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        ) : null}
      </QuietDialog>
    </section>
  );
}
