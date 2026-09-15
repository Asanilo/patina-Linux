use serde::Serialize;

/// Best-effort release of unused glibc pages, not live objects or WebKit caches.
/// Call only at an idle lifecycle boundary, never in a sampling loop.
#[cfg(target_env = "gnu")]
pub(crate) fn release_unused_heap_pages() -> bool {
    extern "C" {
        fn malloc_trim(pad: usize) -> std::os::raw::c_int;
    }
    // SAFETY: glibc's allocator operation is thread-safe and takes no pointers.
    unsafe { malloc_trim(0) != 0 }
}

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

#[cfg(all(test, target_env = "gnu"))]
mod tests {
    #[test]
    fn heap_release_preserves_live_and_concurrent_allocations() {
        let live = vec![0xa5u8; 1024 * 1024];
        let worker = std::thread::spawn(|| {
            for value in 0..16u8 {
                let block = vec![value; 256 * 1024];
                assert!(block.iter().all(|byte| *byte == value));
            }
        });
        // The allocator may or may not have releasable pages; neither result
        // is an error, and live bytes must be unaffected in both cases.
        let _ = super::release_unused_heap_pages();
        worker.join().unwrap();
        assert!(live.iter().all(|byte| *byte == 0xa5));
    }
}
