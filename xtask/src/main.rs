use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [command] if command == "validate" => match xtask::run_validation() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: xtask validate");
            ExitCode::from(2)
        }
    }
}
