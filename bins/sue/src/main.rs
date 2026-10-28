//! `sue`: boots Sue v1.0 "Faustian Enigma" and runs the self-check.

mod engine;
mod quick_notes;
mod selfcheck;

use std::process::ExitCode;

use sue_core::{SUE_CODENAME, SUE_MOTTO, SUE_VERSION};

const USAGE: &str = "\
Usage: sue [--self-check | --version | --help]

  --self-check   Boot every subsystem and verify they work together (default)
  --version      Print the version
  --help         Print this help";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None | Some("--self-check"), None) => {}
        (Some("--version" | "-V"), None) => {
            println!("sue {SUE_VERSION} \"{SUE_CODENAME}\"");
            return ExitCode::SUCCESS;
        }
        (Some("--help" | "-h"), None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }

    println!("Sue v{SUE_VERSION} \"{SUE_CODENAME}\" · Singular Unified Engine");
    println!("\"{SUE_MOTTO}\"");
    println!();

    let mut engine = match engine::Engine::boot() {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("boot failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = selfcheck::run(&mut engine);
    print!("{report}");
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
