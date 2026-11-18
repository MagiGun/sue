//! # sue-ve: Sue VE, the video editing studio
//!
//! Sue VE (SVE for short) edits video the way Premiere Pro and DaVinci
//! Resolve's Edit page do on a computer: a media pool, source and program
//! monitors, a timeline of video and audio tracks, an inspector, and
//! export to MP4.

/// Text from the language database's `ve` namespace (Sue VE).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("ve", $($arg)*) };
}

mod commands;
pub mod edit;
pub mod export;
pub mod media;
mod menus;
pub mod mix;
pub mod model;
pub mod playback;
pub mod render;
pub mod sample;
mod settings_page;
pub mod studio;
pub mod ui;

pub use studio::{ID, SueVe};
