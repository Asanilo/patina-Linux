//! Explicit file installation commands; service activation is a separate operation.
use std::path::PathBuf;

enum Operation {
    Stage {
        source: PathBuf,
        manifest: String,
    },
    Inspect,
    Select {
        manifest: String,
        expected_current: String,
    },
}

struct Options {
    operation: Operation,
    root: PathBuf,
    allow_debug: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut source = None;
    let mut root = None;
    let mut manifest = None;
    let mut selected = None;
    let mut expected_current = None;
    let mut inspect = false;
    let mut allow_debug = false;
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--stage-runtime" => &mut source,
            "--select-runtime" => &mut selected,
            "--runtime-root" => &mut root,
            "--manifest-sha256" => &mut manifest,
            "--expected-current" => &mut expected_current,
            "--inspect-runtime" if !inspect => {
                inspect = true;
                continue;
            }
            "--allow-debug" if !allow_debug => {
                allow_debug = true;
                continue;
            }
            _ => return Err(format!("unknown or repeated installation option `{arg}`")),
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
    let operation = match (source, selected, inspect, manifest, expected_current) {
        (Some(source), None, false, Some(manifest), None) => Operation::Stage {
            source: PathBuf::from(source), manifest,
        },
        (None, None, true, None, None) => Operation::Inspect,
        (None, Some(manifest), false, None, Some(expected_current)) => Operation::Select {
            manifest, expected_current,
        },
        _ => return Err("choose one installation operation: stage requires --manifest-sha256; select requires --expected-current; inspect accepts neither".into()),
    };
    Ok(Options {
        operation,
        root: PathBuf::from(root.ok_or("--runtime-root is required")?),
        allow_debug,
    })
}

pub(super) fn run_from_args(args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    #[cfg(target_os = "linux")]
    {
        use crate::platform::linux::standalone_runtime as runtime;
        let result = match options.operation {
            Operation::Stage { source, manifest } => serde_json::to_value(runtime::stage(
                &source,
                &options.root,
                &manifest,
                options.allow_debug,
            )?),
            Operation::Inspect => {
                serde_json::to_value(runtime::inspect(&options.root, options.allow_debug)?)
            }
            Operation::Select {
                manifest,
                expected_current,
            } => serde_json::to_value(runtime::select(
                &options.root,
                &manifest,
                &runtime::ExpectedCurrent::parse(&expected_current)?,
                options.allow_debug,
            )?),
        }
        .map_err(|error| error.to_string())?;
        println!("{result}");
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        match options.operation {
            Operation::Stage { source, manifest } => {
                let _ = (source, manifest);
            }
            Operation::Select {
                manifest,
                expected_current,
            } => {
                let _ = (manifest, expected_current);
            }
            Operation::Inspect => {}
        }
        let _ = (options.root, options.allow_debug);
        Err("standalone runtime installation is currently supported only on Linux".into())
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

    #[test]
    fn selection_requires_an_explicit_baseline_and_rejects_mixed_operations() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        let mut select = args(&[
            "patinad",
            "--select-runtime",
            "digest",
            "--runtime-root",
            "/runtime",
        ]);
        assert!(parse(&select).is_err());
        select.extend(args(&["--expected-current", "none"]));
        assert!(matches!(
            parse(&select).unwrap().operation,
            Operation::Select { .. }
        ));
        select.push("--inspect-runtime".into());
        assert!(parse(&select).is_err());
        let mut inspect = args(&["patinad", "--inspect-runtime", "--runtime-root", "/runtime"]);
        assert!(matches!(
            parse(&inspect).unwrap().operation,
            Operation::Inspect
        ));
        inspect.extend(args(&["--expected-current", "none"]));
        assert!(parse(&inspect).is_err());
    }
}
