fn main() {
    if let Err(error) = patina_lib::run_daemon(std::env::args()) {
        if patina_lib::is_controlled_daemon_restart(&error) {
            eprintln!("[patinad] {error}");
            std::process::exit(patina_lib::CONTROLLED_DAEMON_RESTART_EXIT_CODE);
        }
        eprintln!("[patinad] failed to start: {error}");
        std::process::exit(1);
    }
}
