use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WindowsProcessResourceSnapshot {
    pub handle_count: Option<u32>,
    pub thread_count: Option<u32>,
    pub working_set_bytes: Option<usize>,
    pub private_usage_bytes: Option<usize>,
}

pub fn current_process_resource_snapshot() -> WindowsProcessResourceSnapshot {
    WindowsProcessResourceSnapshot {
        handle_count: read_handle_count(),
        thread_count: read_thread_count(),
        working_set_bytes: read_vm_rss_bytes(),
        private_usage_bytes: read_vm_data_bytes(),
    }
}

fn read_vm_rss_bytes() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            let trimmed = value.trim();
            let kb_str = trimmed.strip_suffix(" kB").unwrap_or(trimmed);
            let kb: usize = kb_str.trim().parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

fn read_vm_data_bytes() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmData:") {
            let trimmed = value.trim();
            let kb_str = trimmed.strip_suffix(" kB").unwrap_or(trimmed);
            let kb: usize = kb_str.trim().parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

fn read_thread_count() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("Threads:") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn read_handle_count() -> Option<u32> {
    // Count open file descriptors in /proc/self/fd/
    std::fs::read_dir("/proc/self/fd")
        .ok()
        .map(|entries| entries.count() as u32)
}
