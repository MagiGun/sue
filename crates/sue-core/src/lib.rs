//! # sue-core: the substrate
//!
//! The universal data kernel of Sue v1.0 "Faustian Enigma". All engine data
//! lives in one generational [`Arena`] of entities addressed by 64-bit
//! [`AssetHandle`]s. Entities are made of seven typed buffer components
//! ([`ComponentData`]), are viewed through non-destructive [`Projection`]s, and
//! are mutated only through [`ReversibleTransaction`]s recorded in the
//! delta-tree [`UndoStack`].
//!
//! The full specification is in `HUMANZ.md` §1.

mod arena;
mod component;
mod error;
mod handle;
mod projection;
pub mod txn;
mod undo;

pub use arena::{Arena, Entity};
pub use component::{
    ASTNode, ComponentData, ComponentKind, FLICKS_PER_SECOND, MeshBuffer3D, PCMFloat1D,
    PixelFormat, PixelStorage, RasterTile2D, StringBuffer, TimeSpan, VectorSpline2D,
};
pub use error::CoreError;
pub use handle::AssetHandle;
pub use projection::{Projection, ProjectionMask};
pub use undo::{NodeId, ReversibleTransaction, TxnError, UndoStack};

/// Engine version, taken from the workspace manifest.
pub const SUE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Release codename.
pub const SUE_CODENAME: &str = "Faustian Enigma";

/// The engine motto.
pub const SUE_MOTTO: &str =
    "A jack of all trades is a master of none, but oftentimes better than a master of one.";
