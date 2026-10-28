use std::fmt;

/// A 64-bit, generation-indexed reference to an entity in the [`Arena`](crate::Arena).
///
/// The high 32 bits hold the slot generation and the low 32 bits hold the slot
/// index. Generation `0` is never live, so [`AssetHandle::NULL`] (and any handle
/// with generation `0`) never resolves.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct AssetHandle(u64);

impl AssetHandle {
    /// A handle that never resolves to an entity.
    pub const NULL: Self = Self(0);

    /// Packs a slot index and a generation into a handle.
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | index as u64)
    }

    /// The slot index (low 32 bits).
    #[must_use]
    pub const fn index(self) -> u32 {
        // Truncation keeps exactly the low 32 bits.
        self.0 as u32
    }

    /// The slot generation (high 32 bits).
    #[must_use]
    pub const fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// The raw 64-bit representation, for FFI, serialization or GPU buffers.
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }

    /// Rebuilds a handle from [`AssetHandle::to_bits`].
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// `true` for handles that can never be live (generation `0`).
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.generation() == 0
    }
}

impl fmt::Debug for AssetHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AssetHandle({self})")
    }
}

impl fmt::Display for AssetHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index(), self.generation())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_index_low_and_generation_high() {
        let handle = AssetHandle::new(7, 3);
        assert_eq!(handle.to_bits(), (3_u64 << 32) | 7);
        assert_eq!(handle.index(), 7);
        assert_eq!(handle.generation(), 3);
        assert_eq!(AssetHandle::from_bits(handle.to_bits()), handle);
    }

    #[test]
    fn extremes_round_trip() {
        let handle = AssetHandle::new(u32::MAX, u32::MAX);
        assert_eq!(handle.index(), u32::MAX);
        assert_eq!(handle.generation(), u32::MAX);
    }

    #[test]
    fn null_and_generation_zero_are_null() {
        assert!(AssetHandle::NULL.is_null());
        assert!(AssetHandle::new(5, 0).is_null());
        assert!(!AssetHandle::new(0, 1).is_null());
    }

    #[test]
    fn display_is_compact() {
        assert_eq!(AssetHandle::new(12, 2).to_string(), "#12v2");
        assert_eq!(format!("{:?}", AssetHandle::new(1, 1)), "AssetHandle(#1v1)");
    }
}
