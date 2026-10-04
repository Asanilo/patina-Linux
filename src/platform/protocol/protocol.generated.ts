// Generated from patina-protocol. Run npm run generate:protocol; do not edit.
// Wire types only: untrusted input still requires runtime validation.

export type WebActivityUrlPrivacyMode = "full" | "strip_query" | "domain_only";

export type ActivityReadStatus = "healthy" | "stale" | "unavailable";

export type ActivityReadHealth = { status: ActivityReadStatus, last_heartbeat_ms: number | null, live_cutoff_ms: number, stale_after_ms: number, };

export type ExactActivityOrigin = "native" | "import_exact";

export type ExactTitleSample = { title: string, start_ms: number, end_ms: number, };

export type ExactActivityRecord = { origin: ExactActivityOrigin, record_id: number, app_key: string, app_name: string, exe_name: string, category: string, display_name_override: string | null, window_title: string, start_ms: number, end_ms: number, continuity_start_ms: number, is_open: boolean, title_samples: Array<ExactTitleSample>, };

export type ExactHistorySnapshot = { from_ms: number, to_ms: number, sampled_at_ms: number, configuration_revision: string, tracking_health: ActivityReadHealth, records: Array<ExactActivityRecord>, };

export type ActivityCategoryTotal = { category: string, active_ms: number, };

export type ActivityHour = { hour: number, active_ms: number, categories: Array<ActivityCategoryTotal>, };

export type HistoryProductSnapshot = { history: ExactHistorySnapshot, hours: Array<ActivityHour>, };

export type ProductSettings = { idle_timeout_secs: number, timeline_merge_gap_secs: number, min_session_secs: number, tracking_paused: boolean, audio_participation_enabled: boolean, web_activity_enabled: boolean, web_activity_port: number, web_activity_token_present: boolean, web_activity_url_privacy: WebActivityUrlPrivacyMode, };

export type ProductSettingsSnapshot = { revision: string, sampled_at_ms: number, settings: ProductSettings, last_heartbeat_ms: number | null, last_successful_sample_ms: number | null, };

export type ProductSettingsPatch = { idle_timeout_secs?: number | null, timeline_merge_gap_secs?: number | null, min_session_secs?: number | null, tracking_paused?: boolean | null, };

export type ProductSettingsCommitRequest = { expected_revision: string, patch: ProductSettingsPatch, };

export type ClassificationEntry = { key: string, value: string, };

export type ClassificationSnapshot = { revision: string, sampled_at_ms: number, entries: Array<ClassificationEntry>, };

export type ClassificationMutationRequest = { key: string, value: string | null, };

export type ClassificationMutationsRequest = { mutations: Array<ClassificationMutationRequest>, expected_revision?: string | null, };

export type ClassificationCommitResult = { ok: boolean, revision?: string | null, };

export type CachedIcon = { source_key: string, keys: Array<string>, data_url: string, };

export type IconPage = { entries: Array<CachedIcon>, next_after: string | null, };

export type IconLookup = { requested_key: string, icon: CachedIcon | null, };

export type BrowserResourceSettings = { enabled: boolean, port: number, token_present: boolean, url_privacy: WebActivityUrlPrivacyMode, };

export type ResourceSettingsSnapshot = { revision: string, sampled_at_ms: number, audio_participation_enabled: boolean, browser_activity: BrowserResourceSettings, };

export type BrowserResourcePatch = { enabled?: boolean | null, port?: number | null, token?: string | null, url_privacy?: WebActivityUrlPrivacyMode | null, };

export type ResourceSettingsPatch = { audio_participation_enabled?: boolean | null, browser_activity?: BrowserResourcePatch | null, };

export type ResourceSettingsCommitRequest = { expected_revision: string, patch: ResourceSettingsPatch, };

