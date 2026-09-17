import type { AppCategory } from "./categoryTokens.ts";
import type { UiLanguage } from "../copy/uiText.ts";
import catalog from "./defaultMappings.json" with { type: "json" };

export interface DefaultAppMapping {
  name: string;
  localizedNames?: Partial<Record<UiLanguage, string>>;
  category?: Extract<AppCategory, "system">;
}

export const DEFAULT_APP_MAPPINGS = catalog as Record<string, DefaultAppMapping>;
