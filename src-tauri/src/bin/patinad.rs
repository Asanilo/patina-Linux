fn main() {
    if std::env::args_os().len() == 2
        && std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--build-info"))
    {
        println!(
            "{}",
            serde_json::to_string(&patina_lib::daemon_build_info())
                .expect("static build metadata must serialize")
        );
        return;
    }
    if std::env::args_os().len() == 2
        && std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--version"))
    {
        println!("patinad {}", patina_lib::daemon_build_info().package_version);
        return;
    }
    if let Err(error) = patina_lib::run_daemon(std::env::args()) {
        if patina_lib::is_controlled_daemon_restart(&error) {
            eprintln!("[patinad] {error}");
            std::process::exit(patina_lib::CONTROLLED_DAEMON_RESTART_EXIT_CODE);
        }
        eprintln!("[patinad] failed to start: {error}");
        std::process::exit(1);
    }
}
