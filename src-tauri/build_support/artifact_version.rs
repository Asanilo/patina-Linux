//! Build-time selection of the product artifact version, independent of protocol/schema.
pub fn select(
    desktop: bool,
    cargo_version: &str,
    standalone_version: Option<&str>,
) -> Result<String, String> {
    let value = if desktop {
        cargo_version
    } else {
        standalone_version.ok_or("standalone builds require packaging/daemon/VERSION")?
    };
    let version = value.trim_end_matches(['\r', '\n']);
    if version.is_empty() || version.len() > 64 || version.chars().any(char::is_whitespace) {
        return Err("artifact version must be a single SemVer value of at most 64 bytes".into());
    }
    semver::Version::parse(version)
        .map_err(|error| format!("invalid artifact version: {error}"))?;
    Ok(version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_version_is_required_only_for_the_standalone_projection() {
        assert_eq!(select(true, "1.9.2", None).unwrap(), "1.9.2");
        assert_eq!(select(true, "1.9.2", Some("2.0.0")).unwrap(), "1.9.2");
        assert_eq!(
            select(false, "1.9.2", Some("2.0.0-test.1\n")).unwrap(),
            "2.0.0-test.1"
        );
        assert!(select(false, "1.9.2", None).is_err());
        for value in [
            "",
            " 1.9.2",
            "1.9.2 ",
            "v1.9.2",
            "1.9.2\n2.0.0",
            "1.9.2\nmalformed=1",
            "1.9.2\0",
        ] {
            assert!(select(false, "1.9.2", Some(value)).is_err(), "{value:?}");
        }
    }
}
