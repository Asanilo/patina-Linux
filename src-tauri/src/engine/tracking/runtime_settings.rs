pub fn set_idle_threshold(threshold_secs: u64) {
    #[cfg(target_os = "windows")]
    crate::platform::windows::foreground::cmd_set_afk_threshold(threshold_secs);
    #[cfg(target_os = "linux")]
    crate::platform::linux::foreground::cmd_set_afk_threshold(threshold_secs);
}

/// Shared owner operation for HTTP and the explicit embedded migration host.
/// Pause sealing and runtime policy publication happen under the same transition
/// lock used by sampling; a stale revision never changes platform/runtime state.
pub async fn commit_product_settings(
    context: &crate::engine::runtime_context::RuntimeContext,
    runtime_state: Option<&super::runtime_snapshot::TrackingRuntimeSnapshotState>,
    request: &patina_protocol::product_settings::ProductSettingsCommitRequest,
) -> Result<
    patina_protocol::product_settings::ProductSettingsSnapshot,
    crate::data::repositories::product_settings::conditional::CommitError,
> {
    let _guard = match runtime_state {
        Some(state) => Some(state.lock_transition().await),
        None => None,
    };
    let now_ms = context.now_ms();
    let boundary = runtime_state.and_then(|state| state.pending_probe_seal());
    let snapshot = crate::data::repositories::product_settings::conditional::commit(
        context.pool(),
        request,
        now_ms,
        boundary.map_or(now_ms, |value| value.min(now_ms)),
    )
    .await?;
    if let Some(state) = runtime_state {
        if request.patch.tracking_paused == Some(true) {
            if let Some(boundary) = boundary {
                state.acknowledge_probe_seal(boundary);
            }
        }
        if request.patch.tracking_paused.is_some() {
            state.note_tracking_policy_change();
        }
    }
    if let Some(seconds) = request.patch.idle_timeout_secs {
        set_idle_threshold(seconds);
    }
    Ok(snapshot)
}
