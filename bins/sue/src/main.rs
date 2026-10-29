//! The `sue` executable: everything is in the library; see [`sue::cli`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sue::cli(&args)
}
