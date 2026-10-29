//! Sue on iOS, for sue-platform:
//!
//! - UIKit's document picker, for the file picks. It shows
//!   `UIDocumentPickerViewController` over Sue's window to open a copy of
//!   a document, to choose a folder or to export a written file to Files,
//!   and keeps the answer for sue-platform to take ([`take_picked`]).
//! - The on-screen keyboard. It watches UIKit's keyboard notifications
//!   and says how much of the bottom of Sue's window the keyboard covers
//!   ([`keyboard_inset`]), so that Sue lays out above it.
//!
//! On every other system this library is empty.
//!
//! Talking to Objective-C is unsafe in Rust. The audited exceptions,
//! each in an `audited` module and approved by the maintainer (backbone
//! rule 6), are the picker's delegate and the calls that make and show
//! the picker (2026-10-29), and the keyboard's notification observers
//! with UIKit's constants, the notification and the keyboard's frame
//! they read (2026-10-29). Anything more needs its own approval.

#[cfg(any(target_os = "ios", test))]
mod cover;
#[cfg(target_os = "ios")]
mod keyboard;
#[cfg(target_os = "ios")]
mod picker;
#[cfg(target_os = "ios")]
mod window;

#[cfg(target_os = "ios")]
pub use keyboard::keyboard_inset;
#[cfg(target_os = "ios")]
pub use picker::{Picked, show_export, show_folder, show_open, take_picked};
