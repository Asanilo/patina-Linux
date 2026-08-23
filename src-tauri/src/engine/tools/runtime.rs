use crate::data::repositories;
use crate::domain::tools::{
    PomodoroPhase, TimerMode, ToolAlert, ToolAlertKind, ToolsRuntimeSnapshot,
};
use crate::engine::runtime_context::RuntimeContext;
use chrono::{Local, TimeZone};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

const TOOLS_RUNTIME_TICK_MS: u64 = 1_000;

pub trait ToolsRuntimeSink: Send + Sync {
    fn snapshot_changed(&self, snapshot: &ToolsRuntimeSnapshot);
    fn alert(&self, alert: &ToolAlert);
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

#[derive(Clone)]
pub struct ToolsRuntimeOwner {
    context: RuntimeContext,
    sink: Arc<dyn ToolsRuntimeSink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ToolsTickOutcome {
    state_changed: bool,
}

impl ToolsTickOutcome {
    fn mark_changed(&mut self) {
        self.state_changed = true;
    }
}

impl ToolsRuntimeOwner {
    pub fn new(context: RuntimeContext, sink: Arc<dyn ToolsRuntimeSink>) -> Self {
        Self { context, sink }
    }

    pub async fn snapshot(&self) -> Result<ToolsRuntimeSnapshot, String> {
        let now_ms = self.context.now_ms();
        repositories::tools::fetch_tools_snapshot(self.context.pool(), now_ms, &date_key_at(now_ms))
            .await
    }

    pub async fn recover_after_startup(&self) -> Result<ToolsRuntimeSnapshot, String> {
        let now_ms = self.context.now_ms();
        repositories::tools::pause_running_stopwatch_after_restart(self.context.pool(), now_ms)
            .await?;
        self.tick_and_notify(now_ms).await?;
        self.refresh_snapshot().await
    }

    pub async fn run_with_shutdown(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), String> {
        self.recover_after_startup().await?;
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            if let Err(error) = self.tick_and_refresh_if_changed().await {
                eprintln!("[tools] runtime tick failed: {error}");
            }
            tokio::select! {
                _ = sleep(Duration::from_millis(TOOLS_RUNTIME_TICK_MS)) => {}
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
        }
    }

    pub async fn create_reminder(
        &self,
        label: String,
        scheduled_at: i64,
    ) -> Result<ToolsRuntimeSnapshot, String> {
        let now_ms = self.context.now_ms();
        if scheduled_at <= now_ms {
            return Err("reminder time must be in the future".to_string());
        }
        repositories::tools::create_reminder(self.context.pool(), &label, scheduled_at, now_ms)
            .await?;
        self.refresh_snapshot().await
    }

    pub async fn cancel_reminder(&self, reminder_id: i64) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::cancel_reminder(
            self.context.pool(),
            reminder_id,
            self.context.now_ms(),
        )
        .await?;
        self.refresh_snapshot().await
    }

    pub async fn create_software_reminder_rule(
        &self,
        request: CreateSoftwareReminderRuleRequest,
    ) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::create_software_reminder_rule(
            self.context.pool(),
            &request.app_name,
            request.exe_name.as_deref(),
            request.limit_ms,
            &request.message,
            self.context.now_ms(),
        )
        .await?;
        self.refresh_snapshot().await
    }

    pub async fn disable_software_reminder_rule(
        &self,
        rule_id: i64,
    ) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::disable_software_reminder_rule(
            self.context.pool(),
            rule_id,
            self.context.now_ms(),
        )
        .await?;
        self.refresh_snapshot().await
    }

    pub async fn start_timer(
        &self,
        request: StartTimerRequest,
    ) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::start_timer(
            self.context.pool(),
            request.mode,
            request.duration_ms,
            request.label.as_deref(),
            self.context.now_ms(),
        )
        .await?;
        self.refresh_snapshot().await
    }

    pub async fn pause_timer(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::pause_timer(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn resume_timer(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::resume_timer(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn reset_timer(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::reset_timer(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn add_timer_lap(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::add_timer_lap(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn start_pomodoro(
        &self,
        request: StartPomodoroRequest,
    ) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::start_pomodoro(
            self.context.pool(),
            request.focus_ms,
            request.short_break_ms,
            request.long_break_ms,
            request.long_break_every,
            self.context.now_ms(),
        )
        .await?;
        self.refresh_snapshot().await
    }

    pub async fn pause_pomodoro(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::pause_pomodoro(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn resume_pomodoro(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::resume_pomodoro(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    pub async fn skip_pomodoro_phase(&self) -> Result<ToolsRuntimeSnapshot, String> {
        let now_ms = self.context.now_ms();
        repositories::tools::skip_pomodoro_phase(self.context.pool(), &date_key_at(now_ms), now_ms)
            .await?;
        self.refresh_snapshot().await
    }

    pub async fn reset_pomodoro(&self) -> Result<ToolsRuntimeSnapshot, String> {
        repositories::tools::reset_pomodoro(self.context.pool(), self.context.now_ms()).await?;
        self.refresh_snapshot().await
    }

    async fn tick_and_refresh_if_changed(&self) -> Result<(), String> {
        let outcome = self.tick_and_notify(self.context.now_ms()).await?;
        if outcome.state_changed {
            self.refresh_snapshot().await?;
        }
        Ok(())
    }

    async fn tick_and_notify(&self, now_ms: i64) -> Result<ToolsTickOutcome, String> {
        let mut outcome = ToolsTickOutcome::default();
        let pool = self.context.pool();

        let fired_reminders = repositories::tools::fire_due_reminders(pool, now_ms).await?;
        if !fired_reminders.is_empty() {
            outcome.mark_changed();
        }
        for reminder in fired_reminders {
            self.sink.alert(&ToolAlert {
                id: format!("reminder:{}", reminder.id),
                kind: ToolAlertKind::Reminder,
                title: "提醒".to_string(),
                body: if reminder.label.trim().is_empty() {
                    "时间到了".to_string()
                } else {
                    reminder.label
                },
                occurred_at: reminder.fired_at.unwrap_or(now_ms),
            });
        }

        let date_key = date_key_at(now_ms);
        let fired_software_reminders = repositories::tools::fire_due_software_reminders(
            pool,
            &date_key,
            day_start_ms_at(now_ms),
            now_ms,
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
            self.sink.alert(&ToolAlert {
                id: format!("software-reminder:{}:{date_key}", reminder.rule_id),
                kind: ToolAlertKind::SoftwareReminder,
                title: "软件提醒".to_string(),
                body,
                occurred_at: now_ms,
            });
        }

        if let Some(completed_timer) =
            repositories::tools::complete_due_countdown(pool, now_ms).await?
        {
            outcome.mark_changed();
            self.sink.alert(&ToolAlert {
                id: format!("countdown:{}", completed_timer.timer_id),
                kind: ToolAlertKind::Countdown,
                title: "倒计时结束".to_string(),
                body: completed_timer
                    .label
                    .unwrap_or_else(|| "倒计时已完成".to_string()),
                occurred_at: now_ms,
            });
        }

        if let Some(completed_phase) =
            repositories::tools::complete_due_pomodoro_phase(pool, &date_key, now_ms).await?
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
            self.sink.alert(&ToolAlert {
                id: format!(
                    "pomodoro:{}:{}:{}",
                    completed_phase.run_id,
                    completed_phase.completed_focus_count,
                    completed_phase.completed_phase.as_str()
                ),
                kind: ToolAlertKind::Pomodoro,
                title: title.to_string(),
                body: body.to_string(),
                occurred_at: now_ms,
            });
        }

        Ok(outcome)
    }

    async fn refresh_snapshot(&self) -> Result<ToolsRuntimeSnapshot, String> {
        let snapshot = self.snapshot().await?;
        self.sink.snapshot_changed(&snapshot);
        Ok(snapshot)
    }
}

pub(crate) fn date_key_at(now_ms: i64) -> String {
    Local
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn day_start_ms_at(now_ms: i64) -> i64 {
    let now = Local
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(Local::now);
    let Some(start) = now.date_naive().and_hms_opt(0, 0, 0) else {
        return now_ms;
    };
    start
        .and_local_timezone(Local)
        .earliest()
        .map(|date_time| date_time.timestamp_millis())
        .unwrap_or(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;
    use std::sync::Mutex;

    struct FixedClock(i64);

    impl crate::engine::runtime_context::RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[derive(Default)]
    struct MemoryToolsSink {
        snapshots: Mutex<Vec<ToolsRuntimeSnapshot>>,
        alerts: Mutex<Vec<ToolAlert>>,
    }

    impl ToolsRuntimeSink for MemoryToolsSink {
        fn snapshot_changed(&self, snapshot: &ToolsRuntimeSnapshot) {
            self.snapshots.lock().unwrap().push(snapshot.clone());
        }

        fn alert(&self, alert: &ToolAlert) {
            self.alerts.lock().unwrap().push(alert.clone());
        }
    }

    #[test]
    fn tools_tick_outcome_marks_state_changes() {
        let mut outcome = ToolsTickOutcome::default();
        assert!(!outcome.state_changed);
        outcome.mark_changed();
        assert!(outcome.state_changed);
    }

    #[test]
    fn date_boundaries_use_the_same_observed_timestamp() {
        let noon = Local
            .with_ymd_and_hms(2026, 8, 24, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        assert_eq!(date_key_at(noon), "2026-08-24");
        assert!(day_start_ms_at(noon) <= noon);
        assert_eq!(date_key_at(day_start_ms_at(noon)), "2026-08-24");
    }

    #[tokio::test]
    async fn due_reminder_emits_alert_and_refreshed_snapshot_once() {
        let now_ms = 1_782_000_000_000;
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::TOOLS_TABLES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        crate::data::repositories::tools::create_reminder(
            &pool,
            "review",
            now_ms - 1_000,
            now_ms - 2_000,
        )
        .await
        .unwrap();
        let sink = Arc::new(MemoryToolsSink::default());
        let owner = ToolsRuntimeOwner::new(
            RuntimeContext::new(pool.clone(), Arc::new(FixedClock(now_ms))),
            sink.clone(),
        );

        owner.tick_and_refresh_if_changed().await.unwrap();
        owner.tick_and_refresh_if_changed().await.unwrap();

        let alerts = sink.alerts.lock().unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].id, "reminder:1");
        drop(alerts);
        let snapshots = sink.snapshots.lock().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].reminders[0].status,
            crate::domain::tools::ReminderStatus::Fired
        );
        drop(snapshots);
        pool.close().await;
    }
}
