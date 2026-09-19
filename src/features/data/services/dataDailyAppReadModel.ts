import { AppClassification } from "../../../shared/classification/appClassification.ts";
import { isAppCategory, type AppCategory } from "../../../shared/classification/categoryTokens.ts";
import { getUiLocale, UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { DailyAppsRead } from "../../../platform/persistence/dailyAppsRepository.ts";
import { buildDataChartAxis, type DataAppTrendViewModel, type DataAppDayRow, type DataDestinationTrendChartRow } from "./dataReadModel.ts";
import type { DataCategoryOption, DataCategoryTrendViewModel } from "./dataCategoryTrendReadModel.ts";
import { buildDataDayRanges, buildDataMonthRanges, type ResolvedDataTrendRange } from "./dataTrendRange.ts";

function dateKey(ms: number) {
  const date = new Date(ms);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function context(activity: DailyAppsRead, range: ResolvedDataTrendRange) {
  const dates = buildDataDayRanges(range);
  const chartRanges = range.granularity === "month" ? buildDataMonthRanges(range) : dates;
  const days = new Map(activity.days.map(day => [day.date, new Map(day.apps.map(app => [app.appKey, app.duration]))]));
  const apps = activity.applications.filter(app => AppClassification.shouldTrackApp(app.appKey)).map(app => {
    const override = AppClassification.getUserOverride(app.appKey)?.displayName?.trim();
    const name = override || AppClassification.resolveCanonicalDisplayName(app.appKey)
      || app.appName || AppClassification.mapApp(app.appKey).name;
    const daily = dates.map(date => date.endMs <= date.startMs ? 0 : days.get(dateKey(date.startMs))?.get(app.appKey) ?? 0);
    return { ...app, appName: name, daily, total: daily.reduce((sum, value) => sum + value, 0), category: AppClassification.mapApp(app.appKey, { appName: name }).category };
  }).filter(app => app.total > 0);
  const sumRange = (daily: number[], start: number, end: number) => dates.reduce((sum, date, index) => (
    date.startMs >= start && date.startMs < end ? sum + daily[index] : sum
  ), 0);
  const row = (daily: number[], category = false) => dates.map((day, index) => {
    const date = dateKey(day.startMs);
    return { date, label: new Date(day.startMs).toLocaleDateString(category ? undefined : getUiLocale(), { month: "2-digit", day: "2-digit", weekday: "short" }), duration: daily[index], intensity: 0 };
  });
  return { apps, dates, chartRanges, divisor: Math.max(1, chartRanges.length), sumRange, row };
}

function peak(rows: DataAppDayRow[]) {
  const value = rows.reduce<DataAppDayRow | null>((best, row) => !best || row.duration > best.duration ? row : best, null);
  return value && value.duration > 0 ? value : null;
}

export function buildDailyAppTrendViewModel(activity: DailyAppsRead, range: ResolvedDataTrendRange, selectedKey: string | null): DataAppTrendViewModel {
  const model = context(activity, range);
  const total = model.apps.reduce((sum, app) => sum + app.total, 0);
  const appOptions = model.apps.map(app => ({
    appKey: app.appKey, appName: app.appName, exeName: app.exeName, totalDuration: app.total,
    percentage: total > 0 ? app.total / total * 100 : 0,
    averageDuration: Math.round(app.total / model.divisor), activeDayCount: app.daily.filter(value => value > 0).length,
  })).sort((a, b) => b.totalDuration - a.totalDuration);
  const selectedApp = appOptions.find(app => app.appKey === selectedKey) ?? appOptions[0] ?? null;
  const daily = model.apps.find(app => app.appKey === selectedApp?.appKey)?.daily ?? model.dates.map(() => 0);
  const chartData = model.chartRanges.map(item => {
    const date = dateKey(item.startMs);
    const duration = model.sumRange(daily, item.startMs, item.endMs);
    return { date, label: range.granularity === "month" ? UI_TEXT.date.monthLabel(Number(date.slice(5, 7))) : date.slice(5), duration, hours: duration / 3600000 };
  });
  const rows = selectedApp ? model.row(daily) : [];
  const max = Math.max(1, ...daily);
  const dayRows = rows.map(row => ({ ...row, intensity: row.duration > 0 ? Math.max(0.08, row.duration / max) : 0 }));
  return {
    range, rangeLabel: range.label, granularity: range.granularity, appOptions, selectedApp, chartData,
    chartAxis: buildDataChartAxis(chartData), dayRows: dayRows.slice().reverse(), peakDay: peak(dayRows)
  };
}

export function buildDailyCategoryTrendViewModel(activity: DailyAppsRead, range: ResolvedDataTrendRange, selectedKeys: readonly string[]): DataCategoryTrendViewModel {
  const model = context(activity, range);
  const groups = new Map<AppCategory, { daily: number[]; count: number }>();
  for (const app of model.apps) {
    const group = groups.get(app.category) ?? { daily: model.dates.map(() => 0), count: 0 };
    group.count++;
    app.daily.forEach((value, index) => { group.daily[index] += value; });
    groups.set(app.category, group);
  }
  const total = model.apps.reduce((sum, app) => sum + app.total, 0);
  const option = (category: AppCategory): DataCategoryOption => {
    const group = groups.get(category);
    const duration = group?.daily.reduce((sum, value) => sum + value, 0) ?? 0;
    return {
      category, displayName: AppClassification.getCategoryLabel(category), color: AppClassification.getCategoryColor(category),
      appCount: group?.count ?? 0, totalDuration: duration, percentage: total > 0 ? duration / total * 100 : 0,
      averageDuration: Math.round(duration / model.divisor), activeDayCount: group?.daily.filter(value => value > 0).length ?? 0
    };
  };
  const categoryOptions = [...groups.keys()].map(option).sort((a, b) => b.totalDuration - a.totalDuration || a.category.localeCompare(b.category));
  const requested = [...new Set(selectedKeys)].filter(isAppCategory);
  const selected = requested.length ? requested : categoryOptions[0] ? [categoryOptions[0].category] : [];
  const selectedCategories = selected.map(option);
  const chartSeries = selectedCategories.map((item, index) => ({ key: item.category, dataKey: `series${index}`, displayName: item.displayName, color: item.color }));
  const chartRows = model.chartRanges.map(item => {
    const date = dateKey(item.startMs);
    const row: DataDestinationTrendChartRow = { date, label: range.granularity === "month" ? UI_TEXT.date.monthLabel(Number(date.slice(5, 7))) : date.slice(5), duration: 0, hours: 0, totalDuration: 0, totalHours: 0 };
    selected.forEach((category, index) => {
      const duration = model.sumRange(groups.get(category)?.daily ?? model.dates.map(() => 0), item.startMs, item.endMs);
      row[`series${index}`] = duration / 3600000;
      row.totalDuration += duration;
    });
    row.duration = row.totalDuration; row.totalHours = row.hours = row.duration / 3600000;
    return row;
  });
  const daily = model.dates.map((_, index) => selected.reduce((sum, key) => sum + (groups.get(key)?.daily[index] ?? 0), 0));
  const selectedTotal = daily.reduce((sum, value) => sum + value, 0);
  return {
    range, rangeLabel: range.label, granularity: range.granularity, categoryOptions, selectedCategories, chartSeries, chartRows,
    summary: { totalDuration: selectedTotal, averageDuration: Math.round(selectedTotal / model.divisor), activeDayCount: daily.filter(value => value > 0).length },
    chartAxis: buildDataChartAxis(chartRows), peakDay: peak(model.row(daily, true))
  };
}
