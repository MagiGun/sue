//! The brushes of the picture studios.
//!
//! Every new stroke is painted by the brush engine (the `sue-brush`
//! crate); [`engine`] reads a sub tool's tip as the brush it paints with.
//! The 0.0.75 engine stays in [`legacy`] for one release, for two things
//! only: the [`Tip`] a sub tool keeps its settings in (what the Tool
//! Property palette edits), and painting the lines of old vector layers
//! again to the pixels they were saved with. It paints no new stroke.

pub mod engine;
pub mod legacy;

pub use legacy::{
    AntiAlias, Copy2D, Dynamics, Finish, Jitter, SAME, Sample, Stabilizer, Stroke, TAP_SECONDS,
    TILE, TILT_TURN_LEAST, TaperMode, Tip, TipShape, copied, register_shape, shape, shapes,
    symmetry, tilt_turn,
};
