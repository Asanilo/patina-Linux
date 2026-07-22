use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(super) struct DaemonMediaTask {
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl DaemonMediaTask {
    pub(super) fn start(source: crate::platform::linux::media::MediaSignalSource) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = tokio::spawn(async move {
            println!("[patinad] MPRIS participation source ready");
            source.run_with_shutdown(shutdown_rx).await;
        });
        Self {
            shutdown_tx,
            handle,
        }
    }

    pub(super) async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut handle = self.handle;
        if tokio::time::timeout(std::time::Duration::from_secs(5), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            let _ = handle.await;
        }
    }
}
