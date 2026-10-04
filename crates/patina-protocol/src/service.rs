//! Daemon instance diagnostics. Executable identity is not installation provenance.
use crate::build_info::DaemonBuildInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonExecutableIdentity {
    pub build: DaemonBuildInfo,
    pub binary_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonServiceRestartSnapshot {
    pub request_id: String,
    pub status: String,
    pub requested_at_ms: i64,
    pub requested_instance_id: String,
    pub completed_at_ms: Option<i64>,
    pub completed_instance_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonServiceRuntimeSnapshot {
    pub service_name: String,
    pub managed_by_systemd: bool,
    pub instance_id: String,
    pub restart: Option<DaemonServiceRestartSnapshot>,
    /// Absent on older daemons, or when the running image could not be measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<DaemonExecutableIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonServiceRestartResult {
    pub service: DaemonServiceRuntimeSnapshot,
    pub reconnect_required: bool,
}
