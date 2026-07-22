use tokio::sync::watch;
use tokio::task::JoinHandle;

pub(super) struct DaemonAudioTask {
    shutdown_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl DaemonAudioTask {
    pub(super) fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        source: crate::platform::linux::audio::AudioSignalSource,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = tokio::spawn(async move {
            let enabled =
                crate::data::repositories::app_settings::load_audio_participation_enabled(
                    context.pool(),
                )
                .await
                .unwrap_or_else(|error| {
                    eprintln!("[patinad] failed to load audio participation setting: {error}");
                    false
                });
            source.set_enabled(enabled);
            if enabled {
                println!("[patinad] audio participation source ready");
            }
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
