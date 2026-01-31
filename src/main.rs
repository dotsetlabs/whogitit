use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(e) = ai_blame::cli::run() {
        eprintln!("Error: {:#}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
