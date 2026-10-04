import type { AppSettings } from "../../../shared/settings/appSettings.ts";
import { PRODUCT_POLICY_SETTING_KEYS, RESOURCE_SETTING_KEYS } from "../../../shared/settings/appSettings.ts";

const POLICY_KEYS = PRODUCT_POLICY_SETTING_KEYS;

export function hasSettingsDraftResourceEdits(saved: AppSettings | null, draft: AppSettings | null): boolean {
  return !!saved && !!draft && RESOURCE_SETTING_KEYS.some(key => draft[key] !== saved[key]);
}

export function hasSettingsDraftResourceConflict(saved: AppSettings | null, draft: AppSettings | null, incoming: AppSettings): boolean {
  return !!saved && !!draft && RESOURCE_SETTING_KEYS.some(key =>
    draft[key] !== saved[key] && incoming[key] !== saved[key] && incoming[key] !== draft[key]);
}

export function hasSettingsDraftPolicyEdits(saved: AppSettings | null, draft: AppSettings | null): boolean {
  return !!saved && !!draft && POLICY_KEYS.some(key => draft[key] !== saved[key]);
}

export function hasSettingsDraftPolicyConflict(saved: AppSettings | null, draft: AppSettings | null, incoming: AppSettings): boolean {
  if (!saved || !draft) return false;
  return POLICY_KEYS
    .some(key => draft[key] !== saved[key] && incoming[key] !== saved[key] && incoming[key] !== draft[key]);
}

/** Refresh untouched fields while retaining edits relative to the last snapshot. */
export function rebaseSettingsDraft(
  saved: AppSettings | null,
  draft: AppSettings | null,
  incoming: AppSettings,
): AppSettings {
  const next = { ...incoming };
  if (saved && draft) {
    for (const key of Object.keys(draft) as Array<keyof AppSettings>) {
      if (draft[key] !== saved[key]) Object.assign(next, { [key]: draft[key] });
    }
  }
  return next;
}
