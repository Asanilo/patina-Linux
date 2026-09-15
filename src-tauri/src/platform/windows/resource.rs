use crate::platform::windows::handles::OwnedHandle;
use serde::Serialize;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessHandleCount,
};

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

pub fn current_process_resource_snapshot() -> ProcessResourceSnapshot {
    let memory = current_process_memory_counters();
    let working_set_bytes = memory.map(|counters| counters.WorkingSetSize);

    ProcessResourceSnapshot {
        handle_count: current_process_handle_count(),
        thread_count: current_process_thread_count(),
        working_set_bytes,
        private_usage_bytes: memory.map(|counters| counters.PrivateUsage),
        rss_bytes: working_set_bytes,
        pss_bytes: None,
        uss_bytes: None,
        swap_bytes: None,
    }
}

fn current_process_handle_count() -> Option<u32> {
    let mut handle_count = 0_u32;
    unsafe {
        GetProcessHandleCount(GetCurrentProcess(), &mut handle_count).ok()?;
    }
    Some(handle_count)
}

fn current_process_thread_count() -> Option<u32> {
    let process_id = unsafe { GetCurrentProcessId() };
    let snapshot =
        unsafe { OwnedHandle::new(CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).ok()?) }?;
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut count = 0_u32;

    unsafe {
        if Thread32First(snapshot.raw(), &mut entry).is_err() {
            return Some(count);
        }

        loop {
            if entry.th32OwnerProcessID == process_id {
                count = count.saturating_add(1);
            }

            if Thread32Next(snapshot.raw(), &mut entry).is_err() {
                break;
            }
        }
    }

    Some(count)
}

fn current_process_memory_counters() -> Option<PROCESS_MEMORY_COUNTERS_EX> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };

    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut _,
            counters.cb,
        )
        .ok()?;
    }

    Some(counters)
}
