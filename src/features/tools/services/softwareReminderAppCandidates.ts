import { ClassificationService } from "../../classification/services/classificationService.ts";
import type { ClassificationBootstrapData } from "../../classification/services/classificationService.ts";
import type { ObservedAppCandidate } from "../../classification/types.ts";
import { AppClassification } from "../../../shared/classification/appClassification.ts";
import { getUiTextLanguage } from "../../../shared/copy/uiText.ts";
import type { ToolSoftwareReminderAppCandidate } from "../../../shared/types/tools.ts";

export interface SoftwareReminderAppCandidateDeps {
  applyBootstrapToProcessMapper: (bootstrap: ClassificationBootstrapData) => void;
  getBootstrapCache: () => ClassificationBootstrapData | null;
  loadClassificationBootstrap: () => Promise<ClassificationBootstrapData>;
}

const defaultSoftwareReminderAppCandidateDeps: SoftwareReminderAppCandidateDeps = {
  applyBootstrapToProcessMapper: (bootstrap) => ClassificationService.applyBootstrapToProcessMapper(bootstrap),
  getBootstrapCache: () => ClassificationService.getBootstrapCache(),
  loadClassificationBootstrap: () => ClassificationService.loadClassificationBootstrap(),
};

let cachedBootstrap: ClassificationBootstrapData | null = null;
let cachedCandidates: ToolSoftwareReminderAppCandidate[] = [];
let cachedLanguage = "";

function resolveCandidateDisplayName(candidate: ObservedAppCandidate, exeName: string) {
  const mapped = AppClassification.mapApp(exeName, { appName: candidate.appName });
  return mapped.name.trim() || candidate.appName.trim() || exeName;
}

export function buildSoftwareReminderAppCandidates(
  observed: readonly ObservedAppCandidate[],
): ToolSoftwareReminderAppCandidate[] {
  const merged = new Map<string, ToolSoftwareReminderAppCandidate>();

  for (const candidate of observed) {
    if (!AppClassification.shouldTrackProcess(candidate.exeName, { appName: candidate.appName })) {
      continue;
    }

    const exeName = AppClassification.resolveCanonicalExecutable(candidate.exeName);
    if (!exeName || !AppClassification.shouldTrackApp(exeName)) {
      continue;
    }

    const appName = resolveCandidateDisplayName(candidate, exeName);
    const lastSeenAt = Math.max(0, Number(candidate.lastSeenMs ?? 0));
    const existing = merged.get(exeName);
    if (!existing) {
      merged.set(exeName, {
        appName,
        exeName,
        lastSeenAt,
      });
      continue;
    }

    existing.lastSeenAt = Math.max(existing.lastSeenAt, lastSeenAt);
    existing.appName = appName;
  }

  return Array.from(merged.values())
    .sort((left, right) => (
      right.lastSeenAt - left.lastSeenAt
      || left.appName.localeCompare(right.appName, undefined, { numeric: true, sensitivity: "base" })
      || left.exeName.localeCompare(right.exeName, undefined, { numeric: true, sensitivity: "base" })
    ));
}

function cloneCandidates(candidates: readonly ToolSoftwareReminderAppCandidate[]): ToolSoftwareReminderAppCandidate[] {
  return candidates.map((candidate) => ({ ...candidate }));
}

function buildCandidatesForBootstrap(bootstrap: ClassificationBootstrapData): ToolSoftwareReminderAppCandidate[] {
  const language = getUiTextLanguage();
  if (bootstrap !== cachedBootstrap || language !== cachedLanguage) {
    cachedBootstrap = bootstrap;
    cachedLanguage = language;
    cachedCandidates = buildSoftwareReminderAppCandidates(bootstrap.observed);
  }

  return cloneCandidates(cachedCandidates);
}

export async function loadSoftwareReminderAppCandidatesWithDeps(
  deps: SoftwareReminderAppCandidateDeps,
): Promise<ToolSoftwareReminderAppCandidate[]> {
  const bootstrap = deps.getBootstrapCache() ?? await deps.loadClassificationBootstrap();
  deps.applyBootstrapToProcessMapper(bootstrap);
  return buildCandidatesForBootstrap(bootstrap);
}

export async function loadSoftwareReminderAppCandidates(): Promise<ToolSoftwareReminderAppCandidate[]> {
  return loadSoftwareReminderAppCandidatesWithDeps(defaultSoftwareReminderAppCandidateDeps);
}

export function clearSoftwareReminderAppCandidateCache(): void {
  cachedBootstrap = null;
  cachedCandidates = [];
  cachedLanguage = "";
}

export function resetSoftwareReminderAppCandidatesCacheForTests(): void {
  clearSoftwareReminderAppCandidateCache();
}
