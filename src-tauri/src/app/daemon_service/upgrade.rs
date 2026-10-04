use crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot;
use crate::platform::daemon_client::PatinadClient;
use patina_protocol::service::DaemonExecutableIdentity;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

mod installation;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DaemonVersionDiagnostics {
    pub desktop_version: String,
    pub running_version: Option<String>,
    pub target_version: Option<String>,
    pub distribution: String,
    pub target_state: String,
    pub reload_revision: Option<String>,
    pub restart_available: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct ReloadTarget {
    version: String,
    identity: Option<DaemonExecutableIdentity>,
    manifest: Option<String>,
    root: Option<PathBuf>,
}
impl ReloadTarget {
    fn bundled() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").into(),
            identity: None,
            manifest: None,
            root: None,
        }
    }
    fn matches(&self, service: &DaemonServiceRuntimeSnapshot, version: &str) -> bool {
        version == self.version
            && self
                .identity
                .as_ref()
                .is_none_or(|identity| service.executable.as_ref() == Some(identity))
    }
    fn revision(
        &self,
        service: &DaemonServiceRuntimeSnapshot,
        version: &str,
    ) -> Result<String, String> {
        let value = serde_json::to_vec(&(self, &service.instance_id, version, &service.executable))
            .map_err(|error| error.to_string())?;
        Ok(format!("{:x}", Sha256::digest(value)))
    }
}

trait TargetSource {
    type Guard;
    async fn inspect(&self) -> Result<ReloadTarget, String>;
    async fn hold(&self, expected: &ReloadTarget) -> Result<Self::Guard, String>;
}

pub async fn inspect(
    client: &PatinadClient,
    control_available: bool,
    control_root: &Path,
) -> DaemonVersionDiagnostics {
    inspect_with_source(
        client,
        control_available,
        &installation::NativeSource::new(control_root),
    )
    .await
    .0
}

async fn inspect_with_source(
    client: &PatinadClient,
    control_available: bool,
    source: &impl TargetSource,
) -> (
    DaemonVersionDiagnostics,
    Option<ReloadTarget>,
    Option<DaemonServiceRuntimeSnapshot>,
) {
    let mut result = DaemonVersionDiagnostics {
        desktop_version: env!("CARGO_PKG_VERSION").into(),
        running_version: None,
        target_version: None,
        distribution: "unknown".into(),
        target_state: "unverified".into(),
        reload_revision: None,
        restart_available: false,
        error: None,
    };
    let observed = async {
        let before = client
            .service_snapshot()
            .await
            .map_err(|error| error.to_string())?;
        let negotiated = client
            .negotiate_tracking_owner()
            .await
            .map_err(|error| error.to_string())?;
        result.running_version = Some(negotiated.server_version.clone());
        let service = client
            .service_snapshot()
            .await
            .map_err(|error| error.to_string())?;
        if before.instance_id != service.instance_id || before.executable != service.executable {
            return Err("backend changed during diagnostics; refresh before reloading".to_string());
        }
        let target = source.inspect().await?;
        result.distribution = if target.identity.is_some() {
            "standalone"
        } else {
            "bundled"
        }
        .into();
        result.target_version = Some(target.version.clone());
        let pending = !target.matches(&service, &negotiated.server_version);
        result.target_state = if pending { "pending" } else { "current" }.into();
        if control_available
            && pending
            && service.managed_by_systemd
            && service.service_name == "patinad.service"
            && service
                .restart
                .as_ref()
                .is_none_or(|restart| restart.status != "pending")
        {
            let capabilities = client
                .capabilities()
                .await
                .map_err(|error| error.to_string())?;
            result.restart_available = capabilities.daemon_service.owned
                && capabilities.daemon_service.ready
                && capabilities.write_api.available
                && capabilities
                    .write_api
                    .operations
                    .iter()
                    .any(|scope| scope == "service-lifecycle");
            if result.restart_available {
                result.reload_revision =
                    Some(target.revision(&service, &negotiated.server_version)?);
            }
        }
        Ok::<_, String>((target, service))
    }
    .await;
    match observed {
        Ok((target, service)) => (result, Some(target), Some(service)),
        Err(error) => {
            result.restart_available = false;
            result.reload_revision = None;
            result.error = Some(error);
            (result, None, None)
        }
    }
}

pub async fn restart_and_verify(
    client: &PatinadClient,
    expected_running_version: &str,
    expected_revision: &str,
    control_root: &Path,
) -> Result<(), String> {
    restart_with_source(
        client,
        expected_running_version,
        expected_revision,
        &installation::NativeSource::new(control_root),
    )
    .await
}

async fn restart_with_source(
    client: &PatinadClient,
    expected_running_version: &str,
    expected_revision: &str,
    source: &impl TargetSource,
) -> Result<(), String> {
    let (current, target, before) = inspect_with_source(client, true, source).await;
    if !current.restart_available
        || current.running_version.as_deref() != Some(expected_running_version)
        || current.reload_revision.as_deref() != Some(expected_revision)
    {
        return Err(
            "daemon state or installed target changed; refresh diagnostics before confirming again"
                .into(),
        );
    }
    let target = target.ok_or("reload target unavailable")?;
    let before = before.ok_or("running backend identity unavailable")?;
    let _target_guard = source.hold(&target).await?;
    let observed = client
        .service_snapshot()
        .await
        .map_err(|error| error.to_string())?;
    if observed.instance_id != before.instance_id || observed.executable != before.executable {
        return Err("running backend changed after confirmation; refresh diagnostics".into());
    }
    let requested = client
        .restart_service()
        .await
        .map_err(|error| error.to_string())?;
    let ticket = requested
        .service
        .restart
        .as_ref()
        .filter(|ticket| {
            ticket.status == "pending" && ticket.requested_instance_id == before.instance_id
        })
        .ok_or("daemon did not return a matching pending restart ticket")?;
    if !requested.reconnect_required {
        return Err("daemon did not acknowledge a controlled restart".into());
    }

    // The target guard spans the whole operation. Never retry this POST: a lost
    // response may still have scheduled a restart. No automatic rollback.
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            if let Ok(service) = client.service_snapshot().await {
                if restart_completed(&service, &ticket.request_id, &before.instance_id) {
                    let negotiated = client.negotiate_tracking_owner().await
                        .map_err(|error| format!("restarted backend protocol could not be verified: {error}"))?;
                    let after = client.service_snapshot().await.map_err(|error| error.to_string())?;
                    if !restart_completed(&after, &ticket.request_id, &before.instance_id)
                        || after.instance_id != service.instance_id || after.executable != service.executable {
                        return Err("backend changed during restart verification; inspect service state".into());
                    }
                    if !target.matches(&after, &negotiated.server_version) {
                        return Err(format!("daemon restarted but does not match the confirmed installed target {}; inspect installation before retrying", target.version));
                    }
                    if negotiated.tracking_ready { return Ok(()); }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }).await.map_err(|_| "daemon restart completion was not confirmed within 45 seconds; inspect service state before retrying".to_string())?
}

fn restart_completed(
    service: &DaemonServiceRuntimeSnapshot,
    request_id: &str,
    old_instance: &str,
) -> bool {
    service.managed_by_systemd
        && service.service_name == "patinad.service"
        && service.instance_id != old_instance
        && service.restart.as_ref().is_some_and(|ticket| {
            ticket.request_id == request_id
                && ticket.status == "completed"
                && ticket.requested_instance_id == old_instance
                && ticket.completed_instance_id.as_deref() == Some(service.instance_id.as_str())
                && ticket.completed_at_ms.is_some()
        })
}

#[cfg(test)]
mod tests;
