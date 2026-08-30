import {
  type CSSProperties,
  type MouseEvent,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { PanelRightOpen, Search } from "lucide-react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, XAxis, YAxis } from "recharts";
import QuietChartTooltip from "../../../shared/components/QuietChartTooltip.tsx";
import QuietIconAction from "../../../shared/components/QuietIconAction.tsx";
import QuietSegmentedFilter from "../../../shared/components/QuietSegmentedFilter.tsx";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import { formatLocalDateKey } from "../../../shared/lib/localDate.ts";
import type { AppLanguage } from "../../../shared/settings/appSettings.ts";
import {
  createDestinationDetailTarget,
  type DestinationDetailOpenRequest,
} from "../../destination/types.ts";
import {
  formatChartHours,
  formatDuration,
} from "../../history/services/historyFormatting.ts";
import { useDataChartInitialDimension } from "../hooks/useDataChartInitialDimension.ts";
import { useDataWebActivitySnapshot } from "../hooks/useDataWebActivitySnapshot.ts";
import {
  buildDataCategoryTrendViewModel,
  filterDataCategoryOptionsForQuery,
  type DataCategoryTrendViewModel,
} from "../services/dataCategoryTrendReadModel.ts";
import { resolveTrendDateFromChartEvent } from "../services/dataChartInteraction.ts";
import {
  buildDataAppTrendViewModel,
  type DataAppOption,
  type DataAppTrendViewModel,
} from "../services/dataReadModel.ts";
import type { DataTrendRangeSelection } from "../services/dataTrendRange.ts";
import type { DataTrendSnapshot } from "../services/dataTrendSnapshot.ts";
import { getDataWebActivityCopy } from "../services/dataWebActivityCopy.ts";
import {
  buildDataWebActivityTrendViewModel,
  filterDataWebDomainOptionsForQuery,
  type DataWebActivityTrendViewModel,
} from "../services/dataWebActivityReadModel.ts";
import DataTrendRangeControl from "./DataTrendRangeControl.tsx";

interface Props {
  appTrendNowMs: number;
  appTrendRangeCacheKey: string;
  appTrendSnapshot: DataTrendSnapshot | null;
  bootstrapAppTrendViewModel: DataAppTrendViewModel | null;
  icons: Record<string, string>;
  mappingVersion: number;
  onAppTrendViewModelChange: (viewModel: DataAppTrendViewModel | null) => void;
  onOpenDestinationDetail?: (request: DestinationDetailOpenRequest) => void;
  onOpenHistoryDate?: (dateKey: string) => void;
  onSelectionChange: (selection: DataTrendRangeSelection) => void;
  refreshKey: number;
  selection: DataTrendRangeSelection;
  uiLanguage: AppLanguage;
  webEnabled: boolean;
}

const DATA_TREND_X_AXIS_MIN_TICK_GAP = 24;
type DataDestinationMode = "app" | "category" | "web";

function getAppInitial(appName: string) {
  const trimmed = appName.trim();
  return trimmed ? trimmed.charAt(0).toUpperCase() : "?";
}

function getDataAppOptionDisplayKey(app: DataAppOption) {
  return `${app.appName.trim().toLowerCase().replace(/\s+/g, " ")}|${app.exeName.trim().toLowerCase()}`;
}

function dedupeDataAppOptions(options: DataAppOption[]) {
  const merged = new Map<string, DataAppOption>();

  for (const app of options) {
    const key = getDataAppOptionDisplayKey(app);
    const existing = merged.get(key);

    if (!existing) {
      merged.set(key, { ...app });
      continue;
    }

    existing.totalDuration += app.totalDuration;
    existing.percentage += app.percentage;
    existing.averageDuration += app.averageDuration;
    existing.activeDayCount = Math.max(existing.activeDayCount, app.activeDayCount);
  }

  return Array.from(merged.values()).sort((left, right) => right.totalDuration - left.totalDuration);
}

function filterDataAppOptionsForQuery(options: DataAppOption[], query: string) {
  const normalizedQuery = query.trim().toLowerCase();
  const dedupedOptions = dedupeDataAppOptions(options);
  if (!normalizedQuery) return dedupedOptions;

  return dedupedOptions.filter((app) => (
    app.appName.toLowerCase().includes(normalizedQuery)
    || app.exeName.toLowerCase().includes(normalizedQuery)
  ));
}

function updateDataDestinationSelection(
  current: readonly string[],
  key: string,
  multi: boolean,
) {
  if (!multi) return [key];
  if (!current.includes(key)) return [...current, key];
  return current.length > 1 ? current.filter((item) => item !== key) : [...current];
}

export default function DataDestinationTrendPanel({
  appTrendNowMs,
  appTrendRangeCacheKey,
  appTrendSnapshot,
  bootstrapAppTrendViewModel,
  icons,
  mappingVersion,
  onAppTrendViewModelChange,
  onOpenDestinationDetail,
  onOpenHistoryDate,
  onSelectionChange,
  refreshKey,
  selection,
  uiLanguage,
  webEnabled,
}: Props) {
  const webActivityCopy = getDataWebActivityCopy(uiLanguage);
  const [destinationMode, setDestinationMode] = useState<DataDestinationMode>("app");
  const [selectedAppKey, setSelectedAppKey] = useState<string | null>(null);
  const [selectedCategoryKeys, setSelectedCategoryKeys] = useState<string[]>([]);
  const [selectedWebDomainKeys, setSelectedWebDomainKeys] = useState<string[]>([]);
  const [appSearchQuery, setAppSearchQuery] = useState("");
  const [categorySearchQuery, setCategorySearchQuery] = useState("");
  const [webSearchQuery, setWebSearchQuery] = useState("");
  const [webRetryKey, setWebRetryKey] = useState(0);
  const webActivityTrend = useDataWebActivitySnapshot({
    enabled: webEnabled && destinationMode === "web",
    selection,
    refreshKey: refreshKey + webRetryKey,
    cacheVersion: `mapping:${mappingVersion}`,
  });
  const destinationTrendChart = useDataChartInitialDimension("destinationTrend");
  const lastAppTrendViewModelRef = useRef<{
    rangeCacheKey: string;
    viewModel: DataAppTrendViewModel;
  } | null>(null);
  const lastCategoryTrendViewModelRef = useRef<{
    rangeCacheKey: string;
    viewModel: DataCategoryTrendViewModel;
  } | null>(null);
  const lastWebTrendViewModelRef = useRef<{
    rangeCacheKey: string;
    viewModel: DataWebActivityTrendViewModel;
  } | null>(null);
  const destinationListRef = useRef<HTMLDivElement | null>(null);
  const activeDestinationTrendDateRef = useRef<string | null>(null);

  const appTrendViewModel = useMemo(() => {
    if (!appTrendSnapshot) return null;
    return buildDataAppTrendViewModel(
      appTrendSnapshot.sessions,
      appTrendSnapshot.range,
      appTrendNowMs,
      selectedAppKey,
    );
  }, [appTrendNowMs, appTrendSnapshot, mappingVersion, selectedAppKey]);
  if (appTrendViewModel) {
    lastAppTrendViewModelRef.current = {
      rangeCacheKey: appTrendRangeCacheKey,
      viewModel: appTrendViewModel,
    };
  }
  const visibleAppTrendViewModel = appTrendViewModel
    ?? (lastAppTrendViewModelRef.current?.rangeCacheKey === appTrendRangeCacheKey
      ? lastAppTrendViewModelRef.current.viewModel
      : null)
    ?? bootstrapAppTrendViewModel;

  const categoryTrendViewModel = useMemo(() => {
    if (!appTrendSnapshot) return null;
    return buildDataCategoryTrendViewModel(
      appTrendSnapshot.sessions,
      appTrendSnapshot.range,
      appTrendNowMs,
      selectedCategoryKeys,
    );
  }, [appTrendNowMs, appTrendSnapshot, mappingVersion, selectedCategoryKeys]);
  if (categoryTrendViewModel) {
    lastCategoryTrendViewModelRef.current = {
      rangeCacheKey: appTrendRangeCacheKey,
      viewModel: categoryTrendViewModel,
    };
  }
  const visibleCategoryTrendViewModel = categoryTrendViewModel
    ?? (lastCategoryTrendViewModelRef.current?.rangeCacheKey === appTrendRangeCacheKey
      ? lastCategoryTrendViewModelRef.current.viewModel
      : null);

  const webTrendViewModel = useMemo(() => {
    if (!webActivityTrend.snapshot) return null;
    return buildDataWebActivityTrendViewModel(
      webActivityTrend.snapshot.segments,
      webActivityTrend.snapshot.overrides,
      webActivityTrend.snapshot.range,
      webActivityTrend.nowMs,
      selectedWebDomainKeys,
    );
  }, [mappingVersion, selectedWebDomainKeys, webActivityTrend.nowMs, webActivityTrend.snapshot]);
  if (webTrendViewModel) {
    lastWebTrendViewModelRef.current = {
      rangeCacheKey: webActivityTrend.resolvedRange.cacheKey,
      viewModel: webTrendViewModel,
    };
  }
  const visibleWebTrendViewModel = webEnabled
    ? webTrendViewModel
      ?? (lastWebTrendViewModelRef.current?.rangeCacheKey === webActivityTrend.resolvedRange.cacheKey
        ? lastWebTrendViewModelRef.current.viewModel
        : null)
    : null;

  useEffect(() => {
    onAppTrendViewModelChange(appTrendViewModel);
  }, [appTrendViewModel, onAppTrendViewModelChange]);

  useEffect(() => {
    if (selectedAppKey !== null) return;
    const defaultAppKey = appTrendViewModel?.selectedApp?.appKey;
    if (defaultAppKey) setSelectedAppKey(defaultAppKey);
  }, [appTrendViewModel?.selectedApp?.appKey, selectedAppKey]);

  useEffect(() => {
    if (selectedCategoryKeys.length > 0) return;
    const defaultCategory = categoryTrendViewModel?.selectedCategories[0]?.category;
    if (defaultCategory) setSelectedCategoryKeys([defaultCategory]);
  }, [categoryTrendViewModel?.selectedCategories, selectedCategoryKeys.length]);

  useEffect(() => {
    if (selectedWebDomainKeys.length > 0) return;
    const defaultDomain = webTrendViewModel?.selectedDomains[0]?.normalizedDomain;
    if (defaultDomain) setSelectedWebDomainKeys([defaultDomain]);
  }, [selectedWebDomainKeys.length, webTrendViewModel?.selectedDomains]);

  useEffect(() => {
    if (!webEnabled && destinationMode === "web") setDestinationMode("app");
  }, [destinationMode, webEnabled]);

  const filteredAppOptions = useMemo(() => (
    visibleAppTrendViewModel
      ? filterDataAppOptionsForQuery(visibleAppTrendViewModel.appOptions, appSearchQuery)
      : []
  ), [appSearchQuery, visibleAppTrendViewModel]);
  const filteredCategoryOptions = useMemo(() => (
    visibleCategoryTrendViewModel
      ? filterDataCategoryOptionsForQuery(
        visibleCategoryTrendViewModel.categoryOptions,
        categorySearchQuery,
      )
      : []
  ), [categorySearchQuery, visibleCategoryTrendViewModel]);
  const filteredWebDomainOptions = useMemo(() => (
    visibleWebTrendViewModel
      ? filterDataWebDomainOptionsForQuery(visibleWebTrendViewModel.domainOptions, webSearchQuery)
      : []
  ), [visibleWebTrendViewModel, webSearchQuery]);

  const hasAppSearchQuery = appSearchQuery.trim().length > 0;
  const appTrendSelectedAppMatchesSearch = !hasAppSearchQuery
    || Boolean(
      visibleAppTrendViewModel?.selectedApp
      && filteredAppOptions.some((app) => app.appKey === visibleAppTrendViewModel.selectedApp?.appKey),
    );
  const appTrendSelectionHiddenBySearch = hasAppSearchQuery && !appTrendSelectedAppMatchesSearch;
  const selectedAppTrendApp = appTrendSelectionHiddenBySearch ? null : visibleAppTrendViewModel?.selectedApp;
  const appTrendChartData = appTrendSelectionHiddenBySearch && visibleAppTrendViewModel
    ? visibleAppTrendViewModel.chartData.map((point) => ({ ...point, duration: 0, hours: 0 }))
    : (visibleAppTrendViewModel?.chartData ?? []);
  const appTrendChartAxis = appTrendSelectionHiddenBySearch
    ? { domainMax: 3, ticks: [0, 1, 2, 3] }
    : (visibleAppTrendViewModel?.chartAxis ?? { domainMax: 3, ticks: [0, 1, 2, 3] });
  const appTrendPeakDay = appTrendSelectionHiddenBySearch ? null : visibleAppTrendViewModel?.peakDay;
  const isCategoryDestination = destinationMode === "category";
  const isWebDestination = destinationMode === "web";
  const selectedCategories = visibleCategoryTrendViewModel?.selectedCategories ?? [];
  const selectedWebDomains = visibleWebTrendViewModel?.selectedDomains ?? [];
  const selectedWebDomain = selectedWebDomains.length === 1 ? selectedWebDomains[0] : null;
  const destinationReady = isWebDestination
    ? Boolean(visibleWebTrendViewModel)
    : isCategoryDestination
      ? Boolean(visibleCategoryTrendViewModel)
      : Boolean(visibleAppTrendViewModel);
  const destinationOptionsEmpty = isWebDestination
    ? (visibleWebTrendViewModel?.domainOptions.length ?? 0) === 0
    : isCategoryDestination
      ? (visibleCategoryTrendViewModel?.categoryOptions.length ?? 0) === 0
      : (visibleAppTrendViewModel?.appOptions.length ?? 0) === 0;
  const destinationChartData = isWebDestination
    ? visibleWebTrendViewModel?.chartRows ?? []
    : isCategoryDestination
      ? visibleCategoryTrendViewModel?.chartRows ?? []
      : appTrendChartData;
  const destinationChartAxis = isWebDestination
    ? visibleWebTrendViewModel?.chartAxis ?? { domainMax: 3, ticks: [0, 1, 2, 3] }
    : isCategoryDestination
      ? visibleCategoryTrendViewModel?.chartAxis ?? { domainMax: 3, ticks: [0, 1, 2, 3] }
      : appTrendChartAxis;
  const destinationChartSeries = isWebDestination
    ? visibleWebTrendViewModel?.chartSeries ?? []
    : isCategoryDestination ? visibleCategoryTrendViewModel?.chartSeries ?? [] : [];
  const destinationPeakDay = isWebDestination
    ? visibleWebTrendViewModel?.peakDay
    : isCategoryDestination
      ? visibleCategoryTrendViewModel?.peakDay
      : appTrendPeakDay;
  const destinationSummary = isWebDestination
    ? visibleWebTrendViewModel?.summary
    : isCategoryDestination
      ? visibleCategoryTrendViewModel?.summary
      : {
        totalDuration: selectedAppTrendApp?.totalDuration ?? 0,
        averageDuration: selectedAppTrendApp?.averageDuration ?? 0,
        activeDayCount: selectedAppTrendApp?.activeDayCount ?? 0,
      };
  const destinationGranularity = isWebDestination
    ? visibleWebTrendViewModel?.granularity ?? "day"
    : isCategoryDestination
      ? visibleCategoryTrendViewModel?.granularity ?? "day"
      : visibleAppTrendViewModel?.granularity ?? "day";
  const hasDestinationSelection = isWebDestination
    ? selectedWebDomains.length > 0
    : isCategoryDestination
      ? selectedCategories.length > 0
      : Boolean(selectedAppTrendApp);
  const destinationError = isWebDestination ? webActivityTrend.error : null;
  const destinationSearchQuery = isWebDestination
    ? webSearchQuery
    : isCategoryDestination ? categorySearchQuery : appSearchQuery;
  const destinationSearchPlaceholder = isWebDestination
    ? webActivityCopy.searchPlaceholder
    : isCategoryDestination ? UI_TEXT.data.categorySearchPlaceholder : UI_TEXT.data.appSearchPlaceholder;
  const destinationListAriaLabel = isWebDestination
    ? webActivityCopy.domainList
    : isCategoryDestination ? UI_TEXT.data.categoryTrendCategoryList : UI_TEXT.data.appTrendAppList;
  const destinationEmptyLabel = isWebDestination
    ? webActivityCopy.empty
    : isCategoryDestination ? UI_TEXT.data.categoryTrendEmpty : UI_TEXT.data.appTrendEmpty;
  const destinationNoMatchLabel = isWebDestination
    ? webActivityCopy.noMatch
    : isCategoryDestination ? UI_TEXT.data.categoryTrendNoMatch : UI_TEXT.data.appTrendNoMatch;

  useEffect(() => {
    if (!hasAppSearchQuery || !visibleAppTrendViewModel) return;
    const firstMatch = filteredAppOptions[0];
    if (!firstMatch) return;
    const selectedAppKeyIsVisible = Boolean(
      visibleAppTrendViewModel.selectedApp
      && filteredAppOptions.some((app) => app.appKey === visibleAppTrendViewModel.selectedApp?.appKey),
    );
    const nextSelectedAppKey = selectedAppKeyIsVisible ? selectedAppKey : firstMatch.appKey;
    if (selectedAppKey !== nextSelectedAppKey) setSelectedAppKey(nextSelectedAppKey);
  }, [filteredAppOptions, hasAppSearchQuery, selectedAppKey, visibleAppTrendViewModel]);

  useLayoutEffect(() => {
    destinationListRef.current?.scrollTo({ top: 0 });
  }, [categorySearchQuery, destinationMode, hasAppSearchQuery, webSearchQuery]);

  const handleAppSearchQueryChange = (nextQuery: string) => {
    const wasSearching = appSearchQuery.trim().length > 0;
    const isSearching = nextQuery.trim().length > 0;
    setAppSearchQuery(nextQuery);
    destinationListRef.current?.scrollTo({ top: 0 });
    if (wasSearching && !isSearching) {
      setSelectedAppKey(null);
      return;
    }

    if (isSearching && visibleAppTrendViewModel) {
      const nextOptions = filterDataAppOptionsForQuery(visibleAppTrendViewModel.appOptions, nextQuery);
      const selectedAppKeyIsVisible = Boolean(
        visibleAppTrendViewModel.selectedApp
        && nextOptions.some((app) => app.appKey === visibleAppTrendViewModel.selectedApp?.appKey),
      );
      const firstMatch = nextOptions[0];
      if (!selectedAppKeyIsVisible && firstMatch) setSelectedAppKey(firstMatch.appKey);
    }
  };
  const handleCategorySelect = (category: string, multi: boolean) => {
    setSelectedCategoryKeys((current) => updateDataDestinationSelection(current, category, multi));
  };
  const handleWebDomainSelect = (domain: string, multi: boolean) => {
    setSelectedWebDomainKeys((current) => updateDataDestinationSelection(current, domain, multi));
  };
  const destinationModeOptions = useMemo<Array<{ value: DataDestinationMode; label: string }>>(() => {
    const options: Array<{ value: DataDestinationMode; label: string }> = [
      { value: "app", label: UI_TEXT.data.appTrend },
      { value: "category", label: UI_TEXT.data.categoryTrend },
    ];
    if (webEnabled) options.push({ value: "web", label: webActivityCopy.trend });
    return options;
  }, [uiLanguage, webActivityCopy.trend, webEnabled]);
  const canOpenDestinationTrendHistory = destinationGranularity === "day"
    && hasDestinationSelection
    && (destinationMode !== "app" || !appTrendSelectionHiddenBySearch)
    && Boolean(onOpenHistoryDate);
  const handleDestinationTrendMouseMove = (event: unknown) => {
    activeDestinationTrendDateRef.current = canOpenDestinationTrendHistory
      ? resolveTrendDateFromChartEvent(event, destinationChartData)
      : null;
  };
  const handleDestinationTrendDoubleClickCapture = (event: MouseEvent<HTMLDivElement>) => {
    if (!canOpenDestinationTrendHistory) return;
    event.preventDefault();
    const dateKey = activeDestinationTrendDateRef.current;
    if (dateKey) onOpenHistoryDate?.(dateKey);
  };

  return (
    <div className="qp-panel p-5 md:p-6 data-app-panel">
      <div className="data-app-panel-header">
        <div className="data-app-panel-heading">
          <h3 className="font-semibold text-[var(--qp-text-primary)] text-sm">
            {UI_TEXT.data.activityTrend}
          </h3>
          <QuietSegmentedFilter
            value={destinationMode}
            options={destinationModeOptions}
            onChange={setDestinationMode}
            ariaLabel={UI_TEXT.data.activityTrend}
            className="data-destination-mode"
          />
        </div>
        <div className="data-app-header-actions">
          <div
            className={`data-app-selected-status ${isCategoryDestination || isWebDestination ? "data-category-selected-status" : ""} ${
              hasDestinationSelection ? "" : "data-app-selected-status-empty"
            }`}
            aria-label={isWebDestination
              ? selectedWebDomains.map((domain) => domain.displayName).join(", ")
              : isCategoryDestination
                ? selectedCategories.map((category) => category.displayName).join(", ")
                : selectedAppTrendApp?.appName}
          >
            {isWebDestination ? (
              selectedWebDomains.map((domain) => (
                <span
                  key={domain.normalizedDomain}
                  className="data-web-selected-icon"
                  style={{ "--data-category-color": domain.color } as CSSProperties}
                  aria-hidden
                >
                  {domain.faviconUrl ? (
                    <img src={domain.faviconUrl} alt="" draggable={false} />
                  ) : (
                    getAppInitial(domain.displayName)
                  )}
                </span>
              ))
            ) : isCategoryDestination ? (
              selectedCategories.map((category) => (
                <span
                  key={category.category}
                  className="data-category-selected-dot"
                  style={{ "--data-category-color": category.color } as CSSProperties}
                  aria-hidden
                />
              ))
            ) : selectedAppTrendApp && icons[selectedAppTrendApp.exeName] ? (
              <img src={icons[selectedAppTrendApp.exeName]} alt="" draggable={false} />
            ) : selectedAppTrendApp ? (
              getAppInitial(selectedAppTrendApp.appName)
            ) : ""}
          </div>
          {((destinationMode === "app" && selectedAppTrendApp) || selectedWebDomain)
            && onOpenDestinationDetail ? (
            <QuietIconAction
              icon={<PanelRightOpen size={15} aria-hidden />}
              title={UI_TEXT.history.titleDetails}
              showTooltip={false}
              onClick={() => {
                if (selectedWebDomain) {
                  onOpenDestinationDetail({
                    target: createDestinationDetailTarget({
                      mode: "web",
                      key: selectedWebDomain.normalizedDomain,
                      identityKeys: [selectedWebDomain.normalizedDomain],
                      displayName: selectedWebDomain.displayName,
                      secondaryText: selectedWebDomain.normalizedDomain,
                      iconUrl: selectedWebDomain.faviconUrl,
                      color: selectedWebDomain.color,
                    }),
                    initialDateKey: formatLocalDateKey(new Date()),
                  });
                  return;
                }
                if (selectedAppTrendApp) {
                  onOpenDestinationDetail({
                    target: createDestinationDetailTarget({
                      mode: "app",
                      key: selectedAppTrendApp.appKey,
                      identityKeys: [selectedAppTrendApp.appKey, selectedAppTrendApp.exeName],
                      displayName: selectedAppTrendApp.appName,
                      secondaryText: selectedAppTrendApp.exeName,
                      iconUrl: icons[selectedAppTrendApp.exeName] ?? null,
                      color: "var(--qp-accent-default)",
                    }),
                    initialDateKey: formatLocalDateKey(new Date()),
                  });
                }
              }}
            />
          ) : null}
          <DataTrendRangeControl
            ariaLabel={isWebDestination
              ? webActivityCopy.range
              : isCategoryDestination
                ? UI_TEXT.accessibility.data.categoryTrendRange
                : UI_TEXT.accessibility.data.appTrendRange}
            selection={selection}
            onChange={onSelectionChange}
          />
        </div>
      </div>

      {destinationError && !destinationReady ? (
        <div className="data-app-loading data-web-trend-error text-[var(--qp-text-tertiary)] text-xs" role="status">
          <span>{webActivityCopy.unavailable}</span>
          <button
            type="button"
            className="qp-inline-action qp-inline-action-accent"
            onClick={() => setWebRetryKey((current) => current + 1)}
          >
            {webActivityCopy.retry}
          </button>
        </div>
      ) : !destinationReady ? (
        <div className="data-app-loading text-[var(--qp-text-tertiary)] text-xs" aria-hidden="true" />
      ) : destinationOptionsEmpty ? (
        <div className="data-app-loading text-[var(--qp-text-tertiary)] text-xs">
          {destinationEmptyLabel}
        </div>
      ) : (
        <div className="data-app-grid">
          <div className="data-app-sidebar">
            <label className="data-app-search">
              <Search size={14} aria-hidden />
              <input
                value={destinationSearchQuery}
                onChange={(event) => {
                  if (isWebDestination) {
                    setWebSearchQuery(event.target.value);
                  } else if (isCategoryDestination) {
                    setCategorySearchQuery(event.target.value);
                  } else {
                    handleAppSearchQueryChange(event.target.value);
                  }
                  destinationListRef.current?.scrollTo({ top: 0 });
                }}
                placeholder={destinationSearchPlaceholder}
                aria-label={destinationSearchPlaceholder}
              />
            </label>
            <div
              key={`${destinationMode}:${destinationSearchQuery}`}
              ref={destinationListRef}
              className="data-app-list data-app-trend-list"
              aria-label={destinationListAriaLabel}
            >
              {(isWebDestination
                ? filteredWebDomainOptions
                : isCategoryDestination ? filteredCategoryOptions : filteredAppOptions).length === 0 ? (
                <div className="data-app-empty text-[var(--qp-text-tertiary)] text-xs">
                  {destinationNoMatchLabel}
                </div>
              ) : isWebDestination ? filteredWebDomainOptions.map((domain) => {
                const isSelected = selectedWebDomainKeys.includes(domain.normalizedDomain);
                return (
                  <button
                    key={domain.normalizedDomain}
                    type="button"
                    className={`data-app-option ${isSelected ? "data-app-option-selected" : ""}`}
                    onClick={(event) => handleWebDomainSelect(
                      domain.normalizedDomain,
                      event.ctrlKey || event.metaKey,
                    )}
                    aria-pressed={isSelected}
                  >
                    <span
                      className="data-app-option-icon data-web-option-icon"
                      style={{ "--data-category-color": domain.color } as CSSProperties}
                      aria-hidden
                    >
                      {domain.faviconUrl ? (
                        <img src={domain.faviconUrl} alt="" draggable={false} />
                      ) : getAppInitial(domain.displayName)}
                    </span>
                    <span className="data-app-option-main">
                      <span className="data-app-option-name">{domain.displayName}</span>
                      <span className="data-app-option-meta">
                        {Math.round(domain.percentage)}% · {domain.normalizedDomain}
                      </span>
                    </span>
                    <span className="data-app-option-duration">{formatDuration(domain.totalDuration)}</span>
                  </button>
                );
              }) : isCategoryDestination ? filteredCategoryOptions.map((category) => {
                const isSelected = selectedCategoryKeys.includes(category.category);
                return (
                  <button
                    key={category.category}
                    type="button"
                    className={`data-app-option ${isSelected ? "data-app-option-selected" : ""}`}
                    onClick={(event) => handleCategorySelect(
                      category.category,
                      event.ctrlKey || event.metaKey,
                    )}
                    aria-pressed={isSelected}
                  >
                    <span
                      className="data-app-option-icon data-category-option-icon"
                      style={{ "--data-category-color": category.color } as CSSProperties}
                      aria-hidden
                    >
                      <span />
                    </span>
                    <span className="data-app-option-main">
                      <span className="data-app-option-name">{category.displayName}</span>
                      <span className="data-app-option-meta">
                        {Math.round(category.percentage)}% · {UI_TEXT.data.categoryAppCount(category.appCount)}
                      </span>
                    </span>
                    <span className="data-app-option-duration">{formatDuration(category.totalDuration)}</span>
                  </button>
                );
              }) : filteredAppOptions.map((app) => {
                const isSelected = selectedAppTrendApp?.appKey === app.appKey;
                return (
                  <button
                    key={app.appKey}
                    type="button"
                    className={`data-app-option ${isSelected ? "data-app-option-selected" : ""}`}
                    onClick={() => setSelectedAppKey(app.appKey)}
                    aria-pressed={isSelected}
                  >
                    <span className="data-app-option-icon" aria-hidden>
                      {icons[app.exeName]
                        ? <img src={icons[app.exeName]} alt="" draggable={false} />
                        : getAppInitial(app.appName)}
                    </span>
                    <span className="data-app-option-main">
                      <span className="data-app-option-name">{app.appName}</span>
                      <span className="data-app-option-meta">{Math.round(app.percentage)}% · {app.exeName}</span>
                    </span>
                    <span className="data-app-option-duration">{formatDuration(app.totalDuration)}</span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="data-app-chart-column">
            <div className="data-app-metric-strip">
              <div className="data-app-metric">
                <span>{UI_TEXT.data.appTrendTotal}</span>
                <strong>{formatDuration(destinationSummary?.totalDuration ?? 0)}</strong>
              </div>
              <div className="data-app-metric">
                <span>{destinationGranularity === "month" ? UI_TEXT.data.monthlyAverage : UI_TEXT.data.appTrendAverage}</span>
                <strong>{formatDuration(destinationSummary?.averageDuration ?? 0)}</strong>
              </div>
              <div className="data-app-metric">
                <span>{UI_TEXT.data.appTrendActiveDays}</span>
                <strong>{destinationSummary?.activeDayCount ?? 0}</strong>
              </div>
              <div className="data-app-metric">
                <span>{UI_TEXT.data.appTrendPeakDay}</span>
                <strong>{destinationPeakDay ? formatDuration(destinationPeakDay.duration) : "-"}</strong>
              </div>
            </div>
            <div
              ref={destinationTrendChart.chartRef}
              className={`data-app-chart ${canOpenDestinationTrendHistory ? "data-chart-openable" : ""}`}
              onMouseDownCapture={(event) => {
                if (canOpenDestinationTrendHistory && event.detail > 1) event.preventDefault();
              }}
              onDoubleClickCapture={handleDestinationTrendDoubleClickCapture}
            >
              <ResponsiveContainer
                width="100%"
                height="100%"
                initialDimension={destinationTrendChart.initialDimension}
              >
                <AreaChart
                  data={destinationChartData}
                  margin={{ top: 10, right: 18, left: -20, bottom: 0 }}
                  onMouseMove={handleDestinationTrendMouseMove}
                  onMouseLeave={() => {
                    activeDestinationTrendDateRef.current = null;
                  }}
                >
                  <CartesianGrid strokeDasharray="3 3" stroke="var(--qp-border-subtle)" strokeOpacity={0.58} />
                  <XAxis
                    dataKey="label"
                    tick={{ fontSize: 10, fill: "var(--qp-text-tertiary)" }}
                    axisLine={false}
                    tickLine={false}
                    interval="preserveStartEnd"
                    minTickGap={DATA_TREND_X_AXIS_MIN_TICK_GAP}
                  />
                  <YAxis
                    tick={{ fontSize: 10, fill: "var(--qp-text-tertiary)" }}
                    axisLine={false}
                    tickLine={false}
                    ticks={destinationChartAxis.ticks}
                    domain={[0, destinationChartAxis.domainMax]}
                    tickFormatter={(value) => formatChartHours(Number(value))}
                  />
                  <QuietChartTooltip
                    formatter={(value, name) => [
                      formatDuration(Number(value) * 3600000),
                      destinationMode === "app" ? UI_TEXT.data.appTrendUsage : String(name),
                    ]}
                  />
                  {destinationMode !== "app" ? (
                    destinationChartSeries.map((series) => (
                      <Area
                        key={series.key}
                        type="monotone"
                        dataKey={series.dataKey}
                        name={series.displayName}
                        stroke={series.color}
                        strokeWidth={2}
                        fill={series.color}
                        fillOpacity={0.08}
                        dot={{ fill: series.color, r: 2.5 }}
                        isAnimationActive={false}
                      />
                    ))
                  ) : (
                    <Area
                      type="monotone"
                      dataKey="hours"
                      stroke="var(--qp-accent-default)"
                      strokeWidth={2}
                      fill="var(--qp-accent-default)"
                      fillOpacity={0.1}
                      dot={{ fill: "var(--qp-accent-default)", r: 2.5 }}
                      isAnimationActive={false}
                    />
                  )}
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
