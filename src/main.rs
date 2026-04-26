use slicer_engine::cli::CliArgs;

fn main() {
    if let Err(e) = CliArgs::run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
