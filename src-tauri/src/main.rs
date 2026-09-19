// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--patinad")) {
        use std::os::unix::process::CommandExt;
        let result = std::env::current_exe().and_then(|exe| {
            let daemon = exe
                .parent()
                .ok_or_else(|| std::io::Error::other("missing binary directory"))?
                .join("patinad");
            Err::<(), _>(
                std::process::Command::new(daemon)
                    .args(std::env::args_os().skip(2))
                    .exec(),
            )
        });
        eprintln!(
            "[patinad] bundled runtime launch failed: {}",
            result.unwrap_err()
        );
        std::process::exit(1);
    }
    patina_lib::run()
}
