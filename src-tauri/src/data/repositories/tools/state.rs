//! Tools SQL within a caller-owned transaction. Never acquire another pool connection.
use super::*;
use sqlx::{Row, SqliteConnection};

pub(super) async fn create_reminder(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to create reminder: {error}"))?;

    fetch_reminder_by_id(&mut *connection, result.last_insert_rowid()).await
}

pub(super) async fn cancel_reminder(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to cancel reminder: {error}"))?;
    Ok(())
}

pub(super) async fn create_software_reminder_rule(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to create software reminder rule: {error}"))?;

    fetch_software_reminder_rule_by_id(&mut *connection, result.last_insert_rowid()).await
}

pub(super) async fn disable_software_reminder_rule(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to disable software reminder rule: {error}"))?;
    Ok(())
}

pub(super) async fn fire_due_reminders(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<Vec<ToolReminder>, String> {
    let rows = sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         WHERE status = ? AND scheduled_at <= ?
         ORDER BY scheduled_at ASC, id ASC",
    )
    .bind(ReminderStatus::Scheduled.as_str())
    .bind(now_ms)
    .fetch_all(&mut *connection)
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
        .execute(&mut *connection)
        .await
        .map_err(|error| format!("failed to mark reminder fired: {error}"))?;
    }

    Ok(reminders
        .into_iter()
        .map(|mut reminder| {
            reminder.status = ReminderStatus::Fired;
            reminder.fired_at = Some(now_ms);
            reminder
        })
        .collect())
}

pub(super) async fn fire_due_software_reminders(
    connection: &mut SqliteConnection,
    date_key: &str,
    day_start_ms: i64,
    now_ms: i64,
) -> Result<Vec<SoftwareReminderNotification>, String> {
    let rules = fetch_active_software_reminder_rules(&mut *connection).await?;
    let mut notifications = Vec::new();

    for rule in rules {
        if rule.last_fired_date_key.as_deref() == Some(date_key) {
            continue;
        }

        let usage_ms =
            fetch_software_usage_ms_today(&mut *connection, &rule, day_start_ms, now_ms).await?;
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
        .execute(&mut *connection)
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

    Ok(notifications)
}

pub(super) async fn start_timer(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to start timer: {error}"))?;

    fetch_timer_by_id(&mut *connection, result.last_insert_rowid()).await
}

pub(super) async fn pause_timer(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to pause timer: {error}"))?;
    Ok(())
}

pub(super) async fn resume_timer(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to resume timer: {error}"))?;
    Ok(())
}

pub(super) async fn reset_timer(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to reset timer: {error}"))?;
    clear_laps_for_timer(&mut *connection, timer.id).await?;
    Ok(())
}

pub(super) async fn add_timer_lap(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<Option<ToolTimerLap>, String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
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
    .fetch_optional(&mut *connection)
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to add timer lap: {error}"))?;

    let lap_id = result.last_insert_rowid();
    let lap = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         WHERE id = ?",
    )
    .bind(lap_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| format!("failed to read timer lap: {error}"))
    .map(map_timer_lap_row)?;

    Ok(Some(lap))
}

pub(super) async fn complete_due_countdown(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<Option<CompletedTimerNotification>, String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to complete countdown: {error}"))?;

    Ok(Some(CompletedTimerNotification {
        timer_id: timer.id,
        label: timer.label,
    }))
}

pub(super) async fn pause_running_stopwatch_after_restart(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<bool, String> {
    let Some(timer) = fetch_latest_timer(&mut *connection).await? else {
        return Ok(false);
    };
    if timer.mode != TimerMode::Stopwatch || timer.status != TimerStatus::Running {
        return Ok(false);
    }

    pause_timer(&mut *connection, now_ms).await?;
    Ok(true)
}

pub(super) async fn start_pomodoro(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to start pomodoro: {error}"))?;

    fetch_pomodoro_by_id(&mut *connection, result.last_insert_rowid()).await
}

pub(super) async fn pause_pomodoro(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to pause pomodoro: {error}"))?;
    Ok(())
}

pub(super) async fn resume_pomodoro(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to resume pomodoro: {error}"))?;
    Ok(())
}

pub(super) async fn skip_pomodoro_phase(
    connection: &mut SqliteConnection,
    date_key: &str,
    now_ms: i64,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    advance_pomodoro_phase(&mut *connection, date_key, now_ms, false, false).await
}

pub(super) async fn complete_due_pomodoro_phase(
    connection: &mut SqliteConnection,
    date_key: &str,
    now_ms: i64,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    let Some(run) = fetch_latest_pomodoro(&mut *connection).await? else {
        return Ok(None);
    };
    if !run.is_phase_due(now_ms) {
        return Ok(None);
    }

    advance_pomodoro_phase(&mut *connection, date_key, now_ms, true, true).await
}

async fn advance_pomodoro_phase(
    connection: &mut SqliteConnection,
    date_key: &str,
    now_ms: i64,
    count_focus_completion: bool,
    start_next_phase: bool,
) -> Result<Option<CompletedPomodoroNotification>, String> {
    let Some(run) = fetch_latest_pomodoro(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to advance pomodoro phase: {error}"))?;

    let counted_focus = count_focus_completion && run.phase == PomodoroPhase::Focus;
    if counted_focus {
        increment_daily_pomodoro_stat(&mut *connection, date_key, now_ms).await?;
    }

    Ok(Some(CompletedPomodoroNotification {
        run_id: run.id,
        completed_phase: run.phase,
        next_phase,
        completed_focus_count: next_completed_focus_count,
    }))
}

pub(super) async fn reset_pomodoro(
    connection: &mut SqliteConnection,
    now_ms: i64,
) -> Result<(), String> {
    let Some(run) = fetch_latest_pomodoro(&mut *connection).await? else {
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
    .execute(&mut *connection)
    .await
    .map_err(|error| format!("failed to reset pomodoro: {error}"))?;
    Ok(())
}

pub(super) async fn fetch_tools_snapshot(
    connection: &mut SqliteConnection,
    now_ms: i64,
    date_key: &str,
) -> Result<ToolsRuntimeSnapshot, String> {
    let settings = ToolRuntimeSettings::default();
    let reminders = fetch_visible_reminders(&mut *connection).await?;
    let software_reminder_rules = fetch_active_software_reminder_rules(&mut *connection).await?;
    let current_timer = fetch_latest_timer(&mut *connection)
        .await?
        .filter(|timer| timer.status != TimerStatus::Idle);
    let timer_laps = match &current_timer {
        Some(timer) => fetch_timer_laps(&mut *connection, timer.id).await?,
        None => Vec::new(),
    };
    let current_pomodoro = fetch_latest_pomodoro(&mut *connection).await?;
    let today_completed_pomodoros = fetch_daily_pomodoro_count(&mut *connection, date_key).await?;
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

async fn fetch_reminder_by_id(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<ToolReminder, String> {
    sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| format!("failed to read reminder: {error}"))
    .map(map_reminder_row)
}

async fn fetch_software_reminder_rule_by_id(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<ToolSoftwareReminderRule, String> {
    sqlx::query(
        "SELECT id, app_name, exe_name, limit_ms, message, created_at, updated_at,
                disabled_at, last_fired_date_key
         FROM tool_software_reminder_rules
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| format!("failed to read software reminder rule: {error}"))
    .map(map_software_reminder_rule_row)
}

async fn fetch_active_software_reminder_rules(
    connection: &mut SqliteConnection,
) -> Result<Vec<ToolSoftwareReminderRule>, String> {
    let rows = sqlx::query(
        "SELECT id, app_name, exe_name, limit_ms, message, created_at, updated_at,
                disabled_at, last_fired_date_key
         FROM tool_software_reminder_rules
         WHERE disabled_at IS NULL
         ORDER BY created_at ASC, id ASC",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to load active software reminder rules: {error}"))?;

    Ok(rows
        .into_iter()
        .map(map_software_reminder_rule_row)
        .collect())
}

async fn fetch_software_usage_ms_today(
    connection: &mut SqliteConnection,
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
        .fetch_one(&mut *connection)
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
        .fetch_one(&mut *connection)
        .await
    }
    .map_err(|error| format!("failed to read software reminder usage: {error}"))?;

    Ok(row.get::<i64, _>("usage_ms"))
}

async fn fetch_visible_reminders(
    connection: &mut SqliteConnection,
) -> Result<Vec<ToolReminder>, String> {
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
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to load reminders: {error}"))?;

    Ok(rows.into_iter().map(map_reminder_row).collect())
}

async fn fetch_timer_by_id(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<ToolTimer, String> {
    sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| format!("failed to read timer: {error}"))
    .map(map_timer_row)
}

async fn fetch_latest_timer(
    connection: &mut SqliteConnection,
) -> Result<Option<ToolTimer>, String> {
    sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         ORDER BY updated_at DESC, id DESC
         LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|error| format!("failed to read current timer: {error}"))
    .map(|row| row.map(map_timer_row))
}

async fn fetch_timer_laps(
    connection: &mut SqliteConnection,
    timer_id: i64,
) -> Result<Vec<ToolTimerLap>, String> {
    let rows = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         WHERE timer_id = ?
         ORDER BY lap_index ASC, id ASC",
    )
    .bind(timer_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| format!("failed to read timer laps: {error}"))?;

    Ok(rows.into_iter().map(map_timer_lap_row).collect())
}

async fn clear_laps_for_timer(
    connection: &mut SqliteConnection,
    timer_id: i64,
) -> Result<(), String> {
    sqlx::query("DELETE FROM tool_timer_laps WHERE timer_id = ?")
        .bind(timer_id)
        .execute(&mut *connection)
        .await
        .map_err(|error| format!("failed to clear timer laps: {error}"))?;
    Ok(())
}

async fn fetch_pomodoro_by_id(
    connection: &mut SqliteConnection,
    id: i64,
) -> Result<ToolPomodoroRun, String> {
    sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .map_err(|error| format!("failed to read pomodoro run: {error}"))
    .map(map_pomodoro_row)
}

async fn fetch_latest_pomodoro(
    connection: &mut SqliteConnection,
) -> Result<Option<ToolPomodoroRun>, String> {
    sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         ORDER BY updated_at DESC, id DESC
         LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|error| format!("failed to read current pomodoro: {error}"))
    .map(|row| row.map(map_pomodoro_row))
}

async fn fetch_daily_pomodoro_count(
    connection: &mut SqliteConnection,
    date_key: &str,
) -> Result<i64, String> {
    sqlx::query_scalar(
        "SELECT completed_pomodoros
         FROM tool_daily_stats
         WHERE date_key = ?
         LIMIT 1",
    )
    .bind(date_key)
    .fetch_optional(&mut *connection)
    .await
    .map_err(|error| format!("failed to read tool daily stats: {error}"))
    .map(|value| value.unwrap_or(0))
}

async fn increment_daily_pomodoro_stat(
    connection: &mut SqliteConnection,
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
    .execute(&mut *connection)
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
