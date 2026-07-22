use tokio::sync::watch;

pub(super) async fn wait_for_restart(
    shutdown: &mut watch::Receiver<bool>,
    retry_secs: u64,
) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(retry_secs)) => false,
        _ = shutdown.changed() => true,
    }
}
