import type { AppSettings } from "../../../shared/settings/appSettings.ts";

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
