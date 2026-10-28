//! SHA-256 content addressing.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};

use crate::PkgError;

/// The SHA-256 digest that names a payload, shown as `sha256:<hex>`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hashes `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Wraps a raw digest.
    #[must_use]
    pub const fn from_bytes(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// The raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, without the `sha256:` prefix.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Parses 64 hex digits, with or without the `sha256:` prefix.
    ///
    /// # Errors
    /// [`PkgError::InvalidHex`].
    pub fn from_hex(text: &str) -> Result<Self, PkgError> {
        let invalid = || PkgError::InvalidHex(text.to_owned());
        let hex = text.strip_prefix("sha256:").unwrap_or(text);
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let mut digest = [0_u8; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte =
                u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| invalid())?;
        }
        Ok(Self(digest))
    }

    /// `true` if `bytes` hash to this digest.
    #[must_use]
    pub fn matches(&self, bytes: &[u8]) -> bool {
        Self::of(bytes) == *self
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

impl FromStr for ContentHash {
    type Err = PkgError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_hex(text)
    }
}

/// An in-memory content-addressed object store. Identical payloads are stored
/// once, however many packages or generations refer to them.
#[derive(Clone, Debug, Default)]
pub struct ContentStore {
    objects: BTreeMap<ContentHash, Vec<u8>>,
}

impl ContentStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `bytes` and returns their hash. Storing the same bytes twice keeps
    /// one copy.
    pub fn put(&mut self, bytes: Vec<u8>) -> ContentHash {
        let hash = ContentHash::of(&bytes);
        self.objects.entry(hash).or_insert(bytes);
        hash
    }

    /// The object named `hash`.
    #[must_use]
    pub fn get(&self, hash: &ContentHash) -> Option<&[u8]> {
        self.objects.get(hash).map(Vec::as_slice)
    }

    /// `true` if the store holds `hash`.
    #[must_use]
    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.objects.contains_key(hash)
    }

    /// Number of distinct objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// `true` if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Total bytes stored.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.objects.values().map(Vec::len).sum()
    }

    /// Replaces an object's bytes without re-hashing, as disk corruption would.
    #[cfg(test)]
    pub(crate) fn corrupt(&mut self, hash: &ContentHash, bytes: Vec<u8>) {
        self.objects.insert(*hash, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_sha256_vectors() {
        assert_eq!(
            ContentHash::of(b"").to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            ContentHash::of(b"abc").to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        let hash = ContentHash::of(b"sue");
        assert_eq!(ContentHash::from_hex(&hash.to_string()), Ok(hash));
        assert_eq!(hash.to_hex().parse::<ContentHash>(), Ok(hash));
        assert!(ContentHash::from_hex("sha256:1234").is_err());
        assert!(ContentHash::from_hex(&"zz".repeat(32)).is_err());
        assert!(hash.matches(b"sue"));
        assert!(!hash.matches(b"Sue"));
        assert_eq!(ContentHash::from_bytes(*hash.as_bytes()), hash);
    }

    #[test]
    fn store_deduplicates() {
        let mut store = ContentStore::new();
        let a = store.put(b"codec".to_vec());
        let b = store.put(b"codec".to_vec());
        assert_eq!(a, b);
        assert_eq!((store.len(), store.total_bytes()), (1, 5));
        assert_eq!(store.get(&a), Some(&b"codec"[..]));
        assert!(!store.contains(&ContentHash::of(b"other")));
        assert!(!store.is_empty());
    }
}
