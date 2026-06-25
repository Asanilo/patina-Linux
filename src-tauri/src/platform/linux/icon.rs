use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const ICON_RESULT_CACHE_MAX_ENTRIES: usize = 128;
const ICON_RESULT_CACHE_TTL_MS: u64 = 10 * 60 * 1000;
const ICON_RESULT_NEGATIVE_CACHE_TTL_MS: u64 = 60 * 1000;

const ICON_SEARCH_PATHS: &[&str] = &[
    "/usr/share/icons",
    "/usr/share/pixmaps",
    "/usr/local/share/icons",
];

const DESKTOP_ENTRY_PATHS: &[&str] = &["/usr/share/applications", "/usr/local/share/applications"];

#[derive(Clone)]
struct IconResultCacheEntry {
    cached_at_ms: u64,
    value: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct IconResultCacheStats {
    pub entries: usize,
    pub positive_entries: usize,
    pub negative_entries: usize,
    pub max_entries: usize,
    pub ttl_ms: u64,
    pub negative_ttl_ms: u64,
}

pub fn get_icon_base64(exe_path: &str) -> Option<String> {
    let cache_key = format!("file:{}", exe_path.trim().to_ascii_lowercase());
    if let Some(cached) = read_icon_result_cache(&cache_key, now_ms()) {
        return cached;
    }

    let result = get_icon_base64_uncached(exe_path);
    write_icon_result_cache(cache_key, result.clone(), now_ms());
    result
}

fn get_icon_base64_uncached(exe_path: &str) -> Option<String> {
    let exe_name = std::path::Path::new(exe_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())?;

    let desktop_entry = find_desktop_entry(&exe_name)?;
    let icon_name = extract_icon_from_desktop_entry(&desktop_entry)?;

    find_and_load_icon(&icon_name)
}

pub fn get_window_icon_base64(_hwnd_text: &str) -> Option<String> {
    // On Linux, window icon lookup is the same as exe icon lookup
    // The hwnd_text would be an X11 window id, but we don't have a direct
    // way to get the icon from the window without additional X11 queries.
    // Return None to use the exe-based icon instead.
    None
}

fn find_desktop_entry(exe_name: &str) -> Option<PathBuf> {
    let lower_exe = exe_name.to_lowercase();
    let stem = lower_exe.strip_suffix(".exe").unwrap_or(&lower_exe);

    for search_path in DESKTOP_ENTRY_PATHS {
        let dir = std::path::Path::new(search_path);
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }

            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };

            if desktop_entry_matches(&content, stem) {
                return Some(path);
            }
        }
    }

    None
}

fn desktop_entry_matches(content: &str, exe_stem: &str) -> bool {
    for line in content.lines() {
        if let Some(exec_line) = line.strip_prefix("Exec=") {
            let exec_lower = exec_line.to_lowercase();
            if exec_lower.contains(exe_stem) {
                return true;
            }
        }
    }
    false
}

fn extract_icon_from_desktop_entry(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if let Some(icon_value) = line.strip_prefix("Icon=") {
            let trimmed = icon_value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn find_and_load_icon(icon_name: &str) -> Option<String> {
    // If it's an absolute path
    if icon_name.starts_with('/') {
        return load_icon_from_path(icon_name);
    }

    // Search in icon theme directories
    let themes = ["hicolor", "Adwaita", "gnome"];
    let sizes = ["48x48", "64x64", "32x32", "256x256", "128x128", "scalable"];
    let categories = ["apps", "categories", "devices", "mimetypes"];

    for base_path in ICON_SEARCH_PATHS {
        for theme in &themes {
            for size in &sizes {
                for category in categories {
                    // Try PNG first
                    let png_path = format!("{base_path}/{theme}/{size}/{category}/{icon_name}.png");
                    if let Some(result) = load_icon_from_path(&png_path) {
                        return Some(result);
                    }

                    // Try SVG
                    let svg_path = format!("{base_path}/{theme}/{size}/{category}/{icon_name}.svg");
                    if let Some(result) = load_svg_as_base64(&svg_path) {
                        return Some(result);
                    }
                }
            }
        }

        // Check pixmaps directly
        let pixmap_path = format!("{base_path}/{icon_name}.png");
        if let Some(result) = load_icon_from_path(&pixmap_path) {
            return Some(result);
        }

        let pixmap_svg = format!("{base_path}/{icon_name}.svg");
        if let Some(result) = load_svg_as_base64(&pixmap_svg) {
            return Some(result);
        }
    }

    None
}

fn load_icon_from_path(path: &str) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let b64 = STANDARD.encode(&data);
    let mime = if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "image/png"
    };
    Some(format!("data:{mime};base64,{b64}"))
}

fn load_svg_as_base64(path: &str) -> Option<String> {
    if !std::path::Path::new(path).exists() {
        return None;
    }
    load_icon_from_path(path)
}

fn read_icon_result_cache(cache_key: &str, now_ms: u64) -> Option<Option<String>> {
    let mut cache = icon_result_cache().lock().ok()?;
    let entry = cache.get(cache_key)?;

    if is_icon_result_cache_entry_fresh(entry, now_ms) {
        Some(entry.value.clone())
    } else {
        cache.remove(cache_key);
        None
    }
}

fn write_icon_result_cache(cache_key: String, value: Option<String>, now_ms: u64) {
    if let Ok(mut cache) = icon_result_cache().lock() {
        prune_expired_icon_result_cache(&mut cache, now_ms);
        if cache.len() >= ICON_RESULT_CACHE_MAX_ENTRIES && !cache.contains_key(&cache_key) {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.cached_at_ms)
                .map(|(key, _)| key.clone())
            {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(
            cache_key,
            IconResultCacheEntry {
                cached_at_ms: now_ms,
                value,
            },
        );
    }
}

fn is_icon_result_cache_entry_fresh(entry: &IconResultCacheEntry, now_ms: u64) -> bool {
    let ttl_ms = if entry.value.is_some() {
        ICON_RESULT_CACHE_TTL_MS
    } else {
        ICON_RESULT_NEGATIVE_CACHE_TTL_MS
    };

    now_ms.saturating_sub(entry.cached_at_ms) <= ttl_ms
}

fn prune_expired_icon_result_cache(cache: &mut HashMap<String, IconResultCacheEntry>, now_ms: u64) {
    cache.retain(|_, entry| is_icon_result_cache_entry_fresh(entry, now_ms));
}

fn icon_result_cache() -> &'static Mutex<HashMap<String, IconResultCacheEntry>> {
    static ICON_RESULT_CACHE: OnceLock<Mutex<HashMap<String, IconResultCacheEntry>>> =
        OnceLock::new();
    ICON_RESULT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn icon_result_cache_stats() -> IconResultCacheStats {
    let (entries, positive_entries, negative_entries) = icon_result_cache()
        .lock()
        .map(|mut cache| {
            prune_expired_icon_result_cache(&mut cache, now_ms());
            let entries = cache.len();
            let positive_entries = cache.values().filter(|entry| entry.value.is_some()).count();
            (
                entries,
                positive_entries,
                entries.saturating_sub(positive_entries),
            )
        })
        .unwrap_or((0, 0, 0));

    IconResultCacheStats {
        entries,
        positive_entries,
        negative_entries,
        max_entries: ICON_RESULT_CACHE_MAX_ENTRIES,
        ttl_ms: ICON_RESULT_CACHE_TTL_MS,
        negative_ttl_ms: ICON_RESULT_NEGATIVE_CACHE_TTL_MS,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
