use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutostartDesktopFileInspection {
    pub(crate) path: PathBuf,
    pub(crate) exists: bool,
    pub(crate) exec: Option<String>,
    pub(crate) reason: Option<String>,
}

impl AutostartDesktopFileInspection {
    pub(crate) fn valid(&self) -> bool {
        self.reason.is_none()
    }
}

pub(crate) fn inspect_autostart_desktop_file() -> AutostartDesktopFileInspection {
    let expected = current_autostart_executable().ok();
    inspect_autostart_desktop_file_at(&autostart_desktop_file_path(), expected.as_deref())
}

pub(crate) fn repair_current_exe_autostart_desktop_file() -> Result<(), String> {
    let executable_path = current_autostart_executable()?;
    repair_autostart_desktop_file(&autostart_desktop_file_path(), &executable_path)
        .map_err(|error| format!("failed to repair autostart desktop file: {error}"))
}

fn current_autostart_executable() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if tauri::utils::platform::bundle_type() == Some(tauri::utils::config::BundleType::AppImage) {
        let packaged = [
            "/usr/lib/systemd/user/patinad.service",
            "/lib/systemd/user/patinad.service",
        ]
        .iter()
        .any(|path| Path::new(path).is_file());
        return appimage_autostart_executable(
            std::env::var_os("APPIMAGE").as_deref().map(Path::new),
            packaged.then_some(Path::new("/usr/bin/Patina")),
        );
    }
    std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))
}

#[cfg(target_os = "linux")]
fn appimage_autostart_executable(
    image: Option<&Path>,
    packaged: Option<&Path>,
) -> Result<PathBuf, String> {
    // Coexistence follows the installed Desktop. A standalone AppImage must
    // restart through its original package so its runtime restores the AppDir
    // environment; current_exe points inside an ephemeral mount/extraction.
    let executable = packaged
        .filter(|path| path.is_file())
        .or(image)
        .ok_or("AppImage package path is unavailable; autostart entry was not changed")?;
    if !executable.is_absolute()
        || !executable.is_file()
        || executable
            .to_str()
            .is_none_or(|value| value.chars().any(char::is_control))
    {
        return Err(
            "autostart requires an existing absolute UTF-8 package path without control characters"
                .into(),
        );
    }
    Ok(executable.to_path_buf())
}

#[cfg(target_os = "linux")]
pub(crate) fn apply_linux_autostart(launch_at_login: bool) -> Result<(), String> {
    if launch_at_login {
        repair_current_exe_autostart_desktop_file()
    } else {
        remove_autostart_desktop_file(&autostart_desktop_file_path())
            .map_err(|error| format!("failed to remove autostart desktop file: {error}"))
    }
}

fn inspect_autostart_desktop_file_at(
    path: &Path,
    expected: Option<&Path>,
) -> AutostartDesktopFileInspection {
    let content = std::fs::read_to_string(path).ok();
    let exec = content
        .as_deref()
        .and_then(extract_desktop_exec)
        .map(str::to_string);
    let exists = path.exists();
    // An AppImage may have any filename; recognize the exact selected command
    // instead of requiring "patina" to occur in that filename.
    let expected_exec = expected.map(autostart_exec);
    let reason = if exists && expected_exec.is_some() && exec == expected_exec {
        None
    } else {
        resolve_autostart_reason(exists, exec.as_deref())
    };

    AutostartDesktopFileInspection {
        path: path.to_path_buf(),
        exists,
        exec,
        reason,
    }
}

fn repair_autostart_desktop_file(
    desktop_file_path: &Path,
    executable_path: &Path,
) -> std::io::Result<()> {
    if let Some(parent) = desktop_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        desktop_file_path,
        build_autostart_desktop_file(executable_path),
    )
}

fn remove_autostart_desktop_file(desktop_file_path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(desktop_file_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn build_autostart_desktop_file(executable_path: &Path) -> String {
    let executable = autostart_exec(executable_path);
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name=Patina\n\
Comment=Start Patina in the background\n\
Exec={executable}\n\
StartupNotify=false\n\
Terminal=false\n\
X-GNOME-Autostart-enabled=true\n"
    )
}

fn autostart_exec(executable_path: &Path) -> String {
    let path = executable_path.display().to_string();
    // GLib checks argv[0] before expanding %% field codes. Keep a real
    // executable in argv[0] when the selected package has a literal percent.
    let prefix = if path.contains('%') {
        "/usr/bin/env -- "
    } else {
        ""
    };
    format!(
        "{prefix}{} {}",
        quote_desktop_exec_argument(&path),
        crate::app::runtime::AUTOSTART_ARG
    )
}

fn quote_desktop_exec_argument(argument: &str) -> String {
    let argument = argument.replace('%', "%%");
    if !argument
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | '\\' | '$' | '`'))
    {
        return argument;
    }

    let escaped = argument
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{escaped}\"")
}

fn autostart_desktop_file_path() -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            Path::new(&home).join(".config")
        });
    config_home.join("autostart").join("Patina.desktop")
}

fn extract_desktop_exec(content: &str) -> Option<&str> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed.strip_prefix("Exec=").map(str::trim)
    })
}

fn resolve_autostart_reason(exists: bool, exec: Option<&str>) -> Option<String> {
    if !exists {
        return Some("desktop-file-missing".to_string());
    }

    let Some(exec) = exec.filter(|value| !value.trim().is_empty()) else {
        return Some("exec-missing".to_string());
    };

    let normalized = exec.to_ascii_lowercase();
    if !normalized.contains("patina") {
        return Some("exec-not-patina".to_string());
    }
    if !normalized.contains("--autostart") {
        return Some("autostart-arg-missing".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{
        build_autostart_desktop_file, inspect_autostart_desktop_file_at,
        remove_autostart_desktop_file, repair_autostart_desktop_file, resolve_autostart_reason,
    };
    use std::path::Path;

    #[test]
    fn autostart_exec_validation_detects_wrong_or_incomplete_commands() {
        assert_eq!(
            super::extract_desktop_exec("Name=Patina\nExec=/usr/bin/patina --autostart\n"),
            Some("/usr/bin/patina --autostart")
        );
        assert_eq!(
            resolve_autostart_reason(true, Some("/usr/local/bin/ghostty --autostart")).as_deref(),
            Some("exec-not-patina")
        );
        assert_eq!(
            resolve_autostart_reason(true, Some("/usr/bin/patina")).as_deref(),
            Some("autostart-arg-missing")
        );
        assert_eq!(
            resolve_autostart_reason(true, Some("/usr/bin/patina --autostart")),
            None
        );
    }

    #[test]
    fn autostart_repair_file_points_to_patina_with_autostart_arg() {
        let content = build_autostart_desktop_file(Path::new("/opt/Patina/patina"));

        assert!(content.contains("Name=Patina\n"));
        assert!(content.contains("Exec=/opt/Patina/patina --autostart\n"));
        assert!(content.contains("X-GNOME-Autostart-enabled=true\n"));
    }

    #[test]
    fn autostart_repair_quotes_executable_paths_with_spaces() {
        let content = build_autostart_desktop_file(Path::new("/home/user/My Apps/Patina/patina"));

        assert!(content.contains("Exec=\"/home/user/My Apps/Patina/patina\" --autostart\n"));
    }

    #[test]
    fn linux_autostart_repair_replaces_stale_desktop_entry() {
        let path = temp_desktop_file_path("replace-stale");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create temp autostart dir");
        }
        std::fs::write(
            &path,
            "[Desktop Entry]\nName=Patina\nExec=/usr/local/bin/ghostty --autostart\n",
        )
        .expect("write stale desktop file");

        let before = inspect_autostart_desktop_file_at(&path, None);
        assert_eq!(before.reason.as_deref(), Some("exec-not-patina"));

        repair_autostart_desktop_file(&path, Path::new("/opt/Patina/patina"))
            .expect("repair stale desktop file");

        let after = inspect_autostart_desktop_file_at(&path, None);
        assert!(after.valid());
        assert_eq!(
            after.exec.as_deref(),
            Some("/opt/Patina/patina --autostart")
        );

        cleanup_temp_desktop_file(&path);
    }

    #[test]
    fn linux_autostart_disable_removes_existing_desktop_entry() {
        let path = temp_desktop_file_path("disable-removes");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create temp autostart dir");
        }
        repair_autostart_desktop_file(&path, Path::new("/opt/Patina/patina"))
            .expect("write desktop file");

        remove_autostart_desktop_file(&path).expect("remove desktop file");

        let inspection = inspect_autostart_desktop_file_at(&path, None);
        assert!(!inspection.exists);
        assert_eq!(inspection.reason.as_deref(), Some("desktop-file-missing"));

        cleanup_temp_desktop_file(&path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_autostart_selects_existing_package_or_installed_desktop() {
        let path = temp_desktop_file_path("appimage");
        let root = path.parent().unwrap().parent().unwrap();
        std::fs::create_dir_all(root).unwrap();
        let image = root.join("My Tracker 100%.AppImage");
        let installed = root.join("installed/Patina");
        std::fs::write(&image, "package").unwrap();
        let selected =
            super::appimage_autostart_executable(Some(&image), Some(&installed)).unwrap();
        repair_autostart_desktop_file(&path, &selected).unwrap();
        assert_eq!(selected, image);
        assert!(selected.is_file());
        let inspection = inspect_autostart_desktop_file_at(&path, Some(&selected));
        assert!(inspection.valid());
        assert!(inspection.exec.unwrap().contains("100%%.AppImage"));

        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, "installed Desktop").unwrap();
        let selected =
            super::appimage_autostart_executable(Some(&image), Some(&installed)).unwrap();
        assert_eq!(selected, installed);
        repair_autostart_desktop_file(&path, &selected).unwrap();
        std::fs::remove_file(&image).unwrap();
        assert!(super::appimage_autostart_executable(Some(&image), None).is_err());
        assert!(super::appimage_autostart_executable(None, None).is_err());
        assert!(
            super::appimage_autostart_executable(Some(Path::new("relative.AppImage")), None)
                .is_err()
        );
        cleanup_temp_desktop_file(&path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires gio; launches only a private test script, not Patina"]
    fn appimage_autostart_launches_through_real_desktop_entry_parser() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_desktop_file_path("gio-parser");
        let root = path.parent().unwrap().parent().unwrap();
        std::fs::create_dir_all(root).unwrap();
        let image = root.join("My Tracker 100%.AppImage");
        let marker = root.join("arguments");
        std::fs::write(
            &image,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        repair_autostart_desktop_file(&path, &image).unwrap();
        assert!(std::process::Command::new("gio")
            .arg("launch")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "--autostart\n");
        cleanup_temp_desktop_file(&path);
    }

    fn temp_desktop_file_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "patina-autostart-test-{}-{name}",
                std::process::id()
            ))
            .join("autostart")
            .join("Patina.desktop")
    }

    fn cleanup_temp_desktop_file(path: &Path) {
        if let Some(root) = path.parent().and_then(Path::parent) {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
