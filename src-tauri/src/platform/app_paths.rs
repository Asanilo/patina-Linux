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
}

pub fn app_profile<R: Runtime>(app: &AppHandle<R>) -> AppProfile {
    AppProfile::from_identifier(&app.config().identifier)
}

pub fn product_roaming_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(roaming_root(app)?.join(app_profile(app).product_folder()))
}

pub fn product_local_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(local_root(app)?.join(app_profile(app).product_folder()))
}

pub fn product_webview_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    product_local_data_dir(app)
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
}
