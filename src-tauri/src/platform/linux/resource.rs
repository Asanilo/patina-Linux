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
pub struct ProcessResourceSnapshot {
    pub handle_count: Option<u32>,
    pub thread_count: Option<u32>,
    pub working_set_bytes: Option<usize>,
    pub private_usage_bytes: Option<usize>,
    pub rss_bytes: Option<usize>,
    pub pss_bytes: Option<usize>,
    pub uss_bytes: Option<usize>,
    pub swap_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LinuxMemorySnapshot {
    rss_bytes: Option<usize>,
    pss_bytes: Option<usize>,
    uss_bytes: Option<usize>,
    swap_bytes: Option<usize>,
}

pub fn current_process_resource_snapshot() -> ProcessResourceSnapshot {
    let memory = read_smaps_rollup();
    ProcessResourceSnapshot {
        handle_count: read_handle_count(),
        thread_count: read_thread_count(),
        working_set_bytes: memory.rss_bytes,
        private_usage_bytes: memory.uss_bytes,
        rss_bytes: memory.rss_bytes,
        pss_bytes: memory.pss_bytes,
        uss_bytes: memory.uss_bytes,
        swap_bytes: memory.swap_bytes,
    }
}

fn read_smaps_rollup() -> LinuxMemorySnapshot {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .map(|contents| parse_smaps_rollup(&contents))
        .unwrap_or_default()
}

fn parse_smaps_rollup(contents: &str) -> LinuxMemorySnapshot {
    let mut rss_bytes = None;
    let mut pss_bytes = None;
    let mut private_clean_bytes = None;
    let mut private_dirty_bytes = None;
    let mut private_hugetlb_bytes = None;
    let mut swap_bytes = None;

    for line in contents.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let parsed = parse_kib(value);
        match key {
            "Rss" => rss_bytes = parsed,
            "Pss" => pss_bytes = parsed,
            "Private_Clean" => private_clean_bytes = parsed,
            "Private_Dirty" => private_dirty_bytes = parsed,
            "Private_Hugetlb" => private_hugetlb_bytes = parsed,
            "Swap" => swap_bytes = parsed,
            _ => {}
        }
    }

    let uss_bytes = private_clean_bytes
        .zip(private_dirty_bytes)
        .and_then(|(clean, dirty)| clean.checked_add(dirty))
        .and_then(|subtotal| subtotal.checked_add(private_hugetlb_bytes.unwrap_or_default()));

    LinuxMemorySnapshot {
        rss_bytes,
        pss_bytes,
        uss_bytes,
        swap_bytes,
    }
}

fn parse_kib(value: &str) -> Option<usize> {
    value
        .trim()
        .strip_suffix(" kB")?
        .trim()
        .parse::<usize>()
        .ok()?
        .checked_mul(1024)
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

#[cfg(test)]
mod tests {
    #[test]
    fn parses_rss_pss_uss_and_swap_from_smaps_rollup() {
        let snapshot = super::parse_smaps_rollup(
            "Rss:               227024 kB\n\
             Pss:                91770 kB\n\
             Private_Clean:      12000 kB\n\
             Private_Dirty:      63756 kB\n\
             Private_Hugetlb:        4 kB\n\
             Swap:                 128 kB\n\
             VmData:           9999999 kB\n",
        );

        assert_eq!(snapshot.rss_bytes, Some(227_024 * 1024));
        assert_eq!(snapshot.pss_bytes, Some(91_770 * 1024));
        assert_eq!(snapshot.uss_bytes, Some(75_760 * 1024));
        assert_eq!(snapshot.swap_bytes, Some(128 * 1024));
    }

    #[test]
    fn incomplete_smaps_does_not_fabricate_private_usage() {
        let snapshot = super::parse_smaps_rollup("Rss: 200 kB\nPrivate_Dirty: 100 kB\n");

        assert_eq!(snapshot.rss_bytes, Some(200 * 1024));
        assert_eq!(snapshot.uss_bytes, None);
    }

    #[cfg(target_env = "gnu")]
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
