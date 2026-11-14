//! Sue's design system: the tokens every surface is drawn from, the one
//! motion helper and the focus ring.
//!
//! - [`tokens`]: radii, the spacing scale, control sizes per [`Density`],
//!   the type scale, strokes, elevation, the layer order and the color
//!   roles. All of them are read from a [`Theme`](crate::Theme), which
//!   reads them from the rice, so a settings file restyles every one.
//! - [`motion`]: timed transitions and springs that honour
//!   `motion.instant_transitions` and `motion.animation_speed`, and ask
//!   for a new frame only while something moves.
//! - [`focus`]: the focus ring, and a [`FocusScope`] that gives a group of
//!   controls a first focus, keeps Tab inside it and walks it with the
//!   arrow keys.
//!
//! The components built on these (buttons, fields, menus, dialogs) get one
//! file each beside these three.

pub mod focus;
pub mod motion;
pub mod tokens;

pub use focus::{FocusScope, Roving, focus_ring, focus_visible, paint_focus_ring, watch_focus};
pub use motion::{Motion, Pace};
pub use tokens::{
    Density, EDGE_CONTRAST, Elevation, Layer, MIN_TEXT_SIZE, Radius, Space, TEXT_CONTRAST,
    TextRole, contrast, nested_radius,
};
