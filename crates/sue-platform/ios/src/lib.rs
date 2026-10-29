//! Sue on iOS: UIKit's document picker, for sue-platform's file picks.
//! It shows `UIDocumentPickerViewController` over Sue's window to open a
//! copy of a document, to choose a folder or to export a written file to
//! Files, and keeps the answer for sue-platform to take
//! ([`take_picked`]).
//!
//! On every other system this library is empty.
//!
//! Talking to Objective-C is unsafe in Rust. The picker's delegate and
//! the calls that make and show the picker are the audited exceptions,
//! approved by the maintainer on 2026-10-29 (backbone rule 6), and all of
//! them are in the `audited` module; anything more needs its own
//! approval.

#[cfg(target_os = "ios")]
mod picker;
#[cfg(target_os = "ios")]
pub use picker::{Picked, show_export, show_folder, show_open, take_picked};
