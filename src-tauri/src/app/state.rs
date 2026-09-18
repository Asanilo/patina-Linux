use crate::domain::settings::DesktopBehaviorSettings;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

#[derive(Debug, Default)]
pub(crate) struct DesktopBehaviorState {
    inner: Mutex<DesktopBehaviorSettings>,
}

impl DesktopBehaviorState {
    pub(crate) fn snapshot(&self) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }

    pub(crate) fn update_desktop_from_raw(
        &self,
        close_behavior: &str,
        minimize_behavior: &str,
    ) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = guard.with_raw_desktop_behavior(close_behavior, minimize_behavior);
                *guard
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = guard.with_raw_desktop_behavior(close_behavior, minimize_behavior);
                *guard
            }
        }
    }

    pub(crate) fn update_launch(
        &self,
        launch_at_login: bool,
        start_minimized: bool,
    ) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = guard.with_launch_behavior(launch_at_login, start_minimized);
                *guard
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = guard.with_launch_behavior(launch_at_login, start_minimized);
                *guard
            }
        }
    }

    pub(crate) fn update_background_tracking_at_login(
        &self,
        background_tracking_at_login: bool,
    ) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = guard.with_background_tracking_at_login(background_tracking_at_login);
                *guard
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = guard.with_background_tracking_at_login(background_tracking_at_login);
                *guard
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn update_background_optimization(
        &self,
        background_optimization: bool,
    ) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = guard.with_background_optimization(background_optimization);
                *guard
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = guard.with_background_optimization(background_optimization);
                *guard
            }
        }
    }

    pub(crate) fn update_background_resource_policy(
        &self,
        enabled: bool,
        delay_minutes: Option<u32>,
    ) -> bool {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = guard
            .with_background_optimization(enabled)
            .with_background_optimization_delay_minutes(
                delay_minutes.unwrap_or(guard.background_optimization_delay_minutes),
            );
        let changed = next != *guard;
        *guard = next;
        changed
    }

    pub(crate) fn replace(&self, next: DesktopBehaviorSettings) -> DesktopBehaviorSettings {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = next;
                *guard
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = next;
                *guard
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AppExitState {
    requested: AtomicBool,
}

impl AppExitState {
    pub(crate) fn request_exit(&self) {
        self.requested.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_exit_requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default)]
pub(crate) struct MainWindowLifecycleState {
    inner: Mutex<MainWindowLifecycle>,
}

#[derive(Debug, Default)]
struct MainWindowLifecycle {
    desired_visible: bool,
    hide_generation: u64,
}

impl MainWindowLifecycleState {
    pub(crate) fn reset_hidden_timer(&self) -> Option<u64> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.hide_generation = guard.hide_generation.wrapping_add(1);
        (!guard.desired_visible).then_some(guard.hide_generation)
    }

    pub(crate) fn show(&self) {
        match self.inner.lock() {
            Ok(mut guard) => {
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
            }
        }
    }

    pub(crate) fn hide(&self) -> u64 {
        match self.inner.lock() {
            Ok(mut guard) => {
                guard.desired_visible = false;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                guard.hide_generation
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.desired_visible = false;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                guard.hide_generation
            }
        }
    }

    pub(crate) fn should_destroy_hidden_window(&self, hide_generation: u64) -> bool {
        match self.inner.lock() {
            Ok(guard) => !guard.desired_visible && guard.hide_generation == hide_generation,
            Err(poisoned) => {
                let guard = poisoned.into_inner();
                !guard.desired_visible && guard.hide_generation == hide_generation
            }
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct WidgetWindowLifecycleState {
    inner: Mutex<WidgetWindowLifecycle>,
}

#[derive(Debug, Default)]
struct WidgetWindowLifecycle {
    create_in_progress: bool,
    desired_visible: bool,
    hide_generation: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WidgetShowCompletion {
    Show,
    Hidden { hide_generation: u64 },
}

impl WidgetWindowLifecycleState {
    pub(crate) fn show_existing(&self) {
        match self.inner.lock() {
            Ok(mut guard) => {
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
            }
        }
    }

    pub(crate) fn begin_show(&self) -> bool {
        match self.inner.lock() {
            Ok(mut guard) => {
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                if guard.create_in_progress {
                    return false;
                }

                guard.create_in_progress = true;
                true
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.desired_visible = true;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                if guard.create_in_progress {
                    return false;
                }

                guard.create_in_progress = true;
                true
            }
        }
    }

    pub(crate) fn finish_show(&self) -> WidgetShowCompletion {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.create_in_progress = false;
        if guard.desired_visible {
            WidgetShowCompletion::Show
        } else {
            // Return the cancellation generation under the same lock so a
            // later reopen invalidates the pending cleanup timer.
            WidgetShowCompletion::Hidden {
                hide_generation: guard.hide_generation,
            }
        }
    }

    pub(crate) fn hide(&self) -> u64 {
        match self.inner.lock() {
            Ok(mut guard) => {
                guard.desired_visible = false;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                guard.hide_generation
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.desired_visible = false;
                guard.hide_generation = guard.hide_generation.wrapping_add(1);
                guard.hide_generation
            }
        }
    }

    pub(crate) fn should_destroy_hidden_window(&self, hide_generation: u64) -> bool {
        match self.inner.lock() {
            Ok(guard) => {
                !guard.desired_visible
                    && !guard.create_in_progress
                    && guard.hide_generation == hide_generation
            }
            Err(poisoned) => {
                let guard = poisoned.into_inner();
                !guard.desired_visible
                    && !guard.create_in_progress
                    && guard.hide_generation == hide_generation
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MainWindowLifecycleState, WidgetShowCompletion, WidgetWindowLifecycleState};

    #[test]
    fn background_policy_reports_only_changes_and_preserves_other_desktop_preferences() {
        let state = super::DesktopBehaviorState::default();
        state.update_launch(false, false);
        assert!(state.update_background_resource_policy(true, Some(1)));
        assert!(!state.update_background_resource_policy(true, Some(1)));
        assert!(state.update_background_resource_policy(false, None));
        let snapshot = state.snapshot();
        assert!(!snapshot.background_optimization);
        assert_eq!(snapshot.background_optimization_delay_minutes, 1);
        assert!(!snapshot.launch_at_login);
        assert!(!snapshot.start_minimized);
    }

    #[test]
    fn background_policy_changes_invalidate_old_timers_and_reopen_cancels_new_timer() {
        let lifecycle = MainWindowLifecycleState::default();
        lifecycle.show();
        assert_eq!(lifecycle.reset_hidden_timer(), None);
        let old = lifecycle.hide();
        let rescheduled = lifecycle.reset_hidden_timer().unwrap();
        assert!(!lifecycle.should_destroy_hidden_window(old));
        assert!(lifecycle.should_destroy_hidden_window(rescheduled));
        lifecycle.show();
        assert!(!lifecycle.should_destroy_hidden_window(rescheduled));
        let next_hide = lifecycle.hide();
        assert!(!lifecycle.should_destroy_hidden_window(rescheduled));
        assert!(lifecycle.should_destroy_hidden_window(next_hide));
    }

    #[test]
    fn main_window_lifecycle_cancels_stale_destroy_after_show() {
        let state = MainWindowLifecycleState::default();

        state.show();
        let hide_generation = state.hide();
        assert!(state.should_destroy_hidden_window(hide_generation));
        state.show();

        assert!(!state.should_destroy_hidden_window(hide_generation));
    }

    #[test]
    fn main_window_lifecycle_rehide_invalidates_old_reclamation() {
        let state = MainWindowLifecycleState::default();
        let old = state.hide();
        state.show();
        let current = state.hide();
        assert!(!state.should_destroy_hidden_window(old));
        assert!(state.should_destroy_hidden_window(current));
    }

    #[test]
    fn widget_lifecycle_coalesces_concurrent_show_requests() {
        let state = WidgetWindowLifecycleState::default();

        assert!(state.begin_show());
        assert!(!state.begin_show());
        assert_eq!(state.finish_show(), WidgetShowCompletion::Show);
        assert!(state.begin_show());
    }

    #[test]
    fn widget_lifecycle_cancels_pending_show_after_hide() {
        let state = WidgetWindowLifecycleState::default();

        assert!(state.begin_show());
        let hide_generation = state.hide();
        assert_eq!(
            state.finish_show(),
            WidgetShowCompletion::Hidden { hide_generation }
        );
        assert!(state.should_destroy_hidden_window(hide_generation));
        assert!(state.begin_show());
        assert_eq!(state.finish_show(), WidgetShowCompletion::Show);
    }

    #[test]
    fn widget_lifecycle_cancels_stale_destroy_after_show() {
        let state = WidgetWindowLifecycleState::default();

        assert!(state.begin_show());
        assert_eq!(state.finish_show(), WidgetShowCompletion::Show);
        let hide_generation = state.hide();
        state.show_existing();

        assert!(!state.should_destroy_hidden_window(hide_generation));
    }

    #[test]
    fn widget_cancelled_creation_returns_latest_hide_for_cleanup() {
        let state = WidgetWindowLifecycleState::default();
        assert!(state.begin_show());
        let old = state.hide();
        let latest = state.hide();
        assert!(!state.should_destroy_hidden_window(latest));
        assert_eq!(
            state.finish_show(),
            WidgetShowCompletion::Hidden {
                hide_generation: latest
            }
        );
        assert!(!state.should_destroy_hidden_window(old));
        assert!(state.should_destroy_hidden_window(latest));
        state.show_existing();
        assert!(!state.should_destroy_hidden_window(latest));
    }

    #[test]
    fn widget_reopen_during_creation_keeps_latest_show_intent() {
        let state = WidgetWindowLifecycleState::default();
        assert!(state.begin_show());
        let cancelled = state.hide();
        assert!(!state.begin_show());
        assert_eq!(state.finish_show(), WidgetShowCompletion::Show);
        assert!(!state.should_destroy_hidden_window(cancelled));
    }
}
