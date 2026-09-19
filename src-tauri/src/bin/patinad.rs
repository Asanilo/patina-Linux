fn main() {
    if std::env::args_os().len() == 2
        && std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--version"))
    {
        println!("patinad {}", env!("CARGO_PKG_VERSION"));
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
