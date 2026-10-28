//! # sue-pkg: atomic updater and package manager
//!
//! Packages ([`PackageManifest`]: applets, codecs, AI models, rice themes and
//! core updates) carry SHA-256 [`ContentHash`]es of their payloads, are signed
//! with Ed25519 ([`SignedManifest`], [`TrustStore`]), resolved with Cargo's
//! SemVer rules ([`resolve`]) and installed as immutable generations by the
//! [`AtomicUpdater`] state machine, which rolls back automatically when a
//! health check fails.
//!
//! Specification: `HUMANZ.md` §5.

mod error;
mod hash;
mod manifest;
mod resolver;
mod trust;
mod updater;

pub use ed25519_dalek::SigningKey;
pub use error::PkgError;
pub use hash::{ContentHash, ContentStore};
pub use manifest::{Dependency, PackageKind, PackageManifest};
pub use resolver::{PackageIndex, Resolution, resolve};
pub use semver::{Version, VersionReq};
pub use trust::{SignedManifest, TrustStore};
pub use updater::{
    AtomicUpdater, Generation, UpdateOutcome, UpdatePhase, persist_active_pointer,
    read_active_pointer,
};
