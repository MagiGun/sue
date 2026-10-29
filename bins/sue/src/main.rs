//! The `sue` executable: everything is in the library; see [`sue::cli`].
//!
//! Built for release on Windows it is a windowed program, so opening it
//! shows no console; `sue-cli` is the console twin for --self-check and
//! --version. Elsewhere the attribute does nothing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    sue::cli(&args)
}
