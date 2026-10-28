use std::fmt;

use crate::{AssetHandle, ComponentKind, Projection};

/// Errors raised by the kernel: stale handles, projection rules and buffer
/// invariants.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    /// The handle's entity was despawned, or the handle never existed.
    StaleHandle(AssetHandle),
    /// The same handle was passed where two distinct entities are required.
    AliasedHandles(AssetHandle),
    /// The entity has no component of the requested kind.
    MissingComponent {
        /// Entity that was queried.
        handle: AssetHandle,
        /// Component kind that was missing.
        kind: ComponentKind,
    },
    /// A projection was given a component of the wrong kind.
    ProjectionMismatch {
        /// Projection being attached.
        projection: Projection,
        /// Kind of the component that was supplied.
        found: ComponentKind,
    },
    /// The entity already carries this projection.
    AlreadyProjected {
        /// Entity that was targeted.
        handle: AssetHandle,
        /// Projection that is already present.
        projection: Projection,
    },
    /// The entity does not carry this projection.
    NotProjected {
        /// Entity that was targeted.
        handle: AssetHandle,
        /// Projection that is absent.
        projection: Projection,
    },
    /// The base projection owns the entity's data and cannot be detached.
    CannotDetachBase {
        /// Entity that was targeted.
        handle: AssetHandle,
        /// The entity's base projection.
        projection: Projection,
    },
    /// A byte offset lies past the end of a buffer.
    OutOfBounds {
        /// Offending offset.
        offset: usize,
        /// Buffer length.
        len: usize,
    },
    /// A byte offset splits a UTF-8 code point.
    NotCharBoundary {
        /// Offending offset.
        offset: usize,
    },
    /// A component buffer violates its layout invariant.
    InvalidBuffer(&'static str),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleHandle(handle) => write!(f, "stale asset handle {handle}"),
            Self::AliasedHandles(handle) => {
                write!(f, "handle {handle} was passed for two distinct entities")
            }
            Self::MissingComponent { handle, kind } => {
                write!(f, "entity {handle} has no {kind} component")
            }
            Self::ProjectionMismatch { projection, found } => write!(
                f,
                "projection {projection} needs a {} component, got {found}",
                projection.native_kind()
            ),
            Self::AlreadyProjected { handle, projection } => {
                write!(f, "entity {handle} is already projected as {projection}")
            }
            Self::NotProjected { handle, projection } => {
                write!(f, "entity {handle} is not projected as {projection}")
            }
            Self::CannotDetachBase { handle, projection } => {
                write!(
                    f,
                    "cannot detach {projection}: it is the base of entity {handle}"
                )
            }
            Self::OutOfBounds { offset, len } => {
                write!(f, "offset {offset} is out of bounds for length {len}")
            }
            Self::NotCharBoundary { offset } => {
                write!(f, "offset {offset} is not on a UTF-8 character boundary")
            }
            Self::InvalidBuffer(reason) => write!(f, "invalid buffer: {reason}"),
        }
    }
}

impl std::error::Error for CoreError {}
