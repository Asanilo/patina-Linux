fn main() {
    if let Err(error) = patina_lib::run_daemon(std::env::args()) {
        eprintln!("[patinad] failed to start: {error}");
        std::process::exit(1);
    }
}
