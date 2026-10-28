//! The binary codec every backbone encoding uses: little-endian fixed-width
//! numbers, LEB128 variable-length integers, length-prefixed strings and
//! byte runs. It has no dependencies, so the `.sue` format never depends on
//! a third-party serializer's stability.

use std::fmt;

/// A decoding failure, with the byte offset where it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodecError {
    /// What went wrong.
    pub message: String,
    /// Byte offset in the input.
    pub offset: usize,
}

impl CodecError {
    /// An error at `offset`.
    pub fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.offset)
    }
}

impl std::error::Error for CodecError {}

/// Builds an encoding.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// An empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The encoded bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The bytes so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Raw bytes, no length.
    pub fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// One byte.
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// `0` or `1`.
    pub fn bool(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    /// Little-endian `u32`.
    pub fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    /// Little-endian `u64`.
    pub fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    /// Little-endian `f32`.
    pub fn f32(&mut self, value: f32) {
        self.raw(&value.to_le_bytes());
    }

    /// Little-endian `f64`.
    pub fn f64(&mut self, value: f64) {
        self.raw(&value.to_le_bytes());
    }

    /// LEB128 unsigned integer.
    pub fn varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    /// Zigzag LEB128 signed integer.
    pub fn varint_signed(&mut self, value: i64) {
        self.varint(((value << 1) ^ (value >> 63)) as u64);
    }

    /// A length as a varint.
    pub fn len(&mut self, len: usize) {
        self.varint(len as u64);
    }

    /// Length-prefixed bytes.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.raw(bytes);
    }

    /// Length-prefixed UTF-8.
    pub fn str(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }

    /// Length-prefixed `f32` run.
    pub fn f32s(&mut self, values: &[f32]) {
        self.len(values.len());
        self.bytes.reserve(values.len() * 4);
        for value in values {
            self.f32(*value);
        }
    }

    /// Length-prefixed `f64` run.
    pub fn f64s(&mut self, values: &[f64]) {
        self.len(values.len());
        self.bytes.reserve(values.len() * 8);
        for value in values {
            self.f64(*value);
        }
    }

    /// A nested encoding, length-prefixed, so readers can skip it.
    pub fn nested(&mut self, write: impl FnOnce(&mut Self)) {
        let mut inner = Self::new();
        write(&mut inner);
        self.bytes(&inner.bytes);
    }
}

/// Reads an encoding.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Reads `bytes` from the start.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// Current offset.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.at
    }

    /// Bytes left.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    /// `true` when everything was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// An error at the current offset.
    #[must_use]
    pub fn error(&self, message: impl Into<String>) -> CodecError {
        CodecError::new(message, self.at)
    }

    /// `count` raw bytes.
    ///
    /// # Errors
    /// If the input ends early.
    pub fn raw(&mut self, count: usize) -> Result<&'a [u8], CodecError> {
        if count > self.remaining() {
            return Err(self.error(format!("needs {count} bytes, {} left", self.remaining())));
        }
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let mut out = [0; N];
        out.copy_from_slice(self.raw(N)?);
        Ok(out)
    }

    /// One byte.
    ///
    /// # Errors
    /// If the input ends.
    pub fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.raw(1)?[0])
    }

    /// A boolean.
    ///
    /// # Errors
    /// If the input ends or the byte is not 0 or 1.
    pub fn bool(&mut self) -> Result<bool, CodecError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(self.error(format!("{other} is not a boolean"))),
        }
    }

    /// Little-endian `u32`.
    ///
    /// # Errors
    /// If the input ends.
    pub fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Little-endian `u64`.
    ///
    /// # Errors
    /// If the input ends.
    pub fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// Little-endian `f32`.
    ///
    /// # Errors
    /// If the input ends.
    pub fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// Little-endian `f64`.
    ///
    /// # Errors
    /// If the input ends.
    pub fn f64(&mut self) -> Result<f64, CodecError> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// LEB128 unsigned integer.
    ///
    /// # Errors
    /// If the input ends or the number overflows 64 bits.
    pub fn varint(&mut self) -> Result<u64, CodecError> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(self.error("varint overflows 64 bits"))
    }

    /// Zigzag LEB128 signed integer.
    ///
    /// # Errors
    /// As [`Self::varint`].
    pub fn varint_signed(&mut self) -> Result<i64, CodecError> {
        let raw = self.varint()?;
        Ok(((raw >> 1) as i64) ^ -((raw & 1) as i64))
    }

    /// A length, checked against what is left (`unit` bytes per item).
    ///
    /// # Errors
    /// If the length cannot fit in the input.
    pub fn len(&mut self, unit: usize) -> Result<usize, CodecError> {
        let len = usize::try_from(self.varint()?).map_err(|_| self.error("length overflows"))?;
        if len.saturating_mul(unit.max(1)) > self.remaining() {
            return Err(self.error(format!("length {len} exceeds the input")));
        }
        Ok(len)
    }

    /// Length-prefixed bytes.
    ///
    /// # Errors
    /// If the input ends early.
    pub fn bytes(&mut self) -> Result<&'a [u8], CodecError> {
        let len = self.len(1)?;
        self.raw(len)
    }

    /// Length-prefixed UTF-8.
    ///
    /// # Errors
    /// If the input ends early or is not UTF-8.
    pub fn str(&mut self) -> Result<String, CodecError> {
        let at = self.at;
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CodecError::new("text is not UTF-8", at))
    }

    /// Length-prefixed `f32` run.
    ///
    /// # Errors
    /// If the input ends early.
    pub fn f32s(&mut self) -> Result<Vec<f32>, CodecError> {
        let len = self.len(4)?;
        (0..len).map(|_| self.f32()).collect()
    }

    /// Length-prefixed `f64` run.
    ///
    /// # Errors
    /// If the input ends early.
    pub fn f64s(&mut self) -> Result<Vec<f64>, CodecError> {
        let len = self.len(8)?;
        (0..len).map(|_| self.f64()).collect()
    }

    /// A nested encoding written by [`Writer::nested`], as its own reader.
    ///
    /// # Errors
    /// If the input ends early.
    pub fn nested(&mut self) -> Result<Reader<'a>, CodecError> {
        Ok(Reader::new(self.bytes()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_round_trips() {
        let mut out = Writer::new();
        out.u8(7);
        out.bool(true);
        out.u32(0xDEAD_BEEF);
        out.u64(u64::MAX);
        out.f32(1.5);
        out.f64(-2.25);
        out.varint(300);
        out.varint_signed(-12345);
        out.str("héllo");
        out.f32s(&[1.0, 2.0]);
        out.f64s(&[3.0]);
        out.nested(|inner| inner.str("inside"));
        let bytes = out.into_bytes();
        let mut input = Reader::new(&bytes);
        assert_eq!(input.u8().unwrap(), 7);
        assert!(input.bool().unwrap());
        assert_eq!(input.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(input.u64().unwrap(), u64::MAX);
        assert_eq!(input.f32().unwrap(), 1.5);
        assert_eq!(input.f64().unwrap(), -2.25);
        assert_eq!(input.varint().unwrap(), 300);
        assert_eq!(input.varint_signed().unwrap(), -12345);
        assert_eq!(input.str().unwrap(), "héllo");
        assert_eq!(input.f32s().unwrap(), [1.0, 2.0]);
        assert_eq!(input.f64s().unwrap(), [3.0]);
        assert_eq!(input.nested().unwrap().str().unwrap(), "inside");
        assert!(input.is_empty());
    }

    #[test]
    fn truncated_and_hostile_input_is_an_error_not_a_panic() {
        let mut input = Reader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
        assert!(input.bytes().is_err(), "a huge length must not allocate");
        let mut input = Reader::new(&[0x80; 11]);
        assert!(input.varint().is_err());
        let mut input = Reader::new(&[2]);
        assert!(input.bool().is_err());
    }
}
