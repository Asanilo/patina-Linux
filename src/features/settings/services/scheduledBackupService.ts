import {
  getScheduledBackupSnapshot,
  pickScheduledBackupDirectory,
  saveScheduledBackupConfig,
  subscribeScheduledBackupChanges,
  type ScheduledBackupCadence,
  type ScheduledBackupConfigInput,
  type ScheduledBackupSnapshot,
} from "../../../platform/backup/scheduledBackupRuntimeGateway.ts";

export type {
  ScheduledBackupCadence,
  ScheduledBackupConfigInput,
  ScheduledBackupSnapshot,
};

export const ScheduledBackupService = {
  load: getScheduledBackupSnapshot,
  save: saveScheduledBackupConfig,
  pickDirectory: pickScheduledBackupDirectory,
  subscribe: subscribeScheduledBackupChanges,
};
