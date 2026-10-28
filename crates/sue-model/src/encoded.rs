//! Encoded files kept as they are. When Sue opens a format it cannot decode
//! yet (a video, a PDF, a font, a MIDI file), or when decoding would lose
//! something (a camera RAW, a layered PSD), the original bytes ride along
//! on the entity as [`Encoded`]. Nothing is lost: saving writes them back
//! unchanged, exporting to the same format passes them through, and a
//! decoder added later reads them from here.

use std::fmt;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, Shared};

/// The original bytes of a file, and the id of its format.
#[derive(Clone, PartialEq)]
pub struct Encoded {
    /// Format id from the format registry, e.g. `mp4`.
    pub format: String,
    /// The bytes, shared so copies of the entity do not copy them.
    pub bytes: Shared<Vec<u8>>,
}

impl Encoded {
    /// Wraps `bytes` of `format`.
    pub fn new(format: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            format: format.into(),
            bytes: Shared::new(bytes),
        }
    }
}

impl fmt::Debug for Encoded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Encoded")
            .field("format", &self.format)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl ComponentType for Encoded {
    const NAME: &'static str = "sue.encoded";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.str(&self.format);
        out.bytes(&self.bytes);
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self::new(input.str()?, input.bytes()?.to_vec()))
    }

    fn heap_bytes(&self) -> usize {
        self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_bytes_round_trip_and_share() {
        let original = Encoded::new("mp4", vec![0, 0, 0, 24, b'f', b't', b'y', b'p']);
        let copy = original.clone();
        assert!(Shared::ptr_eq(&original.bytes, &copy.bytes));
        let mut out = Writer::new();
        ComponentType::encode(&original, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Encoded::decode(1, &mut Reader::new(&bytes)).unwrap(),
            original
        );
        assert_eq!(
            format!("{original:?}"),
            "Encoded { format: \"mp4\", bytes: 8 }"
        );
    }
}
