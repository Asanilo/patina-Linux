use crate::platform::storage_usage;
use std::fs;
use std::path::{Path, PathBuf};

pub const WEBKIT_CACHE_DIR_NAME: &str = "WebKitCache";

const PRODUCT_DATA_ENTRIES: &[&str] = &[
    "patina.db",
    "patina.db-wal",
    "patina.db-shm",
    "backups",
    "remote-backup-temp",
    "api_token",
];

const PERSISTENT_WEBVIEW_ENTRIES: &[&str] = &[
    "localstorage",
    "storage",
    "CacheStorage",
    "mediakeys",
    "hsts-storage.sqlite",
    "cookies.sqlite",
    "databases",
    "IndexedDB",
    "ServiceWorker",
];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WebviewProfileCopyReport {
    pub copied: Vec<String>,
    pub skipped_unknown: Vec<String>,
}

pub fn webkit_cache_path(webview_root: &Path) -> PathBuf {
    webview_root.join(WEBKIT_CACHE_DIR_NAME)
}

pub fn webkit_cache_size(webview_root: &Path) -> Result<u64, String> {
    storage_usage::path_size(&webkit_cache_path(webview_root))
}

pub fn persistent_profile_size(webview_root: &Path) -> Result<u64, String> {
    ensure_real_directory(webview_root, "active WebView root")?;
    let mut total = 0_u64;
    for name in PERSISTENT_WEBVIEW_ENTRIES {
        total = total.saturating_add(storage_usage::path_size(&webview_root.join(name))?);
    }
    Ok(total)
}

pub fn clear_linux_webkit_cache(webview_root: &Path) -> Result<(), String> {
    ensure_real_directory(webview_root, "active WebView root")?;
    let cache_path = webkit_cache_path(webview_root);
    let metadata = match fs::symlink_metadata(&cache_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to inspect WebKit cache `{}`: {error}",
                cache_path.display()
            ))
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to remove symbolic link `{}`",
            cache_path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "WebKit cache `{}` is not a directory",
            cache_path.display()
        ));
    }

    let canonical_root = webview_root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve active WebView root `{}`: {error}",
            webview_root.display()
        )
    })?;
    let canonical_cache = cache_path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve WebKit cache `{}`: {error}",
            cache_path.display()
        )
    })?;
    if canonical_cache.parent() != Some(canonical_root.as_path())
        || canonical_cache.file_name().and_then(|name| name.to_str()) != Some(WEBKIT_CACHE_DIR_NAME)
    {
        return Err(format!(
            "refusing to remove cache path `{}` outside the active WebView root",
            canonical_cache.display()
        ));
    }

    remove_directory_without_following_links(&canonical_cache)
}

pub fn copy_persistent_profile(
    source_root: &Path,
    target_root: &Path,
) -> Result<WebviewProfileCopyReport, String> {
    ensure_real_directory(source_root, "source WebView root")?;
    ensure_target_directory(target_root)?;
    let mut report = WebviewProfileCopyReport::default();

    for entry in fs::read_dir(source_root).map_err(|error| {
        format!(
            "failed to read source WebView root `{}`: {error}",
            source_root.display()
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read entry in `{}`: {error}",
                source_root.display()
            )
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == WEBKIT_CACHE_DIR_NAME || PRODUCT_DATA_ENTRIES.contains(&name.as_str()) {
            continue;
        }
        if !PERSISTENT_WEBVIEW_ENTRIES.contains(&name.as_str()) {
            report.skipped_unknown.push(name);
            continue;
        }

        let source = entry.path();
        let target = target_root.join(&name);
        if target.exists() {
            return Err(format!(
                "refusing to overwrite existing WebView entry `{}`",
                target.display()
            ));
        }
        copy_entry_without_following_links(&source, &target)?;
        report.copied.push(name);
    }
    report.copied.sort();
    report.skipped_unknown.sort();
    Ok(report)
}

fn ensure_real_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{label} `{}` is unavailable: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} `{}` is a symbolic link", path.display()));
    }
    if !metadata.is_dir() {
        return Err(format!("{label} `{}` is not a directory", path.display()));
    }
    Ok(())
}

fn ensure_target_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        return ensure_real_directory(path, "target WebView root");
    }
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create target WebView root `{}`: {error}",
            path.display()
        )
    })
}

fn copy_entry_without_following_links(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("failed to inspect `{}`: {error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to copy symbolic link `{}`",
            source.display()
        ));
    }
    if metadata.is_file() {
        fs::copy(source, target).map(|_| ()).map_err(|error| {
            format!(
                "failed to copy `{}` to `{}`: {error}",
                source.display(),
                target.display()
            )
        })?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(format!("unsupported WebView entry `{}`", source.display()));
    }

    fs::create_dir(target)
        .map_err(|error| format!("failed to create `{}`: {error}", target.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read `{}`: {error}", source.display()))?
    {
        let entry = entry
            .map_err(|error| format!("failed to read entry in `{}`: {error}", source.display()))?;
        copy_entry_without_following_links(&entry.path(), &target.join(entry.file_name()))?;
    }
    Ok(())
}

fn remove_directory_without_following_links(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to read cache `{}`: {error}", path.display()))?
    {
        let entry = entry
            .map_err(|error| format!("failed to read cache entry `{}`: {error}", path.display()))?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path).map_err(|error| {
            format!(
                "failed to inspect cache entry `{}`: {error}",
                entry_path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(&entry_path).map_err(|error| {
                format!(
                    "failed to remove cache file `{}`: {error}",
                    entry_path.display()
                )
            })?;
        } else if metadata.is_dir() {
            remove_directory_without_following_links(&entry_path)?;
        }
    }
    fs::remove_dir(path).map_err(|error| {
        format!(
            "failed to remove cache directory `{}`: {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "patina-webview-cache-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_file(path: &Path, size: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1_u8; size]).unwrap();
    }

    #[test]
    fn cache_clear_only_removes_webkit_cache() {
        let root = temp_dir("clear");
        write_file(&root.join("WebKitCache/Version 17/Blobs/a"), 10);
        write_file(&root.join("localstorage/tauri.localstorage"), 20);

        clear_linux_webkit_cache(&root).unwrap();

        assert!(!root.join("WebKitCache").exists());
        assert!(root.join("localstorage/tauri.localstorage").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cache_clear_refuses_symlink_candidate() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("symlink");
        let outside = temp_dir("outside");
        write_file(&outside.join("keep"), 10);
        symlink(&outside, root.join("WebKitCache")).unwrap();

        let error = clear_linux_webkit_cache(&root).unwrap_err();

        assert!(error.contains("symbolic link"));
        assert!(outside.join("keep").exists());
        fs::remove_file(root.join("WebKitCache")).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn persistent_profile_copy_skips_cache_and_product_data() {
        let source = temp_dir("copy-source");
        let target = temp_dir("copy-target");
        write_file(&source.join("localstorage/settings"), 10);
        write_file(&source.join("storage/indexed"), 11);
        write_file(&source.join("WebKitCache/cache"), 12);
        write_file(&source.join("patina.db"), 13);
        write_file(&source.join("api_token"), 14);
        write_file(&source.join("unknown-entry"), 15);

        let report = copy_persistent_profile(&source, &target).unwrap();

        assert!(target.join("localstorage/settings").exists());
        assert!(target.join("storage/indexed").exists());
        assert!(!target.join("WebKitCache").exists());
        assert!(!target.join("patina.db").exists());
        assert!(!target.join("api_token").exists());
        assert!(!target.join("unknown-entry").exists());
        assert_eq!(report.skipped_unknown, vec!["unknown-entry"]);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn persistent_profile_size_only_counts_allowlisted_state() {
        let root = temp_dir("persistent-size");
        write_file(&root.join("localstorage/settings"), 10);
        write_file(&root.join("IndexedDB/data"), 11);
        write_file(&root.join("WebKitCache/cache"), 12);
        write_file(&root.join("patina.db"), 13);
        write_file(&root.join("unknown-entry"), 14);

        assert_eq!(persistent_profile_size(&root).unwrap(), 21);
        fs::remove_dir_all(root).unwrap();
    }
}
