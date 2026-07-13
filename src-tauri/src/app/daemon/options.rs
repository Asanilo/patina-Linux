use crate::platform::app_paths::AppProfile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonRunOptions {
    pub profile: AppProfile,
    pub port_override: Option<u16>,
    pub serve_minimal_api: bool,
}

impl Default for DaemonRunOptions {
    fn default() -> Self {
        Self {
            profile: default_profile(),
            port_override: None,
            serve_minimal_api: false,
        }
    }
}

impl DaemonRunOptions {
    pub fn from_args(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        let _program = args.next();
        while let Some(arg) = args.next() {
            match arg.as_ref() {
                "--serve-api" => options.serve_minimal_api = true,
                "--profile" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--profile requires a value".to_string())?;
                    options.profile = parse_profile(value.as_ref())?;
                }
                "--port" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--port requires a value".to_string())?;
                    options.port_override = Some(parse_port(value.as_ref())?);
                }
                unknown => return Err(format!("unknown patinad option `{unknown}`")),
            }
        }
        Ok(options)
    }
}

fn parse_profile(value: &str) -> Result<AppProfile, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "production" => Ok(AppProfile::Production),
        "local" => Ok(AppProfile::Local),
        "dev" => Ok(AppProfile::Dev),
        _ => Err(format!(
            "invalid profile `{value}`; expected production, local, or dev"
        )),
    }
}

fn parse_port(value: &str) -> Result<u16, String> {
    if value.trim() == "0" {
        return Ok(0);
    }
    crate::domain::settings::parse_local_api_port(value)
        .ok_or_else(|| "invalid local API port; expected 0 or 1024-65535".to_string())
}

const fn default_profile() -> AppProfile {
    if cfg!(debug_assertions) {
        AppProfile::Dev
    } else {
        AppProfile::Production
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_profile_and_ephemeral_port_are_parsed() {
        let options = DaemonRunOptions::from_args([
            "patinad",
            "--profile",
            "local",
            "--port",
            "0",
            "--serve-api",
        ])
        .unwrap();

        assert_eq!(options.profile, AppProfile::Local);
        assert_eq!(options.port_override, Some(0));
        assert!(options.serve_minimal_api);
    }

    #[test]
    fn invalid_profile_is_rejected() {
        let error = DaemonRunOptions::from_args(["patinad", "--profile", "windows"]).unwrap_err();

        assert!(error.contains("invalid profile"));
    }

    #[test]
    fn build_default_profile_matches_build_kind() {
        let options = DaemonRunOptions::from_args(["patinad"]).unwrap();

        #[cfg(debug_assertions)]
        assert_eq!(options.profile, AppProfile::Dev);
        #[cfg(not(debug_assertions))]
        assert_eq!(options.profile, AppProfile::Production);
    }
}
