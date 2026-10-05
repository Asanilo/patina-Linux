use std::path::PathBuf;

struct Options {
    manifest: String,
    root: PathBuf,
    allow_debug: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let manifest = args
        .get(2)
        .filter(|value| !value.starts_with("--"))
        .ok_or("--deactivate-runtime requires the selected manifest digest")?
        .clone();
    let mut root = None;
    let mut allow_debug = false;
    let mut tail = args.iter().skip(3);
    while let Some(arg) = tail.next() {
        match arg.as_str() {
            "--runtime-root" if root.is_none() => {
                root = Some(PathBuf::from(
                    tail.next()
                        .filter(|value| !value.starts_with("--"))
                        .ok_or("--runtime-root requires a value")?,
                ));
            }
            "--allow-debug" if !allow_debug => allow_debug = true,
            _ => return Err(format!("unknown or repeated deactivation option `{arg}`")),
        }
    }
    Ok(Options {
        manifest,
        root: root.ok_or("deactivation requires --runtime-root")?,
        allow_debug,
    })
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    #[cfg(target_os = "linux")]
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        let record =
            runtime.block_on(crate::app::standalone_activation::deactivation::deactivate(
                &options.root,
                &options.manifest,
                options.allow_debug,
            ))?;
        println!(
            "{}",
            serde_json::to_string(&record).map_err(|error| error.to_string())?
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (options.root, options.manifest, options.allow_debug);
        Err("standalone service deactivation is supported only on Linux".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deactivation_requires_an_explicit_target_and_rejects_other_operations() {
        let base = [
            "patinad",
            "--deactivate-runtime",
            "digest",
            "--runtime-root",
            "/runtime",
        ];
        let with = |tail: &[&str]| {
            base.iter()
                .chain(tail)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        assert!(parse(&with(&[])).is_ok());
        assert!(parse(&with(&["--allow-debug"])).unwrap().allow_debug);
        for tail in [
            &["--runtime-root", "/other"][..],
            &["--profile", "production"],
            &["--allow-debug", "--allow-debug"],
            &["--uninstall-runtime", "digest"],
        ] {
            assert!(parse(&with(tail)).is_err());
        }
        assert!(parse(&["patinad".into(), "--deactivate-runtime".into()]).is_err());
    }
}
