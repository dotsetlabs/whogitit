use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = whogitit::cli::run() {
        eprintln!("Error: {:#}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
