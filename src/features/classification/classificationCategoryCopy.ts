import { getUiTextLanguage } from "../../shared/copy/uiText.ts";

const CATEGORY_MANAGEMENT_COPY = {
  "zh-CN": {
    renameTitle: "重命名自定义分类",
    renameDescription: "已使用该分类的应用和网页会显示新名称。",
    renamePlaceholder: "新的分类名称",
    mergeTitle: "合并同名分类",
    mergeDetail: (label: string) => `已存在“${label}”分类。继续后会把当前分类并入该分类。`,
    renameAction: (label: string) => `重命名分类：${label}`,
    dialogDescription: "新建或管理自定义分类，并调整分类颜色",
  },
  "en-US": {
    renameTitle: "Rename custom category",
    renameDescription: "Apps and websites using this category will show the new name.",
    renamePlaceholder: "New category name",
    mergeTitle: "Merge matching category",
    mergeDetail: (label: string) => `${label} already exists. Continuing will merge this category into it.`,
    renameAction: (label: string) => `Rename category: ${label}`,
    dialogDescription: "Create or manage custom categories and adjust category colors",
  },
} as const;

export function getCategoryManagementCopy() {
  return CATEGORY_MANAGEMENT_COPY[getUiTextLanguage()];
}
