//! Data of a foreign file format that Sue does not map, kept on the
//! entity it was imported with so an export to that format can write it
//! back (Rule 3 applied to a format that is not Sue's).
//!
//! A [`Foreign`] holds what the format's converter could not turn into
//! Sue's own model, encoded by that converter (`data`), and a
//! [`Fingerprint`] of what the entity looked like in Sue's terms right
//! after the import (`fingerprint`). On export the converter takes the
//! fingerprint of the entity as it is now: the same fingerprint means the
//! entity was not changed in Sue, and the kept data goes back into the
//! file as it was; a different one means it was, and the converter writes
//! the entity from Sue's model and leaves the kept data out, for that
//! entity only.
//!
//! The fingerprint is a hash of content, never an arena revision:
//! revisions are not saved, so only a hash still tells, after a save and
//! a reopen, that nothing changed. `docs/clip-foreign.md` describes the
//! component and the `.clip` converter's data.

use std::fmt;

use sha2::{Digest, Sha256};
use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, Shared};

/// What a foreign format kept on an entity, and what the entity looked
/// like when it was kept.
#[derive(Clone, PartialEq)]
pub struct Foreign {
    /// The format, as its converter names it (`clip`).
    pub format: String,
    /// What part of the file the data is (`layer`, `picture`).
    pub part: String,
    /// The entity's fingerprint right after the import.
    pub fingerprint: [u8; 32],
    /// The kept data, in the converter's own encoding; shared, so copies
    /// of the entity do not copy it.
    pub data: Shared<Vec<u8>>,
}

impl Foreign {
    /// Kept `data` of `format` for `part`, taken when the entity had
    /// `fingerprint`.
    pub fn new(
        format: impl Into<String>,
        part: impl Into<String>,
        fingerprint: [u8; 32],
        data: Vec<u8>,
    ) -> Self {
        Self {
            format: format.into(),
            part: part.into(),
            fingerprint,
            data: Shared::new(data),
        }
    }

    /// Whether the entity, whose fingerprint is now `now`, is unchanged
    /// since the import.
    #[must_use]
    pub fn unchanged(&self, now: &[u8; 32]) -> bool {
        &self.fingerprint == now
    }
}

impl fmt::Debug for Foreign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Foreign")
            .field("format", &self.format)
            .field("part", &self.part)
            .field("data", &self.data.len())
            .finish_non_exhaustive()
    }
}

impl ComponentType for Foreign {
    const NAME: &'static str = "clip.foreign";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.str(&self.format);
        out.str(&self.part);
        out.raw(&self.fingerprint);
        out.bytes(&self.data);
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let format = input.str()?;
        let part = input.str()?;
        let mut fingerprint = [0; 32];
        fingerprint.copy_from_slice(input.raw(32)?);
        let data = input.bytes()?.to_vec();
        Ok(Self::new(format, part, fingerprint, data))
    }

    fn heap_bytes(&self) -> usize {
        self.data.len()
    }
}

/// A SHA-256 over what a converter maps of an entity, fed in a fixed
/// order. Every piece is framed (lengths go in first), so two different
/// sequences of pieces never give the same bytes.
#[derive(Clone, Default)]
pub struct Fingerprint(Sha256);

impl Fingerprint {
    /// An empty fingerprint.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a number.
    pub fn word(&mut self, value: u64) {
        self.0.update(value.to_le_bytes());
    }

    /// Adds a signed number.
    #[allow(clippy::cast_sign_loss)]
    pub fn signed(&mut self, value: i64) {
        self.word(value as u64);
    }

    /// Adds a float, by its bits.
    pub fn float(&mut self, value: f64) {
        self.word(value.to_bits());
    }

    /// Adds bytes.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.word(bytes.len() as u64);
        self.0.update(bytes);
    }

    /// Adds text.
    pub fn text(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    /// The hash.
    #[must_use]
    pub fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_data_round_trips_and_shares() {
        let mut print = Fingerprint::new();
        print.text("Layer 1");
        print.word(7);
        let original = Foreign::new("clip", "layer", print.finish(), vec![1, 2, 3, 250]);
        let copy = original.clone();
        assert!(Shared::ptr_eq(&original.data, &copy.data));
        let mut out = Writer::new();
        ComponentType::encode(&original, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Foreign::decode(1, &mut Reader::new(&bytes)).unwrap(),
            original
        );
        // A short payload is an error, not a panic.
        for cut in 0..bytes.len() {
            assert!(Foreign::decode(1, &mut Reader::new(&bytes[..cut])).is_err());
        }
        assert_eq!(
            format!("{original:?}"),
            "Foreign { format: \"clip\", part: \"layer\", data: 4, .. }"
        );
    }

    #[test]
    fn fingerprints_tell_pieces_apart() {
        let of = |pieces: &[&[u8]]| {
            let mut print = Fingerprint::new();
            for piece in pieces {
                print.bytes(piece);
            }
            print.finish()
        };
        assert_eq!(of(&[b"ab", b"c"]), of(&[b"ab", b"c"]));
        assert_ne!(of(&[b"ab", b"c"]), of(&[b"a", b"bc"]));
        let foreign = Foreign::new("clip", "layer", of(&[b"x"]), Vec::new());
        assert!(foreign.unchanged(&of(&[b"x"])));
        assert!(!foreign.unchanged(&of(&[b"y"])));
    }

    #[test]
    fn the_component_survives_a_file_and_a_build_that_does_not_know_it() {
        use sue_core::{Arena, ComponentRegistry, Entity, SueFile};
        let mut arena = Arena::new();
        let kept = Foreign::new("clip", "picture", [9; 32], b"rows and chunks".to_vec());
        let entity = Entity::empty("Canvas", "sue.image").with_ext(kept.clone());
        let id = entity.id();
        arena.insert(entity);
        let mut file = SueFile::new();
        file.set_arena(&arena);
        let bytes = file.to_bytes();
        // Read with the model: the component comes back typed.
        let back = SueFile::from_bytes(&bytes)
            .unwrap()
            .arena(&crate::registry())
            .unwrap();
        let entity = back.get_by_id(id).unwrap();
        assert_eq!(entity.ext::<Foreign>(), Some(&kept));
        // Read blind, saved and read again with the model: unchanged.
        let blind = SueFile::from_bytes(&bytes)
            .unwrap()
            .arena(&ComponentRegistry::new())
            .unwrap();
        let mut again = SueFile::new();
        again.set_arena(&blind);
        let back = SueFile::from_bytes(&again.to_bytes())
            .unwrap()
            .arena(&crate::registry())
            .unwrap();
        assert_eq!(back.get_by_id(id).unwrap().ext::<Foreign>(), Some(&kept));
    }
}
