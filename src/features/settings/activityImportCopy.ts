import type { AppLanguage } from "../../shared/settings/appSettings.ts";

const ACTIVITY_IMPORT_COPY = {
  "zh-CN": {
    title: "活动导入",
    hint: "导入平台中立的 Patina CSV。外部记录独立保存，统计时本机记录优先。",
    action: "选择 CSV",
    manage: "管理批次",
    dialogTitle: "导入活动记录",
    dialogHint: "提交前会重新验证文件指纹；小时汇总只参与统计，不会伪造成时间线。",
    available: (count: number) => `可导入 ${count} 条`,
    breakdown: (exact: number, buckets: number) => `精确会话 ${exact} 条，小时汇总 ${buckets} 条`,
    duplicates: (count: number) => `已存在 ${count} 条`,
    errors: (count: number) => `无效 ${count} 条`,
    commit: "确认导入",
    success: (count: number) => `已导入 ${count} 条活动记录。`,
    batchesTitle: "导入批次",
    batchesHint: "删除批次只会删除该次导入的外部记录，不会影响本机追踪记录。",
    empty: "暂无导入批次。",
    delete: "删除导入批次",
    deleteConfirm: (name: string) => `将删除“${name}”拥有的全部外部记录。本机追踪记录不受影响。`,
    failed: "活动导入失败。",
  },
  "en-US": {
    title: "Activity import",
    hint: "Import platform-neutral Patina CSV files. External records stay isolated and native records take precedence.",
    action: "Choose CSV",
    manage: "Manage batches",
    dialogTitle: "Import activity records",
    dialogHint: "The file fingerprint is verified again before commit. Hour buckets affect summaries but never fabricate a timeline.",
    available: (count: number) => `${count} records available`,
    breakdown: (exact: number, buckets: number) => `${exact} exact sessions, ${buckets} hour buckets`,
    duplicates: (count: number) => `${count} already imported`,
    errors: (count: number) => `${count} invalid`,
    commit: "Import",
    success: (count: number) => `Imported ${count} activity records.`,
    batchesTitle: "Import batches",
    batchesHint: "Deleting a batch removes only its external records. Native tracking records are not affected.",
    empty: "No import batches yet.",
    delete: "Delete import batch",
    deleteConfirm: (name: string) => `Delete every external record owned by “${name}”. Native tracking records are not affected.`,
    failed: "Activity import failed.",
  },
} as const;

export function getActivityImportCopy(language: AppLanguage) {
  return ACTIVITY_IMPORT_COPY[language];
}
