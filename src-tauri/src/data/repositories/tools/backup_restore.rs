use crate::domain::backup::{
    BackupToolDailyStats, BackupToolPomodoroRun, BackupToolReminder, BackupToolTimer,
    BackupToolTimerLap,
};
use sqlx::{Row, Sqlite, Transaction};

pub async fn fetch_all_reminders_for_backup<'e, E>(
    executor: E,
) -> Result<Vec<BackupToolReminder>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        "SELECT id, label, scheduled_at, created_at, status, fired_at, cancelled_at
         FROM tool_reminders
         ORDER BY id ASC",
    )
    .fetch_all(executor)
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

pub async fn fetch_all_timers_for_backup<'e, E>(executor: E) -> Result<Vec<BackupToolTimer>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        "SELECT id, mode, label, duration_ms, accumulated_ms, started_at, paused_at,
                completed_at, status, created_at, updated_at
         FROM tool_timers
         ORDER BY id ASC",
    )
    .fetch_all(executor)
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

pub async fn fetch_all_timer_laps_for_backup<'e, E>(
    executor: E,
) -> Result<Vec<BackupToolTimerLap>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        "SELECT id, timer_id, lap_index, started_at, ended_at, duration_ms
         FROM tool_timer_laps
         ORDER BY id ASC",
    )
    .fetch_all(executor)
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

pub async fn fetch_all_pomodoro_runs_for_backup<'e, E>(
    executor: E,
) -> Result<Vec<BackupToolPomodoroRun>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        "SELECT id, phase, status, cycle_index, focus_ms, short_break_ms, long_break_ms,
                long_break_every, phase_started_at, phase_paused_at, phase_remaining_ms,
                completed_focus_count, created_at, updated_at
         FROM tool_pomodoro_runs
         ORDER BY id ASC",
    )
    .fetch_all(executor)
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

pub async fn fetch_all_daily_stats_for_backup<'e, E>(
    executor: E,
) -> Result<Vec<BackupToolDailyStats>, String>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        "SELECT date_key, completed_pomodoros, updated_at
         FROM tool_daily_stats
         ORDER BY date_key ASC",
    )
    .fetch_all(executor)
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
