//! # Replay: a stroke kept, and painted again to the same bytes
//!
//! A stroke that may be painted again (a line on a vector layer, a
//! timelapse, a stroke made again after its stabilization changed) is
//! kept as a [`StoredStroke`]: the engine version that painted it, the
//! hash of the brush it was drawn with, the seed of its chances, how the
//! device was read, its colors, the pen's raw samples in layer space
//! with their device time, and the copies a symmetry ruler made. Raw
//! samples, not smoothed ones, so the stabilizer can be changed after
//! drawing. The format is in `docs/stored-stroke-format.md`.
//!
//! [`paint`] paints a kept stroke again. It gives the same bytes as the
//! stroke drawn live, on every thread count and every platform, as long
//! as the live stroke was driven the way replay drives it:
//!
//! 1. Every device sample goes through a [`Recorder`] first, and the
//!    live pipeline takes what the recorder hands back (layer space,
//!    time counted from the pen's landing in the precision it is kept
//!    in), with the recorder's [`Recorder::settings`].
//! 2. After each sample: the points the pipeline newly committed go to
//!    [`crate::paint::PaintStroke::commit`], and, while the pen is down,
//!    [`crate::paint::PaintStroke::hold`] is told that sample's time. To
//!    spray while the pen stands still and the device is silent, the
//!    worker hands the recorder the pen's last sample again with the
//!    time now ([`Recorder::still`]); it is kept like any other.
//! 3. The tail is drawn for show only: it never reaches the layer.
//! 4. On the lift, [`crate::paint::PaintStroke::finish`] and the merge.
//!
//! Nothing a pixel depends on reads a clock, a frame, the zoom of the
//! moment (the zoom when drawn is kept), the thread count or the
//! system's math library ([`crate::math`]). Chance is a counter hash of
//! the kept seed and a dab's number, never of where the stroke lies, so
//! a stroke moved elsewhere keeps its look. The test `tests/replay.rs`
//! paints a fixed set of strokes and compares the hash of their bytes
//! with the one committed for [`ENGINE`] in `tests/golden/`.
//!
//! **Engine versions.** A stored stroke names the engine version that
//! painted it, and is painted again only by that version's code. A
//! change to anything that reaches a pixel changes the committed hash;
//! until the 0.0.8 release the brush steward may record it again after
//! a look at the stroke sheets, and from the release on such a change
//! needs a new [`ENGINE`] number, with the old one kept for the strokes
//! that name it.

mod paint;
mod record;
mod stroke;

pub use paint::{ReplayError, paint, painted};
pub(crate) use paint::{strongest, through};
pub use record::Recorder;
pub use stroke::{FORMAT, StoredStroke};

/// The version of the engine this build paints with. Version 0 is the
/// legacy engine of Sue 0.0.75 and before (which lives in the picture
/// engine, not here); 1 is this one.
pub const ENGINE: u32 = 1;
