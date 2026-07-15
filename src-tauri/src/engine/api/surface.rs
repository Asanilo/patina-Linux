#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiSurface {
    Desktop,
    DaemonReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiEndpoint {
    pub method: &'static str,
    pub path: &'static str,
}

const DESKTOP_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/health",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/capabilities",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/openapi.json",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/diagnostics",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/current",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/sessions",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/sessions/active",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/today",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/range",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/week",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/trend",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/web-activity",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/ai/activity-context",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/apps",
    },
    ApiEndpoint {
        method: "POST",
        path: "/api/v1/apps/{exe_name}/classify",
    },
    ApiEndpoint {
        method: "POST",
        path: "/api/v1/apps/{exe_name}/rename",
    },
    ApiEndpoint {
        method: "POST",
        path: "/api/v1/apps/{exe_name}/exclude",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/settings/tracker",
    },
    ApiEndpoint {
        method: "POST",
        path: "/api/v1/settings/tracker/afk-threshold",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/tools/snapshot",
    },
];

const DAEMON_READ_ONLY_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/health",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/capabilities",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/events",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/openapi.json",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/diagnostics",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/current",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/sessions",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/sessions/active",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/today",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/range",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/summary/week",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/trend",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/web-activity",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/ai/activity-context",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/apps",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/settings/tracker",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/tools/snapshot",
    },
];

impl ApiSurface {
    pub fn runtime_host(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::DaemonReadOnly => "daemon",
        }
    }

    pub fn owns_tracking(self) -> bool {
        matches!(self, Self::Desktop)
    }

    pub fn owns_browser_activity_bridge(self) -> bool {
        matches!(self, Self::Desktop)
    }

    pub fn has_event_stream(self) -> bool {
        self.allows("GET", "/api/v1/events")
    }

    pub fn has_write_api(self) -> bool {
        self.endpoints()
            .iter()
            .any(|endpoint| endpoint.method != "GET")
    }

    pub fn endpoints(self) -> &'static [ApiEndpoint] {
        match self {
            Self::Desktop => DESKTOP_ENDPOINTS,
            Self::DaemonReadOnly => DAEMON_READ_ONLY_ENDPOINTS,
        }
    }

    pub fn allows(self, method: &str, path: &str) -> bool {
        self.endpoints()
            .iter()
            .any(|endpoint| endpoint.method == method && endpoint.path == path)
    }

    pub fn allows_request(self, method: &str, path: &str) -> bool {
        if self.allows(method, path) {
            return true;
        }

        let Some(remainder) = path.strip_prefix("/api/v1/apps/") else {
            return false;
        };
        let Some((exe_name, action)) = remainder.split_once('/') else {
            return false;
        };
        method == "POST"
            && !exe_name.is_empty()
            && !action.contains('/')
            && self.endpoints().iter().any(|endpoint| {
                endpoint.method == method
                    && endpoint.path == format!("/api/v1/apps/{{exe_name}}/{action}")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn desktop_surface_keeps_existing_method_and_path_set() {
        assert_eq!(ApiSurface::Desktop.endpoints().len(), 20);
        assert!(ApiSurface::Desktop.allows("GET", "/api/v1/sessions"));
        assert!(ApiSurface::Desktop.allows("GET", "/api/v1/capabilities"));
        assert!(!ApiSurface::Desktop.allows("GET", "/api/v1/events"));
        assert!(ApiSurface::Desktop.allows("POST", "/api/v1/apps/{exe_name}/rename"));
        assert!(ApiSurface::Desktop.allows("GET", "/api/v1/tools/snapshot"));
    }

    #[test]
    fn daemon_read_only_surface_matches_desktop_gets_plus_event_stream_and_no_post() {
        let desktop_gets = ApiSurface::Desktop
            .endpoints()
            .iter()
            .filter(|endpoint| endpoint.method == "GET")
            .copied()
            .collect::<Vec<_>>();
        let daemon_shared_gets = ApiSurface::DaemonReadOnly
            .endpoints()
            .iter()
            .filter(|endpoint| endpoint.path != "/api/v1/events")
            .copied()
            .collect::<Vec<_>>();

        assert_eq!(daemon_shared_gets, desktop_gets);
        assert!(ApiSurface::DaemonReadOnly
            .endpoints()
            .iter()
            .all(|endpoint| endpoint.method == "GET"));
        assert!(ApiSurface::DaemonReadOnly.allows("GET", "/api/v1/capabilities"));
        assert!(ApiSurface::DaemonReadOnly.allows("GET", "/api/v1/events"));
        assert!(!ApiSurface::DaemonReadOnly.allows_request("POST", "/api/v1/apps/ghostty/rename"));
        assert!(ApiSurface::Desktop.allows_request("POST", "/api/v1/apps/ghostty/rename"));
        assert!(!ApiSurface::Desktop.allows_request("POST", "/api/v1/apps/ghostty/not-rename"));
    }

    #[test]
    fn daemon_read_only_routes_and_openapi_are_bidirectionally_equal() {
        let response =
            crate::engine::api::handlers::openapi::get_openapi(ApiSurface::DaemonReadOnly);
        let advertised = response.body["paths"]
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(path, operations)| {
                operations
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(move |method| (method.to_ascii_uppercase(), path.clone()))
            })
            .collect::<BTreeSet<_>>();
        let enabled = ApiSurface::DaemonReadOnly
            .endpoints()
            .iter()
            .map(|endpoint| (endpoint.method.to_string(), endpoint.path.to_string()))
            .collect::<BTreeSet<_>>();

        assert_eq!(advertised, enabled);
        assert!(response.body["paths"]
            .as_object()
            .unwrap()
            .contains_key("/api/v1/sessions"));
        assert!(!response.body["paths"]
            .as_object()
            .unwrap()
            .contains_key("/api/v1/apps/{exe_name}/rename"));
    }
}
