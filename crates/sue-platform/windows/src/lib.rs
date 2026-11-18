//! Sue on Windows: the calls into Windows' own components that have no
//! safe route, each behind a safe function that `sue-platform` uses.
//! Nothing else in Sue calls this crate.
//!
//! `unsafe` is forbidden in Sue (backbone rule 6); this crate relaxes the
//! workspace's `forbid` to `deny` and allows it item by item, under the
//! maintainer's standing approval of 2026-11-13 for `unsafe` that gives a
//! real benefit. Here the benefit is an operating system API with no safe
//! call: Media Foundation is COM, and every COM call is `unsafe` in the
//! `windows` crate. Every `unsafe` block carries a `// SAFETY:` comment
//! saying why it is sound (`clippy::undocumented_unsafe_blocks` is denied
//! in the manifest), and the tests cover what each module promises,
//! error paths included.
//!
//! The modules:
//!
//! - [`camera`]: the cameras of this computer and pictures from one,
//!   through the Media Foundation Source Reader, for the Hand Scanner.
//! - [`decode`]: movies and sound read through the Media Foundation
//!   Source Reader, on the graphics card where it can, with exact frame
//!   seeking, for Sue VE.
//! - [`media`]: MP4 movies through the Media Foundation Sink Writer, with
//!   Windows' own H.264 and AAC encoders, and reading one back.
//! - [`screen`]: the colour of a pixel on the screen, and a pick that
//!   waits for a click anywhere, for Edit, Obtain Screen Color.
//!
//! On every other system this library is empty.

#[cfg(target_os = "windows")]
pub mod camera;
#[cfg(target_os = "windows")]
pub mod decode;
#[cfg(target_os = "windows")]
pub mod media;
#[cfg(target_os = "windows")]
pub mod screen;
