//! `sue-cli`: Sue for a terminal. The same program as `sue`, built as a
//! console program on every system, so --self-check, --snapshot and
//! --version print where they were run (on Windows, `sue` has no console).

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sue::cli(&args)
}
