//! `sue-cli`: Sue for a terminal. The same program as `sue`, built as a
//! console program on every system, so --self-check, --snapshot and
//! --version print where they were run (on Windows, `sue` has no console).
//!
//! It also has the tools that only make sense in a terminal: `clip dump`
//! and `clip diff`, the survey of Clip Studio files.

use std::process::ExitCode;

#[path = "sue-cli/clip.rs"]
mod clip;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((first, rest)) if first == "clip" => clip::run(rest),
        Some((first, [])) if first == "--help" || first == "-h" => {
            let code = sue::cli(&args);
            println!("\n{}", clip::USAGE);
            code
        }
        _ => sue::cli(&args),
    }
}
