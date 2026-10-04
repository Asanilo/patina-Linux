//! Shared deadlines for bounded analytical reads. A repository must finish
//! before its HTTP handler, and a client must allow time to receive that result.
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadBudget {
    pub query: Duration,
    pub handler: Duration,
    pub client: Duration,
}

const fn budget(query: u64, handler: u64, client: u64) -> ReadBudget {
    assert!(query < handler && handler < client);
    ReadBudget {query: Duration::from_secs(query), handler: Duration::from_secs(handler), client: Duration::from_secs(client)}
}

pub const ANALYTICS: ReadBudget = budget(30, 32, 35);
pub const OBSERVED_APPS: ReadBudget = budget(15, 17, 18);
pub const WEB_HISTORY: ReadBudget = budget(12, 15, 20);

/// `path` is the endpoint without a query. Callers decode the scope using their
/// URL implementation; the protocol crate needs no transport/runtime dependency.
pub fn for_endpoint(path: &str, legacy_migration_scope: bool) -> Option<ReadBudget> {
    match path {
        "/api/v1/heatmap" | "/api/v1/trend" | "/api/v1/activity/daily-apps"
        | "/api/v1/activity/daily-product" | "/api/v1/activity/dashboard"
        | "/api/v1/activity/history" | "/api/v1/activity/history-product" => Some(ANALYTICS),
        "/api/v1/classification/observed-apps" => Some(if legacy_migration_scope { ANALYTICS } else { OBSERVED_APPS }),
        "/api/v1/activity/web-history" => Some(WEB_HISTORY),
        _ => None,
    }
}
