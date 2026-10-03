//! Read-side liveness policy. Heartbeat is evidence of owner progress, not a
//! guarantee about foreground-provider quality. Closed facts remain authoritative.
use patina_protocol::activity::{ActivityReadHealth, ActivityReadStatus};

pub const HEARTBEAT_STALE_AFTER_MS: i64 = 8_000;

pub fn resolve_read_health(heartbeat_ms: Option<i64>, sampled_at_ms: i64) -> ActivityReadHealth {
    let now = sampled_at_ms.max(0);
    // Sampling precedes transaction acquisition; a concurrently committed
    // heartbeat may be slightly newer. Bound that skew by the freshness window
    // and clamp it to this read's timestamp. A far-future value is not evidence.
    let heartbeat = heartbeat_ms
        .filter(|value| *value > 0 && *value <= now.saturating_add(HEARTBEAT_STALE_AFTER_MS))
        .map(|value| value.min(now));
    let status = match heartbeat {
        Some(value) if now - value <= HEARTBEAT_STALE_AFTER_MS => ActivityReadStatus::Healthy,
        Some(_) => ActivityReadStatus::Stale,
        None => ActivityReadStatus::Unavailable,
    };
    ActivityReadHealth {
        status,
        last_heartbeat_ms: heartbeat,
        live_cutoff_ms: if status == ActivityReadStatus::Healthy {
            now
        } else {
            heartbeat.unwrap_or(0)
        },
        stale_after_ms: HEARTBEAT_STALE_AFTER_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_or_invalid_owner_evidence_cannot_extrapolate_open_time() {
        let fresh = resolve_read_health(Some(10_000), 18_000);
        assert_eq!(fresh.status, ActivityReadStatus::Healthy);
        assert_eq!(fresh.live_cutoff_ms, 18_000);
        let stale = resolve_read_health(Some(10_000), 18_001);
        assert_eq!(stale.status, ActivityReadStatus::Stale);
        assert_eq!(stale.live_cutoff_ms, 10_000);
        for heartbeat in [None, Some(-1), Some(0), Some(30_000), Some(i64::MAX)] {
            let read = resolve_read_health(heartbeat, 18_000);
            assert_eq!(read.status, ActivityReadStatus::Unavailable);
            assert_eq!(read.live_cutoff_ms, 0);
        }
        assert_eq!(
            resolve_read_health(Some(19_000), 20_000).live_cutoff_ms,
            20_000
        );
        assert_eq!(
            resolve_read_health(Some(20_001), 20_000).live_cutoff_ms,
            20_000
        );
    }
}
