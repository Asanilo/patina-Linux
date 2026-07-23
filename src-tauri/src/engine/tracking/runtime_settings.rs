pub fn set_idle_threshold(threshold_secs: u64) {
    #[cfg(target_os = "windows")]
    crate::platform::windows::foreground::cmd_set_afk_threshold(threshold_secs);
    #[cfg(target_os = "linux")]
    crate::platform::linux::foreground::cmd_set_afk_threshold(threshold_secs);
}
