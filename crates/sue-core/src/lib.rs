//! # sue-core: the substrate
//!
//! The universal data kernel of Sue v1.0 "Faustian Enigma", and the backbone
//! every feature builds on. All engine data lives in one generational
//! [`Arena`] of [`Entity`]s, addressed at runtime by 64-bit [`AssetHandle`]s
//! and for good by [`EntityId`]s. An entity has a class, an optional base
//! buffer (one of the seven [`ComponentData`] kinds), projection buffers,
//! extension [`Component`]s registered in a [`ComponentRegistry`],
//! [`Properties`] and metadata. Entities link through typed, ordered
//! [`Relation`]s. Every change is a [`ReversibleTransaction`] recorded in the
//! delta-tree [`UndoStack`].
//!
//! Shared vocabularies keep features compatible: [`Value`] for any setting,
//! [`Quantity`] for measurements, [`Color`] for managed color and [`Time`]
//! for the one clock. Shared machinery keeps them cooperating: one
//! [`FormatRegistry`] for every file format, one [`OperatorRegistry`] for
//! every action, [`Preset`]s for any of them, a [`JobPool`] for background
//! work and the [`SueFile`] container that saves it all.
//!
//! The full specification is in `HUMANZ.md` §1.

mod arena;
pub mod codec;
mod color;
mod component;
mod error;
pub mod file;
mod format;
mod handle;
mod id;
mod jobs;
mod operator;
mod projection;
mod registry;
mod relation;
mod shared;
mod time;
pub mod txn;
mod undo;
mod units;
mod value;

pub use arena::{Arena, Entity};
pub use color::{Color, ColorSpace, Transfer};
pub use component::{
    ASTNode, ComponentData, ComponentKind, FLICKS_PER_SECOND, MeshBuffer3D, PCMFloat1D,
    PixelFormat, PixelStorage, RasterTile2D, StringBuffer, TimeSpan, VectorSpline2D,
};
pub use error::CoreError;
pub use file::{Chunk, SueFile};
pub use format::{Bundle, FormatError, FormatHandler, FormatInfo, FormatRegistry, InsertBundle};
pub use handle::AssetHandle;
pub use id::EntityId;
pub use jobs::{JobContext, JobError, JobHandle, JobPool};
pub use operator::{FnOperator, Operator, OperatorRegistry, Plan, Preset, PresetLibrary};
pub use projection::{Projection, ProjectionMask};
pub use registry::{
    Component, ComponentRegistry, ComponentType, OpaqueComponent, components_equal,
};
pub use relation::{CHILD, OrderKey, REF, Relation, RelationError, RelationStore};
pub use shared::Shared;
pub use time::{Rate, Time, Timecode};
pub use undo::{NodeId, ReversibleTransaction, TxnError, UndoStack};
pub use units::{Dimension, DimensionMismatch, Quantity, UNITS, Unit};
pub use value::{Properties, PropertyDef, PropertySchema, Value, ValueType};

/// Engine version, taken from the workspace manifest.
pub const SUE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Release codename.
pub const SUE_CODENAME: &str = "Faustian Enigma";

/// The engine motto.
pub const SUE_MOTTO: &str =
    "A jack of all trades is a master of none, but oftentimes better than a master of one.";
