#[path = "build_support/artifact_version.rs"]
mod artifact_version;

fn main() {
    println!("cargo:rerun-if-changed=build_support/artifact_version.rs");
    let desktop = cfg!(feature = "desktop");
    let standalone = if desktop {
        None
    } else {
        println!("cargo:rerun-if-changed=../packaging/daemon/VERSION");
        Some(
            std::fs::read_to_string("../packaging/daemon/VERSION")
                .expect("read standalone daemon version"),
        )
    };
    let version = artifact_version::select(
        desktop,
        &std::env::var("CARGO_PKG_VERSION").expect("Cargo package version"),
        standalone.as_deref(),
    )
    .expect("valid artifact version");
    println!("cargo:rustc-env=PATINA_ARTIFACT_VERSION={version}");
    let target = std::env::var("TARGET").expect("Cargo must provide the build target");
    println!("cargo:rustc-env=PATINA_BUILD_TARGET={target}");
    println!("cargo:rerun-if-env-changed=TAURI_CONFIG");
    println!("cargo:rustc-check-cfg=cfg(patina_local_build)");
    if std::env::var("TAURI_CONFIG")
        .map(|config| {
            config.contains("com.ceceliaee.patina.local") || config.contains("Patina Local")
        })
        .unwrap_or(false)
    {
        println!("cargo:rustc-cfg=patina_local_build");
    }

    #[cfg(feature = "desktop")]
    tauri_build::build()
}
