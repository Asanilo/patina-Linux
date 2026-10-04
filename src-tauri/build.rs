fn main() {
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
