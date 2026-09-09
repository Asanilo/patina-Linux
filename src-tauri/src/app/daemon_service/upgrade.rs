use std::time::Duration;

use crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot;
use crate::platform::daemon_client::PatinadClient;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DaemonVersionDiagnostics {
    pub desktop_version: String,
    pub running_version: Option<String>,
    pub restart_available: bool,
    pub error: Option<String>,
}

pub async fn inspect(client: &PatinadClient, control_available: bool) -> DaemonVersionDiagnostics {
    let mut result = DaemonVersionDiagnostics {
        desktop_version: env!("CARGO_PKG_VERSION").to_string(),
        running_version: None,
        restart_available: false,
        error: None,
    };
    match client.negotiate_tracking_owner().await {
        Ok(negotiated) => {
            let different = negotiated.server_version != result.desktop_version;
            result.running_version = Some(negotiated.server_version);
            match client.service_snapshot().await {
                Ok(service) => {
                    result.restart_available = control_available
                        && different
                        && service.managed_by_systemd
                        && service.service_name == "patinad.service"
                        && service
                            .restart
                            .is_none_or(|restart| restart.status != "pending");
                    if result.restart_available {
                        match client.capabilities().await {
                            Ok(capabilities) => {
                                result.restart_available = capabilities.daemon_service.owned
                                    && capabilities.daemon_service.ready
                                    && capabilities.write_api.available
                                    && capabilities
                                        .write_api
                                        .operations
                                        .iter()
                                        .any(|scope| scope == "service-lifecycle")
                            }
                            Err(error) => {
                                result.restart_available = false;
                                result.error = Some(error.to_string());
                            }
                        }
                    }
                }
                Err(error) => result.error = Some(error.to_string()),
            }
        }
        Err(error) => result.error = Some(error.to_string()),
    }
    result
}

pub async fn restart_and_verify(
    client: &PatinadClient,
    expected_running_version: &str,
) -> Result<(), String> {
    let current = inspect(client, true).await;
    if !current.restart_available
        || current.running_version.as_deref() != Some(expected_running_version)
    {
        return Err("daemon state changed or reload is unavailable; refresh diagnostics before confirming again".to_string());
    }
    let before = client
        .service_snapshot()
        .await
        .map_err(|error| error.to_string())?;
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
        .ok_or_else(|| "daemon did not return a matching pending restart ticket".to_string())?;
    if !requested.reconnect_required {
        return Err("daemon did not acknowledge a controlled restart".to_string());
    }

    // Never retry the POST: a lost response may still have scheduled a restart.
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            if let Ok(service) = client.service_snapshot().await {
                if restart_completed(&service, &ticket.request_id, &before.instance_id) {
                    if let Ok(negotiated) = client.negotiate_tracking_owner().await {
                        if negotiated.server_version != current.desktop_version {
                            return Err(format!(
                                "daemon restarted but version is {}; desktop is {}. Check the installed package before retrying",
                                negotiated.server_version, current.desktop_version,
                            ));
                        }
                        if negotiated.tracking_ready { return Ok(()); }
                    }
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
mod tests {
    use super::*;
    use crate::engine::api::runtime_control::DaemonServiceRestartSnapshot;

    #[tokio::test]
    async fn reload_transport_posts_once_and_verifies_the_result() {
        use axum::{
            body::Bytes,
            http::{HeaderMap, Method, StatusCode, Uri},
            Json, Router,
        };
        use serde_json::json;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        for mode in ["success", "wrong-version", "rejected", "wrong-ticket"] {
            let posts = Arc::new(AtomicUsize::new(0));
            let seen = posts.clone();
            let router = Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                let posts = seen.clone();
                async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer isolated-test-token");
                    let restarted = posts.load(Ordering::SeqCst) > 0;
                    let ticket = |completed: bool| json!({
                        "request_id": "ticket", "status": if completed {"completed"} else {"pending"},
                        "requested_at_ms": 1, "requested_instance_id": if mode == "wrong-ticket" {"unrelated"} else {"old"},
                        "completed_at_ms": if completed {Some(2)} else {None},
                        "completed_instance_id": if completed {Some("new")} else {None},
                    });
                    let service = |completed: bool| json!({
                        "service_name": "patinad.service", "managed_by_systemd": true,
                        "instance_id": if completed {"new"} else {"old"},
                        "restart": if completed {ticket(true)} else {serde_json::Value::Null},
                    });
                    let data = match (method, uri.path()) {
                        (Method::GET, "/api/v1/capabilities") => json!({
                            "server_version": if restarted && mode != "wrong-version" {env!("CARGO_PKG_VERSION")} else {"old-version"},
                            "protocol_version": 2, "protocol": {"current":2,"min_supported_client":1,"max_supported_client":2},
                            "runtime_host":"daemon", "event_stream":{"available":true},
                            "tracking":{"owned":true,"ready":true}, "browser_activity_bridge":{"owned":true,"ready":true},
                            "tools":{"owned":true,"ready":true}, "daemon_service":{"owned":true,"ready":true},
                            "write_api":{"available":true,"operations":["service-lifecycle"]},
                        }),
                        (Method::GET, "/api/v1/system/service") => service(restarted),
                        (Method::POST, "/api/v1/system/service/restart") => {
                            assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap(), json!({"confirmed":true}));
                            posts.fetch_add(1, Ordering::SeqCst);
                            if mode == "rejected" {
                                return (StatusCode::CONFLICT, Json(json!({"error":{"code":"conflict","message":"busy"}})));
                            }
                            let mut value = service(false);
                            value["restart"] = ticket(false);
                            return (StatusCode::ACCEPTED, Json(json!({"data":{"service":value,"reconnect_required":true}})));
                        }
                        _ => panic!("unexpected test request"),
                    };
                    (StatusCode::OK, Json(json!({"data":data})))
                }
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let task = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            let client = PatinadClient::new(port, "isolated-test-token").unwrap();
            assert!(!inspect(&client, false).await.restart_available);
            assert!(restart_and_verify(&client, "stale-confirmation")
                .await
                .is_err());
            assert_eq!(posts.load(Ordering::SeqCst), 0);
            let result = restart_and_verify(&client, "old-version").await;
            task.abort();
            let _ = task.await;
            assert_eq!(result.is_ok(), mode == "success", "{mode}: {result:?}");
            assert_eq!(posts.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn completion_requires_matching_ticket_and_new_instance() {
        let mut service = DaemonServiceRuntimeSnapshot {
            service_name: "patinad.service".to_string(),
            managed_by_systemd: true,
            instance_id: "new".to_string(),
            restart: Some(DaemonServiceRestartSnapshot {
                request_id: "ticket".to_string(),
                status: "completed".to_string(),
                requested_at_ms: 1,
                requested_instance_id: "old".to_string(),
                completed_at_ms: Some(2),
                completed_instance_id: Some("new".to_string()),
            }),
        };
        assert!(restart_completed(&service, "ticket", "old"));
        assert!(!restart_completed(&service, "other", "old"));
        assert!(!restart_completed(&service, "ticket", "new"));
        service.restart.as_mut().unwrap().status = "pending".to_string();
        assert!(!restart_completed(&service, "ticket", "old"));
        service.restart.as_mut().unwrap().status = "completed".to_string();
        service.restart.as_mut().unwrap().completed_instance_id = Some("other".to_string());
        assert!(!restart_completed(&service, "ticket", "old"));
        service.restart = None;
        assert!(!restart_completed(&service, "ticket", "old"));
    }
}
