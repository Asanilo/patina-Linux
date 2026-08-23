use crate::domain::settings::WebActivitySettings;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::surface::ApiSurface;
use crate::engine::api::types::{
    ApiResponse, AvailabilityCapability, CapabilitiesResponse, OwnedRuntimeCapability,
    ProtocolCapability, RouteResponse, WriteApiCapability,
};

pub fn get_capabilities(context: &ApiRuntimeContext, surface: ApiSurface) -> RouteResponse {
    let tracking_ready = context.tracking_snapshot().is_some();
    let browser_bridge_ready = context
        .web_activity_snapshot(&WebActivitySettings::default())
        .is_some_and(|snapshot| snapshot.listening);

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_capabilities(
                context.version(),
                surface,
                tracking_ready,
                browser_bridge_ready,
                context.tools_runtime_ready(),
            ),
        })
        .unwrap_or_default(),
    }
}

fn build_capabilities(
    server_version: &str,
    surface: ApiSurface,
    tracking_ready: bool,
    browser_bridge_ready: bool,
    tools_ready: bool,
) -> CapabilitiesResponse {
    let owns_tracking = surface.owns_tracking();
    let owns_browser_activity_bridge = surface.owns_browser_activity_bridge();
    let owns_tools_runtime = surface.owns_tools_runtime();
    CapabilitiesResponse {
        server_version: server_version.to_string(),
        protocol_version: crate::engine::api::protocol::CURRENT_PROTOCOL_VERSION,
        protocol: ProtocolCapability {
            current: crate::engine::api::protocol::CURRENT_PROTOCOL_VERSION,
            min_supported_client:
                crate::engine::api::protocol::MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION,
            max_supported_client:
                crate::engine::api::protocol::MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION,
        },
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
        tools: OwnedRuntimeCapability {
            owned: owns_tools_runtime,
            ready: owns_tools_runtime && tools_ready,
        },
        write_api: WriteApiCapability {
            available: surface.has_write_api(),
            operations: surface
                .write_operations()
                .iter()
                .map(|operation| (*operation).to_string())
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_capabilities_do_not_claim_runtime_owners_before_migration() {
        let capabilities =
            build_capabilities("1.8.3", ApiSurface::DaemonReadOnly, true, true, true);

        assert_eq!(capabilities.server_version, "1.8.3");
        assert_eq!(capabilities.protocol_version, 1);
        assert_eq!(capabilities.protocol.current, 1);
        assert_eq!(capabilities.runtime_host, "daemon");
        assert!(capabilities.event_stream.available);
        assert!(!capabilities.tracking.owned);
        assert!(!capabilities.tracking.ready);
        assert!(!capabilities.browser_activity_bridge.owned);
        assert!(!capabilities.browser_activity_bridge.ready);
        assert!(!capabilities.tools.owned);
        assert!(!capabilities.tools.ready);
        assert!(!capabilities.write_api.available);
    }

    #[test]
    fn desktop_capabilities_reflect_live_snapshot_readiness() {
        let ready = build_capabilities("1.8.3", ApiSurface::Desktop, true, true, true);
        assert_eq!(ready.runtime_host, "desktop");
        assert!(!ready.event_stream.available);
        assert!(ready.tracking.owned);
        assert!(ready.tracking.ready);
        assert!(ready.browser_activity_bridge.owned);
        assert!(ready.browser_activity_bridge.ready);
        assert!(ready.tools.owned);
        assert!(ready.tools.ready);
        assert!(ready.write_api.available);

        let unavailable = build_capabilities("1.8.3", ApiSurface::Desktop, false, false, false);
        assert!(unavailable.tracking.owned);
        assert!(!unavailable.tracking.ready);
        assert!(unavailable.browser_activity_bridge.owned);
        assert!(!unavailable.browser_activity_bridge.ready);
        assert!(unavailable.tools.owned);
        assert!(!unavailable.tools.ready);
    }

    #[test]
    fn tracking_daemon_capabilities_are_owned_before_the_first_sample() {
        let starting = build_capabilities("1.8.3", ApiSurface::DaemonTracking, false, false, false);
        assert!(starting.tracking.owned);
        assert!(!starting.tracking.ready);
        assert!(starting.browser_activity_bridge.owned);
        assert!(!starting.browser_activity_bridge.ready);
        assert!(starting.tools.owned);
        assert!(!starting.tools.ready);
        assert!(starting.write_api.available);
        assert!(starting
            .write_api
            .operations
            .contains(&"classification".to_string()));
        assert!(starting
            .write_api
            .operations
            .contains(&"runtime-settings".to_string()));

        let ready = build_capabilities("1.8.3", ApiSurface::DaemonTracking, true, false, true);
        assert!(ready.tracking.owned);
        assert!(ready.tracking.ready);
        assert!(ready.browser_activity_bridge.owned);
        assert!(ready.tools.ready);
    }
}
