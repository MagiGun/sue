//! The brush engine's public surface.
//!
//! Today's engine lives whole in [`legacy`]; this module names what the
//! rest of Sue uses of it, so callers keep writing `brush::Tip` or
//! `brush::Stroke` while the new engine (the `sue-brush` crate) grows
//! beside it and takes these names over one by one.

pub mod legacy;

pub use legacy::{
    AntiAlias, Copy2D, Dynamics, Finish, Jitter, SAME, Sample, Stabilizer, Stroke, TAP_SECONDS,
    TILE, TILT_TURN_LEAST, TaperMode, Tip, TipShape, copied, register_shape, shape, shapes,
    symmetry, tilt_turn,
};
