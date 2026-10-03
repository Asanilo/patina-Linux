export interface TitleSampleDetail {
  title: string;
  startTime: number;
  endTime: number | null;
}

/** Owner-confirmed data. Legacy replay inputs omit this only during migration. */
export interface ConfirmedHistoryMetadata {
  appKey: string;
  category: import("../classification/categoryTokens.ts").AppCategory;
  displayNameOverride: string | null;
  origin: "native" | "import_exact";
  recordId: number;
  isOpen: boolean;
  isLive: boolean;
}

export interface HistorySession {
  id: number;
  appName: string;
  exeName: string;
  windowTitle: string;
  startTime: number;
  endTime: number | null;
  duration: number | null;
  continuityGroupStartTime: number | null;
  titleSampleDetails?: TitleSampleDetail[];
  confirmed?: ConfirmedHistoryMetadata;
}

export interface DailySummary {
  date: string;
  totalDuration: number;
}

export interface AggregateSessionRecord {
  appName: string;
  exeName: string;
  startTime: number;
  endTime: number;
  isLive?: boolean;
}
