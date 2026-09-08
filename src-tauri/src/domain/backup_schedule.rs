use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

pub const DEFAULT_LOCAL_TIME_MINUTES: u16 = 21 * 60;
pub const SCHEDULED_BACKUP_KEEP_COUNT: u8 = 3;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduledBackupCadence {
    Daily,
    Weekly,
}

impl ScheduledBackupCadence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            _ => Err("scheduled backup cadence must be daily or weekly".to_string()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupConfigInput {
    pub enabled: bool,
    pub cadence: ScheduledBackupCadence,
    pub weekday: Option<u8>,
    pub local_time_minutes: u16,
    pub target_dir: String,
}

impl ScheduledBackupConfigInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.local_time_minutes >= 24 * 60 {
            return Err("scheduled backup time is outside the valid day".to_string());
        }
        if self.target_dir.trim().is_empty() {
            return Err("scheduled backup directory cannot be empty".to_string());
        }
        match self.cadence {
            ScheduledBackupCadence::Daily if self.weekday.is_some() => {
                Err("daily scheduled backup must not include a weekday".to_string())
            }
            ScheduledBackupCadence::Weekly if !matches!(self.weekday, Some(1..=7)) => {
                Err("weekly scheduled backup requires a weekday from 1 to 7".to_string())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupConfig {
    pub enabled: bool,
    pub cadence: ScheduledBackupCadence,
    pub weekday: Option<u8>,
    pub local_time_minutes: u16,
    pub target_dir: String,
    pub target_generation: String,
    pub schedule_anchor_at_ms: i64,
    pub updated_at_ms: i64,
}

impl ScheduledBackupConfig {
    pub fn validate(&self) -> Result<(), String> {
        ScheduledBackupConfigInput {
            enabled: self.enabled,
            cadence: self.cadence,
            weekday: self.weekday,
            local_time_minutes: self.local_time_minutes,
            target_dir: self.target_dir.clone(),
        }
        .validate()?;
        if self.target_generation.trim().is_empty() {
            return Err("scheduled backup target generation cannot be empty".to_string());
        }
        if self.schedule_anchor_at_ms < 0 || self.updated_at_ms < 0 {
            return Err("scheduled backup timestamps cannot be negative".to_string());
        }
        Ok(())
    }

    pub fn schedule_changed_from(&self, input: &ScheduledBackupConfigInput) -> bool {
        self.enabled != input.enabled
            || self.cadence != input.cadence
            || self.weekday != input.weekday
            || self.local_time_minutes != input.local_time_minutes
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupRun {
    pub run_key: String,
    pub target_generation: String,
    pub logical_date: String,
    pub logical_time_minutes: u16,
    pub target_path: String,
    pub status: String,
    pub file_state: String,
    pub attempt_count: u8,
    pub retry_at_ms: Option<i64>,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub archive_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub cleanup_warning: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledBackupSnapshot {
    pub config: ScheduledBackupConfig,
    pub next_execution_at_ms: Option<i64>,
    pub recent_success: Option<ScheduledBackupRun>,
    pub recent_failure: Option<ScheduledBackupRun>,
    pub active_run: Option<ScheduledBackupRun>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalBackupSlot {
    pub date: NaiveDate,
    pub local_time_minutes: u16,
}

impl LogicalBackupSlot {
    pub fn local_datetime(self) -> NaiveDateTime {
        self.date.and_time(
            NaiveTime::from_num_seconds_from_midnight_opt(
                u32::from(self.local_time_minutes) * 60,
                0,
            )
            .expect("validated scheduled backup time"),
        )
    }

    pub fn date_key(self) -> String {
        self.date.format("%Y-%m-%d").to_string()
    }

    pub fn run_key(self, generation: &str) -> String {
        let hours = self.local_time_minutes / 60;
        let minutes = self.local_time_minutes % 60;
        format!(
            "scheduled-backup:{generation}:{}:{hours:02}{minutes:02}",
            self.date_key()
        )
    }
}

pub fn latest_due_slot(
    now_local: NaiveDateTime,
    anchor_local: NaiveDateTime,
    config: &ScheduledBackupConfig,
) -> Option<LogicalBackupSlot> {
    if !config.enabled || now_local < anchor_local {
        return None;
    }
    let time = schedule_time(config.local_time_minutes)?;
    let mut date = now_local.date();
    if now_local.time() < time {
        date = date.pred_opt()?;
    }
    if config.cadence == ScheduledBackupCadence::Weekly {
        let target = weekday_from_number(config.weekday?)?;
        let days_back =
            (date.weekday().num_days_from_monday() + 7 - target.num_days_from_monday()) % 7;
        date = date.checked_sub_signed(Duration::days(i64::from(days_back)))?;
    }
    let slot = LogicalBackupSlot {
        date,
        local_time_minutes: config.local_time_minutes,
    };
    (slot.local_datetime() >= anchor_local).then_some(slot)
}

pub fn next_slot_after(
    now_local: NaiveDateTime,
    config: &ScheduledBackupConfig,
) -> Option<LogicalBackupSlot> {
    if !config.enabled {
        return None;
    }
    let time = schedule_time(config.local_time_minutes)?;
    let mut date = now_local.date();
    if config.cadence == ScheduledBackupCadence::Daily {
        if now_local.time() >= time {
            date = date.succ_opt()?;
        }
    } else {
        let target = weekday_from_number(config.weekday?)?;
        let mut days_forward =
            (target.num_days_from_monday() + 7 - date.weekday().num_days_from_monday()) % 7;
        if days_forward == 0 && now_local.time() >= time {
            days_forward = 7;
        }
        date = date.checked_add_signed(Duration::days(i64::from(days_forward)))?;
    }
    Some(LogicalBackupSlot {
        date,
        local_time_minutes: config.local_time_minutes,
    })
}

fn schedule_time(local_time_minutes: u16) -> Option<NaiveTime> {
    NaiveTime::from_num_seconds_from_midnight_opt(u32::from(local_time_minutes) * 60, 0)
}

fn weekday_from_number(value: u8) -> Option<Weekday> {
    match value {
        1 => Some(Weekday::Mon),
        2 => Some(Weekday::Tue),
        3 => Some(Weekday::Wed),
        4 => Some(Weekday::Thu),
        5 => Some(Weekday::Fri),
        6 => Some(Weekday::Sat),
        7 => Some(Weekday::Sun),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(cadence: ScheduledBackupCadence, weekday: Option<u8>) -> ScheduledBackupConfig {
        ScheduledBackupConfig {
            enabled: true,
            cadence,
            weekday,
            local_time_minutes: 21 * 60,
            target_dir: "/tmp/patina-backups".to_string(),
            target_generation: "generation".to_string(),
            schedule_anchor_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn enabling_after_the_slot_does_not_backfill_before_anchor() {
        let now = NaiveDate::from_ymd_opt(2026, 8, 9)
            .unwrap()
            .and_hms_opt(22, 0, 0)
            .unwrap();
        assert!(latest_due_slot(now, now, &config(ScheduledBackupCadence::Daily, None)).is_none());
    }

    #[test]
    fn weekly_schedule_selects_latest_matching_weekday() {
        let now = NaiveDate::from_ymd_opt(2026, 8, 9)
            .unwrap()
            .and_hms_opt(22, 0, 0)
            .unwrap();
        let anchor = NaiveDate::from_ymd_opt(2026, 7, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let slot = latest_due_slot(
            now,
            anchor,
            &config(ScheduledBackupCadence::Weekly, Some(5)),
        )
        .unwrap();
        assert_eq!(slot.date, NaiveDate::from_ymd_opt(2026, 8, 7).unwrap());
    }

    #[test]
    fn config_rejects_invalid_cadence_fields() {
        let mut input = ScheduledBackupConfigInput {
            enabled: true,
            cadence: ScheduledBackupCadence::Daily,
            weekday: Some(1),
            local_time_minutes: 0,
            target_dir: "/tmp".to_string(),
        };
        assert!(input.validate().is_err());
        input.cadence = ScheduledBackupCadence::Weekly;
        input.weekday = None;
        assert!(input.validate().is_err());
    }
}
