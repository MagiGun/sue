//! `sue`: boots Sue on its Hub (or the studio or tool pinned there) in a
//! native GPU window, or runs the headless self-check, or renders offscreen
//! snapshots.
//! The executable is [`cli`]; phones and tablets start [`run_window`] from
//! their platform entry (`crates/sue-platform/android` on Android).

/// Text from the language database's `sue` namespace (the window, files and projects).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("sue", $($arg)*) };
}

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
mod update;
mod util;
mod viewer;

use std::path::PathBuf;
use std::process::ExitCode;

use sue_core::{SUE_CODENAME, SUE_MOTTO, SUE_VERSION};
use sue_platform::HeadlessHost;

const USAGE: &str = "\
Usage: sue [--self-check | --snapshot <dir> | --version | --help]
       sue lang [list | check [<tag>]]

  (no flags)        Open the Sue window (the Hub)
  --self-check      Boot every subsystem headlessly and verify they work together
  --snapshot <dir>  Render the shell offscreen into PNG files in <dir>
  --version         Print the version
  --help            Print this help
  lang list         List Sue's languages and how much of each is translated
  lang check [tag]  Check one language (or all) against English: missing,
                    stale and mismatched messages, ready to translate";

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

/// Prints how each language in `tags` stands against English. Fails when
/// one has stale or mismatched messages or does not parse (missing ones
/// only fall back to English).
fn lang_check(tags: &[String]) -> ExitCode {
    let mut sound = true;
    for tag in tags {
        match sue_lang::check(tag) {
            Some(report) => {
                print!("{report}");
                sound &= report.sound();
            }
            None => {
                eprintln!(
                    "sue: there is no language {tag}; add crates/sue-lang/locales/{tag}/ (see CONTRIBUTING.md)"
                );
                sound = false;
            }
        }
    }
    if sound {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
        ["lang"] | ["lang", "list"] => {
            for language in sue_lang::languages() {
                let Some(report) = sue_lang::check(&language.tag) else {
                    continue;
                };
                let status = if language.draft { ", draft" } else { "" };
                println!(
                    "{:<8} {} ({} of {} messages{status})",
                    language.tag,
                    language.name,
                    report.translated(),
                    report.total
                );
            }
            ExitCode::SUCCESS
        }
        ["lang", "check"] => lang_check(
            &sue_lang::languages()
                .into_iter()
                .map(|language| language.tag)
                .collect::<Vec<_>>(),
        ),
        ["lang", "check", tag] => lang_check(&[(*tag).to_owned()]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
