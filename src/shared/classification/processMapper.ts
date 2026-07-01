import {
  getCategoryToken,
  isCustomCategory,
  normalizeCustomCategory,
  USER_ASSIGNABLE_CATEGORIES,
  type AppCategory,
  type CustomAppCategory,
  type UserAssignableAppCategory,
} from "./categoryTokens.ts";
import { DEFAULT_APP_MAPPINGS } from "./defaultMappings.ts";
import { resolveCanonicalExecutable, shouldTrackProcess } from "./processNormalization.ts";
import { CategoryColorRegistry } from "./categoryColorRegistry.ts";
import { getUiTextLanguage } from "../copy/uiText.ts";

export type MappingConfidence = "high" | "medium" | "low";

export interface MappingHints {
  appName?: string;
}

export interface AppOverride {
  category?: UserAssignableAppCategory;
  displayName?: string;
  color?: string;
  track?: boolean;
  captureTitle?: boolean;
  enabled?: boolean;
  updatedAt?: number;
}

export interface AppInfo {
  name: string;
  category: AppCategory;
  color: string;
  confidence: MappingConfidence;
  source: "default" | "override" | "fallback";
}

const USER_ASSIGNABLE_CATEGORY_SET = new Set<string>(USER_ASSIGNABLE_CATEGORIES);

function formatFallbackName(exeName: string) {
  return exeName
    .replace(/\.exe$/i, "")
    .split(/[_\-\s.]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function normalizeDisplayName(name: string | undefined) {
  return (name ?? "").trim().replace(/\.exe$/i, "");
}

function normalizeHexColor(color: string | undefined): string | null {
  const raw = (color ?? "").trim();
  if (!raw) return null;

  const normalized = raw.startsWith("#") ? raw : `#${raw}`;
  if (!/^#[0-9A-Fa-f]{6}$/.test(normalized)) {
    return null;
  }

  return normalized.toUpperCase();
}

function resolveDefaultMappingName(defaultMapping: { name: string; localizedNames?: Partial<Record<string, string>> }) {
  return defaultMapping.localizedNames?.[getUiTextLanguage()] ?? defaultMapping.name;
}

function normalizeUserAssignableCategory(category: string | undefined): UserAssignableAppCategory | null {
  const normalized = (category ?? "").trim();
  if (!normalized) {
    return null;
  }

  if (isCustomCategory(normalized)) {
    return normalizeCustomCategory(normalized);
  }

  if (USER_ASSIGNABLE_CATEGORY_SET.has(normalized)) {
    return normalized as UserAssignableAppCategory;
  }

  return null;
}

function normalizeOverride(override: AppOverride | null | undefined): AppOverride | null {
  if (!override) return null;
  if (override.enabled === false) return null;

  const normalized: AppOverride = {};

  const normalizedCategory = normalizeUserAssignableCategory(override.category);
  if (normalizedCategory) {
    normalized.category = normalizedCategory;
  }
  if (override.displayName?.trim()) {
    normalized.displayName = override.displayName.trim();
  }
  const color = normalizeHexColor(override.color);
  if (color) {
    normalized.color = color;
  }
  if (override.track === false) {
    normalized.track = false;
  }
  if (override.captureTitle === false) {
    normalized.captureTitle = false;
  }
  if (typeof override.updatedAt === "number" && Number.isFinite(override.updatedAt)) {
    normalized.updatedAt = override.updatedAt;
  }
  normalized.enabled = true;

  const hasMeaningfulOverride = Boolean(
    normalized.category
    || normalized.displayName
    || normalized.color
    || normalized.track === false
    || normalized.captureTitle === false,
  );

  return hasMeaningfulOverride ? normalized : null;
}

export class ProcessMapper {
  private static userOverrides: Record<string, AppOverride> = {};
  private static categoryLabelOverrides: Record<string, string> = {};
  private static categoryColors = new CategoryColorRegistry();

  static getUserAssignableCategories() {
    return [...USER_ASSIGNABLE_CATEGORIES];
  }

  static setDeletedCategories(categories: AppCategory[]) {
    this.categoryColors.setDeletedCategories(categories);
  }

  static getDeletedCategories(): AppCategory[] {
    return this.categoryColors.getDeletedCategories();
  }

  static isCategoryDeleted(category: AppCategory): boolean {
    return this.categoryColors.isCategoryDeleted(category);
  }

  static getCategoryLabel(category: AppCategory) {
    return this.categoryLabelOverrides[category] ?? getCategoryToken(category).label;
  }

  static setCategoryLabelOverrides(overrides: Record<string, string>) {
    this.categoryLabelOverrides = Object.fromEntries(
      Object.entries(overrides)
        .map(([category, label]) => [category, label.trim().replace(/\s+/g, " ")] as const)
        .filter(([, label]) => label.length > 0),
    );
  }

  static setCategoryLabelOverride(category: CustomAppCategory, labelValue?: string | null) {
    const nextLabel = labelValue?.trim().replace(/\s+/g, " ");
    if (!nextLabel) {
      delete this.categoryLabelOverrides[category];
      return;
    }
    this.categoryLabelOverrides[category] = nextLabel;
  }

  static clearCategoryLabelOverrides() {
    this.categoryLabelOverrides = {};
  }

  static getCategoryColor(category: AppCategory) {
    return this.categoryColors.getCategoryColor(category);
  }

  static getDefaultCategoryColor(category: AppCategory) {
    return this.categoryColors.getDefaultCategoryColor(category);
  }

  static getCategoryColorOverride(category: AppCategory): string | null {
    return this.categoryColors.getCategoryColorOverride(category);
  }

  static setCategoryColorOverrides(overrides: Record<string, string>) {
    this.categoryColors.setCategoryColorOverrides(overrides);
  }

  static setCategoryColorOverride(category: AppCategory, colorValue?: string | null) {
    this.categoryColors.setCategoryColorOverride(category, colorValue);
  }

  static clearCategoryColorOverrides() {
    this.categoryColors.clearCategoryColorOverrides();
  }

  static setCategoryDefaultColorAssignments(assignments: Record<string, string>) {
    this.categoryColors.setCategoryDefaultColorAssignments(assignments);
  }

  static setCategoryDefaultColorAssignmentPersistence(
    handler: ((category: AppCategory, colorValue: string | null) => Promise<void>) | null,
  ) {
    this.categoryColors.setCategoryDefaultColorAssignmentPersistence(handler);
  }

  static async removeCategoryDefaultColorAssignment(category: AppCategory) {
    await this.categoryColors.removeCategoryDefaultColorAssignment(category);
  }

  static setUserOverrides(overrides: Record<string, AppOverride>) {
    const normalized: Record<string, AppOverride> = {};
    for (const [exeName, override] of Object.entries(overrides)) {
      const canonicalExe = resolveCanonicalExecutable(exeName);
      if (!canonicalExe) continue;

      const safeOverride = normalizeOverride(override);
      if (!safeOverride) continue;
      normalized[canonicalExe] = safeOverride;
    }
    this.userOverrides = normalized;
  }

  static setUserOverride(exeName: string, override: AppOverride | null) {
    const canonicalExe = resolveCanonicalExecutable(exeName);
    if (!canonicalExe) return;

    const safeOverride = normalizeOverride(override);
    if (!safeOverride) {
      delete this.userOverrides[canonicalExe];
      return;
    }

    this.userOverrides[canonicalExe] = safeOverride;
  }

  static clearUserOverrides() {
    this.userOverrides = {};
  }

  static getUserOverride(exeName: string): AppOverride | null {
    const canonicalExe = resolveCanonicalExecutable(exeName);
    return this.userOverrides[canonicalExe] ?? null;
  }

  private static mapWithOverride(exeName: string, hints: MappingHints, override: AppOverride | null | undefined): AppInfo {
    const canonicalExe = resolveCanonicalExecutable(exeName);
    const defaultMapping = DEFAULT_APP_MAPPINGS[canonicalExe];
    const hasOverride = Boolean(
      override?.category
      || override?.displayName
      || override?.color
      || override?.track === false
      || override?.captureTitle === false,
    );

    if (defaultMapping) {
      const mappedCategory = override?.category ?? defaultMapping.category ?? "other";
      const category = this.categoryColors.resolveActiveCategory(mappedCategory);
      const name = override?.displayName || resolveDefaultMappingName(defaultMapping);
      return {
        name,
        category,
        color: override?.color ?? this.categoryColors.getCategoryColor(category),
        confidence: "high",
        source: hasOverride ? "override" : "default",
      };
    }

    const fallbackName = formatFallbackName(canonicalExe) || canonicalExe;
    const resolvedName = override?.displayName || normalizeDisplayName(hints.appName) || fallbackName;
    const rawCategory = override?.category ?? "other";
    const resolvedCategory = this.categoryColors.resolveActiveCategory(rawCategory);

    return {
      name: resolvedName,
      category: resolvedCategory,
      color: override?.color ?? this.categoryColors.getCategoryColor(resolvedCategory),
      confidence: override?.category || override?.color || override?.track === false || override?.captureTitle === false
        ? "high"
        : "low",
      source: hasOverride ? "override" : "fallback",
    };
  }

  static map(exeName: string, hints: MappingHints = {}): AppInfo {
    const canonicalExe = resolveCanonicalExecutable(exeName);
    return this.mapWithOverride(exeName, hints, this.userOverrides[canonicalExe]);
  }

  static mapDefault(exeName: string, hints: MappingHints = {}): AppInfo {
    return this.mapWithOverride(exeName, hints, null);
  }

  static shouldTrack(exeName: string): boolean {
    const canonicalExe = resolveCanonicalExecutable(exeName);
    if (!shouldTrackProcess(canonicalExe)) {
      return false;
    }

    const override = this.userOverrides[canonicalExe];
    if (override?.track === false) {
      return false;
    }

    return this.map(canonicalExe).category !== "system";
  }

  static toOverrideStorageValue(override: AppOverride) {
    return JSON.stringify({
      category: override.category ?? null,
      displayName: override.displayName ?? null,
      color: normalizeHexColor(override.color) ?? null,
      track: override.track !== false,
      captureTitle: override.captureTitle !== false,
      enabled: override.enabled !== false,
      updatedAt: override.updatedAt ?? Date.now(),
    });
  }

  static fromOverrideStorageValue(rawValue: string): AppOverride | null {
    if (!rawValue.trim()) return null;
    try {
      const parsed = JSON.parse(rawValue) as AppOverride;
      return normalizeOverride(parsed);
    } catch {
      return null;
    }
  }
}
