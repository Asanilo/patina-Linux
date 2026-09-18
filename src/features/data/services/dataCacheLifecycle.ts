import { clearDataBootstrapSnapshot } from "./dataBootstrapSnapshot.ts";
import { clearDataReadModelCache } from "./dataReadModel.ts";
import { clearDataTrendSnapshotCache } from "./dataTrendSnapshot.ts";
import { clearDataOverviewSnapshotCache } from "./dataOverviewSnapshot.ts";

export function clearDataHeavyCaches(): void {
  clearDataReadModelCache();
  clearDataTrendSnapshotCache();
  clearDataOverviewSnapshotCache();
}

export function clearDataBootstrapCache(): Promise<void> {
  return clearDataBootstrapSnapshot();
}
