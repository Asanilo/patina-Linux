//! Tools wire data only. Scheduling, clock projection and storage rules remain in the backend.
use serde::{Deserialize, Serialize};

pub const MAX_TOOLS_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    Scheduled,
    Fired,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TimerMode {
    Stopwatch,
    Countdown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
    Completed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PomodoroPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PomodoroStatus {
    Idle,
    Running,
    Paused,
    Completed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolRuntimeSettings {
    pub default_countdown_minutes: i64,
    pub pomodoro_focus_minutes: i64,
    pub pomodoro_short_break_minutes: i64,
    pub pomodoro_long_break_minutes: i64,
    pub pomodoro_long_break_every: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolReminder {
    pub id: i64,
    pub label: String,
    pub scheduled_at: i64,
    pub created_at: i64,
    pub status: ReminderStatus,
    pub fired_at: Option<i64>,
    pub cancelled_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolSoftwareReminderRule {
    pub id: i64,
    pub app_name: String,
    pub exe_name: Option<String>,
    pub limit_ms: i64,
    pub message: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub disabled_at: Option<i64>,
    pub last_fired_date_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolTimer {
    pub id: i64,
    pub mode: TimerMode,
    pub label: Option<String>,
    pub duration_ms: Option<i64>,
    pub accumulated_ms: i64,
    pub started_at: Option<i64>,
    pub paused_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub status: TimerStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolTimerLap {
    pub id: i64,
    pub timer_id: i64,
    pub lap_index: i64,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration_ms: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolPomodoroRun {
    pub id: i64,
    pub phase: PomodoroPhase,
    pub status: PomodoroStatus,
    pub cycle_index: i64,
    pub focus_ms: i64,
    pub short_break_ms: i64,
    pub long_break_ms: i64,
    pub long_break_every: i64,
    pub phase_started_at: Option<i64>,
    pub phase_paused_at: Option<i64>,
    pub phase_remaining_ms: Option<i64>,
    pub completed_focus_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct ToolsRuntimeSnapshot {
    pub settings: ToolRuntimeSettings,
    pub reminders: Vec<ToolReminder>,
    pub software_reminder_rules: Vec<ToolSoftwareReminderRule>,
    pub current_timer: Option<ToolTimer>,
    pub timer_laps: Vec<ToolTimerLap>,
    pub current_pomodoro: Option<ToolPomodoroRun>,
    pub today_completed_pomodoros: i64,
    pub next_reminder_at: Option<i64>,
    pub sampled_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct CreateReminderRequest {
    pub label: String,
    pub scheduled_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct CreateSoftwareReminderRuleRequest {
    pub app_name: String,
    pub exe_name: Option<String>,
    pub limit_ms: i64,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct StartTimerRequest {
    pub mode: TimerMode,
    pub duration_ms: Option<i64>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
pub struct StartPomodoroRequest {
    pub focus_ms: i64,
    pub short_break_ms: i64,
    pub long_break_ms: i64,
    pub long_break_every: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolsAction {
    PauseTimer,
    ResumeTimer,
    ResetTimer,
    AddTimerLap,
    PausePomodoro,
    ResumePomodoro,
    SkipPomodoroPhase,
    ResetPomodoro,
}

impl ToolsAction {
    pub fn path(self) -> &'static str {
        match self {
            Self::PauseTimer => "/api/v1/tools/timer/pause",
            Self::ResumeTimer => "/api/v1/tools/timer/resume",
            Self::ResetTimer => "/api/v1/tools/timer/reset",
            Self::AddTimerLap => "/api/v1/tools/timer/laps",
            Self::PausePomodoro => "/api/v1/tools/pomodoro/pause",
            Self::ResumePomodoro => "/api/v1/tools/pomodoro/resume",
            Self::SkipPomodoroPhase => "/api/v1/tools/pomodoro/skip",
            Self::ResetPomodoro => "/api/v1/tools/pomodoro/reset",
        }
    }
}
