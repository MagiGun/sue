//! # sue-picture: the picture engine
//!
//! Everything that works on a layered picture without a window: the
//! picture document over [`sue_model::picture`] ([`doc`]), compositing
//! ([`composite`]), the brush engine ([`brush`]), selections ([`select`]),
//! fills ([`fill`]), filters ([`filters`]), transforms and warps, vector
//! lines, rulers, comic frames and balloons, animation and the camera,
//! 3D layers, print and proof settings, PSD files and time-lapse. They
//! are plain functions over the arena and its pixels with their own
//! tests.
//!
//! Sue Paint is the studio on top of it and re-exports these modules, so
//! `sue_paint::composite` and `sue_picture::composite` are the same.
//! Other studios use the engine directly: Sue Writer shows a picture
//! through [`composite::preview`] without depending on Sue Paint.
//!
//! The engine has no interface code of its own (it calls neither `egui`
//! nor `sue-wm`) and depends on no studio. Its messages stay in the
//! language database's `paint` namespace, where they were before the
//! engine left Sue Paint.

/// Text from the language database's `paint` namespace (Sue Paint).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("paint", $($arg)*) };
}

/// `text`, kept in English in a table or in data (a tool's default name),
/// in the language in use; a name someone typed stays theirs.
pub(crate) fn localize(text: &str) -> String {
    sue_lang::translate("paint", text)
}

pub mod anim;
pub mod blend;
pub mod brush;
pub mod camera;
pub mod color;
pub mod colorize;
pub mod composite;
pub mod comps;
pub mod correct;
pub mod decoration;
pub mod doc;
pub mod effects;
pub mod figure;
pub mod model3d;
pub mod fill;
pub mod filters;
pub mod gradient;
pub mod gradient_map;
pub mod manga;
pub mod print;
pub mod proof;
pub mod psd_io;
pub mod quick;
pub mod rulers;
pub mod select;
pub mod smart;
pub mod story;
pub mod templates;
pub mod three;
pub mod timelapse;
pub mod tools;
pub mod transform;
pub mod vector;
pub mod warp;
