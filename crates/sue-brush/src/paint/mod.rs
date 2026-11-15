//! # Paint behaviours
//!
//! What makes one brush paint differently from another, beyond its tip,
//! size and dynamics: everything Clip Studio Paint's sub tool settings
//! show for a brush, done Sue's way (watched in the tutorials, never
//! copied), every setting stored as data on the [`sue_model::BrushPreset`]
//! ([`Behaviours`], with stored names in `docs/brush-behaviours.md`).
//!
//! | Behaviour | Where |
//! | --- | --- |
//! | Spacing, gap presets, density kept by gap, scatter, the spraying effect (particle size, linked or in pixels, density, deviation, direction), continuous spraying, several tip pictures with a repeat method, flips, thickness and direction (the tip's aspect and angle, which bindings turn with the line, the tilt or the twist) | [`plan`] |
//! | Color jitter, dab by dab and stroke by stroke, and toward the sub color | [`color`] |
//! | Tip pictures, flips and the paper laid dab by dab | [`stamp`] |
//! | Strokes whose dabs differ in color, and how their dabs meet (over or darken) | [`color_buffer`] |
//! | Mixing (amount of paint, density of paint, color stretch, blur) and the blend tools | [`wet`] |
//! | The dual brush, the paper laid on the whole stroke, anti-overflow, the watercolor edge, tone scraping | [`finish`] |
//! | Sharp angles | [`corners`] |
//! | The anti-overflow region | [`overflow`] |
//! | Blend modes, behind, lock transparent pixels, the eraser and the multiple layers eraser | [`lay`] |
//! | All of it together, from stroke points to the layer | [`PaintStroke`] |
//!
//! Build-up is the brush model's `build_up` with [`crate::buffer::Combine`]
//! `BuildUp`; the paper textures themselves are [`crate::texture`].
//!
//! Every chance (scatter, spray, picks, flips, color) is a counter hash of
//! the stroke's seed and the dab's number, and no value that reaches a
//! pixel goes through the platform's trigonometry ([`math`]), so a stroke
//! replays to the same pixels.

pub mod color;
pub mod color_buffer;
pub mod corners;
pub mod finish;
pub mod lay;
pub mod math;
pub mod overflow;
pub mod plan;
mod settings;
pub mod stamp;
mod stroke;
pub mod wet;

pub use settings::{
    Behaviours, DualBrush, DualMode, Flip, PaperChoice, ParticleDirection, Spray, TextureChoice,
    TipBlend, TipOrder, Tone, ToneShape, WatercolorEdge, field, key, register, registry,
};
pub use stroke::{Colors, PaintStroke, Resources};
