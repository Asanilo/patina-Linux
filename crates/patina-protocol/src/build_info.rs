//! Static executable identity for installation tooling, not runtime readiness.
use crate::ProtocolCapability;
use serde::{Deserialize, Serialize};

pub const BUILD_INFO_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonBuildInfo {
    pub format_version: u32,
    pub package_version: String,
    pub protocol: ProtocolCapability,
    pub target: String,
    pub desktop_feature: bool,
    pub debug_assertions: bool,
}
