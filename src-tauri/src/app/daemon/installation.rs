//! Explicit file staging command; service activation is a separate operation.
use std::path::PathBuf;

struct Options {
    source: PathBuf,
    root: PathBuf,
    manifest_sha256: String,
    allow_debug: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut source = None;
    let mut root = None;
    let mut manifest = None;
    let mut allow_debug = false;
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--stage-runtime" => &mut source,
            "--runtime-root" => &mut root,
            "--manifest-sha256" => &mut manifest,
            "--allow-debug" if !allow_debug => {
                allow_debug = true;
                continue;
            }
            _ => return Err(format!("unknown or repeated staging option `{arg}`")),
        };
        if slot.is_some() {
            return Err(format!("repeated staging option `{arg}`"));
        }
        let value = args
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("{arg} requires a value"))?;
        *slot = Some(value.clone());
    }
    Ok(Options {
        source: PathBuf::from(
            source.ok_or("--stage-runtime requires an unpacked candidate directory")?,
        ),
        root: PathBuf::from(root.ok_or("--runtime-root is required")?),
        manifest_sha256: manifest.ok_or("--manifest-sha256 is required")?,
        allow_debug,
    })
}

pub(super) fn stage_from_args(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    #[cfg(target_os = "linux")]
    {
        let staged = crate::platform::linux::standalone_runtime::stage(
            &options.source,
            &options.root,
            &options.manifest_sha256,
            options.allow_debug,
        )?;
        println!(
            "{}",
            serde_json::to_string(&staged).map_err(|error| error.to_string())?
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (
            options.source,
            options.root,
            options.manifest_sha256,
            options.allow_debug,
        );
        Err("standalone runtime staging is currently supported only on Linux".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_requires_an_explicit_root_and_manifest_identity() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        assert!(parse(&args(&["patinad", "--stage-runtime", "/candidate"])).is_err());
        assert!(parse(&args(&[
            "patinad",
            "--stage-runtime",
            "/candidate",
            "--runtime-root",
            "/runtime"
        ]))
        .is_err());
        let mut valid = args(&[
            "patinad",
            "--stage-runtime",
            "/candidate",
            "--runtime-root",
            "/runtime",
            "--manifest-sha256",
            "digest",
            "--allow-debug",
        ]);
        assert!(parse(&valid).unwrap().allow_debug);
        valid.push("--allow-debug".into());
        assert!(parse(&valid).is_err());
    }
}
