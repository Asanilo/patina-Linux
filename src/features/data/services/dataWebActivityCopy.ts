import type { AppLanguage } from "../../../shared/settings/appSettings.ts";

const WEB_ACTIVITY_COPY = {
  "zh-CN": {
    trend: "网页趋势",
    range: "选择网页趋势范围",
    searchPlaceholder: "搜索网站或域名",
    domainList: "网站列表",
    empty: "当前范围暂无网页活动",
    noMatch: "没有匹配的网站",
    unavailable: "网页趋势暂不可用",
    retry: "重试",
  },
  "en-US": {
    trend: "Web Trends",
    range: "Select web trend range",
    searchPlaceholder: "Search websites or domains",
    domainList: "Website list",
    empty: "No web activity in this range",
    noMatch: "No matching websites",
    unavailable: "Web trends are temporarily unavailable",
    retry: "Retry",
  },
} as const;

export function getDataWebActivityCopy(language: AppLanguage) {
  return WEB_ACTIVITY_COPY[language];
}
