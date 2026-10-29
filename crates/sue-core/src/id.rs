//! Stable entity ids. An [`AssetHandle`](crate::AssetHandle) is a fast,
//! runtime-only slot reference; an [`EntityId`] is the entity's name for
//! good: it survives saving and loading, copying between documents, and
//! sending to another device. Everything that links data across features
//! or files links by `EntityId`.

use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

/// A globally unique, stable entity id: a random (version 4) UUID.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EntityId(u128);

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SEED: AtomicU64 = AtomicU64::new(0);

/// Mixes `seed` into every id made from now on. The randomness of ids
/// comes from std's hasher keys, which the operating system fills; a
/// system that gives std none (the browser) calls this once at start-up
/// with random bits of its own, so two sessions never make the same ids.
pub fn seed_entity_ids(seed: u64) {
    SEED.fetch_xor(seed, Ordering::Relaxed);
}

fn random_u64(salt: u64) -> u64 {
    let mut hasher = std::hash::RandomState::new().build_hasher();
    hasher.write_u64(SEED.load(Ordering::Relaxed));
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u64(salt);
    hasher.finish()
}

impl EntityId {
    /// The nil id, never given to an entity.
    pub const NIL: Self = Self(0);

    /// A new random id.
    #[must_use]
    pub fn new() -> Self {
        let high = u128::from(random_u64(0x5E));
        let low = u128::from(random_u64(0xE5));
        let mut bits = (high << 64) | low;
        // RFC 9562 version 4 and variant bits, so the id is a valid UUID.
        bits &= !(0xF_u128 << 76);
        bits |= 0x4_u128 << 76;
        bits &= !(0x3_u128 << 62);
        bits |= 0x2_u128 << 62;
        if bits == 0 { Self(1) } else { Self(bits) }
    }

    /// An id from its 128 bits.
    #[must_use]
    pub const fn from_u128(bits: u128) -> Self {
        Self(bits)
    }

    /// The 128 bits.
    #[must_use]
    pub const fn to_u128(self) -> u128 {
        self.0
    }

    /// Big-endian bytes.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    /// From big-endian bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(u128::from_be_bytes(bytes))
    }

    /// `true` for [`Self::NIL`].
    #[must_use]
    pub const fn is_nil(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityId({self})")
    }
}

impl FromStr for EntityId {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let hex: String = text.chars().filter(|ch| *ch != '-').collect();
        if hex.len() != 32 {
            return Err(format!("`{text}` is not a 32-digit id"));
        }
        u128::from_str_radix(&hex, 16)
            .map(Self)
            .map_err(|_| format!("`{text}` is not hexadecimal"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_are_unique_valid_uuids_that_round_trip_as_text_and_bytes() {
        let ids: BTreeSet<EntityId> = (0..10_000).map(|_| EntityId::new()).collect();
        assert_eq!(ids.len(), 10_000);
        let id = *ids.iter().next().unwrap();
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert_eq!(&text[14..15], "4", "version 4");
        assert!(
            matches!(&text[19..20], "8" | "9" | "a" | "b"),
            "RFC variant"
        );
        assert_eq!(text.parse::<EntityId>().unwrap(), id);
        assert_eq!(EntityId::from_bytes(id.to_bytes()), id);
        assert!(!id.is_nil());
        assert!("xyz".parse::<EntityId>().is_err());
    }

    #[test]
    fn seeded_ids_stay_unique_valid_uuids() {
        let before: BTreeSet<EntityId> = (0..1_000).map(|_| EntityId::new()).collect();
        seed_entity_ids(0x5EED_0F_5E55_1015);
        let after: BTreeSet<EntityId> = (0..1_000).map(|_| EntityId::new()).collect();
        assert_eq!(before.len() + after.len(), 2_000);
        assert!(before.is_disjoint(&after));
        assert!(after.iter().all(|id| id.to_string()[14..15] == *"4"));
    }
}
