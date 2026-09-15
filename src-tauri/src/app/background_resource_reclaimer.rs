use crate::app::state::{AppExitState, DesktopBehaviorState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

const RECLAIM_AFTER_LAST_WEBVIEW_SECS: u64 = 2;

#[derive(Debug, Default)]
pub(crate) struct BackgroundResourceReclaimerState {
    generation: AtomicU64,
    #[cfg(test)]
    reclaim_attempts: AtomicU64,
}

impl BackgroundResourceReclaimerState {
    fn request(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Relaxed) == generation
    }

    #[cfg(test)]
    pub(super) fn reclaim_attempt_count(&self) -> u64 {
        self.reclaim_attempts.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    fn record_reclaim_attempt(&self) {
        self.reclaim_attempts.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn schedule_after_webview_destroyed<R: Runtime + 'static>(app: AppHandle<R>) {
    let generation = app.state::<BackgroundResourceReclaimerState>().request();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(RECLAIM_AFTER_LAST_WEBVIEW_SECS)).await;

        let should_reclaim = should_reclaim_background_heap(
            app.state::<DesktopBehaviorState>()
                .snapshot()
                .should_optimize_background_resources(),
            app.state::<AppExitState>().is_exit_requested(),
            app.state::<BackgroundResourceReclaimerState>()
                .is_current(generation),
            app.webview_windows().is_empty(),
        );
        if !should_reclaim {
            return;
        }

        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let _ = tauri::async_runtime::spawn_blocking(move || {
            #[cfg(test)]
            app.state::<BackgroundResourceReclaimerState>()
                .record_reclaim_attempt();
            let started = std::time::Instant::now();
            let released = crate::platform::linux::resource::release_unused_heap_pages();
            eprintln!(
                "[desktop-resources] idle heap release: released={released}, elapsed_ms={}",
                started.elapsed().as_millis(),
            );
        })
        .await;
    });
}

fn should_reclaim_background_heap(
    enabled: bool,
    exit_requested: bool,
    latest_request: bool,
    no_webviews: bool,
) -> bool {
    enabled && !exit_requested && latest_request && no_webviews
}

#[cfg(test)]
mod tests {
    use super::{should_reclaim_background_heap, BackgroundResourceReclaimerState};

    #[test]
    fn a_new_destroy_request_invalidates_older_reclaim_work() {
        let state = BackgroundResourceReclaimerState::default();
        let first = state.request();
        let second = state.request();

        assert!(!state.is_current(first));
        assert!(state.is_current(second));
    }

    #[test]
    fn reclaim_requires_opt_in_idle_runtime_and_latest_empty_state() {
        for enabled in [false, true] {
            for exit_requested in [false, true] {
                for latest_request in [false, true] {
                    for no_webviews in [false, true] {
                        assert_eq!(
                            should_reclaim_background_heap(
                                enabled,
                                exit_requested,
                                latest_request,
                                no_webviews,
                            ),
                            enabled && !exit_requested && latest_request && no_webviews,
                        );
                    }
                }
            }
        }
    }
}
