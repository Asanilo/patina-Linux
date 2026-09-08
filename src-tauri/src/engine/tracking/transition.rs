use crate::data::tracking_runtime::{TrackingRuntimeDataError, TrackingRuntimeDataStore};
use crate::domain::tracking::{
    self, WindowSessionIdentity, WindowTrackingCandidate, WindowTransitionDecision,
};
#[cfg(target_os = "linux")]
use crate::platform::linux::foreground as tracker;
#[cfg(target_os = "windows")]
use crate::platform::windows::foreground as tracker;
use std::future::Future;
use std::pin::Pin;

pub(crate) type StartSessionFn = for<'a> fn(
    data: &'a TrackingRuntimeDataStore,
    window: &'a tracker::WindowInfo,
    start_time: i64,
    continuity_group_start_time: i64,
) -> Pin<
    Box<dyn Future<Output = Result<bool, TrackingRuntimeDataError>> + Send + 'a>,
>;

pub(crate) async fn apply_window_transition(
    data: &TrackingRuntimeDataStore,
    previous_window: Option<&tracker::WindowInfo>,
    next_window: &tracker::WindowInfo,
    now_ms: i64,
    next_continuity_group_start_time: i64,
    start_session: StartSessionFn,
) -> Result<Option<&'static str>, TrackingRuntimeDataError> {
    let decision = plan_window_transition(previous_window, next_window, now_ms);
    if !decision.has_mutation_plan() {
        return recover_missing_active_session(
            data,
            next_window,
            now_ms,
            next_continuity_group_start_time,
            start_session,
        )
        .await;
    }

    let mut did_mutate = false;

    if decision.should_end_previous {
        did_mutate |= data
            .end_active_sessions(decision.resolved_end_time(now_ms))
            .await?;
    }

    if decision.should_start_next {
        did_mutate |=
            start_session(data, next_window, now_ms, next_continuity_group_start_time).await?;
    }

    if decision.should_refresh_metadata {
        did_mutate |= data
            .refresh_active_session_metadata(&next_window.exe_name, &next_window.title, now_ms)
            .await?;
    }

    Ok(decision.mutation_reason(did_mutate))
}

pub(crate) async fn recover_missing_active_session(
    data: &TrackingRuntimeDataStore,
    window: &tracker::WindowInfo,
    now_ms: i64,
    continuity_group_start_time: i64,
    start_session: StartSessionFn,
) -> Result<Option<&'static str>, TrackingRuntimeDataError> {
    if !is_trackable_window(Some(window)) {
        return Ok(None);
    }

    if let Some(active) = data.load_active_session().await? {
        if active.exe_name.eq_ignore_ascii_case(&window.exe_name) {
            return Ok(data
                .refresh_active_session_metadata(&window.exe_name, &window.title, now_ms)
                .await?
                .then_some("session-metadata-refreshed"));
        }
    }

    if start_session(data, window, now_ms, continuity_group_start_time).await? {
        return Ok(Some("session-recovered"));
    }

    Ok(None)
}

pub(crate) fn plan_window_transition(
    previous_window: Option<&tracker::WindowInfo>,
    next_window: &tracker::WindowInfo,
    now_ms: i64,
) -> WindowTransitionDecision {
    let last_trackable = is_trackable_window(previous_window);
    let next_trackable = is_trackable_window(Some(next_window));
    let previous_identity = resolve_window_session_identity(previous_window);
    let next_identity = resolve_window_session_identity(Some(next_window));
    let app_changed = match (previous_identity.as_ref(), next_identity.as_ref()) {
        (Some(previous), Some(next)) => !previous.is_same_app(next),
        _ => last_trackable != next_trackable,
    };
    let instance_changed = match (previous_identity.as_ref(), next_identity.as_ref()) {
        (Some(previous), Some(next)) => !previous.is_same_instance(next),
        _ => false,
    };
    let tracking_state_changed = last_trackable != next_trackable;
    let did_change = app_changed || tracking_state_changed;
    let should_end_previous = last_trackable && did_change;
    let should_start_next = next_trackable && did_change;
    let title_changed = previous_window
        .map(|window| window.title != next_window.title)
        .unwrap_or(false);
    let should_refresh_metadata =
        !did_change && next_trackable && (title_changed || instance_changed);
    let reason = if app_changed {
        "session-transition-app-change"
    } else if tracking_state_changed {
        "session-transition-state-change"
    } else if should_refresh_metadata {
        "session-metadata-refreshed"
    } else if instance_changed {
        "session-instance-unchanged-app"
    } else {
        "session-no-change"
    };

    WindowTransitionDecision {
        reason,
        should_end_previous,
        should_start_next,
        should_refresh_metadata,
        end_time_override: if should_end_previous && !next_trackable && next_window.is_afk {
            Some(now_ms - i64::from(next_window.idle_time_ms))
        } else {
            None
        },
    }
}

pub(crate) fn resolve_window_session_identity(
    window: Option<&tracker::WindowInfo>,
) -> Option<WindowSessionIdentity> {
    let window = window?;
    if !is_trackable_window(Some(window)) {
        return None;
    }

    WindowSessionIdentity::from_window_fields(
        &window.exe_name,
        window.process_id,
        &window.root_owner_hwnd,
        &window.hwnd,
        &window.window_class,
    )
}

pub(crate) fn is_trackable_window(window: Option<&tracker::WindowInfo>) -> bool {
    tracking::is_trackable_window(window.map(to_tracking_candidate))
}

fn to_tracking_candidate(window: &tracker::WindowInfo) -> WindowTrackingCandidate<'_> {
    WindowTrackingCandidate::from_window_fields(
        &window.exe_name,
        &window.title,
        &window.window_class,
        window.is_afk,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{repositories::sessions, schema, tracking_runtime::TrackingRuntimeDataStore};
    use crate::engine::tracking::active_session::start_session_for_transition;
    use sqlx::{Executor, SqlitePool};

    fn window(exe_name: &str, title: &str) -> tracker::WindowInfo {
        tracker::WindowInfo {
            hwnd: "1".to_string(),
            root_owner_hwnd: "1".to_string(),
            process_id: 1,
            window_class: "Window".to_string(),
            title: title.to_string(),
            exe_name: exe_name.to_string(),
            process_path: String::new(),
            is_afk: false,
            idle_time_ms: 0,
        }
    }

    #[tokio::test]
    async fn recovery_reconciles_a_stale_active_app_with_the_latest_window() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        sessions::start_session(&pool, "A", "a", "A", 1_000, 1_000)
            .await
            .unwrap();
        let data = TrackingRuntimeDataStore::new(pool.clone());
        let current = window("b", "B");

        let reason = recover_missing_active_session(
            &data,
            &current,
            5_000,
            5_000,
            start_session_for_transition,
        )
        .await
        .unwrap();

        assert_eq!(reason, Some("session-recovered"));
        let rows: Vec<(String, i64, Option<i64>)> =
            sqlx::query_as("SELECT exe_name, start_time, end_time FROM sessions ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                ("a".to_string(), 1_000, Some(5_000)),
                ("b".to_string(), 5_000, None),
            ]
        );
        pool.close().await;
    }
}
