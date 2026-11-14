//! The files browser, shared by every studio and the Hub: the work Sue
//! remembers (files opened and saved lately, Sue projects, the work kept
//! while not saved) as cards with thumbnails, sorted with the shared
//! labels and favorites, searched, filtered by type and by studio.
//!
//! - [`model`]: what is listed ([`items`], [`shown`]) and the browser's
//!   state ([`ProjectsState`]).
//! - [`thumbs`]: the cards' pictures, made in the background
//!   ([`Thumbs`]), from a `.sue` file's own preview or the studio's
//!   [`Thumbnailer`] ([`crate::SueApplet::thumbnail`]).
//! - [`browser`]: the page itself, the project popup and the Data
//!   Transfers popup, drawn with the start page kit ([`crate::start`]).
//!
//! It came out of Sue Paint's start page, which draws it until the Hub's
//! Files place takes over.

pub mod browser;
pub mod model;
pub mod thumbs;

pub use browser::{Event, project_popup, projects_ui, transfers_popup};
pub use model::{
    Item, ProjectKey, ProjectsState, Shelf, Studio, Type, View, forget, items, shown, texture_key,
};
pub use thumbs::{Thumb, Thumbnailer, Thumbs};
