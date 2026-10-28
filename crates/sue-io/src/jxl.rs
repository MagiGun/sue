//! The JPEG XL pipeline: native container and header probing, plus the
//! codec-facing trait that codec packages implement.

use std::fmt;

use sue_core::RasterTile2D;

/// The 12-byte signature box that starts every JPEG XL container.
pub const JXL_CONTAINER_SIGNATURE: [u8; 12] = [
    0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
];

/// The marker that starts a bare JPEG XL codestream.
pub const JXL_CODESTREAM_SIGNATURE: [u8; 2] = [0xFF, 0x0A];

/// An ISO-BMFF box type.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc(pub [u8; 4]);

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in &self.0 {
            if byte.is_ascii_graphic() || byte == b' ' {
                write!(f, "{}", char::from(byte))?;
            } else {
                write!(f, "\\x{byte:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FourCc(\"{self}\")")
    }
}

/// What a probe learned without decoding any pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JxlProbe {
    /// `true` for the ISO-BMFF container form.
    pub container: bool,
    /// Image width from the codestream `SizeHeader`.
    pub width: Option<u32>,
    /// Image height from the codestream `SizeHeader`.
    pub height: Option<u32>,
    /// Box types in file order (container only).
    pub boxes: Vec<FourCc>,
    /// A `jbrd` box allows bit-exact reconstruction of the original JPEG.
    pub has_jpeg_reconstruction: bool,
    /// Exif metadata is present (plain or Brotli-wrapped).
    pub has_exif: bool,
    /// XMP metadata is present (plain or Brotli-wrapped).
    pub has_xmp: bool,
    /// Conformance level from the `jxll` box.
    pub level: Option<u8>,
    /// Codestream bytes found (`jxlc`, or the sum of `jxlp` parts).
    pub codestream_bytes: usize,
    /// The data ends before the last box or header is complete.
    pub truncated: bool,
}

/// JPEG XL pipeline errors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum JxlError {
    /// Neither a container signature nor a codestream marker.
    NotJxl,
    /// The input is not a JPEG.
    NotJpeg,
    /// A container box is inconsistent.
    MalformedBox {
        /// Byte offset of the box.
        offset: usize,
        /// What is wrong.
        reason: &'static str,
    },
    /// The file has no `jbrd` box, so the original JPEG cannot be rebuilt.
    NoJpegReconstruction,
    /// Nothing to encode.
    NoLayers,
    /// No codec package provides this operation yet.
    CodecUnavailable {
        /// The operation that was requested.
        operation: &'static str,
    },
}

impl fmt::Display for JxlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJxl => f.write_str("not a JPEG XL file"),
            Self::NotJpeg => f.write_str("not a JPEG file"),
            Self::MalformedBox { offset, reason } => {
                write!(f, "malformed box at byte {offset}: {reason}")
            }
            Self::NoJpegReconstruction => f.write_str("no JPEG reconstruction data (jbrd box)"),
            Self::NoLayers => f.write_str("no layers to encode"),
            Self::CodecUnavailable { operation } => write!(
                f,
                "{operation} needs a JPEG XL codec package (install `sue.codec.jxl`)"
            ),
        }
    }
}

impl std::error::Error for JxlError {}

/// How far a progressive decode should go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressiveTarget {
    /// The 1:8 DC image: enough for instant thumbnails.
    DcPreview,
    /// Stop after this many AC passes.
    Pass(u8),
    /// Decode everything.
    Full,
}

/// The codec-facing JPEG XL interface. Codec packages implement it; every
/// image and texture cache in Sue goes through it (LAW 6).
pub trait JxlPipeline: Send + Sync {
    /// Implementation name, for diagnostics.
    fn name(&self) -> &'static str;

    /// Parses container boxes and the codestream header, without decoding.
    ///
    /// # Errors
    /// [`JxlError::NotJxl`] or [`JxlError::MalformedBox`].
    fn probe(&self, bytes: &[u8]) -> Result<JxlProbe, JxlError> {
        probe_jxl(bytes)
    }

    /// Decodes up to `target` into a tile.
    ///
    /// # Errors
    /// Probe errors, or codec errors.
    fn decode_progressive(
        &self,
        bytes: &[u8],
        target: ProgressiveTarget,
    ) -> Result<RasterTile2D, JxlError>;

    /// Recompresses a JPEG losslessly, keeping the data to rebuild it exactly.
    ///
    /// # Errors
    /// [`JxlError::NotJpeg`], or codec errors.
    fn transcode_jpeg_lossless(&self, jpeg: &[u8]) -> Result<Vec<u8>, JxlError>;

    /// Rebuilds the original JPEG, bit for bit, from a recompressed file.
    ///
    /// # Errors
    /// [`JxlError::NoJpegReconstruction`], probe errors, or codec errors.
    fn reconstruct_jpeg(&self, jxl: &[u8]) -> Result<Vec<u8>, JxlError>;

    /// Encodes a layered canvas.
    ///
    /// # Errors
    /// [`JxlError::NoLayers`], or codec errors.
    fn encode_layers(&self, layers: &[&RasterTile2D], effort: u8) -> Result<Vec<u8>, JxlError>;
}

/// The pipeline Sue ships with before a codec package is installed: probing is
/// complete and native; pixel operations report [`JxlError::CodecUnavailable`].
#[derive(Clone, Copy, Debug, Default)]
pub struct ContainerOnlyPipeline;

impl JxlPipeline for ContainerOnlyPipeline {
    fn name(&self) -> &'static str {
        "container-only"
    }

    fn decode_progressive(
        &self,
        bytes: &[u8],
        _target: ProgressiveTarget,
    ) -> Result<RasterTile2D, JxlError> {
        probe_jxl(bytes)?;
        Err(JxlError::CodecUnavailable {
            operation: "progressive decoding",
        })
    }

    fn transcode_jpeg_lossless(&self, jpeg: &[u8]) -> Result<Vec<u8>, JxlError> {
        if !jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Err(JxlError::NotJpeg);
        }
        Err(JxlError::CodecUnavailable {
            operation: "lossless JPEG recompression",
        })
    }

    fn reconstruct_jpeg(&self, jxl: &[u8]) -> Result<Vec<u8>, JxlError> {
        if !probe_jxl(jxl)?.has_jpeg_reconstruction {
            return Err(JxlError::NoJpegReconstruction);
        }
        Err(JxlError::CodecUnavailable {
            operation: "JPEG reconstruction",
        })
    }

    fn encode_layers(&self, layers: &[&RasterTile2D], _effort: u8) -> Result<Vec<u8>, JxlError> {
        if layers.is_empty() {
            return Err(JxlError::NoLayers);
        }
        Err(JxlError::CodecUnavailable {
            operation: "layered encoding",
        })
    }
}

/// Probes a JPEG XL codestream or container without decoding pixels.
///
/// # Errors
/// [`JxlError::NotJxl`] for other data, [`JxlError::MalformedBox`] for an
/// inconsistent container. Data that simply ends early is reported through
/// [`JxlProbe::truncated`] instead, so partial downloads can still be sized.
pub fn probe_jxl(bytes: &[u8]) -> Result<JxlProbe, JxlError> {
    if bytes.starts_with(&JXL_CODESTREAM_SIGNATURE) {
        let size = read_size_header(&bytes[JXL_CODESTREAM_SIGNATURE.len()..]);
        return Ok(JxlProbe {
            width: size.map(|(width, _)| width),
            height: size.map(|(_, height)| height),
            codestream_bytes: bytes.len(),
            truncated: size.is_none(),
            ..JxlProbe::default()
        });
    }
    if !bytes.starts_with(&JXL_CONTAINER_SIGNATURE) {
        return Err(JxlError::NotJxl);
    }

    // The header may straddle `jxlp` parts, so collect the first bytes of the
    // codestream across boxes.
    const HEAD_BYTES: usize = 64;
    let mut probe = JxlProbe {
        container: true,
        ..JxlProbe::default()
    };
    let mut saw_codestream = false;
    let mut head: Vec<u8> = Vec::with_capacity(HEAD_BYTES);
    let mut collect = |part: &[u8], probe: &mut JxlProbe| {
        saw_codestream = true;
        probe.codestream_bytes += part.len();
        let take = (HEAD_BYTES - head.len()).min(part.len());
        head.extend_from_slice(&part[..take]);
    };
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(header) = bytes.get(offset..offset + 8) else {
            probe.truncated = true;
            break;
        };
        let size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let kind = FourCc([header[4], header[5], header[6], header[7]]);
        let (header_len, total) = match size {
            0 => (8, bytes.len() - offset),
            1 => {
                let Some(extended) = bytes.get(offset + 8..offset + 16) else {
                    probe.truncated = true;
                    break;
                };
                let mut wide = [0_u8; 8];
                wide.copy_from_slice(extended);
                let total = usize::try_from(u64::from_be_bytes(wide)).map_err(|_| {
                    JxlError::MalformedBox {
                        offset,
                        reason: "box size exceeds the address space",
                    }
                })?;
                (16, total)
            }
            size => (8, size as usize),
        };
        if total < header_len {
            return Err(JxlError::MalformedBox {
                offset,
                reason: "box is smaller than its header",
            });
        }
        let end = offset.saturating_add(total);
        if end > bytes.len() {
            probe.truncated = true;
        }
        let payload = &bytes[offset + header_len..end.min(bytes.len())];
        probe.boxes.push(kind);
        match &kind.0 {
            b"jxlc" => collect(payload, &mut probe),
            // A partial codestream box starts with a 4-byte sequence index.
            b"jxlp" => collect(payload.get(4..).unwrap_or_default(), &mut probe),
            b"jbrd" => probe.has_jpeg_reconstruction = true,
            b"Exif" => probe.has_exif = true,
            b"xml " => probe.has_xmp = true,
            b"jxll" => probe.level = payload.first().copied(),
            b"brob" => match payload.get(..4) {
                Some(b"Exif") => probe.has_exif = true,
                Some(b"xml ") => probe.has_xmp = true,
                _ => {}
            },
            _ => {}
        }
        offset = end;
    }

    if saw_codestream {
        if head.len() < JXL_CODESTREAM_SIGNATURE.len() {
            probe.truncated = true;
        } else if let Some(header) = head.strip_prefix(&JXL_CODESTREAM_SIGNATURE) {
            match read_size_header(header) {
                Some((width, height)) => {
                    probe.width = Some(width);
                    probe.height = Some(height);
                }
                None => probe.truncated = true,
            }
        } else {
            return Err(JxlError::MalformedBox {
                offset: 0,
                reason: "codestream does not start with FF 0A",
            });
        }
    }
    Ok(probe)
}

/// Reads bits least-significant first, as the JPEG XL bitstream requires.
struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl BitReader<'_> {
    fn read(&mut self, count: u32) -> Option<u64> {
        let mut value = 0_u64;
        for bit in 0..count {
            let byte = *self.bytes.get(self.position / 8)?;
            value |= u64::from((byte >> (self.position % 8)) & 1) << bit;
            self.position += 1;
        }
        Some(value)
    }

    /// `U32(1 + u(9), 1 + u(13), 1 + u(18), 1 + u(30))`, the size distribution.
    fn read_size(&mut self) -> Option<u64> {
        const BITS: [u32; 4] = [9, 13, 18, 30];
        let selector = self.read(2)? as usize;
        Some(1 + self.read(BITS[selector])?)
    }
}

/// Decodes the `SizeHeader` that follows the `FF 0A` marker.
fn read_size_header(bytes: &[u8]) -> Option<(u32, u32)> {
    // xsize = ysize * numerator / denominator, for ratio codes 1..=7.
    const RATIOS: [(u64, u64); 7] = [(1, 1), (12, 10), (4, 3), (3, 2), (16, 9), (5, 4), (2, 1)];
    let mut reader = BitReader { bytes, position: 0 };
    let small = reader.read(1)? == 1;
    let height = if small {
        (reader.read(5)? + 1) * 8
    } else {
        reader.read_size()?
    };
    let ratio = reader.read(3)? as usize;
    let width = match ratio {
        0 if small => (reader.read(5)? + 1) * 8,
        0 => reader.read_size()?,
        code => {
            let (numerator, denominator) = RATIOS[code - 1];
            height * numerator / denominator
        }
    };
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes bits least-significant first, mirroring `BitReader`.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bits: usize,
    }

    impl BitWriter {
        fn write(&mut self, value: u64, count: u32) {
            for bit in 0..count {
                if self.bits.is_multiple_of(8) {
                    self.bytes.push(0);
                }
                let last = self.bytes.len() - 1;
                self.bytes[last] |= u8::from((value >> bit) & 1 == 1) << (self.bits % 8);
                self.bits += 1;
            }
        }

        fn write_size(&mut self, value: u64) {
            let (selector, bits) = match value - 1 {
                v if v < 1 << 9 => (0, 9),
                v if v < 1 << 13 => (1, 13),
                v if v < 1 << 18 => (2, 18),
                _ => (3, 30),
            };
            self.write(selector, 2);
            self.write(value - 1, bits);
        }
    }

    fn codestream(write: impl FnOnce(&mut BitWriter)) -> Vec<u8> {
        let mut writer = BitWriter::default();
        write(&mut writer);
        let mut bytes = JXL_CODESTREAM_SIGNATURE.to_vec();
        bytes.extend(writer.bytes);
        bytes
    }

    fn jxl_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(payload.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn small_square_header_matches_the_spec_bit_layout() {
        // small = 1, ysize_div8_minus_1 = 0, ratio = 1 (1:1): bits 1,00000,100.
        let probe = probe_jxl(&[0xFF, 0x0A, 0x41, 0x00]).unwrap();
        assert_eq!((probe.width, probe.height), (Some(8), Some(8)));
        assert!(!probe.container);
    }

    #[test]
    fn explicit_and_ratio_sizes_decode() {
        let explicit = codestream(|w| {
            w.write(0, 1);
            w.write_size(1000);
            w.write(0, 3);
            w.write_size(1500);
        });
        let probe = probe_jxl(&explicit).unwrap();
        assert_eq!((probe.width, probe.height), (Some(1500), Some(1000)));

        let widescreen = codestream(|w| {
            w.write(0, 1);
            w.write_size(1080);
            w.write(5, 3);
        });
        let probe = probe_jxl(&widescreen).unwrap();
        assert_eq!((probe.width, probe.height), (Some(1920), Some(1080)));

        let huge = codestream(|w| {
            w.write(0, 1);
            w.write_size(1 << 20);
            w.write(7, 3);
        });
        assert_eq!(probe_jxl(&huge).unwrap().width, Some(1 << 21));
    }

    #[test]
    fn containers_report_boxes_and_reconstruction_data() {
        let mut file = JXL_CONTAINER_SIGNATURE.to_vec();
        file.extend(jxl_box(b"ftyp", b"jxl \0\0\0\0jxl "));
        file.extend(jxl_box(b"jxll", &[10]));
        file.extend(jxl_box(b"brob", b"Exif\x1b\x00"));
        file.extend(jxl_box(b"jxlc", &[0xFF, 0x0A, 0x41, 0x00]));
        file.extend(jxl_box(b"jbrd", &[0; 6]));
        let probe = probe_jxl(&file).unwrap();
        assert!(probe.container && probe.has_jpeg_reconstruction && probe.has_exif);
        assert!(!probe.has_xmp && !probe.truncated);
        assert_eq!(probe.level, Some(10));
        assert_eq!((probe.width, probe.height), (Some(8), Some(8)));
        let names: Vec<String> = probe.boxes.iter().map(ToString::to_string).collect();
        assert_eq!(names, ["JXL ", "ftyp", "jxll", "brob", "jxlc", "jbrd"]);
    }

    #[test]
    fn partial_codestreams_and_truncation() {
        let mut file = JXL_CONTAINER_SIGNATURE.to_vec();
        file.extend(jxl_box(b"jxlp", &[0, 0, 0, 0, 0xFF, 0x0A, 0x41]));
        file.extend(jxl_box(b"jxlp", &[0x80, 0, 0, 1, 0x00]));
        let probe = probe_jxl(&file).unwrap();
        assert_eq!(probe.codestream_bytes, 4);
        assert_eq!(probe.height, Some(8));

        let mut cut = JXL_CONTAINER_SIGNATURE.to_vec();
        cut.extend(jxl_box(b"jxlc", &[0xFF, 0x0A, 0x41, 0x00, 1, 2, 3]));
        cut.truncate(cut.len() - 2);
        assert!(probe_jxl(&cut).unwrap().truncated);
    }

    #[test]
    fn malformed_and_foreign_input() {
        assert_eq!(probe_jxl(b"\x89PNG"), Err(JxlError::NotJxl));
        let mut bad = JXL_CONTAINER_SIGNATURE.to_vec();
        bad.extend_from_slice(&[0, 0, 0, 4, b'j', b'x', b'l', b'c']);
        assert!(matches!(
            probe_jxl(&bad),
            Err(JxlError::MalformedBox { offset: 12, .. })
        ));
        let mut not_codestream = JXL_CONTAINER_SIGNATURE.to_vec();
        not_codestream.extend(jxl_box(b"jxlc", &[1, 2, 3]));
        assert!(matches!(
            probe_jxl(&not_codestream),
            Err(JxlError::MalformedBox { .. })
        ));
    }

    #[test]
    fn container_only_pipeline_is_honest_about_missing_codecs() {
        let pipeline = ContainerOnlyPipeline;
        let jxl = [0xFF, 0x0A, 0x41, 0x00];
        assert_eq!(pipeline.probe(&jxl).unwrap().width, Some(8));
        assert!(matches!(
            pipeline.decode_progressive(&jxl, ProgressiveTarget::DcPreview),
            Err(JxlError::CodecUnavailable { .. })
        ));
        assert_eq!(
            pipeline.reconstruct_jpeg(&jxl),
            Err(JxlError::NoJpegReconstruction)
        );
        assert_eq!(
            pipeline.transcode_jpeg_lossless(b"GIF89a"),
            Err(JxlError::NotJpeg)
        );
        assert_eq!(pipeline.encode_layers(&[], 7), Err(JxlError::NoLayers));
        assert_eq!(FourCc(*b"jx\x01c").to_string(), "jx\\x01c");
    }
}
