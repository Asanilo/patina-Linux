use crate::data::repositories::web_activity::{
    end_active_segment, load_domain_recording_enabled,
    repair_active_segment_after_restart as repair_active_segment_row_after_restart,
    seal_active_segment_if_not_updated_after, upsert_active_segment, WebActivitySegmentInput,
};
use crate::domain::settings::WebActivitySettings;
use crate::domain::web_activity::{
    is_supported_browser_exe, sanitize_active_tab_payload, sanitize_browser_client_id,
    sanitize_browser_kind, sanitize_extension_version, BrowserActiveTabPayload,
    WebActivityBridgeSnapshot,
};
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot;
use serde_json::json;
use sqlx::{Pool, Sqlite};
use std::sync::Mutex;
use std::time::Duration;

const BROWSER_BRIDGE_STALE_AFTER_MS: i64 = 75_000;
pub const WEB_ACTIVITY_STALE_CHECK_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Default)]
struct WebActivityClientSnapshot {
    listening: bool,
    browser_client_id: Option<String>,
    browser_kind: Option<String>,
    extension_version: Option<String>,
    last_activity_at_ms: Option<i64>,
}

#[derive(Debug, Default)]
pub struct WebActivityRuntimeState {
    inner: Mutex<WebActivityClientSnapshot>,
}

impl WebActivityRuntimeState {
    pub fn set_listening(&self, listening: bool) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.listening = listening;
    }

    pub fn observe_active_tab(&self, payload: &BrowserActiveTabPayload, now_ms: i64) {
        self.update_client(
            Some(sanitize_browser_client_id(
                payload.browser_client_id.as_deref(),
            )),
            Some(sanitize_browser_kind(payload.browser_kind.as_deref())),
            sanitize_extension_version(payload.extension_version.as_deref()),
            now_ms,
        );
    }

    pub fn snapshot(
        &self,
        settings: &WebActivitySettings,
        now_ms: i64,
    ) -> WebActivityBridgeSnapshot {
        let client = match self.inner.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let connected = client
            .last_activity_at_ms
            .map(|last| now_ms.saturating_sub(last) <= BROWSER_BRIDGE_STALE_AFTER_MS)
            .unwrap_or(false);

        WebActivityBridgeSnapshot {
            enabled: settings.enabled,
            listening: client.listening,
            connected,
            browser_client_id: client.browser_client_id,
            browser_kind: client.browser_kind,
            extension_version: client.extension_version,
            last_activity_at_ms: client.last_activity_at_ms,
        }
    }

    fn stale_activity_boundary(&self, now_ms: i64) -> Option<i64> {
        let last_activity_at_ms = match self.inner.lock() {
            Ok(guard) => guard.last_activity_at_ms,
            Err(poisoned) => poisoned.into_inner().last_activity_at_ms,
        }?;
        (now_ms.saturating_sub(last_activity_at_ms) > BROWSER_BRIDGE_STALE_AFTER_MS)
            .then_some(last_activity_at_ms)
    }

    fn update_client(
        &self,
        browser_client_id: Option<String>,
        browser_kind: Option<String>,
        extension_version: Option<String>,
        now_ms: i64,
    ) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if browser_client_id.is_some() {
            guard.browser_client_id = browser_client_id;
        }
        if browser_kind.is_some() {
            guard.browser_kind = browser_kind;
        }
        if extension_version.is_some() {
            guard.extension_version = extension_version;
        }
        guard.last_activity_at_ms = Some(now_ms);
    }
}

pub async fn record_active_tab(
    pool: &Pool<Sqlite>,
    settings: &WebActivitySettings,
    state: &WebActivityRuntimeState,
    tracking_snapshot: Option<TrackingRuntimeSnapshot>,
    payload: BrowserActiveTabPayload,
    now_ms: i64,
) -> Result<bool, String> {
    state.observe_active_tab(&payload, now_ms);

    if !settings.enabled {
        return seal_active_segment(pool, now_ms).await;
    }

    let Some(sanitized) = sanitize_active_tab_payload(payload)? else {
        return seal_active_segment(pool, now_ms).await;
    };
    if !load_domain_recording_enabled(pool, &sanitized.normalized_domain)
        .await
        .map_err(|error| format!("failed to load web domain override: {error}"))?
    {
        return seal_active_segment(pool, now_ms).await;
    }

    let Some(snapshot) = tracking_snapshot else {
        return seal_active_segment(pool, now_ms).await;
    };
    if !snapshot.status.is_tracking_active
        || snapshot.window.is_afk
        || !is_supported_browser_exe(&snapshot.window.exe_name)
    {
        return seal_active_segment(pool, now_ms).await;
    }

    let input = WebActivitySegmentInput::from_sanitized(
        sanitized,
        snapshot.window.exe_name.trim().to_ascii_lowercase(),
    );
    upsert_active_segment(pool, &input, now_ms)
        .await
        .map_err(|error| format!("failed to save web activity: {error}"))
}

pub async fn seal_if_tracking_inactive(
    pool: &Pool<Sqlite>,
    tracking_snapshot: Option<TrackingRuntimeSnapshot>,
    now_ms: i64,
) -> Result<bool, String> {
    let should_seal = tracking_snapshot
        .map(|snapshot| {
            !snapshot.status.is_tracking_active
                || snapshot.window.is_afk
                || !is_supported_browser_exe(&snapshot.window.exe_name)
        })
        .unwrap_or(true);

    if should_seal {
        return seal_active_segment(pool, now_ms).await;
    }

    Ok(false)
}

pub async fn handle_http_request(
    context: &RuntimeContext,
    state: &WebActivityRuntimeState,
    tracking_snapshot: Option<TrackingRuntimeSnapshot>,
    event_sink: &dyn RuntimeEventSink,
    request: WebActivityBridgeHttpRequest,
) -> WebActivityBridgeHttpResponse {
    if !request.method.eq_ignore_ascii_case("POST") {
        return http_response(
            405,
            false,
            "method-not-allowed",
            "unsupported web activity method",
        );
    }
    if request.path != "/web-activity" {
        return http_response(404, false, "not-found", "unsupported web activity path");
    }

    let now_ms = context.now_ms();
    let settings =
        match crate::data::repositories::app_settings::load_web_activity_settings(context.pool())
            .await
        {
            Ok(settings) => settings,
            Err(error) => {
                return http_response(
                    500,
                    false,
                    "settings-unavailable",
                    &format!("failed to load web activity settings: {error}"),
                );
            }
        };

    let token = bearer_token(request.authorization.as_deref());
    if settings.token.is_empty() || token.as_deref() != Some(settings.token.as_str()) {
        return http_response(401, false, "unauthorized", "invalid web activity token");
    }

    if !settings.enabled {
        let _ = seal_active_segment(context.pool(), now_ms).await;
        return WebActivityBridgeHttpResponse::json(
            409,
            json!({
                "ok": false,
                "enabled": false,
                "code": "web-recording-disabled",
                "message": "Patina web recording is off.",
                "serverTimeMs": now_ms,
            }),
        );
    }

    let payload = match serde_json::from_slice::<BrowserActiveTabPayload>(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            return http_response(
                400,
                false,
                "invalid-payload",
                &format!("invalid active tab: {error}"),
            );
        }
    };

    match record_active_tab(
        context.pool(),
        &settings,
        state,
        tracking_snapshot,
        payload,
        now_ms,
    )
    .await
    {
        Ok(changed) => {
            if changed {
                let _ = event_sink.emit(RuntimeEvent::TrackingDataChanged {
                    reason: crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON.to_string(),
                    changed_at_ms: now_ms.max(0) as u64,
                });
            }
            WebActivityBridgeHttpResponse::json(
                200,
                json!({
                    "ok": true,
                    "enabled": true,
                    "changed": changed,
                    "serverTimeMs": now_ms,
                }),
            )
        }
        Err(error) => http_response(400, false, "record-failed", &error),
    }
}

fn bearer_token(authorization: Option<&str>) -> Option<String> {
    let value = authorization?.trim();
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or(value)
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn http_response(
    status: u16,
    ok: bool,
    code: &str,
    message: &str,
) -> WebActivityBridgeHttpResponse {
    WebActivityBridgeHttpResponse::json(
        status,
        json!({ "ok": ok, "code": code, "message": message }),
    )
}

pub async fn seal_active_segment(pool: &Pool<Sqlite>, now_ms: i64) -> Result<bool, String> {
    end_active_segment(pool, now_ms)
        .await
        .map_err(|error| format!("failed to seal web activity: {error}"))
}

pub async fn repair_active_segment_after_restart(
    pool: &Pool<Sqlite>,
    restart_time_ms: i64,
) -> Result<Option<i64>, String> {
    repair_active_segment_row_after_restart(pool, restart_time_ms)
        .await
        .map_err(|error| format!("failed to repair web activity after restart: {error}"))
}

pub async fn seal_stale_active_segment(
    pool: &Pool<Sqlite>,
    state: &WebActivityRuntimeState,
    now_ms: i64,
) -> Result<Option<i64>, String> {
    let Some(stale_boundary_ms) = state.stale_activity_boundary(now_ms) else {
        return Ok(None);
    };
    seal_active_segment_if_not_updated_after(pool, stale_boundary_ms)
        .await
        .map_err(|error| format!("failed to seal stale web activity: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::app_settings::{
        commit_app_setting_mutations, AppSettingMutation,
    };
    use crate::data::schema as db_schema;
    use crate::domain::tracking::TrackingStatusSnapshot;
    use crate::engine::runtime_event::MemoryRuntimeEventSink;
    use crate::engine::tracking::runtime_snapshot::{
        TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
    };
    #[cfg(target_os = "linux")]
    use crate::platform::linux::foreground::WindowInfo;
    #[cfg(target_os = "windows")]
    use crate::platform::windows::foreground::WindowInfo;
    use sqlx::{Executor, SqlitePool};
    use std::sync::Arc;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::WEB_ACTIVITY_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    struct FixedClock(i64);

    impl crate::engine::runtime_context::RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    fn browser_tracking_snapshot() -> TrackingRuntimeSnapshot {
        TrackingRuntimeSnapshot {
            generation: 0,
            window: WindowInfo {
                hwnd: "0x100".into(),
                root_owner_hwnd: "0x100".into(),
                process_id: 42,
                window_class: "zen".into(),
                title: "Example".into(),
                exe_name: "zen".into(),
                process_path: "/usr/bin/zen".into(),
                is_afk: false,
                idle_time_ms: 0,
            },
            status: TrackingStatusSnapshot {
                is_tracking_active: true,
                ..TrackingStatusSnapshot::default()
            },
            sampled_at_ms: 2_000,
            probe_status: TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
        }
    }

    fn active_tab_request(token: &str) -> WebActivityBridgeHttpRequest {
        WebActivityBridgeHttpRequest {
            method: "POST".into(),
            path: "/web-activity".into(),
            authorization: Some(format!("Bearer {token}")),
            body: serde_json::to_vec(&serde_json::json!({
                "browserClientId": "zen-profile",
                "browserKind": "firefox",
                "extensionVersion": "0.1.0",
                "url": "https://example.com/work",
                "title": "Example",
                "incognito": false
            }))
            .unwrap(),
        }
    }

    #[tokio::test]
    async fn host_neutral_http_handler_authenticates_and_records_browser_activity() {
        let pool = setup_test_db().await;
        commit_app_setting_mutations(
            &pool,
            &[
                AppSettingMutation {
                    key: "web_activity_enabled".into(),
                    value: "1".into(),
                },
                AppSettingMutation {
                    key: "web_activity_token".into(),
                    value: "secret".into(),
                },
            ],
        )
        .await
        .unwrap();
        let context = RuntimeContext::new(pool.clone(), Arc::new(FixedClock(2_000)));
        let state = WebActivityRuntimeState::default();
        let events = MemoryRuntimeEventSink::default();

        let unauthorized = handle_http_request(
            &context,
            &state,
            Some(browser_tracking_snapshot()),
            &events,
            active_tab_request("wrong"),
        )
        .await;
        assert_eq!(unauthorized.status, 401);

        let response = handle_http_request(
            &context,
            &state,
            Some(browser_tracking_snapshot()),
            &events,
            active_tab_request("secret"),
        )
        .await;
        assert_eq!(response.status, 200);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.body).unwrap()["changed"],
            true
        );
        let domain: String =
            sqlx::query_scalar("SELECT normalized_domain FROM web_activity_segments LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(domain, "example.com");
        assert_eq!(events.events().len(), 1);
    }

    #[test]
    fn inactive_settings_seal_existing_web_segment() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let input = WebActivitySegmentInput {
                browser_client_id: "client".into(),
                browser_kind: "chrome".into(),
                browser_exe_name: "chrome.exe".into(),
                domain: "github.com".into(),
                normalized_domain: "github.com".into(),
                url: None,
                title: Some("Issue".into()),
                favicon_url: None,
            };
            upsert_active_segment(&pool, &input, 1_000).await.unwrap();
            assert!(seal_active_segment(&pool, 2_000).await.unwrap());

            let duration: Option<i64> =
                sqlx::query_scalar("SELECT duration FROM web_activity_segments LIMIT 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(duration, Some(1_000));
        });
    }

    #[test]
    fn bridge_snapshot_marks_recent_client_connected() {
        let state = WebActivityRuntimeState::default();
        state.observe_active_tab(
            &BrowserActiveTabPayload {
                browser_client_id: Some("client".into()),
                browser_kind: Some("chrome".into()),
                extension_version: Some("0.1.0".into()),
                tab_id: Some(1),
                window_id: Some(1),
                url: Some("https://example.com".into()),
                title: Some("Example".into()),
                fav_icon_url: None,
                incognito: Some(false),
                captured_at_ms: Some(1_000),
                event_reason: Some("activated".into()),
            },
            1_000,
        );

        let snapshot = state.snapshot(
            &WebActivitySettings {
                enabled: true,
                token: "secret".into(),
                url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
            },
            2_000,
        );

        assert!(snapshot.connected);
        assert!(!snapshot.listening);
        assert_eq!(snapshot.browser_kind.as_deref(), Some("chrome"));

        let delayed_heartbeat_snapshot = state.snapshot(
            &WebActivitySettings {
                enabled: true,
                token: "secret".into(),
                url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
            },
            61_000,
        );
        assert!(delayed_heartbeat_snapshot.connected);

        let stale_snapshot = state.snapshot(
            &WebActivitySettings {
                enabled: true,
                token: "secret".into(),
                url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode::Full,
            },
            76_001,
        );
        assert!(!stale_snapshot.connected);
    }

    #[test]
    fn stale_watchdog_seals_at_the_last_browser_observation() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            let input = WebActivitySegmentInput {
                browser_client_id: "client".into(),
                browser_kind: "chrome".into(),
                browser_exe_name: "chrome.exe".into(),
                domain: "github.com".into(),
                normalized_domain: "github.com".into(),
                url: None,
                title: Some("Issue".into()),
                favicon_url: None,
            };
            upsert_active_segment(&pool, &input, 1_000).await.unwrap();
            upsert_active_segment(&pool, &input, 2_000).await.unwrap();

            let state = WebActivityRuntimeState::default();
            state.observe_active_tab(
                &BrowserActiveTabPayload {
                    browser_client_id: Some("client".into()),
                    browser_kind: Some("chrome".into()),
                    extension_version: Some("0.1.0".into()),
                    tab_id: Some(1),
                    window_id: Some(1),
                    url: Some("https://github.com".into()),
                    title: Some("Issue".into()),
                    fav_icon_url: None,
                    incognito: Some(false),
                    captured_at_ms: Some(2_000),
                    event_reason: Some("heartbeat".into()),
                },
                2_000,
            );

            assert_eq!(
                seal_stale_active_segment(&pool, &state, 77_000)
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                seal_stale_active_segment(&pool, &state, 77_001)
                    .await
                    .unwrap(),
                Some(2_000)
            );

            let duration: Option<i64> =
                sqlx::query_scalar("SELECT duration FROM web_activity_segments LIMIT 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(duration, Some(1_000));
        });
    }
}
mod http_contract;

pub use http_contract::{WebActivityBridgeHttpRequest, WebActivityBridgeHttpResponse};
