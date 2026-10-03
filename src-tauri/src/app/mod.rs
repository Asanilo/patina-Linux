#[cfg(feature = "desktop")]
pub mod activity_import;
#[cfg(feature = "desktop")]
pub mod api_runtime;
#[cfg(feature = "desktop")]
pub mod autostart;
#[cfg(feature = "desktop")]
pub mod background_resource_reclaimer;
#[cfg(feature = "desktop")]
pub mod backup;
#[cfg(feature = "desktop")]
pub mod bootstrap;
pub mod daemon;
#[cfg(feature = "desktop")]
pub mod daemon_client;
#[cfg(feature = "desktop")]
pub mod daemon_service;
#[cfg(feature = "desktop")]
pub mod desktop_behavior;
#[cfg(all(test, target_os = "linux"))]
#[cfg(feature = "desktop")]
mod heatmap_acceptance_tests;
#[cfg(feature = "desktop")]
pub mod main_window;
#[cfg(all(test, target_os = "linux"))]
#[cfg(feature = "desktop")]
mod native_window_tests;
#[cfg(feature = "desktop")]
pub mod runtime;
pub mod runtime_lease;
#[cfg(feature = "desktop")]
pub mod runtime_owner_cutover;
#[cfg(feature = "desktop")]
pub mod runtime_tasks;
#[cfg(feature = "desktop")]
pub mod scheduled_backup;
#[cfg(feature = "desktop")]
pub mod state;
#[cfg(all(test, target_os = "linux"))]
#[cfg(feature = "desktop")]
mod storage_acceptance_tests;
#[cfg(feature = "desktop")]
pub mod storage_maintenance;
#[cfg(feature = "desktop")]
pub mod tray;
#[cfg(feature = "desktop")]
pub mod web_activity;
#[cfg(feature = "desktop")]
pub mod web_activity_bridge;
#[cfg(feature = "desktop")]
pub mod widget;

#[cfg(feature = "desktop")]
pub mod settings_commit;
