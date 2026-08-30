import { getUiLocale } from "../../shared/copy/uiText.ts";

const ZH_CN_DESTINATION_DETAIL_COPY = {
  title: "活动详情",
  close: "关闭",
  loading: "正在加载活动详情...",
  dayError: "无法读取这一天的活动详情。",
  retry: "重试",
  previousDay: "上一天",
  nextDay: "下一天",
  recordedDuration: "记录时长",
  timeline: "时间轴",
  timelineZoom: "调整时间轴窗口",
  timelineAria: "活动时间轴",
  zoomHours: (hours: number) => `${hours} 小时窗口`,
  panEarlier: "查看更早时间",
  panLater: "查看更晚时间",
  records: "活动记录",
  minimumDuration: "最短显示时长",
  minimumMinutes: (minutes: number) => `至少 ${minutes} 分钟`,
  noActivity: "这一天没有相关活动。",
  noActivityInWindow: "当前时间窗口没有相关活动。",
  noActivityAtMinimum: (minutes: number) => `没有达到 ${minutes} 分钟的活动。`,
  current: "当前",
  titleRows: (count: number) => `${count} 条标题明细`,
  untitled: "未记录标题",
};

type DestinationDetailCopy = typeof ZH_CN_DESTINATION_DETAIL_COPY;

const EN_US_DESTINATION_DETAIL_COPY: DestinationDetailCopy = {
  title: "Activity details",
  close: "Close",
  loading: "Loading activity details...",
  dayError: "Activity details for this day could not be loaded.",
  retry: "Retry",
  previousDay: "Previous day",
  nextDay: "Next day",
  recordedDuration: "Recorded",
  timeline: "Timeline",
  timelineZoom: "Adjust timeline window",
  timelineAria: "Activity timeline",
  zoomHours: (hours: number) => `${hours}-hour window`,
  panEarlier: "Show earlier time",
  panLater: "Show later time",
  records: "Activity records",
  minimumDuration: "Minimum visible duration",
  minimumMinutes: (minutes: number) => `At least ${minutes} min`,
  noActivity: "No matching activity on this day.",
  noActivityInWindow: "No matching activity in this time window.",
  noActivityAtMinimum: (minutes: number) => `No activity reached ${minutes} minutes.`,
  current: "Current",
  titleRows: (count: number) => `${count} title rows`,
  untitled: "Title not recorded",
};

export function getDestinationDetailCopy(): DestinationDetailCopy {
  return getUiLocale() === "en-US"
    ? EN_US_DESTINATION_DETAIL_COPY
    : ZH_CN_DESTINATION_DETAIL_COPY;
}
