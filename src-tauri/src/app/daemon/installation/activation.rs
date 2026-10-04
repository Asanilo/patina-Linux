use std::path::PathBuf;

struct Options {
    root: PathBuf,
    manifest: String,
    api_port: u16,
    allow_debug: bool,
    migration: Option<(String, String, String)>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let manifest = args
        .get(2)
        .filter(|value| !value.starts_with("--"))
        .ok_or("--activate-runtime requires the selected manifest digest")?
        .clone();
    let mut root = None;
    let mut port = None;
    let mut source = None;
    let mut source_digest = None;
    let mut source_version = None;
    let mut allow_debug = false;
    let mut remaining = args.iter().skip(3);
    while let Some(arg) = remaining.next() {
        if arg == "--allow-debug" && !allow_debug {
            allow_debug = true;
            continue;
        }
        let slot = match arg.as_str() {
            "--runtime-root" => &mut root,
            "--api-port" => &mut port,
            "--migrate-from" => &mut source,
            "--source-unit-sha256" => &mut source_digest,
            "--source-version" => &mut source_version,
            _ => return Err(format!("unknown or repeated activation option `{arg}`")),
        };
        if slot.is_some() {
            return Err(format!("repeated activation option `{arg}`"));
        }
        *slot = Some(
            remaining
                .next()
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| format!("{arg} requires a value"))?
                .clone(),
        );
    }
    let api_port = port
        .as_deref()
        .unwrap_or("14840")
        .parse::<u16>()
        .map_err(|_| "invalid configured API port")?;
    if api_port < 1024 {
        return Err("configured API port must be in 1024..65535".into());
    }
    let migration = match (source, source_digest, source_version) {
        (None, None, None) => None,
        (Some(kind), Some(digest), Some(version)) => Some((kind, digest, version)),
        _ => return Err(
            "migration requires --migrate-from, --source-unit-sha256 and --source-version together"
                .into(),
        ),
    };
    Ok(Options {
        root: PathBuf::from(root.ok_or("activation requires --runtime-root")?),
        manifest,
        api_port,
        allow_debug,
        migration,
    })
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    #[cfg(target_os = "linux")]
    {
        let migration = options
            .migration
            .as_ref()
            .map(|(kind, digest, version)| {
                crate::app::standalone_activation::migration::MigrationRequest::parse(
                    kind, digest, version,
                )
            })
            .transpose()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let activated = runtime.block_on(crate::app::standalone_activation::activate(
            &options.root,
            &options.manifest,
            options.api_port,
            options.allow_debug,
            migration,
        ))?;
        println!(
            "{}",
            serde_json::to_string(&activated).map_err(|error| error.to_string())?
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (
            options.root,
            options.manifest,
            options.api_port,
            options.allow_debug,
            options.migration,
        );
        Err("standalone service activation is supported only on Linux".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_confirmation_is_all_or_none_and_unique() {
        let base = [
            "patinad",
            "--activate-runtime",
            "digest",
            "--runtime-root",
            "/runtime",
        ];
        let parse_tail = |tail: &[&str]| {
            parse(
                &base
                    .iter()
                    .chain(tail)
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>(),
            )
        };
        for tail in [
            vec!["--migrate-from", "packaged"],
            vec!["--source-unit-sha256", "digest"],
            vec!["--source-version", "1.9.2"],
        ] {
            assert!(parse_tail(&tail).is_err());
        }
        let valid = [
            "--migrate-from",
            "packaged",
            "--source-unit-sha256",
            "digest",
            "--source-version",
            "1.9.2",
        ];
        assert_eq!(
            parse_tail(&valid).unwrap().migration,
            Some(("packaged".into(), "digest".into(), "1.9.2".into()))
        );
        let mut duplicate = valid.to_vec();
        duplicate.extend(["--source-version", "1.9.3"]);
        assert!(parse_tail(&duplicate).is_err());
    }
    #[test]
    fn activation_requires_an_explicit_target_and_rejects_profile_or_duplicate_flags() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        assert!(parse(&args(&["patinad", "--activate-runtime", "digest"])).is_err());
        let mut valid = args(&[
            "patinad",
            "--activate-runtime",
            "digest",
            "--runtime-root",
            "/runtime",
        ]);
        assert_eq!(parse(&valid).unwrap().api_port, 14840);
        valid.extend(args(&["--api-port", "16000", "--allow-debug"]));
        assert_eq!(parse(&valid).unwrap().api_port, 16000);
        valid.push("--allow-debug".into());
        assert!(parse(&valid).is_err());
        for tail in [["--profile", "local"], ["--api-port", "0"]] {
            let mut values = args(&[
                "patinad",
                "--activate-runtime",
                "digest",
                "--runtime-root",
                "/runtime",
            ]);
            values.extend(args(&tail));
            assert!(parse(&values).is_err());
        }
    }
}
