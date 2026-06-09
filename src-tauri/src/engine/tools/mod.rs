use crate::data::repositories;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::tools::{
    PomodoroPhase, TimerMode, ToolAlert, ToolAlertKind, ToolsRuntimeSnapshot,
};
#[cfg(test)]
use crate::domain::tools::{PomodoroStatus, TimerStatus};
use chrono::Local;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::time::{sleep, Duration};

mod notification;

pub const TOOLS_RUNTIME_CHANGED_EVENT: &str = "tools-runtime-changed";
pub const TOOLS_ALERT_EVENT: &str = "tools-alert";
const TOOLS_RUNTIME_TICK_MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ToolsTickOutcome {
    state_changed: bool,
}

impl ToolsTickOutcome {
    fn changed() -> Self {
        Self {
            state_changed: true,
        }
    }

    fn mark_changed(&mut self) {
        self.state_changed = true;
    }

    fn merge(&mut self, next: Self) {
        self.state_changed |= next.state_changed;
    }
}

#[derive(Debug, Default)]
pub struct ToolsRuntimeState {
    inner: Mutex<ToolsRuntimeSnapshot>,
    alerts: Mutex<Vec<ToolAlert>>,
}

impl ToolsRuntimeState {
    fn replace(&self, snapshot: ToolsRuntimeSnapshot) {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = snapshot;
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = snapshot;
            }
        }
    }

    fn push_alert(&self, alert: ToolAlert) {
        match self.alerts.lock() {
            Ok(mut guard) => push_unique_alert(&mut guard, alert),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                push_unique_alert(&mut guard, alert);
            }
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
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.retain(|alert| alert.id != alert_id);
            }
        }
    }
}

fn push_unique_alert(alerts: &mut Vec<ToolAlert>, alert: ToolAlert) {
    if alerts.iter().any(|existing| existing.id == alert.id) {
        return;
    }

    alerts.push(alert);
}

#[derive(Clone, Debug)]
pub struct StartTimerRequest {
    pub mode: TimerMode,
    pub duration_ms: Option<i64>,
    pub label: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StartPomodoroRequest {
    pub focus_ms: i64,
    pub short_break_ms: i64,
    pub long_break_ms: i64,
    pub long_break_every: i64,
}

#[derive(Clone, Debug)]
pub struct CreateSoftwareReminderRuleRequest {
    pub app_name: String,
    pub exe_name: Option<String>,
    pub limit_ms: i64,
    pub message: String,
}

pub async fn run<R: Runtime + 'static>(app: AppHandle<R>) -> Result<(), String> {
    wait_for_sqlite_pool(&app).await?;
    recover_after_startup(&app).await?;

    loop {
        if let Err(error) = tick_and_refresh_if_changed(&app).await {
            eprintln!("[tools] runtime tick failed: {error}");
        }
        sleep(Duration::from_millis(TOOLS_RUNTIME_TICK_MS)).await;
    }
}

pub async fn get_snapshot<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    load_snapshot(app).await
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

pub async fn create_reminder<R: Runtime>(
    app: &AppHandle<R>,
    label: String,
    scheduled_at: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    let now_ms = now_ms();
    if scheduled_at <= now_ms {
        return Err("reminder time must be in the future".to_string());
    }
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::create_reminder(&pool, &label, scheduled_at, now_ms).await?;
    refresh_snapshot(app).await
}

pub async fn cancel_reminder<R: Runtime>(
    app: &AppHandle<R>,
    reminder_id: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::cancel_reminder(&pool, reminder_id, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn create_software_reminder_rule<R: Runtime>(
    app: &AppHandle<R>,
    request: CreateSoftwareReminderRuleRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::create_software_reminder_rule(
        &pool,
        &request.app_name,
        request.exe_name.as_deref(),
        request.limit_ms,
        &request.message,
        now_ms(),
    )
    .await?;
    refresh_snapshot(app).await
}

pub async fn disable_software_reminder_rule<R: Runtime>(
    app: &AppHandle<R>,
    rule_id: i64,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::disable_software_reminder_rule(&pool, rule_id, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn start_timer<R: Runtime>(
    app: &AppHandle<R>,
    request: StartTimerRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::start_timer(
        &pool,
        request.mode,
        request.duration_ms,
        request.label.as_deref(),
        now_ms(),
    )
    .await?;
    refresh_snapshot(app).await
}

pub async fn pause_timer<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::pause_timer(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn resume_timer<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::resume_timer(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn reset_timer<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::reset_timer(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn add_timer_lap<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::add_timer_lap(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn start_pomodoro<R: Runtime>(
    app: &AppHandle<R>,
    request: StartPomodoroRequest,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::start_pomodoro(
        &pool,
        request.focus_ms,
        request.short_break_ms,
        request.long_break_ms,
        request.long_break_every,
        now_ms(),
    )
    .await?;
    refresh_snapshot(app).await
}

pub async fn pause_pomodoro<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::pause_pomodoro(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn resume_pomodoro<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::resume_pomodoro(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn skip_pomodoro_phase<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::skip_pomodoro_phase(&pool, &date_key(), now_ms()).await?;
    refresh_snapshot(app).await
}

pub async fn reset_pomodoro<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    repositories::tools::reset_pomodoro(&pool, now_ms()).await?;
    refresh_snapshot(app).await
}

async fn recover_after_startup<R: Runtime + 'static>(app: &AppHandle<R>) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    let now = now_ms();
    repositories::tools::pause_running_stopwatch_after_restart(&pool, now).await?;
    tick_and_notify(app, &pool, now).await?;
    refresh_snapshot(app).await?;
    Ok(())
}

async fn tick_and_refresh_if_changed<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    let outcome = tick_and_notify(app, &pool, now_ms()).await?;
    if outcome.state_changed {
        refresh_snapshot(app).await?;
    }
    Ok(())
}

async fn tick_and_notify<R: Runtime + 'static>(
    app: &AppHandle<R>,
    pool: &sqlx::Pool<sqlx::Sqlite>,
    now: i64,
) -> Result<ToolsTickOutcome, String> {
    let mut outcome = ToolsTickOutcome::default();

    let fired_reminders = repositories::tools::fire_due_reminders(pool, now).await?;
    if !fired_reminders.is_empty() {
        outcome.merge(ToolsTickOutcome::changed());
    }
    for reminder in fired_reminders {
        send_tool_alert(
            app,
            ToolAlert {
                id: format!("reminder:{}", reminder.id),
                kind: ToolAlertKind::Reminder,
                title: "提醒".to_string(),
                body: if reminder.label.trim().is_empty() {
                    "时间到了".to_string()
                } else {
                    reminder.label
                },
                occurred_at: reminder.fired_at.unwrap_or(now),
            },
        );
    }

    let current_date_key = date_key();
    let fired_software_reminders = repositories::tools::fire_due_software_reminders(
        pool,
        &current_date_key,
        day_start_ms(),
        now,
    )
    .await?;
    if !fired_software_reminders.is_empty() {
        outcome.mark_changed();
    }
    for reminder in fired_software_reminders {
        let limit_minutes = (reminder.limit_ms / 60_000).max(1);
        let usage_minutes = (reminder.usage_ms / 60_000).max(limit_minutes);
        let body = if reminder.message.trim().is_empty() {
            format!(
                "{} 今日已使用 {} 分钟，已达到 {} 分钟上限",
                reminder.app_name, usage_minutes, limit_minutes
            )
        } else {
            reminder.message
        };
        send_tool_alert(
            app,
            ToolAlert {
                id: format!(
                    "software-reminder:{}:{}",
                    reminder.rule_id, current_date_key
                ),
                kind: ToolAlertKind::SoftwareReminder,
                title: "软件提醒".to_string(),
                body,
                occurred_at: now,
            },
        );
    }

    if let Some(completed_timer) = repositories::tools::complete_due_countdown(pool, now).await? {
        outcome.mark_changed();
        send_tool_alert(
            app,
            ToolAlert {
                id: format!("countdown:{}", completed_timer.timer_id),
                kind: ToolAlertKind::Countdown,
                title: "倒计时结束".to_string(),
                body: completed_timer
                    .label
                    .unwrap_or_else(|| "倒计时已完成".to_string()),
                occurred_at: now,
            },
        );
    }

    if let Some(completed_phase) =
        repositories::tools::complete_due_pomodoro_phase(pool, &date_key(), now).await?
    {
        outcome.mark_changed();
        let title = match completed_phase.completed_phase {
            PomodoroPhase::Focus => "专注结束",
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => "休息结束",
        };
        let body = match completed_phase.next_phase {
            PomodoroPhase::Focus => "下一阶段：专注",
            PomodoroPhase::ShortBreak => "下一阶段：短休息",
            PomodoroPhase::LongBreak => "下一阶段：长休息",
        };
        send_tool_alert(
            app,
            ToolAlert {
                id: format!(
                    "pomodoro:{}:{}:{}",
                    completed_phase.run_id,
                    completed_phase.completed_focus_count,
                    completed_phase.completed_phase.as_str()
                ),
                kind: ToolAlertKind::Pomodoro,
                title: title.to_string(),
                body: body.to_string(),
                occurred_at: now,
            },
        );
    }

    Ok(outcome)
}

async fn load_snapshot<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    let snapshot = repositories::tools::fetch_tools_snapshot(&pool, now_ms(), &date_key()).await?;

    if let Some(state) = app.try_state::<ToolsRuntimeState>() {
        state.replace(snapshot.clone());
    }

    Ok(snapshot)
}

async fn refresh_snapshot<R: Runtime>(app: &AppHandle<R>) -> Result<ToolsRuntimeSnapshot, String> {
    let snapshot = load_snapshot(app).await?;

    if let Err(error) = app.emit(TOOLS_RUNTIME_CHANGED_EVENT, &snapshot) {
        eprintln!("[tools] failed to emit tools snapshot: {error}");
    }

    Ok(snapshot)
}

fn send_tool_alert<R: Runtime + 'static>(app: &AppHandle<R>, alert: ToolAlert) {
    if let Some(state) = app.try_state::<ToolsRuntimeState>() {
        state.push_alert(alert.clone());
    }

    crate::app::main_window::show_main_window(app);

    if let Err(error) = app.emit(TOOLS_ALERT_EVENT, &alert) {
        eprintln!(
            "[tools] failed to emit tool alert, falling back to system notification: {error}"
        );
        if let Err(error) = notification::send(app, &alert.title, &alert.body) {
            eprintln!("[tools] failed to send fallback notification: {error}");
        }
    }
}

#[cfg(test)]
fn snapshot_has_active_work(snapshot: &ToolsRuntimeSnapshot) -> bool {
    snapshot
        .current_timer
        .as_ref()
        .map(|timer| timer.status == TimerStatus::Running)
        .unwrap_or(false)
        || snapshot
            .current_pomodoro
            .as_ref()
            .map(|pomodoro| pomodoro.status == PomodoroStatus::Running)
            .unwrap_or(false)
        || snapshot.next_reminder_at.is_some()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn date_key() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn day_start_ms() -> i64 {
    let now = Local::now();
    let Some(start) = now.date_naive().and_hms_opt(0, 0, 0) else {
        return now.timestamp_millis();
    };
    start
        .and_local_timezone(Local)
        .earliest()
        .map(|date_time| date_time.timestamp_millis())
        .unwrap_or_else(|| now.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tools::{ToolPomodoroRun, ToolReminder, ToolTimer};

    #[test]
    fn tools_tick_outcome_merges_state_changes() {
        let mut outcome = ToolsTickOutcome::default();
        assert!(!outcome.state_changed);

        outcome.merge(ToolsTickOutcome::default());
        assert!(!outcome.state_changed);

        outcome.merge(ToolsTickOutcome::changed());
        assert!(outcome.state_changed);

        outcome.mark_changed();
        assert!(outcome.state_changed);
    }

    #[test]
    fn active_snapshot_detects_running_timer_and_pending_reminder() {
        let mut snapshot = ToolsRuntimeSnapshot::default();
        assert!(!snapshot_has_active_work(&snapshot));

        snapshot.next_reminder_at = Some(1_000);
        assert!(snapshot_has_active_work(&snapshot));

        snapshot.next_reminder_at = None;
        snapshot.current_timer = Some(ToolTimer {
            id: 1,
            mode: TimerMode::Stopwatch,
            label: None,
            duration_ms: None,
            accumulated_ms: 0,
            started_at: Some(1_000),
            paused_at: None,
            completed_at: None,
            status: TimerStatus::Running,
            created_at: 1_000,
            updated_at: 1_000,
        });
        assert!(snapshot_has_active_work(&snapshot));

        snapshot.current_timer = None;
        snapshot.current_pomodoro = Some(ToolPomodoroRun {
            id: 1,
            phase: PomodoroPhase::Focus,
            status: PomodoroStatus::Paused,
            cycle_index: 1,
            focus_ms: 1_000,
            short_break_ms: 1_000,
            long_break_ms: 1_000,
            long_break_every: 4,
            phase_started_at: None,
            phase_paused_at: None,
            phase_remaining_ms: Some(1_000),
            completed_focus_count: 0,
            created_at: 1_000,
            updated_at: 1_000,
        });
        assert!(!snapshot_has_active_work(&snapshot));

        let _ = ToolReminder {
            id: 1,
            label: "x".to_string(),
            scheduled_at: 1,
            created_at: 1,
            status: crate::domain::tools::ReminderStatus::Scheduled,
            fired_at: None,
            cancelled_at: None,
        };
    }

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
}
