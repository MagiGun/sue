use std::fmt;

use crate::{ContentHash, UpdatePhase};

/// Package manager errors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PkgError {
    /// A package id that is not `[a-z0-9][a-z0-9._-]*` (at most 128 bytes).
    InvalidId(String),
    /// A version that is not SemVer.
    InvalidVersion(String),
    /// A version requirement that does not parse.
    InvalidRequirement(String),
    /// A content hash that is not 64 hex digits.
    InvalidHex(String),
    /// A payload whose hash differs from its manifest.
    HashMismatch {
        /// Package id.
        id: String,
        /// Hash in the manifest.
        expected: ContentHash,
        /// Hash of the bytes received.
        actual: ContentHash,
    },
    /// A payload whose size differs from its manifest.
    SizeMismatch {
        /// Package id.
        id: String,
        /// Size in the manifest.
        expected: u64,
        /// Size of the bytes received.
        actual: u64,
    },
    /// A package's payload is not in the content store.
    MissingPayload {
        /// Package id.
        id: String,
    },
    /// A public key that is not a valid Ed25519 point.
    InvalidPublicKey,
    /// A manifest signed by a key that is not trusted.
    UntrustedKey(String),
    /// A signature that does not verify.
    BadSignature {
        /// Package id.
        id: String,
    },
    /// The same package version was added twice.
    DuplicateVersion {
        /// Package id.
        id: String,
        /// Version.
        version: String,
    },
    /// No package has this id.
    UnknownPackage(String),
    /// No version satisfies every requirement.
    NoMatchingVersion {
        /// Package id.
        id: String,
        /// All requirements placed on it.
        requirements: Vec<String>,
    },
    /// Packages depend on each other in a cycle.
    DependencyCycle(Vec<String>),
    /// An updater action that is not allowed in the current phase.
    InvalidTransition {
        /// Current phase.
        phase: UpdatePhase,
        /// Rejected action.
        action: &'static str,
    },
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "`{id}` is not a valid package id"),
            Self::InvalidVersion(version) => write!(f, "`{version}` is not a SemVer version"),
            Self::InvalidRequirement(requirement) => {
                write!(f, "`{requirement}` is not a version requirement")
            }
            Self::InvalidHex(text) => write!(f, "`{text}` is not a SHA-256 hex digest"),
            Self::HashMismatch {
                id,
                expected,
                actual,
            } => write!(f, "{id}: payload hash {actual} does not match {expected}"),
            Self::SizeMismatch {
                id,
                expected,
                actual,
            } => write!(f, "{id}: payload is {actual} bytes, expected {expected}"),
            Self::MissingPayload { id } => write!(f, "{id}: payload is not in the content store"),
            Self::InvalidPublicKey => f.write_str("invalid Ed25519 public key"),
            Self::UntrustedKey(key) => write!(f, "signing key `{key}` is not trusted"),
            Self::BadSignature { id } => write!(f, "{id}: signature does not verify"),
            Self::DuplicateVersion { id, version } => write!(f, "{id} {version} was added twice"),
            Self::UnknownPackage(id) => write!(f, "no package named `{id}`"),
            Self::NoMatchingVersion { id, requirements } => write!(
                f,
                "no version of {id} satisfies {}",
                requirements.join(", ")
            ),
            Self::DependencyCycle(ids) => write!(f, "dependency cycle among {}", ids.join(", ")),
            Self::InvalidTransition { phase, action } => {
                write!(f, "cannot {action} while the updater is {phase:?}")
            }
        }
    }
}

impl std::error::Error for PkgError {}
