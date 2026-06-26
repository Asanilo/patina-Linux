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
    inspect_autostart_desktop_file_at(&autostart_desktop_file_path())
}

pub(crate) fn repair_current_exe_autostart_desktop_file() -> Result<(), String> {
    let executable_path = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    repair_autostart_desktop_file(&autostart_desktop_file_path(), &executable_path)
        .map_err(|error| format!("failed to repair autostart desktop file: {error}"))
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

fn inspect_autostart_desktop_file_at(path: &Path) -> AutostartDesktopFileInspection {
    let content = std::fs::read_to_string(path).ok();
    let exec = content
        .as_deref()
        .and_then(extract_desktop_exec)
        .map(str::to_string);
    let exists = path.exists();
    let reason = resolve_autostart_reason(exists, exec.as_deref());

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
    let executable = quote_desktop_exec_argument(&executable_path.display().to_string());
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name=Patina\n\
Comment=Start Patina in the background\n\
Exec={executable} {}\n\
StartupNotify=false\n\
Terminal=false\n\
X-GNOME-Autostart-enabled=true\n",
        crate::app::runtime::AUTOSTART_ARG
    )
}

fn quote_desktop_exec_argument(argument: &str) -> String {
    if !argument
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | '\\' | '$' | '`'))
    {
        return argument.to_string();
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

        let before = inspect_autostart_desktop_file_at(&path);
        assert_eq!(before.reason.as_deref(), Some("exec-not-patina"));

        repair_autostart_desktop_file(&path, Path::new("/opt/Patina/patina"))
            .expect("repair stale desktop file");

        let after = inspect_autostart_desktop_file_at(&path);
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

        let inspection = inspect_autostart_desktop_file_at(&path);
        assert!(!inspection.exists);
        assert_eq!(inspection.reason.as_deref(), Some("desktop-file-missing"));

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
