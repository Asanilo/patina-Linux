use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAlertKind {
    Reminder,
    Countdown,
    Pomodoro,
    SoftwareReminder,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ToolAlert {
    pub id: String,
    pub kind: ToolAlertKind,
    pub title: String,
    pub body: String,
    pub occurred_at: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RuntimeEvent {
    TrackingDataChanged { reason: String, changed_at_ms: u64 },
    ScheduledBackupChanged { changed_at_ms: u64 },
    ToolsRuntimeChanged { changed_at_ms: u64 },
    ToolAlert { alert: ToolAlert },
}

impl RuntimeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::TrackingDataChanged { .. } => "tracking-data-changed",
            Self::ScheduledBackupChanged { .. } => "scheduled-backup-changed",
            Self::ToolsRuntimeChanged { .. } => "tools-runtime-changed",
            Self::ToolAlert { .. } => "tool-alert",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeEventEnvelope {
    pub sequence: u64,
    pub event: RuntimeEvent,
}
