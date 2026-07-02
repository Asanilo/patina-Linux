use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

pub const PRODUCT_FOLDER: &str = "Patina";
pub const PRODUCT_FOLDER_LOCAL: &str = "Patina Local";
pub const PRODUCT_FOLDER_DEV: &str = "Patina Dev";

pub const IDENTIFIER_PROD: &str = "com.ceceliaee.patina";
pub const IDENTIFIER_LOCAL: &str = "com.ceceliaee.patina.local";
pub const IDENTIFIER_DEV: &str = "com.ceceliaee.patina.dev";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppProfile {
    Production,
    Local,
    Dev,
}

impl AppProfile {
    pub fn from_identifier(identifier: &str) -> Self {
        match identifier {
            IDENTIFIER_PROD => Self::Production,
            IDENTIFIER_LOCAL => Self::Local,
            IDENTIFIER_DEV => Self::Dev,
            _ => Self::Production,
        }
    }

    pub fn product_folder(self) -> &'static str {
        match self {
            Self::Production => PRODUCT_FOLDER,
            Self::Local => PRODUCT_FOLDER_LOCAL,
            Self::Dev => PRODUCT_FOLDER_DEV,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Local => "local",
            Self::Dev => "dev",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPathRoots {
    pub config: PathBuf,
    pub data: PathBuf,
    pub local_data: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfilePaths {
    pub control_root: PathBuf,
    pub data_root: PathBuf,
    pub webview_root: PathBuf,
}

pub fn app_profile<R: Runtime>(app: &AppHandle<R>) -> AppProfile {
    AppProfile::from_identifier(&app.config().identifier)
}

pub fn product_config_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(default_profile_paths(app)?.control_root)
}

pub fn default_profile_paths<R: Runtime>(app: &AppHandle<R>) -> Result<ProfilePaths, String> {
    Ok(profile_paths(&app_path_roots(app)?, app_profile(app)))
}

pub fn profile_paths(roots: &AppPathRoots, profile: AppProfile) -> ProfilePaths {
    ProfilePaths {
        control_root: derive_product_root(&roots.config, profile),
        data_root: derive_product_root(&roots.data, profile),
        webview_root: derive_product_root(&roots.local_data, profile),
    }
}

pub fn derive_product_root(selected_root: &Path, profile: AppProfile) -> PathBuf {
    let product_folder = profile.product_folder();
    if selected_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| product_folder_name_eq(name, product_folder))
    {
        return selected_root.to_path_buf();
    }
    selected_root.join(product_folder)
}

fn app_path_roots<R: Runtime>(app: &AppHandle<R>) -> Result<AppPathRoots, String> {
    Ok(AppPathRoots {
        config: config_root(app)?,
        data: roaming_root(app)?,
        local_data: local_root(app)?,
    })
}

fn config_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    parent_of_identifier_dir(
        app.path()
            .app_config_dir()
            .map_err(|error| format!("failed to resolve app config dir: {error}"))?,
    )
}

fn roaming_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    parent_of_identifier_dir(
        app.path()
            .app_data_dir()
            .map_err(|error| format!("failed to resolve app data dir: {error}"))?,
    )
}

fn local_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    parent_of_identifier_dir(
        app.path()
            .app_local_data_dir()
            .map_err(|error| format!("failed to resolve app local data dir: {error}"))?,
    )
}

fn parent_of_identifier_dir(path: PathBuf) -> Result<PathBuf, String> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "failed to resolve parent directory for identifier path `{}`",
                path.display()
            )
        })
}

fn product_folder_name_eq(left: &str, right: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        left.eq_ignore_ascii_case(right)
    }

    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_profile_from_current_identifiers() {
        assert_eq!(
            AppProfile::from_identifier("com.ceceliaee.patina"),
            AppProfile::Production
        );
        assert_eq!(
            AppProfile::from_identifier("com.ceceliaee.patina.local"),
            AppProfile::Local
        );
        assert_eq!(
            AppProfile::from_identifier("com.ceceliaee.patina.dev"),
            AppProfile::Dev
        );
    }

    #[test]
    fn profile_folder_names_are_user_visible() {
        assert_eq!(AppProfile::Production.product_folder(), "Patina");
        assert_eq!(AppProfile::Local.product_folder(), "Patina Local");
        assert_eq!(AppProfile::Dev.product_folder(), "Patina Dev");
    }

    #[test]
    fn profile_folder_names_do_not_use_internal_identifiers() {
        for profile in [AppProfile::Production, AppProfile::Local, AppProfile::Dev] {
            let folder = profile.product_folder();
            assert!(!folder.contains("com.ceceliaee.patina"));
            assert!(!folder.contains("io.github"));
        }
    }

    #[test]
    fn linux_profile_paths_keep_control_outside_movable_data() {
        let roots = AppPathRoots {
            config: PathBuf::from("/home/u/.config"),
            data: PathBuf::from("/home/u/.local/share"),
            local_data: PathBuf::from("/home/u/.local/share"),
        };

        let paths = profile_paths(&roots, AppProfile::Production);

        assert_eq!(paths.control_root, PathBuf::from("/home/u/.config/Patina"));
        assert_eq!(
            paths.data_root,
            PathBuf::from("/home/u/.local/share/Patina")
        );
        assert_eq!(
            paths.webview_root,
            PathBuf::from("/home/u/.local/share/Patina")
        );
    }

    #[test]
    fn custom_parent_derives_profile_owned_directory() {
        assert_eq!(
            derive_product_root(Path::new("/mnt/work"), AppProfile::Dev),
            PathBuf::from("/mnt/work/Patina Dev")
        );
        assert_eq!(
            derive_product_root(Path::new("/mnt/work/Patina Dev"), AppProfile::Dev),
            PathBuf::from("/mnt/work/Patina Dev")
        );
    }
}
