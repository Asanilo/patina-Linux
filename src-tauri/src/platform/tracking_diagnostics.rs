use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PlatformTrackingDiagnostics {
    pub window_tracking: WindowTrackingDiagnostics,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WindowTrackingDiagnostics {
    pub status: String,
    pub reason: Option<String>,
    pub provider: String,
    pub session_type: Option<String>,
    pub desktop: Option<String>,
}

#[cfg(target_os = "linux")]
pub fn current() -> PlatformTrackingDiagnostics {
    PlatformTrackingDiagnostics {
        window_tracking: crate::platform::linux::foreground::window_tracking_diagnostics(),
    }
}

#[cfg(target_os = "windows")]
pub fn current() -> PlatformTrackingDiagnostics {
    PlatformTrackingDiagnostics {
        window_tracking: WindowTrackingDiagnostics {
            status: "available".to_string(),
            reason: None,
            provider: "windows-foreground-api".to_string(),
            session_type: Some("windows".to_string()),
            desktop: None,
        },
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn current() -> PlatformTrackingDiagnostics {
    PlatformTrackingDiagnostics {
        window_tracking: WindowTrackingDiagnostics {
            status: "unsupported".to_string(),
            reason: Some("platform-unsupported".to_string()),
            provider: "none".to_string(),
            session_type: None,
            desktop: None,
        },
    }
}
