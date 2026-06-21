use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};

use crate::platform::tracking_diagnostics::WindowTrackingDiagnostics;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WindowInfo {
    pub hwnd: String,
    pub root_owner_hwnd: String,
    pub process_id: u32,
    pub window_class: String,
    pub title: String,
    pub exe_name: String,
    pub process_path: String,
    pub is_afk: bool,
    pub idle_time_ms: u32,
}

static AFK_THRESHOLD_SECS: AtomicU64 = AtomicU64::new(180);
const PROCESS_DETAILS_CACHE_TTL_MS: u64 = 10_000;
const PROCESS_DETAILS_NEGATIVE_CACHE_TTL_MS: u64 = 1_000;
const PROCESS_DETAILS_CACHE_MAX_ENTRIES: usize = 128;

#[derive(Clone, Debug)]
struct ProcessDetailsCacheEntry {
    exe_name: String,
    process_path: String,
    cached_at_ms: u64,
    is_negative: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ProcessDetailsCacheStats {
    pub entries: usize,
    pub positive_entries: usize,
    pub negative_entries: usize,
    pub max_entries: usize,
    pub ttl_ms: u64,
    pub negative_ttl_ms: u64,
}

pub fn has_meaningful_change(previous: Option<&WindowInfo>, next: &WindowInfo) -> bool {
    let Some(previous) = previous else {
        return true;
    };

    previous.title != next.title
        || previous.exe_name != next.exe_name
        || previous.process_path != next.process_path
        || previous.hwnd != next.hwnd
        || previous.root_owner_hwnd != next.root_owner_hwnd
        || previous.process_id != next.process_id
        || previous.window_class != next.window_class
        || previous.is_afk != next.is_afk
}

pub fn cmd_set_afk_threshold(threshold_secs: u64) {
    AFK_THRESHOLD_SECS.store(threshold_secs, Ordering::Relaxed);
}

pub fn get_active_window() -> WindowInfo {
    let idle_time = query_idle_time_ms();
    let afk_threshold_ms = (AFK_THRESHOLD_SECS.load(Ordering::Relaxed) as u32) * 1000;
    let is_afk = idle_time > afk_threshold_ms;

    match query_focused_window() {
        Some(window) => WindowInfo {
            is_afk,
            idle_time_ms: idle_time,
            ..window
        },
        None => build_inactive_window(idle_time, is_afk),
    }
}

// ── Idle time detection ─────────────────────────────────────────────

fn query_idle_time_ms() -> u32 {
    // Try GNOME/Wayland D-Bus first, fall back to X11 screensaver
    query_idle_time_dbus().unwrap_or_else(query_idle_time_x11)
}

fn query_idle_time_dbus() -> Option<u32> {
    let conn = zbus::blocking::Connection::session().ok()?;
    // Try GNOME Mutter IdleMonitor first (works on GNOME Wayland)
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core",
        "org.gnome.Mutter.IdleMonitor",
    )
    .ok()?;
    let idle_time_ms: u64 = proxy
        .call_method("GetIdletime", &())
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    Some(mutter_idle_time_to_ms(idle_time_ms))
}

fn mutter_idle_time_to_ms(idle_time_ms: u64) -> u32 {
    idle_time_ms.min(u64::from(u32::MAX)) as u32
}

fn query_idle_time_x11() -> u32 {
    use xcb::x::Drawable;
    use xcb::Xid;

    let Ok((conn, _screen_num)) = xcb::Connection::connect(None) else {
        return 0;
    };

    let screensaver_cookie = conn.send_request(&xcb::screensaver::QueryInfo {
        drawable: Drawable::none(),
    });

    match conn.wait_for_reply(screensaver_cookie) {
        Ok(reply) => reply.ms_since_user_input(),
        Err(_) => 0,
    }
}

// ── Focused window detection ────────────────────────────────────────

fn query_focused_window() -> Option<WindowInfo> {
    let session_type = current_session_type();
    let desktop = current_desktop();

    if let Some(window) = query_focused_window_gnome() {
        return Some(window);
    }

    if should_try_x11_focused_window(session_type.as_deref(), desktop.as_deref()) {
        query_focused_window_x11()
    } else {
        None
    }
}

pub fn window_tracking_diagnostics() -> WindowTrackingDiagnostics {
    let session_type = current_session_type();
    let desktop = current_desktop();
    let has_gnome_owner =
        if is_wayland_session(session_type.as_deref()) && is_gnome_desktop(desktop.as_deref()) {
            dbus_name_has_owner("org.patina.WindowTracker").ok()
        } else {
            None
        };

    resolve_window_tracking_diagnostics(
        session_type.as_deref(),
        desktop.as_deref(),
        has_gnome_owner,
    )
}

fn resolve_window_tracking_diagnostics(
    session_type: Option<&str>,
    desktop: Option<&str>,
    has_gnome_window_tracker_owner: Option<bool>,
) -> WindowTrackingDiagnostics {
    let normalized_session = session_type.map(|value| value.trim().to_ascii_lowercase());

    match normalized_session.as_deref() {
        Some("x11") => WindowTrackingDiagnostics {
            status: "available".to_string(),
            reason: None,
            provider: "x11".to_string(),
            session_type: session_type.map(str::to_string),
            desktop: desktop.map(str::to_string),
        },
        Some("wayland") if is_gnome_desktop(desktop) => {
            if has_gnome_window_tracker_owner == Some(true) {
                WindowTrackingDiagnostics {
                    status: "available".to_string(),
                    reason: None,
                    provider: "gnome-shell-extension".to_string(),
                    session_type: session_type.map(str::to_string),
                    desktop: desktop.map(str::to_string),
                }
            } else {
                WindowTrackingDiagnostics {
                    status: "unavailable".to_string(),
                    reason: Some(
                        has_gnome_window_tracker_owner
                            .map(|_| "gnome-extension-dbus-unavailable")
                            .unwrap_or("session-bus-unavailable")
                            .to_string(),
                    ),
                    provider: "gnome-shell-extension".to_string(),
                    session_type: session_type.map(str::to_string),
                    desktop: desktop.map(str::to_string),
                }
            }
        }
        Some("wayland") => WindowTrackingDiagnostics {
            status: "unsupported".to_string(),
            reason: Some("wayland-compositor-unsupported".to_string()),
            provider: "none".to_string(),
            session_type: session_type.map(str::to_string),
            desktop: desktop.map(str::to_string),
        },
        _ => WindowTrackingDiagnostics {
            status: "unavailable".to_string(),
            reason: Some("unknown-session-type".to_string()),
            provider: "none".to_string(),
            session_type: session_type.map(str::to_string),
            desktop: desktop.map(str::to_string),
        },
    }
}

fn should_try_x11_focused_window(session_type: Option<&str>, _desktop: Option<&str>) -> bool {
    !is_wayland_session(session_type)
}

fn is_wayland_session(session_type: Option<&str>) -> bool {
    session_type
        .map(|value| value.trim().eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

fn is_gnome_desktop(desktop: Option<&str>) -> bool {
    desktop
        .map(|value| value.to_ascii_lowercase().contains("gnome"))
        .unwrap_or(false)
}

fn current_session_type() -> Option<String> {
    env::var("XDG_SESSION_TYPE")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn current_desktop() -> Option<String> {
    env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| env::var("DESKTOP_SESSION"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn dbus_name_has_owner(name: &str) -> Result<bool, String> {
    let conn = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .map_err(|error| error.to_string())?;

    let reply = proxy
        .call_method("NameHasOwner", &(name))
        .map_err(|error| error.to_string())?;
    reply
        .body()
        .deserialize()
        .map_err(|error| error.to_string())
}

fn query_focused_window_gnome() -> Option<WindowInfo> {
    // Use the Patina GNOME Shell extension's D-Bus interface
    let conn = zbus::blocking::Connection::session().ok()?;

    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.patina.WindowTracker",
        "/org/patina/WindowTracker",
        "org.patina.WindowTracker",
    )
    .ok()?;

    // GetFocusedWindow() returns (title, app_id, wm_class, pid, window_id)
    let reply = proxy.call_method("GetFocusedWindow", &()).ok()?;
    let body = reply.body();
    let (title, app_id, wm_class, pid, window_id): (String, String, String, u32, u64) =
        body.deserialize().ok()?;

    if app_id.trim().is_empty() && title.trim().is_empty() {
        return None;
    }

    let (exe_name, process_path) = if pid > 0 {
        get_process_details(pid)
    } else {
        (app_id.clone(), String::new())
    };

    Some(WindowInfo {
        hwnd: window_id.to_string(),
        root_owner_hwnd: window_id.to_string(),
        process_id: pid,
        window_class: wm_class,
        title,
        exe_name: if exe_name.is_empty() {
            app_id
        } else {
            exe_name
        },
        process_path,
        is_afk: false,
        idle_time_ms: 0,
    })
}

// ── X11 fallback ────────────────────────────────────────────────────

fn query_focused_window_x11() -> Option<WindowInfo> {
    use xcb::x;
    use xcb::Xid;

    let Ok((conn, screen_num)) = xcb::Connection::connect(None) else {
        return None;
    };

    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize)?;

    let focus_cookie = conn.send_request(&x::GetInputFocus {});
    let focus_reply = conn.wait_for_reply(focus_cookie).ok()?;
    let focus_window = focus_reply.focus();

    if focus_window == x::Window::none() || focus_window == screen.root() {
        return None;
    }

    let root_owner = get_root_owner(&conn, focus_window, screen.root());
    let process_id = get_window_pid(&conn, root_owner);

    if process_id == 0 {
        return None;
    }

    let (exe_name, process_path) = get_process_details(process_id);

    if exe_name.trim().is_empty() {
        return None;
    }

    let title = get_window_title(&conn, focus_window);
    let window_class = get_window_class(&conn, focus_window);

    Some(WindowInfo {
        hwnd: format!("0x{:X}", focus_window.resource_id()),
        root_owner_hwnd: format!("0x{:X}", root_owner.resource_id()),
        process_id,
        window_class,
        title,
        exe_name,
        process_path,
        is_afk: false,
        idle_time_ms: 0,
    })
}

fn get_root_owner(
    conn: &xcb::Connection,
    window: xcb::x::Window,
    root: xcb::x::Window,
) -> xcb::x::Window {
    use xcb::x;
    use xcb::Xid;

    let wm_state_atom = intern_atom(conn, "WM_STATE");

    let mut current = window;
    loop {
        let tree_cookie = conn.send_request(&x::QueryTree { window: current });
        let Ok(tree_reply) = conn.wait_for_reply(tree_cookie) else {
            return current;
        };

        let parent = tree_reply.parent();
        if parent == root || parent == x::Window::none() || parent == current {
            return current;
        }

        let wm_state_cookie = conn.send_request(&x::GetProperty {
            delete: false,
            window: current,
            property: wm_state_atom,
            r#type: wm_state_atom,
            long_offset: 0,
            long_length: 2,
        });

        if conn.wait_for_reply(wm_state_cookie).is_ok() && current != window {
            return current;
        }

        current = parent;
    }
}

fn get_window_pid(conn: &xcb::Connection, window: xcb::x::Window) -> u32 {
    use xcb::x;

    let net_wm_pid_atom = intern_atom(conn, "_NET_WM_PID");

    let cookie = conn.send_request(&x::GetProperty {
        delete: false,
        window,
        property: net_wm_pid_atom,
        r#type: x::ATOM_CARDINAL,
        long_offset: 0,
        long_length: 1,
    });

    match conn.wait_for_reply(cookie) {
        Ok(reply) => {
            let value = reply.value();
            if value.len() >= 4 {
                u32::from_ne_bytes([value[0], value[1], value[2], value[3]])
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

fn get_window_title(conn: &xcb::Connection, window: xcb::x::Window) -> String {
    use xcb::x;

    let net_wm_name_atom = intern_atom(conn, "_NET_WM_NAME");
    let utf8_string_atom = intern_atom(conn, "UTF8_STRING");

    let cookie = conn.send_request(&x::GetProperty {
        delete: false,
        window,
        property: net_wm_name_atom,
        r#type: utf8_string_atom,
        long_offset: 0,
        long_length: 4096,
    });

    if let Ok(reply) = conn.wait_for_reply(cookie) {
        let value = reply.value();
        if !value.is_empty() {
            return String::from_utf8_lossy(value).into_owned();
        }
    }

    let wm_name_cookie = conn.send_request(&x::GetProperty {
        delete: false,
        window,
        property: x::ATOM_WM_NAME,
        r#type: x::ATOM_STRING,
        long_offset: 0,
        long_length: 4096,
    });

    match conn.wait_for_reply(wm_name_cookie) {
        Ok(reply) => String::from_utf8_lossy(reply.value()).into_owned(),
        Err(_) => String::new(),
    }
}

fn get_window_class(conn: &xcb::Connection, window: xcb::x::Window) -> String {
    use xcb::x;

    let wm_class_cookie = conn.send_request(&x::GetProperty {
        delete: false,
        window,
        property: x::ATOM_WM_CLASS,
        r#type: x::ATOM_STRING,
        long_offset: 0,
        long_length: 4096,
    });

    match conn.wait_for_reply(wm_class_cookie) {
        Ok(reply) => {
            let value = reply.value();
            let class_str = String::from_utf8_lossy(value);
            class_str.split('\0').nth(1).unwrap_or("").to_string()
        }
        Err(_) => String::new(),
    }
}

fn intern_atom(conn: &xcb::Connection, name: &str) -> xcb::x::Atom {
    use xcb::x;
    use xcb::Xid;

    let cookie = conn.send_request(&x::InternAtom {
        only_if_exists: false,
        name: name.as_bytes(),
    });

    match conn.wait_for_reply(cookie) {
        Ok(reply) => reply.atom(),
        Err(_) => x::Atom::none(),
    }
}

// ── Process details cache ───────────────────────────────────────────

pub fn get_process_path(process_id: u32) -> String {
    get_process_details(process_id).1
}

pub fn get_process_exe_name(process_id: u32) -> String {
    get_process_details(process_id).0
}

fn get_process_details(process_id: u32) -> (String, String) {
    let now_ms = now_ms();
    if let Some(entry) = read_cached_process_details(process_id, now_ms) {
        return (entry.exe_name, entry.process_path);
    }

    let process_path = std::fs::read_link(format!("/proc/{process_id}/exe"))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let exe_name = if !process_path.is_empty() {
        std::path::Path::new(&process_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        std::fs::read_to_string(format!("/proc/{process_id}/comm"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    let details = (exe_name, process_path);
    write_cached_process_details(process_id, &details, now_ms);
    details
}

fn build_inactive_window(idle_time_ms: u32, is_afk: bool) -> WindowInfo {
    WindowInfo {
        hwnd: String::new(),
        root_owner_hwnd: String::new(),
        process_id: 0,
        window_class: String::new(),
        title: String::new(),
        exe_name: String::new(),
        process_path: String::new(),
        is_afk,
        idle_time_ms,
    }
}

fn read_cached_process_details(process_id: u32, now_ms: u64) -> Option<ProcessDetailsCacheEntry> {
    let mut cache = process_details_cache().lock().ok()?;
    let entry = cache.get(&process_id)?;

    if is_process_details_cache_entry_fresh(entry, now_ms) {
        Some(entry.clone())
    } else {
        cache.remove(&process_id);
        None
    }
}

fn write_cached_process_details(process_id: u32, details: &(String, String), now_ms: u64) {
    if let Ok(mut cache) = process_details_cache().lock() {
        prune_expired_process_details_cache(&mut cache, now_ms);
        if cache.len() >= PROCESS_DETAILS_CACHE_MAX_ENTRIES && !cache.contains_key(&process_id) {
            if let Some(oldest_process_id) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.cached_at_ms)
                .map(|(cached_process_id, _)| *cached_process_id)
            {
                cache.remove(&oldest_process_id);
            }
        }

        cache.insert(
            process_id,
            ProcessDetailsCacheEntry {
                exe_name: details.0.clone(),
                process_path: details.1.clone(),
                cached_at_ms: now_ms,
                is_negative: details.0.trim().is_empty() && details.1.trim().is_empty(),
            },
        );
    }
}

fn is_process_details_cache_entry_fresh(entry: &ProcessDetailsCacheEntry, now_ms: u64) -> bool {
    let ttl_ms = if entry.is_negative {
        PROCESS_DETAILS_NEGATIVE_CACHE_TTL_MS
    } else {
        PROCESS_DETAILS_CACHE_TTL_MS
    };

    now_ms.saturating_sub(entry.cached_at_ms) <= ttl_ms
}

fn prune_expired_process_details_cache(
    cache: &mut HashMap<u32, ProcessDetailsCacheEntry>,
    now_ms: u64,
) {
    cache.retain(|_, entry| is_process_details_cache_entry_fresh(entry, now_ms));
}

fn process_details_cache() -> &'static Mutex<HashMap<u32, ProcessDetailsCacheEntry>> {
    static PROCESS_DETAILS_CACHE: OnceLock<Mutex<HashMap<u32, ProcessDetailsCacheEntry>>> =
        OnceLock::new();
    PROCESS_DETAILS_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn process_details_cache_stats() -> ProcessDetailsCacheStats {
    let (entries, positive_entries, negative_entries) = process_details_cache()
        .lock()
        .map(|mut cache| {
            prune_expired_process_details_cache(&mut cache, now_ms());
            let entries = cache.len();
            let negative_entries = cache.values().filter(|entry| entry.is_negative).count();
            (
                entries,
                entries.saturating_sub(negative_entries),
                negative_entries,
            )
        })
        .unwrap_or((0, 0, 0));

    ProcessDetailsCacheStats {
        entries,
        positive_entries,
        negative_entries,
        max_entries: PROCESS_DETAILS_CACHE_MAX_ENTRIES,
        ttl_ms: PROCESS_DETAILS_CACHE_TTL_MS,
        negative_ttl_ms: PROCESS_DETAILS_NEGATIVE_CACHE_TTL_MS,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutter_idle_monitor_value_is_already_milliseconds() {
        assert_eq!(mutter_idle_time_to_ms(181), 181);
    }

    #[test]
    fn gnome_wayland_without_extension_reports_dbus_unavailable() {
        let diagnostic =
            resolve_window_tracking_diagnostics(Some("wayland"), Some("GNOME"), Some(false));

        assert_eq!(diagnostic.status, "unavailable");
        assert_eq!(
            diagnostic.reason.as_deref(),
            Some("gnome-extension-dbus-unavailable")
        );
        assert_eq!(diagnostic.provider, "gnome-shell-extension");
    }

    #[test]
    fn non_gnome_wayland_reports_unsupported_window_tracking() {
        let diagnostic = resolve_window_tracking_diagnostics(Some("wayland"), Some("KDE"), None);

        assert_eq!(diagnostic.status, "unsupported");
        assert_eq!(
            diagnostic.reason.as_deref(),
            Some("wayland-compositor-unsupported")
        );
    }

    #[test]
    fn gnome_wayland_does_not_try_x11_fallback() {
        assert!(!should_try_x11_focused_window(
            Some("wayland"),
            Some("GNOME")
        ));
        assert!(should_try_x11_focused_window(Some("x11"), Some("GNOME")));
    }
}
