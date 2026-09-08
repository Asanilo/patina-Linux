use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::tools::{ToolAlert, ToolsRuntimeSnapshot};
use crate::engine::runtime_context::RuntimeContext;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

mod notification;
pub mod runtime;

pub use runtime::{
    CreateSoftwareReminderRuleRequest, StartPomodoroRequest, StartTimerRequest, ToolsRuntimeOwner,
    ToolsRuntimeSink,
};

pub const TOOLS_RUNTIME_CHANGED_EVENT: &str = "tools-runtime-changed";
pub const TOOLS_ALERT_EVENT: &str = "tools-alert";

#[derive(Debug, Default)]
pub struct ToolsRuntimeState {
    inner: Mutex<ToolsRuntimeSnapshot>,
    alerts: Mutex<Vec<ToolAlert>>,
    ready: AtomicBool,
}

impl ToolsRuntimeState {
    pub(crate) fn replace(&self, snapshot: ToolsRuntimeSnapshot) {
        match self.inner.lock() {
            Ok(mut guard) => *guard = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
        self.ready.store(true, Ordering::Release);
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub(crate) fn push_alert(&self, alert: ToolAlert) {
        match self.alerts.lock() {
            Ok(mut guard) => push_unique_alert(&mut guard, alert),
            Err(poisoned) => push_unique_alert(&mut poisoned.into_inner(), alert),
        }
    }

    fn alerts(&self) -> Vec<ToolAlert> {
        match self.alerts.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn dismiss_alert(&self, alert_id: &str) {
        match self.alerts.lock() {
            Ok(mut guard) => guard.retain(|alert| alert.id != alert_id),
            Err(poisoned) => poisoned.into_inner().retain(|alert| alert.id != alert_id),
        }
    }
}

fn push_unique_alert(alerts: &mut Vec<ToolAlert>, alert: ToolAlert) {
    if !alerts.iter().any(|existing| existing.id == alert.id) {
        alerts.push(alert);
    }
}

struct TauriToolsRuntimeSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime + 'static> ToolsRuntimeSink for TauriToolsRuntimeSink<R> {
    fn snapshot_changed(&self, snapshot: &ToolsRuntimeSnapshot) {
        if let Some(state) = self.app.try_state::<ToolsRuntimeState>() {
            state.replace(snapshot.clone());
        }
        if let Err(error) = self.app.emit(TOOLS_RUNTIME_CHANGED_EVENT, snapshot) {
            eprintln!("[tools] failed to emit tools snapshot: {error}");
        }
    }

    fn alert(&self, alert: &ToolAlert) {
        deliver_alert_to_desktop(&self.app, alert);
    }
}

pub(crate) fn deliver_alert_to_desktop<R: Runtime>(app: &AppHandle<R>, alert: &ToolAlert) {
    if let Some(state) = app.try_state::<ToolsRuntimeState>() {
        state.push_alert(alert.clone());
    }
    crate::app::main_window::show_main_window(app);
    if let Err(error) = app.emit(TOOLS_ALERT_EVENT, alert) {
        eprintln!(
            "[tools] failed to emit tool alert, falling back to system notification: {error}"
        );
        if let Err(error) = notification::send(app, &alert.title, &alert.body) {
            eprintln!("[tools] failed to send fallback notification: {error}");
        }
    }
}

async fn runtime_owner<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeOwner, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    let sink: Arc<dyn ToolsRuntimeSink> = Arc::new(TauriToolsRuntimeSink { app: app.clone() });
    Ok(ToolsRuntimeOwner::new(RuntimeContext::system(pool), sink))
}

pub async fn run<R: Runtime + 'static>(app: AppHandle<R>) -> Result<(), String> {
    let owner = runtime_owner(&app).await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    owner.run_with_shutdown(shutdown_rx).await
}

pub async fn get_snapshot<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    load_snapshot(app).await
}

pub async fn get_snapshot_from_pool(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    now_ms: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    crate::data::repositories::tools::fetch_tools_snapshot(
        pool,
        now_ms,
        &runtime::date_key_at(now_ms),
    )
    .await
}

pub fn get_alerts<R: Runtime>(app: &AppHandle<R>) -> Vec<ToolAlert> {
    app.try_state::<ToolsRuntimeState>()
        .map(|state| state.alerts())
        .unwrap_or_default()
}

pub fn dismiss_alert<R: Runtime>(app: &AppHandle<R>, alert_id: &str) {
    if let Some(state) = app.try_state::<ToolsRuntimeState>() {
        state.dismiss_alert(alert_id);
    }
}

pub async fn create_reminder<R: Runtime + 'static>(
    app: &AppHandle<R>,
    label: String,
    scheduled_at: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app)
        .await?
        .create_reminder(label, scheduled_at)
        .await
}

pub async fn cancel_reminder<R: Runtime + 'static>(
    app: &AppHandle<R>,
    reminder_id: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.cancel_reminder(reminder_id).await
}

pub async fn create_software_reminder_rule<R: Runtime + 'static>(
    app: &AppHandle<R>,
    request: CreateSoftwareReminderRuleRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app)
        .await?
        .create_software_reminder_rule(request)
        .await
}

pub async fn disable_software_reminder_rule<R: Runtime + 'static>(
    app: &AppHandle<R>,
    rule_id: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app)
        .await?
        .disable_software_reminder_rule(rule_id)
        .await
}

pub async fn start_timer<R: Runtime + 'static>(
    app: &AppHandle<R>,
    request: StartTimerRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.start_timer(request).await
}

pub async fn pause_timer<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.pause_timer().await
}

pub async fn resume_timer<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.resume_timer().await
}

pub async fn reset_timer<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.reset_timer().await
}

pub async fn add_timer_lap<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.add_timer_lap().await
}

pub async fn start_pomodoro<R: Runtime + 'static>(
    app: &AppHandle<R>,
    request: StartPomodoroRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.start_pomodoro(request).await
}

pub async fn pause_pomodoro<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.pause_pomodoro().await
}

pub async fn resume_pomodoro<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.resume_pomodoro().await
}

pub async fn skip_pomodoro_phase<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.skip_pomodoro_phase().await
}

pub async fn reset_pomodoro<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    runtime_owner(app).await?.reset_pomodoro().await
}

async fn load_snapshot<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let snapshot = runtime_owner(app).await?.snapshot().await?;
    if let Some(state) = app.try_state::<ToolsRuntimeState>() {
        state.replace(snapshot.clone());
    }
    Ok(snapshot)
}

#[allow(dead_code)]
async fn refresh_snapshot<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let snapshot = load_snapshot(app).await?;
    if let Err(error) = app.emit(TOOLS_RUNTIME_CHANGED_EVENT, &snapshot) {
        eprintln!("[tools] failed to emit tools snapshot: {error}");
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tools::ToolAlertKind;

    #[test]
    fn tool_alerts_are_queued_once_and_dismissed_by_id() {
        let state = ToolsRuntimeState::default();
        let alert = ToolAlert {
            id: "reminder:1".to_string(),
            kind: ToolAlertKind::Reminder,
            title: "提醒".to_string(),
            body: "时间到了".to_string(),
            occurred_at: 1_000,
        };

        state.push_alert(alert.clone());
        state.push_alert(alert);
        assert_eq!(state.alerts().len(), 1);
        state.dismiss_alert("reminder:1");
        assert!(state.alerts().is_empty());
    }

    #[test]
    fn runtime_state_becomes_ready_after_first_snapshot() {
        let state = ToolsRuntimeState::default();
        assert!(!state.is_ready());

        state.replace(ToolsRuntimeSnapshot::default());

        assert!(state.is_ready());
    }
}
