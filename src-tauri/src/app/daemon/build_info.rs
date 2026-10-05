//! Pure build metadata; this path must never prepare a profile or runtime.
use crate::engine::api::protocol::{
    CURRENT_PROTOCOL_VERSION, MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION,
    MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION,
};
use patina_protocol::{
    build_info::{DaemonBuildInfo, BUILD_INFO_FORMAT_VERSION},
    ProtocolCapability,
};

pub fn current() -> DaemonBuildInfo {
    DaemonBuildInfo {
        format_version: BUILD_INFO_FORMAT_VERSION,
        package_version: crate::platform::build_metadata::ARTIFACT_VERSION.to_string(),
        protocol: ProtocolCapability {
            current: CURRENT_PROTOCOL_VERSION,
            min_supported_client: MIN_SUPPORTED_CLIENT_PROTOCOL_VERSION,
            max_supported_client: MAX_SUPPORTED_CLIENT_PROTOCOL_VERSION,
        },
        target: env!("PATINA_BUILD_TARGET").to_string(),
        desktop_feature: cfg!(feature = "desktop"),
        debug_assertions: cfg!(debug_assertions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_actual_build_and_server_protocol() {
        let info = current();
        assert_eq!(info.format_version, 1);
        assert_eq!(
            info.package_version,
            crate::platform::build_metadata::ARTIFACT_VERSION
        );
        assert!(semver::Version::parse(&info.package_version).is_ok());
        assert!(!info.target.is_empty());
        assert_eq!(info.desktop_feature, cfg!(feature = "desktop"));
        assert_eq!(info.debug_assertions, cfg!(debug_assertions));
        assert!(
            (info.protocol.min_supported_client..=info.protocol.max_supported_client)
                .contains(&CURRENT_PROTOCOL_VERSION)
        );
        let encoded = serde_json::to_string(&info).unwrap();
        assert_eq!(
            serde_json::from_str::<DaemonBuildInfo>(&encoded).unwrap(),
            info
        );
    }
}
