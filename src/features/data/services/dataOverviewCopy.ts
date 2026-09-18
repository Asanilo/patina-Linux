import type { AppLanguage } from "../../../shared/settings/appSettings.ts";

export function getDataOverviewCopy(language: AppLanguage) {
  return language === "en-US" ? {
    failed: "Could not load activity trend. Check the background service or retry.",
    unsupported: "Update the background service to load daily activity summaries.",
    rangeLimit: "Select a range of at most 378 days.",
    retry: "Retry activity trend",
  } : {
    failed: "趋势加载失败，请检查后台服务或重试。",
    unsupported: "请更新后台服务以读取每日活动汇总。",
    rangeLimit: "请选择不超过 378 天的范围。",
    retry: "重试活动趋势",
  };
}
