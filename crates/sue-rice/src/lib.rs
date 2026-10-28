//! # sue-rice: the introspective theme and settings engine
//!
//! Parses `.suerice` files (a TOML subset) and turns every key into a typed
//! [`RiceParameter`], inferring sliders, toggles, color pickers, dropdowns,
//! keybind recorders and shader pickers from the values and from inline
//! metadata comments such as `# [0..64]`. [`SettingsTree::build`] produces the
//! Settings GUI descriptors, edits flow back through [`SetRiceValue`] (an
//! undoable transaction), and [`RiceSchema::to_suerice`] writes the file back
//! with every untouched byte preserved.
//!
//! [`HardwareProfile`] lives here too, so the frame and shader budget is itself
//! riceable. [`ThemePreset`]s apply a whole palette as one undoable step, and
//! [`migrate`] brings files written by older releases up to [`RICE_FORMAT`].
//!
//! Specification: `HUMANZ.md` §4.

mod error;
mod gui;
mod migrate;
mod parameter;
mod parser;
mod preset;
mod profile;
mod schema;
mod value;

pub use error::{RiceDiagnostic, RiceError};
pub use gui::{Control, SettingsSection, SettingsTree, SettingsWidget};
pub use migrate::{Migration, RICE_FORMAT, format_of, migrate};
pub use parameter::{RiceDeclaration, RiceParameter};
pub use preset::{ThemePreset, builtin_presets};
pub use profile::{FrameBudget, HardwareProfile};
pub use schema::{RegisterOutcome, RiceEntry, RiceSchema, SetRiceValue, SetRiceValues};
pub use value::{Keybind, Rgba, RiceValue};

/// The rice Sue ships with. Its keys match the declarations of the engine
/// crates (tested in `sue-wm`).
pub const DEFAULT_RICE: &str = include_str!("../rice/default.suerice");

/// File extensions recognised as shader sources.
pub const SHADER_EXTENSIONS: &[&str] = &[
    ".wgsl", ".glsl", ".hlsl", ".spv", ".metal", ".frag", ".vert", ".comp",
];
