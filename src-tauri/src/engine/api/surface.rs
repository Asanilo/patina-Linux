#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiSurface {
    Desktop,
    DaemonStage0,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApiEndpoint {
    pub method: &'static str,
    pub path: &'static str,
}

const STAGE_ZERO_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/health",
    },
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/openapi.json",
    },
];

const DESKTOP_ENDPOINTS: &[ApiEndpoint] = &[
    ApiEndpoint {
        method: "GET",
        path: "/api/v1/health",
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

impl ApiSurface {
    pub fn endpoints(self) -> &'static [ApiEndpoint] {
        match self {
            Self::Desktop => DESKTOP_ENDPOINTS,
            Self::DaemonStage0 => STAGE_ZERO_ENDPOINTS,
        }
    }

    pub fn allows(self, method: &str, path: &str) -> bool {
        self.endpoints()
            .iter()
            .any(|endpoint| endpoint.method == method && endpoint.path == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn stage_zero_surface_contains_only_health_and_openapi() {
        assert_eq!(
            ApiSurface::DaemonStage0.endpoints(),
            [
                ApiEndpoint {
                    method: "GET",
                    path: "/api/v1/health"
                },
                ApiEndpoint {
                    method: "GET",
                    path: "/api/v1/openapi.json"
                },
            ]
        );
    }

    #[test]
    fn desktop_surface_keeps_existing_method_and_path_set() {
        assert_eq!(ApiSurface::Desktop.endpoints().len(), 19);
        assert!(ApiSurface::Desktop.allows("GET", "/api/v1/sessions"));
        assert!(ApiSurface::Desktop.allows("POST", "/api/v1/apps/{exe_name}/rename"));
        assert!(ApiSurface::Desktop.allows("GET", "/api/v1/tools/snapshot"));
    }

    #[test]
    fn stage_zero_routes_and_openapi_are_bidirectionally_equal() {
        let response = crate::engine::api::handlers::openapi::get_openapi(ApiSurface::DaemonStage0);
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
        let enabled = ApiSurface::DaemonStage0
            .endpoints()
            .iter()
            .map(|endpoint| (endpoint.method.to_string(), endpoint.path.to_string()))
            .collect::<BTreeSet<_>>();

        assert_eq!(advertised, enabled);
        assert!(!response.body["paths"]
            .as_object()
            .unwrap()
            .contains_key("/api/v1/sessions"));
        for endpoint in ApiSurface::DaemonStage0.endpoints() {
            assert_eq!(
                crate::engine::api::router::route_minimal_request(endpoint.method, endpoint.path,)
                    .status,
                200
            );
        }
    }
}
