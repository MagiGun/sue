//! `sue`: boots Sue on its Hub (or the studio or tool pinned there) in a
//! native GPU window, or runs the headless self-check, or renders offscreen
//! snapshots.
//! The executable is [`cli`]; phones and tablets start [`run_window`] from
//! their platform entry (`crates/sue-platform/android` on Android).

mod applets;
mod convert;
mod desktop;
mod engine;
mod runtime;
mod selfcheck;
mod settings;
mod snapshot;
#[cfg(test)]
mod style;
mod util;
mod viewer;

use std::path::PathBuf;
use std::process::ExitCode;

use sue_core::{SUE_CODENAME, SUE_MOTTO, SUE_VERSION};
use sue_platform::HeadlessHost;

const USAGE: &str = "\
Usage: sue [--self-check | --snapshot <dir> | --version | --help]

  (no flags)        Open the Sue window (the Hub)
  --self-check      Boot every subsystem headlessly and verify they work together
  --snapshot <dir>  Render the shell offscreen into PNG files in <dir>
  --version         Print the version
  --help            Print this help";

fn banner() {
    println!("Sue v{SUE_VERSION} \"{SUE_CODENAME}\", the Singular Unified Engine");
    println!("\"{SUE_MOTTO}\"");
    println!();
}

fn self_check() -> ExitCode {
    banner();
    let subject = HeadlessHost::detect()
        .map_err(|error| format!("platform: {error}"))
        .and_then(|host| {
            let engine = engine::Engine::boot(&host)?;
            Ok(selfcheck::Subject {
                runtime: runtime::Runtime::new(engine)?,
                host,
            })
        });
    let mut subject = match subject {
        Ok(subject) => subject,
        Err(error) => {
            eprintln!("boot failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = selfcheck::run(&mut subject);
    print!("{report}");
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn snapshots(dir: PathBuf) -> ExitCode {
    match snapshot::write_snapshots(&dir, 1440, 900) {
        Ok(paths) => {
            for path in paths {
                println!("{}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("snapshot failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Opens the Sue window and runs until it closes.
///
/// # Errors
/// If the window or the GPU cannot start.
pub fn run_window() -> Result<(), String> {
    desktop::run()
}

/// Runs Sue as the command line says (`args` without the program name).
#[must_use]
pub fn cli(args: &[String]) -> ExitCode {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => match run_window() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("sue: {error}");
                eprintln!("sue: run `sue --self-check` for headless diagnostics");
                ExitCode::FAILURE
            }
        },
        ["--self-check"] => self_check(),
        ["--snapshot", dir] => snapshots(PathBuf::from(dir)),
        ["--version" | "-V"] => {
            println!("sue {SUE_VERSION} \"{SUE_CODENAME}\"");
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
