use crate::domain::settings::WebActivitySettings;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::surface::ApiSurface;
use crate::engine::api::types::{
    ApiResponse, AvailabilityCapability, CapabilitiesResponse, OwnedRuntimeCapability,
    RouteResponse,
};

pub fn get_capabilities(context: &ApiRuntimeContext, surface: ApiSurface) -> RouteResponse {
    let tracking_ready = context.tracking_snapshot().is_some();
    let browser_bridge_ready = context
        .web_activity_snapshot(&WebActivitySettings::default())
        .is_some();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_capabilities(surface, tracking_ready, browser_bridge_ready),
        })
        .unwrap_or_default(),
    }
}

fn build_capabilities(
    surface: ApiSurface,
    tracking_ready: bool,
    browser_bridge_ready: bool,
) -> CapabilitiesResponse {
    let owns_tracking = surface.owns_tracking();
    let owns_browser_activity_bridge = surface.owns_browser_activity_bridge();
    CapabilitiesResponse {
        protocol_version: 1,
        runtime_host: surface.runtime_host().to_string(),
        event_stream: AvailabilityCapability {
            available: surface.has_event_stream(),
        },
        tracking: OwnedRuntimeCapability {
            owned: owns_tracking,
            ready: owns_tracking && tracking_ready,
        },
        browser_activity_bridge: OwnedRuntimeCapability {
            owned: owns_browser_activity_bridge,
            ready: owns_browser_activity_bridge && browser_bridge_ready,
        },
        write_api: AvailabilityCapability {
            available: surface.has_write_api(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_capabilities_do_not_claim_runtime_owners_before_migration() {
        let capabilities = build_capabilities(ApiSurface::DaemonReadOnly, true, true);

        assert_eq!(capabilities.protocol_version, 1);
        assert_eq!(capabilities.runtime_host, "daemon");
        assert!(capabilities.event_stream.available);
        assert!(!capabilities.tracking.owned);
        assert!(!capabilities.tracking.ready);
        assert!(!capabilities.browser_activity_bridge.owned);
        assert!(!capabilities.browser_activity_bridge.ready);
        assert!(!capabilities.write_api.available);
    }

    #[test]
    fn desktop_capabilities_reflect_live_snapshot_readiness() {
        let ready = build_capabilities(ApiSurface::Desktop, true, true);
        assert_eq!(ready.runtime_host, "desktop");
        assert!(!ready.event_stream.available);
        assert!(ready.tracking.owned);
        assert!(ready.tracking.ready);
        assert!(ready.browser_activity_bridge.owned);
        assert!(ready.browser_activity_bridge.ready);
        assert!(ready.write_api.available);

        let unavailable = build_capabilities(ApiSurface::Desktop, false, false);
        assert!(unavailable.tracking.owned);
        assert!(!unavailable.tracking.ready);
        assert!(unavailable.browser_activity_bridge.owned);
        assert!(!unavailable.browser_activity_bridge.ready);
    }
}
