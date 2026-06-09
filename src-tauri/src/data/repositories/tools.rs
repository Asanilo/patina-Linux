use crate::domain::backup::{
    BackupToolDailyStats, BackupToolPomodoroRun, BackupToolReminder, BackupToolTimer,
    BackupToolTimerLap,
};
use crate::domain::tools::{
    PomodoroPhase, PomodoroStatus, ReminderStatus, TimerMode, TimerStatus, ToolPomodoroRun,
    ToolReminder, ToolRuntimeSettings, ToolSoftwareReminderRule, ToolTimer, ToolTimerLap,
    ToolsRuntimeSnapshot,
};
use sqlx::{Pool, Row, Sqlite, Transaction};
const RECENT_REMINDER_LIMIT: i64 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedTimerNotification {
    pub timer_id: i64,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedPomodoroNotification {
    pub run_id: i64,
    pub completed_phase: PomodoroPhase,
    pub next_phase: PomodoroPhase,
    pub completed_focus_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftwareReminderNotification {
    pub rule_id: i64,
    pub app_name: String,
    pub limit_ms: i64,
    pub usage_ms: i64,
    pub message: String,
}

pub async fn load_tool_runtime_settings(
    _pool: &Pool<Sqlite>,
) -> Result<ToolRuntimeSettings, sqlx::Error> {
    Ok(ToolRuntimeSettings::default())
}

pub async fn create_reminder(
    pool: &Pool<Sqlite>,
    label: &str,
    scheduled_at: i64,
    now_ms: i64,
) -> Result<ToolReminder, String> {
    let label = label.trim();
    let safe_label = if label.is_empty() {
        "时间到了"
    } else {
        label
    };

    let result = sqlx::query(
        "INSERT INTO tool_reminders (label, scheduled_at, created_at, status)
         VALUES (?, ?, ?, ?)",
    )
    .bind(safe_label)
    .bind(scheduled_at)
    .bind(now_ms)
    .bind(ReminderStatus::Scheduled.as_str())
    .execute(pool)
    .await
    .map_err(|error| format!("failed to create reminder: {error}"))?;

    fetch_reminder_by_id(pool, result.last_insert_rowid()).await
}

pub async fn cancel_reminder(
    pool: &Pool<Sqlite>,
    reminder_id: i64,
    now_ms: i64,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE tool_reminders
         SET status = ?, cancelled_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(ReminderStatus::Cancelled.as_str())
    .bind(now_ms)
    .bind(reminder_id)
    .bind(ReminderStatus::Scheduled.as_str())
    .execute(pool)
    .await
    .map_err(|error| format!("failed to cancel reminder: {error}"))?;
    Ok(())
}

pub async fn create_software_reminder_rule(
    pool: &Pool<Sqlite>,
    app_name: &str,
    exe_name: Option<&str>,
    limit_ms: i64,
    message: &str,
    now_ms: i64,
) -> Result<ToolSoftwareReminderRule, String> {
    let app_name = app_name.trim();
    if app_name.is_empty() {
        return Err("software reminder app is required".to_string());
    }

    let exe_name = exe_name.map(str::trim).filter(|value| !value.is_empty());
    let safe_limit_ms = limit_ms.max(60_000);
    let message = message.trim();
    let safe_message = if message.is_empty() {
        "休息一下".to_string()
    } else {
        message.to_string()
    };

    let result = sqlx::query(
        "INSERT INTO tool_software_reminder_rules (
            app_name, exe_name, limit_ms, message, created_at, updated_at, disabled_at,
            last_fired_date_key
         ) VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(app_name)
    .bind(exe_name)
    .bind(safe_limit_ms)
    .bind(&safe_message)
    .bind(now_ms)
    .bind(now_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to create software reminder rule: {error}"))?;

    fetch_software_reminder_rule_by_id(pool, result.last_insert_rowid()).await
}

pub async fn disable_software_reminder_rule(
    pool: &Pool<Sqlite>,
    rule_id: i64,
    now_ms: i64,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE tool_software_reminder_rules
         SET disabled_at = ?, updated_at = ?
         WHERE id = ? AND disabled_at IS NULL",
    )
    .bind(now_ms)
    .bind(now_ms)
    .bind(rule_id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to disable software reminder rule: {error}"))?;
    Ok(())
}

pub async fn fire_due_reminders(
    pool: &Pool<Sqlite>,
    now_ms: i64,
) -> Result<Vec<ToolReminder>, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start reminder transaction: {error}"))?;
    let rows = sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         WHERE status = ? AND scheduled_at <= ?
         ORDER BY scheduled_at ASC, id ASC",
    )
    .bind(ReminderStatus::Scheduled.as_str())
    .bind(now_ms)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| format!("failed to load due reminders: {error}"))?;

    let reminders = rows.into_iter().map(map_reminder_row).collect::<Vec<_>>();
    for reminder in &reminders {
        sqlx::query(
            "UPDATE tool_reminders
             SET status = ?, fired_at = ?
             WHERE id = ? AND status = ?",
        )
        .bind(ReminderStatus::Fired.as_str())
        .bind(now_ms)
        .bind(reminder.id)
        .bind(ReminderStatus::Scheduled.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to mark reminder fired: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit reminder transaction: {error}"))?;

    Ok(reminders
        .into_iter()
        .map(|mut reminder| {
            reminder.status = ReminderStatus::Fired;
            reminder.fired_at = Some(now_ms);
            reminder
        })
        .collect())
}

pub async fn fire_due_software_reminders(
    pool: &Pool<Sqlite>,
    date_key: &str,
    day_start_ms: i64,
    now_ms: i64,
) -> Result<Vec<SoftwareReminderNotification>, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start software reminder transaction: {error}"))?;
    let rules = fetch_active_software_reminder_rules_tx(&mut tx).await?;
    let mut notifications = Vec::new();

    for rule in rules {
        if rule.last_fired_date_key.as_deref() == Some(date_key) {
            continue;
        }

        let usage_ms =
            fetch_software_usage_ms_today_tx(&mut tx, &rule, day_start_ms, now_ms).await?;
        if usage_ms < rule.limit_ms {
            continue;
        }

        sqlx::query(
            "UPDATE tool_software_reminder_rules
             SET last_fired_date_key = ?, updated_at = ?
             WHERE id = ? AND disabled_at IS NULL",
        )
        .bind(date_key)
        .bind(now_ms)
        .bind(rule.id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("failed to mark software reminder fired: {error}"))?;

        notifications.push(SoftwareReminderNotification {
            rule_id: rule.id,
            app_name: rule.app_name,
            limit_ms: rule.limit_ms,
            usage_ms,
            message: rule.message,
        });
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit software reminder transaction: {error}"))?;

    Ok(notifications)
}

pub async fn start_timer(
    pool: &Pool<Sqlite>,
    mode: TimerMode,
    duration_ms: Option<i64>,
    label: Option<&str>,
    now_ms: i64,
) -> Result<ToolTimer, String> {
    let duration_ms = match mode {
        TimerMode::Stopwatch => None,
        TimerMode::Countdown => Some(duration_ms.unwrap_or(0).max(1_000)),
    };
    let label = label.map(str::trim).filter(|value| !value.is_empty());

    let result = sqlx::query(
        "INSERT INTO tool_timers (
            mode, label, duration_ms, accumulated_ms, started_at, paused_at,
            completed_at, status, created_at, updated_at
         ) VALUES (?, ?, ?, 0, ?, NULL, NULL, ?, ?, ?)",
    )
    .bind(mode.as_str())
    .bind(label)
    .bind(duration_ms)
    .bind(now_ms)
    .bind(TimerStatus::Running.as_str())
    .bind(now_ms)
    .bind(now_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to start timer: {error}"))?;

    fetch_timer_by_id(pool, result.last_insert_rowid()).await
}

pub async fn pause_timer(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(());
    };
    if timer.status != TimerStatus::Running {
        return Ok(());
    }

    let elapsed = timer.elapsed_ms_at(now_ms);
    sqlx::query(
        "UPDATE tool_timers
         SET accumulated_ms = ?, status = ?, started_at = NULL, paused_at = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(elapsed)
    .bind(TimerStatus::Paused.as_str())
    .bind(now_ms)
    .bind(now_ms)
    .bind(timer.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to pause timer: {error}"))?;
    Ok(())
}

pub async fn resume_timer(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(());
    };
    if timer.status != TimerStatus::Paused {
        return Ok(());
    }

    sqlx::query(
        "UPDATE tool_timers
         SET status = ?, started_at = ?, paused_at = NULL, updated_at = ?
         WHERE id = ?",
    )
    .bind(TimerStatus::Running.as_str())
    .bind(now_ms)
    .bind(now_ms)
    .bind(timer.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to resume timer: {error}"))?;
    Ok(())
}

pub async fn reset_timer(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE tool_timers
         SET accumulated_ms = 0,
             started_at = NULL,
             paused_at = NULL,
             completed_at = NULL,
             status = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(TimerStatus::Idle.as_str())
    .bind(now_ms)
    .bind(timer.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to reset timer: {error}"))?;
    clear_laps_for_timer(pool, timer.id).await?;
    Ok(())
}

pub async fn add_timer_lap(
    pool: &Pool<Sqlite>,
    now_ms: i64,
) -> Result<Option<ToolTimerLap>, String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(None);
    };
    if timer.status != TimerStatus::Running {
        return Ok(None);
    }

    let last_lap = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         WHERE timer_id = ?
         ORDER BY lap_index DESC, id DESC
         LIMIT 1",
    )
    .bind(timer.id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load previous timer lap: {error}"))?
    .map(map_timer_lap_row);

    let lap_index = last_lap
        .as_ref()
        .map(|lap| lap.lap_index.saturating_add(1))
        .unwrap_or(1);
    let started_at = last_lap
        .map(|lap| lap.ended_at)
        .or(timer.started_at)
        .unwrap_or(now_ms);
    let ended_at = now_ms.max(started_at);
    let duration_ms = ended_at.saturating_sub(started_at);

    let result = sqlx::query(
        "INSERT INTO tool_timer_laps (timer_id, lap_index, started_at, ended_at, duration_ms)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(timer.id)
    .bind(lap_index)
    .bind(started_at)
    .bind(ended_at)
    .bind(duration_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to add timer lap: {error}"))?;

    let lap_id = result.last_insert_rowid();
    let lap = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         WHERE id = ?",
    )
    .bind(lap_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to read timer lap: {error}"))
    .map(map_timer_lap_row)?;

    Ok(Some(lap))
}

pub async fn complete_due_countdown(
    pool: &Pool<Sqlite>,
    now_ms: i64,
) -> Result<Option<CompletedTimerNotification>, String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(None);
    };
    if !timer.is_countdown_due(now_ms) {
        return Ok(None);
    }

    let duration_ms = timer
        .duration_ms
        .unwrap_or_else(|| timer.elapsed_ms_at(now_ms));
    sqlx::query(
        "UPDATE tool_timers
         SET accumulated_ms = ?,
             started_at = NULL,
             paused_at = NULL,
             completed_at = ?,
             status = ?,
             updated_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(duration_ms.max(0))
    .bind(now_ms)
    .bind(TimerStatus::Completed.as_str())
    .bind(now_ms)
    .bind(timer.id)
    .bind(TimerStatus::Running.as_str())
    .execute(pool)
    .await
    .map_err(|error| format!("failed to complete countdown: {error}"))?;

    Ok(Some(CompletedTimerNotification {
        timer_id: timer.id,
        label: timer.label,
    }))
}

pub async fn pause_running_stopwatch_after_restart(
    pool: &Pool<Sqlite>,
    now_ms: i64,
) -> Result<bool, String> {
    let Some(timer) = fetch_latest_timer(pool).await? else {
        return Ok(false);
    };
    if timer.mode != TimerMode::Stopwatch || timer.status != TimerStatus::Running {
        return Ok(false);
    }

    pause_timer(pool, now_ms).await?;
    Ok(true)
}

pub async fn start_pomodoro(
    pool: &Pool<Sqlite>,
    focus_ms: i64,
    short_break_ms: i64,
    long_break_ms: i64,
    long_break_every: i64,
    now_ms: i64,
) -> Result<ToolPomodoroRun, String> {
    let focus_ms = focus_ms.max(1_000);
    let short_break_ms = short_break_ms.max(1_000);
    let long_break_ms = long_break_ms.max(1_000);
    let long_break_every = long_break_every.clamp(2, 12);

    let result = sqlx::query(
        "INSERT INTO tool_pomodoro_runs (
            phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
            long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
            completed_focus_count, created_at, updated_at
         ) VALUES (?, ?, 1, ?, ?, ?, ?, ?, NULL, ?, 0, ?, ?)",
    )
    .bind(PomodoroPhase::Focus.as_str())
    .bind(PomodoroStatus::Running.as_str())
    .bind(focus_ms)
    .bind(short_break_ms)
    .bind(long_break_ms)
    .bind(long_break_every)
    .bind(now_ms)
    .bind(focus_ms)
    .bind(now_ms)
    .bind(now_ms)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to start pomodoro: {error}"))?;

    fetch_pomodoro_by_id(pool, result.last_insert_rowid()).await
}

pub async fn pause_pomodoro(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(pool).await? else {
        return Ok(());
    };
    if run.status != PomodoroStatus::Running {
        return Ok(());
    }

    let remaining_ms = run.remaining_ms_at(now_ms);
    sqlx::query(
        "UPDATE tool_pomodoro_runs
         SET status = ?,
             phase_started_at = NULL,
             phase_paused_at = ?,
             phase_remaining_ms = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(PomodoroStatus::Paused.as_str())
    .bind(now_ms)
    .bind(remaining_ms)
    .bind(now_ms)
    .bind(run.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to pause pomodoro: {error}"))?;
    Ok(())
}

pub async fn resume_pomodoro(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(pool).await? else {
        return Ok(());
    };
    if run.status != PomodoroStatus::Paused {
        return Ok(());
    }

    sqlx::query(
        "UPDATE tool_pomodoro_runs
         SET status = ?,
             phase_started_at = ?,
             phase_paused_at = NULL,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(PomodoroStatus::Running.as_str())
    .bind(now_ms)
    .bind(now_ms)
    .bind(run.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to resume pomodoro: {error}"))?;
    Ok(())
}

pub async fn skip_pomodoro_phase(
    pool: &Pool<Sqlite>,
    date_key: &str,
    now_ms: i64,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    advance_pomodoro_phase(pool, date_key, now_ms, false, false).await
}

pub async fn complete_due_pomodoro_phase(
    pool: &Pool<Sqlite>,
    date_key: &str,
    now_ms: i64,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    let Some(run) = fetch_latest_pomodoro(pool).await? else {
        return Ok(None);
    };
    if !run.is_phase_due(now_ms) {
        return Ok(None);
    }

    advance_pomodoro_phase(pool, date_key, now_ms, true, true).await
}

async fn advance_pomodoro_phase(
    pool: &Pool<Sqlite>,
    date_key: &str,
    now_ms: i64,
    count_focus_completion: bool,
    start_next_phase: bool,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    let Some(run) = fetch_latest_pomodoro(pool).await? else {
        return Ok(None);
    };
    if run.status == PomodoroStatus::Idle || run.status == PomodoroStatus::Completed {
        return Ok(None);
    }

    let (next_phase, next_cycle_index, next_completed_focus_count) =
        if run.phase == PomodoroPhase::Focus && !count_focus_completion {
            let phase = if (run.completed_focus_count + 1) % run.long_break_every.max(1) == 0 {
                PomodoroPhase::LongBreak
            } else {
                PomodoroPhase::ShortBreak
            };
            let cycle_index = if phase == PomodoroPhase::LongBreak {
                run.long_break_every
            } else {
                (run.completed_focus_count + 1) % run.long_break_every.max(1)
            };
            (phase, cycle_index.max(1), run.completed_focus_count)
        } else {
            run.next_phase_after_completion()
        };
    let next_remaining_ms = match next_phase {
        PomodoroPhase::Focus => run.focus_ms,
        PomodoroPhase::ShortBreak => run.short_break_ms,
        PomodoroPhase::LongBreak => run.long_break_ms,
    };

    let next_status = if start_next_phase {
        PomodoroStatus::Running
    } else {
        PomodoroStatus::Paused
    };
    let next_started_at = start_next_phase.then_some(now_ms);
    let next_paused_at = (!start_next_phase).then_some(now_ms);

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start pomodoro transaction: {error}"))?;
    sqlx::query(
        "UPDATE tool_pomodoro_runs
         SET phase = ?,
             status = ?,
             cycle_index = ?,
             phase_started_at = ?,
             phase_paused_at = ?,
             phase_remaining_ms = ?,
             completed_focus_count = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(next_phase.as_str())
    .bind(next_status.as_str())
    .bind(next_cycle_index)
    .bind(next_started_at)
    .bind(next_paused_at)
    .bind(next_remaining_ms)
    .bind(next_completed_focus_count)
    .bind(now_ms)
    .bind(run.id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("failed to advance pomodoro phase: {error}"))?;

    let counted_focus = count_focus_completion && run.phase == PomodoroPhase::Focus;
    if counted_focus {
        increment_daily_pomodoro_stat_tx(&mut tx, date_key, now_ms).await?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit pomodoro transaction: {error}"))?;

    Ok(Some(CompletedPomodoroNotification {
        run_id: run.id,
        completed_phase: run.phase,
        next_phase,
        completed_focus_count: next_completed_focus_count,
    }))
}

pub async fn reset_pomodoro(pool: &Pool<Sqlite>, now_ms: i64) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(pool).await? else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE tool_pomodoro_runs
         SET phase = ?,
             status = ?,
             cycle_index = 1,
             phase_started_at = NULL,
             phase_paused_at = NULL,
             phase_remaining_ms = focus_ms,
             completed_focus_count = 0,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(PomodoroPhase::Focus.as_str())
    .bind(PomodoroStatus::Idle.as_str())
    .bind(now_ms)
    .bind(run.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to reset pomodoro: {error}"))?;
    Ok(())
}

pub async fn fetch_tools_snapshot(
    pool: &Pool<Sqlite>,
    now_ms: i64,
    date_key: &str,
) -> Result<ToolsRuntimeSnapshot, String> {
    let settings = load_tool_runtime_settings(pool)
        .await
        .map_err(|error| format!("failed to load tools settings: {error}"))?;
    let reminders = fetch_visible_reminders(pool).await?;
    let software_reminder_rules = fetch_active_software_reminder_rules(pool).await?;
    let current_timer = fetch_latest_timer(pool)
        .await?
        .filter(|timer| timer.status != TimerStatus::Idle);
    let timer_laps = match &current_timer {
        Some(timer) => fetch_timer_laps(pool, timer.id).await?,
        None => Vec::new(),
    };
    let current_pomodoro = fetch_latest_pomodoro(pool).await?;
    let today_completed_pomodoros = fetch_daily_pomodoro_count(pool, date_key).await?;
    let next_reminder_at = reminders
        .iter()
        .filter(|reminder| reminder.status == ReminderStatus::Scheduled)
        .map(|reminder| reminder.scheduled_at)
        .min();

    Ok(ToolsRuntimeSnapshot {
        settings,
        reminders,
        software_reminder_rules,
        current_timer,
        timer_laps,
        current_pomodoro,
        today_completed_pomodoros,
        next_reminder_at,
        sampled_at_ms: now_ms,
    })
}

pub async fn fetch_all_reminders_for_backup(
    pool: &Pool<Sqlite>,
) -> Result<Vec<BackupToolReminder>, String> {
    let rows = sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read tool reminders for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupToolReminder {
            id: row.get("id"),
            label: row.get("label"),
            scheduled_at: row.get("scheduled_at"),
            created_at: row.get("created_at"),
            status: row.get("status"),
            fired_at: row.get("fired_at"),
            cancelled_at: row.get("cancelled_at"),
        })
        .collect())
}

pub async fn fetch_all_timers_for_backup(
    pool: &Pool<Sqlite>,
) -> Result<Vec<BackupToolTimer>, String> {
    let rows = sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read tool timers for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupToolTimer {
            id: row.get("id"),
            mode: row.get("mode"),
            label: row.get("label"),
            duration_ms: row.get("duration_ms"),
            accumulated_ms: row.get("accumulated_ms"),
            started_at: row.get("started_at"),
            paused_at: row.get("paused_at"),
            completed_at: row.get("completed_at"),
            status: row.get("status"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

pub async fn fetch_all_timer_laps_for_backup(
    pool: &Pool<Sqlite>,
) -> Result<Vec<BackupToolTimerLap>, String> {
    let rows = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read tool timer laps for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupToolTimerLap {
            id: row.get("id"),
            timer_id: row.get("timer_id"),
            lap_index: row.get("lap_index"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            duration_ms: row.get("duration_ms"),
        })
        .collect())
}

pub async fn fetch_all_pomodoro_runs_for_backup(
    pool: &Pool<Sqlite>,
) -> Result<Vec<BackupToolPomodoroRun>, String> {
    let rows = sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read tool pomodoro runs for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupToolPomodoroRun {
            id: row.get("id"),
            phase: row.get("phase"),
            status: row.get("status"),
            cycle_index: row.get("cycle_index"),
            focus_ms: row.get("focus_ms"),
            short_break_ms: row.get("short_break_ms"),
            long_break_ms: row.get("long_break_ms"),
            long_break_every: row.get("long_break_every"),
            phase_started_at: row.get("phase_started_at"),
            phase_paused_at: row.get("phase_paused_at"),
            phase_remaining_ms: row.get("phase_remaining_ms"),
            completed_focus_count: row.get("completed_focus_count"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

pub async fn fetch_all_daily_stats_for_backup(
    pool: &Pool<Sqlite>,
) -> Result<Vec<BackupToolDailyStats>, String> {
    let rows = sqlx::query(
        "SELECT date_key, completed_pomodoros, updated_at
         FROM tool_daily_stats
         ORDER BY date_key ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read tool daily stats for backup: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BackupToolDailyStats {
            date_key: row.get("date_key"),
            completed_pomodoros: row.get("completed_pomodoros"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

pub async fn clear_for_restore(tx: &mut Transaction<'_, Sqlite>) -> Result<(), String> {
    sqlx::query("DELETE FROM tool_timer_laps")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear tool timer laps before restore: {error}"))?;
    sqlx::query("DELETE FROM tool_timers")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear tool timers before restore: {error}"))?;
    sqlx::query("DELETE FROM tool_reminders")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear tool reminders before restore: {error}"))?;
    sqlx::query("DELETE FROM tool_pomodoro_runs")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear tool pomodoro runs before restore: {error}"))?;
    sqlx::query("DELETE FROM tool_daily_stats")
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to clear tool daily stats before restore: {error}"))?;
    Ok(())
}

pub async fn insert_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    reminders: &[BackupToolReminder],
    timers: &[BackupToolTimer],
    laps: &[BackupToolTimerLap],
    pomodoro_runs: &[BackupToolPomodoroRun],
    daily_stats: &[BackupToolDailyStats],
) -> Result<(), String> {
    for reminder in reminders {
        sqlx::query(
            "INSERT INTO tool_reminders (id, label, scheduled_at, created_at, status, fired_at, cancelled_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(reminder.id)
        .bind(&reminder.label)
        .bind(reminder.scheduled_at)
        .bind(reminder.created_at)
        .bind(&reminder.status)
        .bind(reminder.fired_at)
        .bind(reminder.cancelled_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to restore tool reminders: {error}"))?;
    }

    for timer in timers {
        sqlx::query(
            "INSERT INTO tool_timers (
                id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(timer.id)
        .bind(&timer.mode)
        .bind(&timer.label)
        .bind(timer.duration_ms)
        .bind(timer.accumulated_ms)
        .bind(timer.started_at)
        .bind(timer.paused_at)
        .bind(timer.completed_at)
        .bind(&timer.status)
        .bind(timer.created_at)
        .bind(timer.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to restore tool timers: {error}"))?;
    }

    for lap in laps {
        sqlx::query(
            "INSERT INTO tool_timer_laps (id, timer_id, lap_index, started_at, ended_at, duration_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(lap.id)
        .bind(lap.timer_id)
        .bind(lap.lap_index)
        .bind(lap.started_at)
        .bind(lap.ended_at)
        .bind(lap.duration_ms)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to restore tool timer laps: {error}"))?;
    }

    for run in pomodoro_runs {
        sqlx::query(
            "INSERT INTO tool_pomodoro_runs (
                id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id)
        .bind(&run.phase)
        .bind(&run.status)
        .bind(run.cycle_index)
        .bind(run.focus_ms)
        .bind(run.short_break_ms)
        .bind(run.long_break_ms)
        .bind(run.long_break_every)
        .bind(run.phase_started_at)
        .bind(run.phase_paused_at)
        .bind(run.phase_remaining_ms)
        .bind(run.completed_focus_count)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to restore tool pomodoro runs: {error}"))?;
    }

    for stat in daily_stats {
        sqlx::query(
            "INSERT INTO tool_daily_stats (date_key, completed_pomodoros, updated_at)
             VALUES (?, ?, ?)",
        )
        .bind(&stat.date_key)
        .bind(stat.completed_pomodoros)
        .bind(stat.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to restore tool daily stats: {error}"))?;
    }

    Ok(())
}

pub async fn insert_missing_for_restore(
    tx: &mut Transaction<'_, Sqlite>,
    reminders: &[BackupToolReminder],
    timers: &[BackupToolTimer],
    laps: &[BackupToolTimerLap],
    pomodoro_runs: &[BackupToolPomodoroRun],
    daily_stats: &[BackupToolDailyStats],
) -> Result<(), String> {
    for reminder in reminders {
        sqlx::query(
            "INSERT OR IGNORE INTO tool_reminders (id, label, scheduled_at, created_at, status, fired_at, cancelled_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(reminder.id)
        .bind(&reminder.label)
        .bind(reminder.scheduled_at)
        .bind(reminder.created_at)
        .bind(&reminder.status)
        .bind(reminder.fired_at)
        .bind(reminder.cancelled_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to merge restore tool reminders: {error}"))?;
    }

    for timer in timers {
        sqlx::query(
            "INSERT OR IGNORE INTO tool_timers (
                id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(timer.id)
        .bind(&timer.mode)
        .bind(&timer.label)
        .bind(timer.duration_ms)
        .bind(timer.accumulated_ms)
        .bind(timer.started_at)
        .bind(timer.paused_at)
        .bind(timer.completed_at)
        .bind(&timer.status)
        .bind(timer.created_at)
        .bind(timer.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to merge restore tool timers: {error}"))?;
    }

    for lap in laps {
        sqlx::query(
            "INSERT OR IGNORE INTO tool_timer_laps (id, timer_id, lap_index, started_at, ended_at, duration_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(lap.id)
        .bind(lap.timer_id)
        .bind(lap.lap_index)
        .bind(lap.started_at)
        .bind(lap.ended_at)
        .bind(lap.duration_ms)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to merge restore tool timer laps: {error}"))?;
    }

    for run in pomodoro_runs {
        sqlx::query(
            "INSERT OR IGNORE INTO tool_pomodoro_runs (
                id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id)
        .bind(&run.phase)
        .bind(&run.status)
        .bind(run.cycle_index)
        .bind(run.focus_ms)
        .bind(run.short_break_ms)
        .bind(run.long_break_ms)
        .bind(run.long_break_every)
        .bind(run.phase_started_at)
        .bind(run.phase_paused_at)
        .bind(run.phase_remaining_ms)
        .bind(run.completed_focus_count)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to merge restore tool pomodoro runs: {error}"))?;
    }

    for stat in daily_stats {
        sqlx::query(
            "INSERT OR IGNORE INTO tool_daily_stats (date_key, completed_pomodoros, updated_at)
             VALUES (?, ?, ?)",
        )
        .bind(&stat.date_key)
        .bind(stat.completed_pomodoros)
        .bind(stat.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("failed to merge restore tool daily stats: {error}"))?;
    }

    Ok(())
}

async fn fetch_reminder_by_id(pool: &Pool<Sqlite>, id: i64) -> Result<ToolReminder, String> {
    sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to read reminder: {error}"))
    .map(map_reminder_row)
}

async fn fetch_software_reminder_rule_by_id(
    pool: &Pool<Sqlite>,
    id: i64,
) -> Result<ToolSoftwareReminderRule, String> {
    sqlx::query(
        "SELECT id, app_name, exe_name, limit_ms, message, created_at, updated_at,
                disabled_at, last_fired_date_key
         FROM tool_software_reminder_rules
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to read software reminder rule: {error}"))
    .map(map_software_reminder_rule_row)
}

async fn fetch_active_software_reminder_rules_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<Vec<ToolSoftwareReminderRule>, String> {
    let rows = sqlx::query(
        "SELECT id, app_name, exe_name, limit_ms, message, created_at, updated_at,
                disabled_at, last_fired_date_key
         FROM tool_software_reminder_rules
         WHERE disabled_at IS NULL
         ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("failed to load active software reminder rules: {error}"))?;

    Ok(rows
        .into_iter()
        .map(map_software_reminder_rule_row)
        .collect())
}

async fn fetch_active_software_reminder_rules(
    pool: &Pool<Sqlite>,
) -> Result<Vec<ToolSoftwareReminderRule>, String> {
    let rows = sqlx::query(
        "SELECT id, app_name, exe_name, limit_ms, message, created_at, updated_at,
                disabled_at, last_fired_date_key
         FROM tool_software_reminder_rules
         WHERE disabled_at IS NULL
         ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load active software reminder rules: {error}"))?;

    Ok(rows
        .into_iter()
        .map(map_software_reminder_rule_row)
        .collect())
}

async fn fetch_software_usage_ms_today_tx(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &ToolSoftwareReminderRule,
    day_start_ms: i64,
    now_ms: i64,
) -> Result<i64, String> {
    let match_value = rule
        .exe_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(rule.app_name.as_str());
    let match_column_is_exe = rule
        .exe_name
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    let row = if match_column_is_exe {
        sqlx::query(
            "SELECT COALESCE(SUM(
                MAX(0, MIN(COALESCE(end_time, ?), ?) - MAX(start_time, ?))
             ), 0) AS usage_ms
             FROM sessions
             WHERE exe_name = ? COLLATE NOCASE
               AND start_time < ?
               AND COALESCE(end_time, ?) > ?",
        )
        .bind(now_ms)
        .bind(now_ms)
        .bind(day_start_ms)
        .bind(match_value)
        .bind(now_ms)
        .bind(now_ms)
        .bind(day_start_ms)
        .fetch_one(&mut **tx)
        .await
    } else {
        sqlx::query(
            "SELECT COALESCE(SUM(
                MAX(0, MIN(COALESCE(end_time, ?), ?) - MAX(start_time, ?))
             ), 0) AS usage_ms
             FROM sessions
             WHERE app_name = ? COLLATE NOCASE
               AND start_time < ?
               AND COALESCE(end_time, ?) > ?",
        )
        .bind(now_ms)
        .bind(now_ms)
        .bind(day_start_ms)
        .bind(match_value)
        .bind(now_ms)
        .bind(now_ms)
        .bind(day_start_ms)
        .fetch_one(&mut **tx)
        .await
    }
    .map_err(|error| format!("failed to read software reminder usage: {error}"))?;

    Ok(row.get::<i64, _>("usage_ms"))
}

async fn fetch_visible_reminders(pool: &Pool<Sqlite>) -> Result<Vec<ToolReminder>, String> {
    let rows = sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         WHERE status = ?
            OR id IN (
                SELECT id
                FROM tool_reminders
                WHERE status <> ?
                ORDER BY COALESCE(fired_at, cancelled_at, created_at) DESC, id DESC
                LIMIT ?
            )
         ORDER BY
            CASE WHEN status = ? THEN 0 ELSE 1 END ASC,
            CASE WHEN status = ? THEN scheduled_at ELSE -COALESCE(fired_at, cancelled_at, created_at) END ASC,
            id ASC",
    )
    .bind(ReminderStatus::Scheduled.as_str())
    .bind(ReminderStatus::Scheduled.as_str())
    .bind(RECENT_REMINDER_LIMIT)
    .bind(ReminderStatus::Scheduled.as_str())
    .bind(ReminderStatus::Scheduled.as_str())
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load reminders: {error}"))?;

    Ok(rows.into_iter().map(map_reminder_row).collect())
}

async fn fetch_timer_by_id(pool: &Pool<Sqlite>, id: i64) -> Result<ToolTimer, String> {
    sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to read timer: {error}"))
    .map(map_timer_row)
}

async fn fetch_latest_timer(pool: &Pool<Sqlite>) -> Result<Option<ToolTimer>, String> {
    sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         ORDER BY updated_at DESC, id DESC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to read current timer: {error}"))
    .map(|row| row.map(map_timer_row))
}

async fn fetch_timer_laps(pool: &Pool<Sqlite>, timer_id: i64) -> Result<Vec<ToolTimerLap>, String> {
    let rows = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         WHERE timer_id = ?
         ORDER BY lap_index ASC, id ASC",
    )
    .bind(timer_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to read timer laps: {error}"))?;

    Ok(rows.into_iter().map(map_timer_lap_row).collect())
}

async fn clear_laps_for_timer(pool: &Pool<Sqlite>, timer_id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM tool_timer_laps WHERE timer_id = ?")
        .bind(timer_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to clear timer laps: {error}"))?;
    Ok(())
}

async fn fetch_pomodoro_by_id(pool: &Pool<Sqlite>, id: i64) -> Result<ToolPomodoroRun, String> {
    sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("failed to read pomodoro run: {error}"))
    .map(map_pomodoro_row)
}

async fn fetch_latest_pomodoro(pool: &Pool<Sqlite>) -> Result<Option<ToolPomodoroRun>, String> {
    sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         ORDER BY updated_at DESC, id DESC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to read current pomodoro: {error}"))
    .map(|row| row.map(map_pomodoro_row))
}

async fn fetch_daily_pomodoro_count(pool: &Pool<Sqlite>, date_key: &str) -> Result<i64, String> {
    sqlx::query_scalar(
        "SELECT completed_pomodoros
         FROM tool_daily_stats
         WHERE date_key = ?
         LIMIT 1",
    )
    .bind(date_key)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to read tool daily stats: {error}"))
    .map(|value| value.unwrap_or(0))
}

async fn increment_daily_pomodoro_stat_tx(
    tx: &mut Transaction<'_, Sqlite>,
    date_key: &str,
    now_ms: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO tool_daily_stats (date_key, completed_pomodoros, updated_at)
         VALUES (?, 1, ?)
         ON CONFLICT(date_key) DO UPDATE SET
             completed_pomodoros = completed_pomodoros + 1,
             updated_at = excluded.updated_at",
    )
    .bind(date_key)
    .bind(now_ms)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("failed to update pomodoro daily stats: {error}"))?;
    Ok(())
}

fn map_reminder_row(row: sqlx::sqlite::SqliteRow) -> ToolReminder {
    let status: String = row.get("status");
    ToolReminder {
        id: row.get("id"),
        label: row.get("label"),
        scheduled_at: row.get("scheduled_at"),
        created_at: row.get("created_at"),
        status: ReminderStatus::from_storage(&status),
        fired_at: row.get("fired_at"),
        cancelled_at: row.get("cancelled_at"),
    }
}

fn map_software_reminder_rule_row(row: sqlx::sqlite::SqliteRow) -> ToolSoftwareReminderRule {
    ToolSoftwareReminderRule {
        id: row.get("id"),
        app_name: row.get("app_name"),
        exe_name: row.get("exe_name"),
        limit_ms: row.get("limit_ms"),
        message: row.get("message"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        disabled_at: row.get("disabled_at"),
        last_fired_date_key: row.get("last_fired_date_key"),
    }
}

fn map_timer_row(row: sqlx::sqlite::SqliteRow) -> ToolTimer {
    let mode: String = row.get("mode");
    let status: String = row.get("status");
    ToolTimer {
        id: row.get("id"),
        mode: TimerMode::from_storage(&mode),
        label: row.get("label"),
        duration_ms: row.get("duration_ms"),
        accumulated_ms: row.get("accumulated_ms"),
        started_at: row.get("started_at"),
        paused_at: row.get("paused_at"),
        completed_at: row.get("completed_at"),
        status: TimerStatus::from_storage(&status),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_timer_lap_row(row: sqlx::sqlite::SqliteRow) -> ToolTimerLap {
    ToolTimerLap {
        id: row.get("id"),
        timer_id: row.get("timer_id"),
        lap_index: row.get("lap_index"),
        started_at: row.get("started_at"),
        ended_at: row.get("ended_at"),
        duration_ms: row.get("duration_ms"),
    }
}

fn map_pomodoro_row(row: sqlx::sqlite::SqliteRow) -> ToolPomodoroRun {
    let phase: String = row.get("phase");
    let status: String = row.get("status");
    ToolPomodoroRun {
        id: row.get("id"),
        phase: PomodoroPhase::from_storage(&phase),
        status: PomodoroStatus::from_storage(&status),
        cycle_index: row.get("cycle_index"),
        focus_ms: row.get("focus_ms"),
        short_break_ms: row.get("short_break_ms"),
        long_break_ms: row.get("long_break_ms"),
        long_break_every: row.get("long_break_every"),
        phase_started_at: row.get("phase_started_at"),
        phase_paused_at: row.get("phase_paused_at"),
        phase_remaining_ms: row.get("phase_remaining_ms"),
        completed_focus_count: row.get("completed_focus_count"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema as db_schema;
    use sqlx::{Executor, SqlitePool};

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(db_schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::TOOLS_TABLES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(db_schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        pool
    }

    #[test]
    fn created_reminder_can_be_read_in_snapshot() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;

            create_reminder(&pool, "'; DROP TABLE tool_reminders; --", 2_000, 1_000)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 1_000, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.reminders.len(), 1);
            assert_eq!(
                snapshot.reminders[0].label,
                "'; DROP TABLE tool_reminders; --"
            );
            assert_eq!(snapshot.next_reminder_at, Some(2_000));
        });
    }

    #[test]
    fn due_reminder_fires_only_once() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            create_reminder(&pool, "Stand up", 1_000, 900)
                .await
                .unwrap();

            let first = fire_due_reminders(&pool, 1_100).await.unwrap();
            let second = fire_due_reminders(&pool, 1_200).await.unwrap();

            assert_eq!(first.len(), 1);
            assert_eq!(first[0].status, ReminderStatus::Fired);
            assert!(second.is_empty());
        });
    }

    #[test]
    fn software_reminder_counts_today_usage_and_active_session_once() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            sqlx::query(
                "INSERT INTO sessions (
                    app_name, exe_name, window_title, start_time, end_time, duration,
                    continuity_group_start_time
                 ) VALUES (?, ?, ?, ?, ?, ?, ?), (?, ?, ?, ?, NULL, NULL, ?)",
            )
            .bind("Editor")
            .bind("editor.exe")
            .bind("Doc")
            .bind(0_i64)
            .bind(40_000_i64)
            .bind(40_000_i64)
            .bind(0_i64)
            .bind("Editor")
            .bind("editor.exe")
            .bind("Doc")
            .bind(40_000_i64)
            .bind(40_000_i64)
            .execute(&pool)
            .await
            .unwrap();
            create_software_reminder_rule(
                &pool,
                "Editor",
                Some("editor.exe"),
                60_000,
                "Take a break",
                900,
            )
            .await
            .unwrap();

            let first = fire_due_software_reminders(&pool, "2026-06-07", 0, 70_000)
                .await
                .unwrap();
            let second = fire_due_software_reminders(&pool, "2026-06-07", 0, 71_000)
                .await
                .unwrap();

            assert_eq!(first.len(), 1);
            assert_eq!(first[0].usage_ms, 70_000);
            assert_eq!(first[0].message, "Take a break");
            assert!(second.is_empty());
        });
    }

    #[test]
    fn timer_laps_are_committed_in_order() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();

            add_timer_lap(&pool, 1_500).await.unwrap();
            add_timer_lap(&pool, 2_000).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.timer_laps.len(), 2);
            assert_eq!(snapshot.timer_laps[0].duration_ms, 500);
            assert_eq!(
                snapshot.timer_laps[1].started_at,
                snapshot.timer_laps[0].ended_at
            );
        });
    }

    #[test]
    fn countdown_completion_updates_current_timer_once() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Countdown, Some(1_000), None, 1_000)
                .await
                .unwrap();

            let completed = complete_due_countdown(&pool, 2_100).await.unwrap();
            let second = complete_due_countdown(&pool, 2_200).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_200, "2026-06-07")
                .await
                .unwrap();

            assert!(completed.is_some());
            assert!(second.is_none());
            assert_eq!(
                snapshot.current_timer.unwrap().status,
                TimerStatus::Completed
            );
        });
    }

    #[test]
    fn pausing_running_timer_sets_paused_status() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();

            pause_timer(&pool, 1_500).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 3_000, "2026-06-07")
                .await
                .unwrap();
            let timer = snapshot.current_timer.unwrap();

            assert_eq!(timer.status, TimerStatus::Paused);
            assert_eq!(timer.elapsed_ms_at(3_000), 500);
        });
    }

    #[test]
    fn reset_timer_clears_current_timer_from_snapshot() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();
            add_timer_lap(&pool, 1_500).await.unwrap();

            reset_timer(&pool, 2_000).await.unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();

            assert!(snapshot.current_timer.is_none());
            assert!(snapshot.timer_laps.is_empty());
        });
    }

    #[test]
    fn pomodoro_focus_completion_updates_daily_stats() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            let completed = complete_due_pomodoro_phase(&pool, "2026-06-07", 2_100)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 2_100, "2026-06-07")
                .await
                .unwrap();

            assert!(completed.is_some());
            assert_eq!(snapshot.today_completed_pomodoros, 1);
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::ShortBreak);
            assert_eq!(run.status, PomodoroStatus::Running);
            assert_eq!(run.phase_started_at, Some(2_100));
            assert_eq!(run.phase_paused_at, None);
        });
    }

    #[test]
    fn pause_then_resume_pomodoro_restarts_current_phase() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            pause_pomodoro(&pool, 1_400).await.unwrap();
            resume_pomodoro(&pool, 2_000).await.unwrap();

            let snapshot = fetch_tools_snapshot(&pool, 2_000, "2026-06-07")
                .await
                .unwrap();
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::Focus);
            assert_eq!(run.status, PomodoroStatus::Running);
            assert_eq!(run.phase_started_at, Some(2_000));
            assert_eq!(run.phase_paused_at, None);
            assert_eq!(run.phase_remaining_ms, Some(600));
        });
    }

    #[test]
    fn skip_pomodoro_phase_pauses_next_phase() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();

            skip_pomodoro_phase(&pool, "2026-06-07", 1_500)
                .await
                .unwrap();
            let snapshot = fetch_tools_snapshot(&pool, 1_500, "2026-06-07")
                .await
                .unwrap();

            assert_eq!(snapshot.today_completed_pomodoros, 0);
            let run = snapshot.current_pomodoro.unwrap();
            assert_eq!(run.phase, PomodoroPhase::ShortBreak);
            assert_eq!(run.status, PomodoroStatus::Paused);
            assert_eq!(run.phase_started_at, None);
            assert_eq!(run.phase_paused_at, Some(1_500));
        });
    }

    #[test]
    fn backup_restore_round_trips_tool_tables() {
        tauri::async_runtime::block_on(async {
            let pool = setup_test_db().await;
            create_reminder(&pool, "Check", 2_000, 1_000).await.unwrap();
            start_timer(&pool, TimerMode::Stopwatch, None, None, 1_000)
                .await
                .unwrap();
            add_timer_lap(&pool, 1_500).await.unwrap();
            start_pomodoro(&pool, 1_000, 500, 700, 4, 1_000)
                .await
                .unwrap();
            complete_due_pomodoro_phase(&pool, "2026-06-07", 2_100)
                .await
                .unwrap();

            let reminders = fetch_all_reminders_for_backup(&pool).await.unwrap();
            let timers = fetch_all_timers_for_backup(&pool).await.unwrap();
            let laps = fetch_all_timer_laps_for_backup(&pool).await.unwrap();
            let pomodoros = fetch_all_pomodoro_runs_for_backup(&pool).await.unwrap();
            let stats = fetch_all_daily_stats_for_backup(&pool).await.unwrap();

            let mut tx = pool.begin().await.unwrap();
            clear_for_restore(&mut tx).await.unwrap();
            insert_for_restore(&mut tx, &reminders, &timers, &laps, &pomodoros, &stats)
                .await
                .unwrap();
            tx.commit().await.unwrap();

            assert_eq!(
                fetch_all_reminders_for_backup(&pool).await.unwrap().len(),
                1
            );
            assert_eq!(fetch_all_timers_for_backup(&pool).await.unwrap().len(), 1);
            assert_eq!(
                fetch_all_timer_laps_for_backup(&pool).await.unwrap().len(),
                1
            );
            assert_eq!(
                fetch_all_pomodoro_runs_for_backup(&pool)
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                fetch_all_daily_stats_for_backup(&pool).await.unwrap().len(),
                1
            );
        });
    }
}
