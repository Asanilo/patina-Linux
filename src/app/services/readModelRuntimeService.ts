import { loadDashboardSnapshot, type DashboardSnapshot } from "../../features/dashboard/services/dashboardReadModel.ts";
import { loadHistorySnapshot, type HistorySnapshot } from "../../features/history/services/historyReadModel.ts";
import { ensureProcessMapperRuntimeReady } from "./processMapperRuntimeGate.ts";
import { setDashboardSnapshotCache, getDashboardSnapshotCacheGeneration } from "../../features/dashboard/services/dashboardSnapshotCache.ts";
import { setHistorySnapshotCache, getHistorySnapshotCacheGeneration } from "../../features/history/services/historySnapshotCache.ts";
import {
  loadDataTrendSnapshot,
  type DataTrendSnapshot,
} from "../../features/data/services/dataTrendSnapshot.ts";
import type { DataTrendRangeSelection } from "../../features/data/services/dataTrendRange.ts";

type DashboardRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady: () => Promise<void>;
  loadDashboardSnapshot: (date?: Date) => Promise<DashboardSnapshot>;
  setDashboardSnapshotCache: (snapshot: DashboardSnapshot, date?: Date) => void;
};

type HistoryRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady: () => Promise<void>;
  loadHistorySnapshot: (date: Date) => Promise<HistorySnapshot>;
  setHistorySnapshotCache: (
    snapshot: HistorySnapshot,
    date?: Date,
  ) => void;
};

type DataTrendRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady: () => Promise<void>;
  loadDataTrendSnapshot: (
    selection: DataTrendRangeSelection,
    nowMs?: number,
  ) => Promise<DataTrendSnapshot>;
};

const dashboardRuntimeSnapshotDeps: DashboardRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady,
  loadDashboardSnapshot,
  setDashboardSnapshotCache,
};

const historyRuntimeSnapshotDeps: HistoryRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady,
  loadHistorySnapshot,
  setHistorySnapshotCache,
};

const dataTrendRuntimeSnapshotDeps: DataTrendRuntimeSnapshotDeps = {
  ensureProcessMapperRuntimeReady,
  loadDataTrendSnapshot,
};

export async function loadDashboardRuntimeSnapshotWithDeps(
  date: Date = new Date(),
  deps: DashboardRuntimeSnapshotDeps,
): Promise<DashboardSnapshot> {
  const generation = getDashboardSnapshotCacheGeneration();
  await deps.ensureProcessMapperRuntimeReady();
  const snapshot = await deps.loadDashboardSnapshot(date);
  if (generation === getDashboardSnapshotCacheGeneration()) deps.setDashboardSnapshotCache(snapshot, date);
  return snapshot;
}

export async function loadDashboardRuntimeSnapshot(date: Date = new Date()): Promise<DashboardSnapshot> {
  return loadDashboardRuntimeSnapshotWithDeps(date, dashboardRuntimeSnapshotDeps);
}

export async function loadHistoryRuntimeSnapshotWithDeps(
  date: Date,
  deps: HistoryRuntimeSnapshotDeps,
): Promise<HistorySnapshot> {
  const generation = getHistorySnapshotCacheGeneration();
  await deps.ensureProcessMapperRuntimeReady();
  const snapshot = await deps.loadHistorySnapshot(date);
  if (generation === getHistorySnapshotCacheGeneration()) deps.setHistorySnapshotCache(snapshot, date);
  return snapshot;
}

export async function loadHistoryRuntimeSnapshot(
  date: Date,
): Promise<HistorySnapshot> {
  return loadHistoryRuntimeSnapshotWithDeps(date, historyRuntimeSnapshotDeps);
}

export async function loadDataTrendRuntimeSnapshot(
  selection: DataTrendRangeSelection,
  nowMs: number = Date.now(),
): Promise<DataTrendSnapshot> {
  return loadDataTrendRuntimeSnapshotWithDeps(selection, nowMs, dataTrendRuntimeSnapshotDeps);
}

export async function loadDataTrendRuntimeSnapshotWithDeps(
  selection: DataTrendRangeSelection,
  nowMs: number,
  deps: DataTrendRuntimeSnapshotDeps,
): Promise<DataTrendSnapshot> {
  await deps.ensureProcessMapperRuntimeReady();
  return deps.loadDataTrendSnapshot(selection, nowMs);
}
