// Generated from patina-protocol. Run npm run generate:protocol; do not edit.
// Wire types only: untrusted input still requires runtime validation.

export type WebActivityUrlPrivacyMode = "full" | "strip_query" | "domain_only";

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

