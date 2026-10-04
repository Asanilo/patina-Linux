//! Mechanical domain/wire conversion; never execute Tools business operations here.
use crate::domain::tools as domain;
use patina_protocol::tools as wire;

impl From<domain::ReminderStatus> for wire::ReminderStatus {
    fn from(value: domain::ReminderStatus) -> Self {
        match value {
            domain::ReminderStatus::Scheduled => Self::Scheduled,
            domain::ReminderStatus::Fired => Self::Fired,
            domain::ReminderStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<wire::ReminderStatus> for domain::ReminderStatus {
    fn from(value: wire::ReminderStatus) -> Self {
        match value {
            wire::ReminderStatus::Scheduled => Self::Scheduled,
            wire::ReminderStatus::Fired => Self::Fired,
            wire::ReminderStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<domain::TimerMode> for wire::TimerMode {
    fn from(value: domain::TimerMode) -> Self {
        match value {
            domain::TimerMode::Stopwatch => Self::Stopwatch,
            domain::TimerMode::Countdown => Self::Countdown,
        }
    }
}

impl From<wire::TimerMode> for domain::TimerMode {
    fn from(value: wire::TimerMode) -> Self {
        match value {
            wire::TimerMode::Stopwatch => Self::Stopwatch,
            wire::TimerMode::Countdown => Self::Countdown,
        }
    }
}

impl From<domain::TimerStatus> for wire::TimerStatus {
    fn from(value: domain::TimerStatus) -> Self {
        match value {
            domain::TimerStatus::Idle => Self::Idle,
            domain::TimerStatus::Running => Self::Running,
            domain::TimerStatus::Paused => Self::Paused,
            domain::TimerStatus::Completed => Self::Completed,
        }
    }
}

impl From<wire::TimerStatus> for domain::TimerStatus {
    fn from(value: wire::TimerStatus) -> Self {
        match value {
            wire::TimerStatus::Idle => Self::Idle,
            wire::TimerStatus::Running => Self::Running,
            wire::TimerStatus::Paused => Self::Paused,
            wire::TimerStatus::Completed => Self::Completed,
        }
    }
}

impl From<domain::PomodoroPhase> for wire::PomodoroPhase {
    fn from(value: domain::PomodoroPhase) -> Self {
        match value {
            domain::PomodoroPhase::Focus => Self::Focus,
            domain::PomodoroPhase::ShortBreak => Self::ShortBreak,
            domain::PomodoroPhase::LongBreak => Self::LongBreak,
        }
    }
}

impl From<wire::PomodoroPhase> for domain::PomodoroPhase {
    fn from(value: wire::PomodoroPhase) -> Self {
        match value {
            wire::PomodoroPhase::Focus => Self::Focus,
            wire::PomodoroPhase::ShortBreak => Self::ShortBreak,
            wire::PomodoroPhase::LongBreak => Self::LongBreak,
        }
    }
}

impl From<domain::PomodoroStatus> for wire::PomodoroStatus {
    fn from(value: domain::PomodoroStatus) -> Self {
        match value {
            domain::PomodoroStatus::Idle => Self::Idle,
            domain::PomodoroStatus::Running => Self::Running,
            domain::PomodoroStatus::Paused => Self::Paused,
            domain::PomodoroStatus::Completed => Self::Completed,
        }
    }
}

impl From<wire::PomodoroStatus> for domain::PomodoroStatus {
    fn from(value: wire::PomodoroStatus) -> Self {
        match value {
            wire::PomodoroStatus::Idle => Self::Idle,
            wire::PomodoroStatus::Running => Self::Running,
            wire::PomodoroStatus::Paused => Self::Paused,
            wire::PomodoroStatus::Completed => Self::Completed,
        }
    }
}

impl From<domain::ToolRuntimeSettings> for wire::ToolRuntimeSettings {
    fn from(value: domain::ToolRuntimeSettings) -> Self {
        Self {
            default_countdown_minutes: value.default_countdown_minutes,
            pomodoro_focus_minutes: value.pomodoro_focus_minutes,
            pomodoro_short_break_minutes: value.pomodoro_short_break_minutes,
            pomodoro_long_break_minutes: value.pomodoro_long_break_minutes,
            pomodoro_long_break_every: value.pomodoro_long_break_every,
        }
    }
}

impl From<wire::ToolRuntimeSettings> for domain::ToolRuntimeSettings {
    fn from(value: wire::ToolRuntimeSettings) -> Self {
        Self {
            default_countdown_minutes: value.default_countdown_minutes,
            pomodoro_focus_minutes: value.pomodoro_focus_minutes,
            pomodoro_short_break_minutes: value.pomodoro_short_break_minutes,
            pomodoro_long_break_minutes: value.pomodoro_long_break_minutes,
            pomodoro_long_break_every: value.pomodoro_long_break_every,
        }
    }
}

impl From<domain::ToolReminder> for wire::ToolReminder {
    fn from(value: domain::ToolReminder) -> Self {
        Self {
            id: value.id,
            label: value.label,
            scheduled_at: value.scheduled_at,
            created_at: value.created_at,
            status: value.status.into(),
            fired_at: value.fired_at,
            cancelled_at: value.cancelled_at,
        }
    }
}

impl From<wire::ToolReminder> for domain::ToolReminder {
    fn from(value: wire::ToolReminder) -> Self {
        Self {
            id: value.id,
            label: value.label,
            scheduled_at: value.scheduled_at,
            created_at: value.created_at,
            status: value.status.into(),
            fired_at: value.fired_at,
            cancelled_at: value.cancelled_at,
        }
    }
}

impl From<domain::ToolSoftwareReminderRule> for wire::ToolSoftwareReminderRule {
    fn from(value: domain::ToolSoftwareReminderRule) -> Self {
        Self {
            id: value.id,
            app_name: value.app_name,
            exe_name: value.exe_name,
            limit_ms: value.limit_ms,
            message: value.message,
            created_at: value.created_at,
            updated_at: value.updated_at,
            disabled_at: value.disabled_at,
            last_fired_date_key: value.last_fired_date_key,
        }
    }
}

impl From<wire::ToolSoftwareReminderRule> for domain::ToolSoftwareReminderRule {
    fn from(value: wire::ToolSoftwareReminderRule) -> Self {
        Self {
            id: value.id,
            app_name: value.app_name,
            exe_name: value.exe_name,
            limit_ms: value.limit_ms,
            message: value.message,
            created_at: value.created_at,
            updated_at: value.updated_at,
            disabled_at: value.disabled_at,
            last_fired_date_key: value.last_fired_date_key,
        }
    }
}

impl From<domain::ToolTimer> for wire::ToolTimer {
    fn from(value: domain::ToolTimer) -> Self {
        Self {
            id: value.id,
            mode: value.mode.into(),
            label: value.label,
            duration_ms: value.duration_ms,
            accumulated_ms: value.accumulated_ms,
            started_at: value.started_at,
            paused_at: value.paused_at,
            completed_at: value.completed_at,
            status: value.status.into(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<wire::ToolTimer> for domain::ToolTimer {
    fn from(value: wire::ToolTimer) -> Self {
        Self {
            id: value.id,
            mode: value.mode.into(),
            label: value.label,
            duration_ms: value.duration_ms,
            accumulated_ms: value.accumulated_ms,
            started_at: value.started_at,
            paused_at: value.paused_at,
            completed_at: value.completed_at,
            status: value.status.into(),
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<domain::ToolTimerLap> for wire::ToolTimerLap {
    fn from(value: domain::ToolTimerLap) -> Self {
        Self {
            id: value.id,
            timer_id: value.timer_id,
            lap_index: value.lap_index,
            started_at: value.started_at,
            ended_at: value.ended_at,
            duration_ms: value.duration_ms,
        }
    }
}

impl From<wire::ToolTimerLap> for domain::ToolTimerLap {
    fn from(value: wire::ToolTimerLap) -> Self {
        Self {
            id: value.id,
            timer_id: value.timer_id,
            lap_index: value.lap_index,
            started_at: value.started_at,
            ended_at: value.ended_at,
            duration_ms: value.duration_ms,
        }
    }
}

impl From<domain::ToolPomodoroRun> for wire::ToolPomodoroRun {
    fn from(value: domain::ToolPomodoroRun) -> Self {
        Self {
            id: value.id,
            phase: value.phase.into(),
            status: value.status.into(),
            cycle_index: value.cycle_index,
            focus_ms: value.focus_ms,
            short_break_ms: value.short_break_ms,
            long_break_ms: value.long_break_ms,
            long_break_every: value.long_break_every,
            phase_started_at: value.phase_started_at,
            phase_paused_at: value.phase_paused_at,
            phase_remaining_ms: value.phase_remaining_ms,
            completed_focus_count: value.completed_focus_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<wire::ToolPomodoroRun> for domain::ToolPomodoroRun {
    fn from(value: wire::ToolPomodoroRun) -> Self {
        Self {
            id: value.id,
            phase: value.phase.into(),
            status: value.status.into(),
            cycle_index: value.cycle_index,
            focus_ms: value.focus_ms,
            short_break_ms: value.short_break_ms,
            long_break_ms: value.long_break_ms,
            long_break_every: value.long_break_every,
            phase_started_at: value.phase_started_at,
            phase_paused_at: value.phase_paused_at,
            phase_remaining_ms: value.phase_remaining_ms,
            completed_focus_count: value.completed_focus_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<domain::ToolsRuntimeSnapshot> for wire::ToolsRuntimeSnapshot {
    fn from(value: domain::ToolsRuntimeSnapshot) -> Self {
        Self {
            settings: value.settings.into(),
            reminders: value.reminders.into_iter().map(Into::into).collect(),
            software_reminder_rules: value
                .software_reminder_rules
                .into_iter()
                .map(Into::into)
                .collect(),
            current_timer: value.current_timer.map(Into::into),
            timer_laps: value.timer_laps.into_iter().map(Into::into).collect(),
            current_pomodoro: value.current_pomodoro.map(Into::into),
            today_completed_pomodoros: value.today_completed_pomodoros,
            next_reminder_at: value.next_reminder_at,
            sampled_at_ms: value.sampled_at_ms,
        }
    }
}

impl From<wire::ToolsRuntimeSnapshot> for domain::ToolsRuntimeSnapshot {
    fn from(value: wire::ToolsRuntimeSnapshot) -> Self {
        Self {
            settings: value.settings.into(),
            reminders: value.reminders.into_iter().map(Into::into).collect(),
            software_reminder_rules: value
                .software_reminder_rules
                .into_iter()
                .map(Into::into)
                .collect(),
            current_timer: value.current_timer.map(Into::into),
            timer_laps: value.timer_laps.into_iter().map(Into::into).collect(),
            current_pomodoro: value.current_pomodoro.map(Into::into),
            today_completed_pomodoros: value.today_completed_pomodoros,
            next_reminder_at: value.next_reminder_at,
            sampled_at_ms: value.sampled_at_ms,
        }
    }
}
