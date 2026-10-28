//! Package manifests and their canonical, signable form.

use std::fmt;

use semver::{Version, VersionReq};

use crate::{ContentHash, PkgError};

/// What a package installs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackageKind {
    /// A tool, utility or studio (`SueApplet`), native or WASM.
    Applet,
    /// A file-format codec pack.
    Codec,
    /// An offline AI model.
    AiModel,
    /// A `.suerice` theme with its assets.
    RiceTheme,
    /// An engine update, optionally a binary delta.
    CoreUpdate,
}

impl PackageKind {
    /// The kind's name in canonical manifests.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Applet => "applet",
            Self::Codec => "codec",
            Self::AiModel => "ai-model",
            Self::RiceTheme => "rice-theme",
            Self::CoreUpdate => "core-update",
        }
    }
}

impl fmt::Display for PackageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A dependency on another package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// Package id.
    pub id: String,
    /// Accepted versions, with Cargo's SemVer semantics.
    pub requirement: VersionReq,
}

impl Dependency {
    /// Parses a dependency such as `("sue.codec.jxl", "^1.2")`.
    ///
    /// # Errors
    /// [`PkgError::InvalidId`] or [`PkgError::InvalidRequirement`].
    pub fn parse(id: &str, requirement: &str) -> Result<Self, PkgError> {
        validate_id(id)?;
        let requirement = VersionReq::parse(requirement)
            .map_err(|_| PkgError::InvalidRequirement(requirement.to_owned()))?;
        Ok(Self {
            id: id.to_owned(),
            requirement,
        })
    }
}

/// Checks that `id` is `[a-z0-9][a-z0-9._-]*` and at most 128 bytes.
///
/// # Errors
/// [`PkgError::InvalidId`].
pub(crate) fn validate_id(id: &str) -> Result<(), PkgError> {
    let valid_start = id
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    let valid_body = id.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    });
    if valid_start && valid_body && id.len() <= 128 {
        Ok(())
    } else {
        Err(PkgError::InvalidId(id.to_owned()))
    }
}

/// Describes one version of one package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    /// Reverse-DNS style id, e.g. `sue.codec.jxl`.
    pub id: String,
    /// SemVer version.
    pub version: Version,
    /// What the package installs.
    pub kind: PackageKind,
    /// One-line description.
    pub summary: String,
    /// Hash of the payload.
    pub payload: ContentHash,
    /// Size of the payload in bytes.
    pub payload_size: u64,
    /// Packages this one needs.
    pub dependencies: Vec<Dependency>,
    /// For a binary-delta `CoreUpdate`: the payload the delta applies to.
    pub delta_base: Option<ContentHash>,
}

impl PackageManifest {
    /// A manifest for `payload`, hashing it.
    ///
    /// # Errors
    /// [`PkgError::InvalidId`] or [`PkgError::InvalidVersion`].
    pub fn new(
        id: &str,
        version: &str,
        kind: PackageKind,
        payload: &[u8],
    ) -> Result<Self, PkgError> {
        validate_id(id)?;
        let version =
            Version::parse(version).map_err(|_| PkgError::InvalidVersion(version.to_owned()))?;
        Ok(Self {
            id: id.to_owned(),
            version,
            kind,
            summary: String::new(),
            payload: ContentHash::of(payload),
            payload_size: payload.len() as u64,
            dependencies: Vec::new(),
            delta_base: None,
        })
    }

    /// Sets the summary.
    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    /// Adds a dependency.
    ///
    /// # Errors
    /// As for [`Dependency::parse`].
    pub fn with_dependency(mut self, id: &str, requirement: &str) -> Result<Self, PkgError> {
        self.dependencies.push(Dependency::parse(id, requirement)?);
        Ok(self)
    }

    /// Marks the payload as a binary delta against `base`.
    #[must_use]
    pub fn with_delta_base(mut self, base: ContentHash) -> Self {
        self.delta_base = Some(base);
        self
    }

    /// The deterministic, domain-separated bytes that are signed. Dependencies
    /// are sorted, so their declaration order does not matter.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut dependencies: Vec<String> = self
            .dependencies
            .iter()
            .map(|dependency| format!("dep={} {}\n", dependency.id, dependency.requirement))
            .collect();
        dependencies.sort();
        let delta = self
            .delta_base
            .map_or_else(|| "none".to_owned(), |base| base.to_hex());
        let summary = self
            .summary
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        let mut text = format!(
            "sue-manifest-v1\nid={}\nversion={}\nkind={}\npayload={}\nsize={}\ndelta-base={delta}\n",
            self.id,
            self.version,
            self.kind,
            self.payload.to_hex(),
            self.payload_size,
        );
        text.extend(dependencies);
        text.push_str(&format!("summary={summary}\n"));
        text.into_bytes()
    }

    /// Checks that `bytes` are this manifest's payload.
    ///
    /// # Errors
    /// [`PkgError::SizeMismatch`] or [`PkgError::HashMismatch`].
    pub fn verify_payload(&self, bytes: &[u8]) -> Result<(), PkgError> {
        let actual_size = bytes.len() as u64;
        if actual_size != self.payload_size {
            return Err(PkgError::SizeMismatch {
                id: self.id.clone(),
                expected: self.payload_size,
                actual: actual_size,
            });
        }
        let actual = ContentHash::of(bytes);
        if actual != self.payload {
            return Err(PkgError::HashMismatch {
                id: self.id.clone(),
                expected: self.payload,
                actual,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_validated() {
        assert!(validate_id("sue.codec.jxl").is_ok());
        assert!(validate_id("7zip-codec_v2").is_ok());
        for bad in ["", "Sue", ".hidden", "with space", "über", &"a".repeat(129)] {
            assert!(validate_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn canonical_bytes_ignore_dependency_order() {
        let base =
            PackageManifest::new("sue.studio.daw", "1.2.0", PackageKind::Applet, b"daw").unwrap();
        let a = base
            .clone()
            .with_dependency("sue.codec.flac", "^1")
            .unwrap()
            .with_dependency("sue.codec.mp3", "^2")
            .unwrap();
        let b = base
            .with_dependency("sue.codec.mp3", "^2")
            .unwrap()
            .with_dependency("sue.codec.flac", "^1")
            .unwrap();
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        let text = String::from_utf8(a.canonical_bytes()).unwrap();
        assert!(
            text.starts_with("sue-manifest-v1\nid=sue.studio.daw\nversion=1.2.0\nkind=applet\n")
        );
        assert!(text.contains("dep=sue.codec.flac ^1\ndep=sue.codec.mp3 ^2\n"));
    }

    #[test]
    fn summaries_cannot_forge_fields() {
        let manifest = PackageManifest::new("x", "1.0.0", PackageKind::Codec, b"")
            .unwrap()
            .with_summary("nice\ndep=evil *");
        let text = String::from_utf8(manifest.canonical_bytes()).unwrap();
        assert!(!text.contains("\ndep=evil"));
    }

    #[test]
    fn payloads_are_verified() {
        let manifest = PackageManifest::new(
            "sue.model.whisper",
            "0.3.1",
            PackageKind::AiModel,
            b"weights",
        )
        .unwrap();
        assert_eq!(manifest.verify_payload(b"weights"), Ok(()));
        assert!(matches!(
            manifest.verify_payload(b"weightz"),
            Err(PkgError::HashMismatch { .. })
        ));
        assert!(matches!(
            manifest.verify_payload(b"w"),
            Err(PkgError::SizeMismatch { .. })
        ));
        assert!(PackageManifest::new("x", "one", PackageKind::Codec, b"").is_err());
        assert!(Dependency::parse("x", ">>1").is_err());
    }
}
