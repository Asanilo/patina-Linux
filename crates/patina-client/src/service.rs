use crate::{Client, ClientError};
use patina_protocol::service::{DaemonServiceRestartResult, DaemonServiceRuntimeSnapshot};

impl Client {
    pub async fn service_snapshot(&self) -> Result<DaemonServiceRuntimeSnapshot, ClientError> {
        let snapshot = self
            .get_json("/api/v1/system/service", "daemon service")
            .await?;
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    /// Explicitly request a restart once. A lost response never retries the POST.
    /// The caller must separately verify the new instance, ticket, readiness and target.
    pub async fn restart_service(&self) -> Result<DaemonServiceRestartResult, ClientError> {
        let capabilities = self.capabilities().await?;
        if !capabilities.daemon_service.owned
            || !capabilities.daemon_service.ready
            || !capabilities.write_api.available
            || !capabilities
                .write_api
                .operations
                .iter()
                .any(|scope| scope == "service-lifecycle")
        {
            return Err(ClientError::UnsupportedCapability(
                "service-lifecycle".into(),
            ));
        }
        crate::negotiate_tracking_capabilities(capabilities)?;
        let result: DaemonServiceRestartResult = self
            .post_json(
                "/api/v1/system/service/restart",
                &serde_json::json!({"confirmed":true}),
                "daemon restart",
            )
            .await?;
        validate_snapshot(&result.service)?;
        Ok(result)
    }
}

fn validate_snapshot(snapshot: &DaemonServiceRuntimeSnapshot) -> Result<(), ClientError> {
    let invalid = || ClientError::InvalidResponse("invalid daemon service identity".into());
    if snapshot.service_name != "patinad.service"
        || !text(&snapshot.instance_id, 128)
        || snapshot
            .executable_error
            .as_ref()
            .is_some_and(|error| !text(error, 4096))
    {
        return Err(invalid());
    }
    if let Some(identity) = &snapshot.executable {
        let build = &identity.build;
        if snapshot.executable_error.is_some()
            || identity.binary_sha256.len() != 64
            || !identity
                .binary_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || build.format_version != patina_protocol::build_info::BUILD_INFO_FORMAT_VERSION
            || !text(&build.package_version, 128)
            || !text(&build.target, 128)
            || build.protocol.min_supported_client == 0
            || !(build.protocol.min_supported_client..=build.protocol.max_supported_client)
                .contains(&build.protocol.current)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
