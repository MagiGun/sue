//! The universal magic-byte sniffer.

use sue_core::ComponentKind;

use crate::jxl::{JXL_CODESTREAM_SIGNATURE, JXL_CONTAINER_SIGNATURE};
use crate::shannon_entropy;

/// Bytes inspected when deciding whether a stream is text.
const TEXT_WINDOW: usize = 4096;
/// Bytes used for the entropy estimate of unknown binaries.
const ENTROPY_WINDOW: usize = 64 * 1024;

/// The three ingest tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IngestTier {
    /// Tier 1: media with a native decoder.
    NativeMedia = 1,
    /// Tier 2: structured text parsed to an AST.
    StructuredAst = 2,
    /// Tier 3: interactive hex view and byte map.
    ByteMap = 3,
}

/// Formats with a native (Tier 1) decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NativeFormat {
    /// JPEG XL, as a bare codestream or an ISO-BMFF container.
    JpegXl {
        /// `true` for the boxed container form.
        container: bool,
    },
    /// JPEG / JFIF.
    Jpeg,
    /// PNG.
    Png,
    /// GIF.
    Gif,
    /// WebP.
    WebP,
    /// Windows bitmap.
    Bmp,
    /// TIFF.
    Tiff,
    /// Photoshop document.
    Psd,
    /// Windows icon.
    Ico,
    /// AVIF.
    Avif,
    /// HEIF / HEIC.
    Heif,
    /// MPEG audio layer III.
    Mp3,
    /// FLAC.
    Flac,
    /// RIFF WAVE.
    Wav,
    /// Ogg.
    Ogg,
    /// Standard MIDI file.
    Midi,
    /// MPEG-4 audio.
    M4a,
    /// MPEG-4 video.
    Mp4,
    /// QuickTime movie.
    QuickTime,
    /// Matroska / WebM.
    Matroska,
    /// AVI.
    Avi,
    /// Binary glTF.
    Glb,
    /// PDF.
    Pdf,
    /// TrueType, OpenType or WOFF font.
    Font,
}

impl NativeFormat {
    /// The component the decoder produces.
    #[must_use]
    pub const fn target_component(self) -> ComponentKind {
        match self {
            Self::JpegXl { .. }
            | Self::Jpeg
            | Self::Png
            | Self::Gif
            | Self::WebP
            | Self::Bmp
            | Self::Tiff
            | Self::Psd
            | Self::Ico
            | Self::Avif
            | Self::Heif => ComponentKind::RasterTile2D,
            Self::Mp3 | Self::Flac | Self::Wav | Self::Ogg | Self::M4a => ComponentKind::PCMFloat1D,
            Self::Midi | Self::Mp4 | Self::QuickTime | Self::Matroska | Self::Avi => {
                ComponentKind::TimeSpan
            }
            Self::Glb => ComponentKind::MeshBuffer3D,
            Self::Pdf => ComponentKind::StringBuffer,
            Self::Font => ComponentKind::VectorSpline2D,
        }
    }
}

/// Structured text formats (Tier 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StructuredFormat {
    /// JSON.
    Json,
    /// Generic XML.
    Xml,
    /// SVG.
    Svg,
    /// HTML.
    Html,
    /// TOML, including `.suerice`.
    Toml,
    /// Markdown.
    Markdown,
    /// A script with a `#!` interpreter line.
    Script,
    /// Unstructured UTF-8 text.
    PlainText,
}

/// Known binary containers that still go to the byte-map view (Tier 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinarySignature {
    /// ZIP archive (also DOCX, XLSX, EPUB, JAR, …).
    Zip,
    /// gzip stream.
    Gzip,
    /// Zstandard frame.
    Zstd,
    /// 7-Zip archive.
    SevenZip,
    /// xz stream.
    Xz,
    /// ELF executable.
    Elf,
    /// Windows PE executable.
    PortableExecutable,
    /// Mach-O executable.
    MachO,
    /// WebAssembly module.
    Wasm,
    /// SQLite database.
    Sqlite,
}

/// What the byte-map view can tell about an unknown binary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ByteMapHint {
    /// Shannon entropy of the first 64 KiB, in bits per byte.
    pub entropy_bits_per_byte: f32,
    /// A recognised container signature, if any.
    pub signature: Option<BinarySignature>,
}

impl ByteMapHint {
    /// `true` if the data is probably compressed or encrypted.
    #[must_use]
    pub fn likely_compressed(&self) -> bool {
        self.entropy_bits_per_byte > 7.5
    }
}

/// Where an incoming stream goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IngestRoute {
    /// Tier 1: a native decoder.
    Native(NativeFormat),
    /// Tier 2: a structured parser.
    Structured(StructuredFormat),
    /// Tier 3: the hex / byte-map view.
    ByteMap(ByteMapHint),
}

impl IngestRoute {
    /// The route's tier.
    #[must_use]
    pub const fn tier(&self) -> IngestTier {
        match self {
            Self::Native(_) => IngestTier::NativeMedia,
            Self::Structured(_) => IngestTier::StructuredAst,
            Self::ByteMap(_) => IngestTier::ByteMap,
        }
    }

    /// The component the stream lands in.
    #[must_use]
    pub const fn target_component(&self) -> ComponentKind {
        match self {
            Self::Native(format) => format.target_component(),
            Self::Structured(StructuredFormat::Svg) => ComponentKind::VectorSpline2D,
            Self::Structured(StructuredFormat::Markdown | StructuredFormat::PlainText) => {
                ComponentKind::StringBuffer
            }
            Self::Structured(_) => ComponentKind::ASTNode,
            Self::ByteMap(_) => ComponentKind::RasterTile2D,
        }
    }
}

impl NativeFormat {
    /// Display name, e.g. `JPEG XL`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::JpegXl { .. } => "JPEG XL",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Gif => "GIF",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Psd => "PSD",
            Self::Ico => "ICO",
            Self::Avif => "AVIF",
            Self::Heif => "HEIF",
            Self::Mp3 => "MP3",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Ogg => "Ogg",
            Self::Midi => "MIDI",
            Self::M4a => "M4A",
            Self::Mp4 => "MP4",
            Self::QuickTime => "QuickTime",
            Self::Matroska => "Matroska",
            Self::Avi => "AVI",
            Self::Glb => "glTF binary",
            Self::Pdf => "PDF",
            Self::Font => "font",
        }
    }
}

impl StructuredFormat {
    /// Display name, e.g. `TOML`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Xml => "XML",
            Self::Svg => "SVG",
            Self::Html => "HTML",
            Self::Toml => "TOML",
            Self::Markdown => "Markdown",
            Self::Script => "script",
            Self::PlainText => "plain text",
        }
    }
}

impl BinarySignature {
    /// Display name, e.g. `ZIP`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Zip => "ZIP",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
            Self::SevenZip => "7z",
            Self::Xz => "xz",
            Self::Elf => "ELF",
            Self::PortableExecutable => "PE",
            Self::MachO => "Mach-O",
            Self::Wasm => "WASM",
            Self::Sqlite => "SQLite",
        }
    }
}

/// Routes a stream by content alone. Only the first 64 KiB are read.
///
/// ```
/// use sue_io::{sniff_stream, IngestRoute, IngestTier, NativeFormat};
///
/// let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
/// assert_eq!(sniff_stream(png), IngestRoute::Native(NativeFormat::Png));
/// assert_eq!(sniff_stream(b"[layout]\ngap = 8\n").tier(), IngestTier::StructuredAst);
/// ```
#[must_use]
pub fn sniff_stream(bytes: &[u8]) -> IngestRoute {
    if bytes.is_empty() {
        return IngestRoute::Structured(StructuredFormat::PlainText);
    }
    if let Some(format) = sniff_native(bytes) {
        return IngestRoute::Native(format);
    }
    let signature = sniff_binary(bytes);
    if signature.is_none()
        && let Some(format) = sniff_text(bytes)
    {
        return IngestRoute::Structured(format);
    }
    IngestRoute::ByteMap(ByteMapHint {
        entropy_bits_per_byte: shannon_entropy(&bytes[..bytes.len().min(ENTROPY_WINDOW)]),
        signature,
    })
}

fn sniff_native(bytes: &[u8]) -> Option<NativeFormat> {
    let starts = |signature: &[u8]| bytes.starts_with(signature);
    let format = if starts(&JXL_CONTAINER_SIGNATURE) {
        NativeFormat::JpegXl { container: true }
    } else if starts(&JXL_CODESTREAM_SIGNATURE) {
        NativeFormat::JpegXl { container: false }
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        NativeFormat::Jpeg
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        NativeFormat::Png
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        NativeFormat::Gif
    } else if starts(b"8BPS") {
        NativeFormat::Psd
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        NativeFormat::Tiff
    } else if starts(b"fLaC") {
        NativeFormat::Flac
    } else if starts(b"OggS") {
        NativeFormat::Ogg
    } else if starts(b"MThd") {
        NativeFormat::Midi
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        NativeFormat::Matroska
    } else if starts(b"glTF") {
        NativeFormat::Glb
    } else if starts(b"%PDF-") {
        NativeFormat::Pdf
    } else if starts(b"ID3") {
        NativeFormat::Mp3
    } else if starts(b"wOFF") || starts(b"wOF2") || starts(b"OTTO") || starts(&[0, 1, 0, 0]) {
        NativeFormat::Font
    } else if let Some(format) = sniff_riff(bytes).or_else(|| sniff_iso_bmff(bytes)) {
        format
    } else if is_mpeg_audio_frame(bytes) {
        NativeFormat::Mp3
    } else if is_bmp(bytes) {
        NativeFormat::Bmp
    } else if is_ico(bytes) {
        NativeFormat::Ico
    } else {
        return None;
    };
    Some(format)
}

fn sniff_riff(bytes: &[u8]) -> Option<NativeFormat> {
    if !bytes.starts_with(b"RIFF") {
        return None;
    }
    match bytes.get(8..12)? {
        b"WAVE" => Some(NativeFormat::Wav),
        b"WEBP" => Some(NativeFormat::WebP),
        b"AVI " => Some(NativeFormat::Avi),
        _ => None,
    }
}

fn sniff_iso_bmff(bytes: &[u8]) -> Option<NativeFormat> {
    if bytes.get(4..8)? != b"ftyp" {
        return None;
    }
    Some(match bytes.get(8..12)? {
        b"avif" | b"avis" => NativeFormat::Avif,
        b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => NativeFormat::Heif,
        b"qt  " => NativeFormat::QuickTime,
        b"M4A " | b"M4B " => NativeFormat::M4a,
        _ => NativeFormat::Mp4,
    })
}

/// An MPEG audio frame header with valid version, layer, bitrate and rate bits.
fn is_mpeg_audio_frame(bytes: &[u8]) -> bool {
    let [0xFF, second, third, ..] = *bytes else {
        return false;
    };
    let sync = second & 0xE0 == 0xE0;
    let version_ok = (second >> 3) & 0b11 != 0b01;
    let layer_ok = (second >> 1) & 0b11 != 0;
    let bitrate_ok = third >> 4 != 0xF;
    let rate_ok = (third >> 2) & 0b11 != 0b11;
    bytes.len() >= 4 && sync && version_ok && layer_ok && bitrate_ok && rate_ok
}

fn is_bmp(bytes: &[u8]) -> bool {
    bytes.len() >= 26 && bytes.starts_with(b"BM") && bytes[6..10] == [0, 0, 0, 0]
}

fn is_ico(bytes: &[u8]) -> bool {
    bytes.len() >= 6
        && bytes.starts_with(&[0, 0, 1, 0])
        && u16::from_le_bytes([bytes[4], bytes[5]]) > 0
}

fn sniff_binary(bytes: &[u8]) -> Option<BinarySignature> {
    let starts = |signature: &[u8]| bytes.starts_with(signature);
    if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        Some(BinarySignature::Zip)
    } else if starts(&[0x1F, 0x8B]) {
        Some(BinarySignature::Gzip)
    } else if starts(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some(BinarySignature::Zstd)
    } else if starts(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        Some(BinarySignature::SevenZip)
    } else if starts(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
        Some(BinarySignature::Xz)
    } else if starts(b"\x7fELF") {
        Some(BinarySignature::Elf)
    } else if starts(b"\0asm") {
        Some(BinarySignature::Wasm)
    } else if starts(b"SQLite format 3\0") {
        Some(BinarySignature::Sqlite)
    } else if starts(&[0xFE, 0xED, 0xFA, 0xCE])
        || starts(&[0xFE, 0xED, 0xFA, 0xCF])
        || starts(&[0xCE, 0xFA, 0xED, 0xFE])
        || starts(&[0xCF, 0xFA, 0xED, 0xFE])
    {
        Some(BinarySignature::MachO)
    } else if is_portable_executable(bytes) {
        Some(BinarySignature::PortableExecutable)
    } else {
        None
    }
}

/// `MZ` with an `e_lfanew` offset that points at `PE\0\0`.
fn is_portable_executable(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"MZ") {
        return false;
    }
    let Some(&[a, b, c, d]) = bytes.get(0x3C..0x40) else {
        return false;
    };
    let offset = u32::from_le_bytes([a, b, c, d]) as usize;
    bytes
        .get(offset..offset.saturating_add(4))
        .is_some_and(|signature| signature == b"PE\0\0")
}

fn sniff_text(bytes: &[u8]) -> Option<StructuredFormat> {
    let window = &bytes[..bytes.len().min(TEXT_WINDOW)];
    let text = match std::str::from_utf8(window) {
        Ok(text) => text,
        // A code point cut by the window edge is fine; anything else is binary.
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&window[..error.valid_up_to()]).ok()?
        }
        Err(_) => return None,
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\u{c}'))
    {
        return None;
    }
    let trimmed = text.trim_start();
    let head: String = trimmed
        .chars()
        .take(64)
        .collect::<String>()
        .to_ascii_lowercase();
    let first_line = trimmed.lines().next().unwrap_or("");
    Some(if head.starts_with("<?xml") {
        if text.contains("<svg") {
            StructuredFormat::Svg
        } else {
            StructuredFormat::Xml
        }
    } else if head.starts_with("<svg") {
        StructuredFormat::Svg
    } else if head.starts_with("<!doctype html") || head.starts_with("<html") {
        StructuredFormat::Html
    } else if trimmed.starts_with("#!") {
        StructuredFormat::Script
    } else if trimmed.starts_with('{') || (trimmed.starts_with('[') && !is_toml_header(first_line))
    {
        StructuredFormat::Json
    } else {
        classify_lines(trimmed)
    })
}

fn is_bare_key(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b' '))
}

fn is_toml_header(line: &str) -> bool {
    let line = line.trim();
    let Some(rest) = line.strip_prefix('[') else {
        return false;
    };
    let Some((name, after)) = rest.split_once(']') else {
        return false;
    };
    let after = after.trim();
    is_bare_key(name.trim()) && !name.contains(' ') && (after.is_empty() || after.starts_with('#'))
}

fn is_toml_assignment(line: &str) -> bool {
    line.split_once('=').is_some_and(|(key, value)| {
        let key = key.trim();
        is_bare_key(key) && !key.contains(' ') && !value.trim().is_empty()
    })
}

/// Distinguishes TOML (whose comments start with `#`) from Markdown (whose
/// headings do) by the first line that is not blank and not a `#` line.
fn classify_lines(text: &str) -> StructuredFormat {
    let mut saw_hash_line = false;
    for line in text.lines().take(32) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            saw_hash_line = true;
            continue;
        }
        if is_toml_header(line) || is_toml_assignment(line) {
            return StructuredFormat::Toml;
        }
        let markdown_block = ["- ", "* ", "> ", "```", "1. "]
            .iter()
            .any(|marker| line.starts_with(marker));
        return if saw_hash_line || markdown_block {
            StructuredFormat::Markdown
        } else {
            StructuredFormat::PlainText
        };
    }
    if saw_hash_line {
        StructuredFormat::Markdown
    } else {
        StructuredFormat::PlainText
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(bytes: &[u8]) -> Option<NativeFormat> {
        match sniff_stream(bytes) {
            IngestRoute::Native(format) => Some(format),
            _ => None,
        }
    }

    fn structured(bytes: &[u8]) -> Option<StructuredFormat> {
        match sniff_stream(bytes) {
            IngestRoute::Structured(format) => Some(format),
            _ => None,
        }
    }

    #[test]
    fn recognises_native_media() {
        let mut container = JXL_CONTAINER_SIGNATURE.to_vec();
        container.extend_from_slice(b"\0\0\0\x14ftypjxl \0\0\0\0jxl ");
        assert_eq!(
            native(&container),
            Some(NativeFormat::JpegXl { container: true })
        );
        assert_eq!(
            native(&[0xFF, 0x0A, 0x41, 0x00]),
            Some(NativeFormat::JpegXl { container: false })
        );
        assert_eq!(
            native(b"\xFF\xD8\xFF\xE0\0\x10JFIF"),
            Some(NativeFormat::Jpeg)
        );
        assert_eq!(native(b"GIF89a\x01\0\x01\0"), Some(NativeFormat::Gif));
        assert_eq!(native(b"RIFF\x24\0\0\0WAVEfmt "), Some(NativeFormat::Wav));
        assert_eq!(native(b"RIFF\x24\0\0\0WEBPVP8 "), Some(NativeFormat::WebP));
        assert_eq!(
            native(b"\0\0\0\x1cftypavif\0\0\0\0"),
            Some(NativeFormat::Avif)
        );
        assert_eq!(
            native(b"\0\0\0\x18ftypisom\0\0\x02\0"),
            Some(NativeFormat::Mp4)
        );
        assert_eq!(
            native(b"\0\0\0\x18ftypM4A \0\0\0\0"),
            Some(NativeFormat::M4a)
        );
        assert_eq!(native(b"ID3\x04\0\0\0\0\0\0"), Some(NativeFormat::Mp3));
        assert_eq!(native(&[0xFF, 0xFB, 0x90, 0x64]), Some(NativeFormat::Mp3));
        assert_eq!(native(b"fLaC\0\0\0\x22"), Some(NativeFormat::Flac));
        assert_eq!(
            native(&[0x1A, 0x45, 0xDF, 0xA3, 0x9F]),
            Some(NativeFormat::Matroska)
        );
        assert_eq!(native(b"glTF\x02\0\0\0"), Some(NativeFormat::Glb));
        assert_eq!(native(b"%PDF-1.7\n"), Some(NativeFormat::Pdf));
        assert_eq!(native(b"wOF2\0\x01\0\0"), Some(NativeFormat::Font));
        let mut bmp = b"BM\x36\0\0\0\0\0\0\0\x36\0\0\0".to_vec();
        bmp.resize(30, 0);
        assert_eq!(native(&bmp), Some(NativeFormat::Bmp));
    }

    #[test]
    fn weak_signatures_need_plausible_headers() {
        // Reserved MPEG layer bits.
        assert_ne!(native(&[0xFF, 0xF9, 0x50, 0x80]), Some(NativeFormat::Mp3));
        // "BM" text is not a bitmap.
        assert_eq!(
            structured(b"BMW is a car maker, founded 1916."),
            Some(StructuredFormat::PlainText)
        );
        // "MZ" text is not an executable.
        assert_eq!(structured(b"MZ notes"), Some(StructuredFormat::PlainText));
    }

    #[test]
    fn classifies_structured_text() {
        assert_eq!(structured(b"{\"gap\": 8}"), Some(StructuredFormat::Json));
        assert_eq!(structured(b"  [1, 2, 3]"), Some(StructuredFormat::Json));
        assert_eq!(
            structured(b"\xEF\xBB\xBF<?xml version=\"1.0\"?><svg/>"),
            Some(StructuredFormat::Svg)
        );
        assert_eq!(
            structured(b"<?xml version=\"1.0\"?><feed/>"),
            Some(StructuredFormat::Xml)
        );
        assert_eq!(
            structured(b"<!DOCTYPE html><html></html>"),
            Some(StructuredFormat::Html)
        );
        assert_eq!(
            structured(b"#!/bin/sh\necho hi\n"),
            Some(StructuredFormat::Script)
        );
        assert_eq!(
            structured(b"# Sue rice\n\n[layout]\ngap = 8\n"),
            Some(StructuredFormat::Toml)
        );
        assert_eq!(
            structured(b"# Title\n\nSome prose.\n"),
            Some(StructuredFormat::Markdown)
        );
        assert_eq!(
            structured(b"- [ ] buy milk\n"),
            Some(StructuredFormat::Markdown)
        );
        assert_eq!(
            structured(b"Dear diary,\nnothing happened."),
            Some(StructuredFormat::PlainText)
        );
        assert_eq!(structured(b""), Some(StructuredFormat::PlainText));
    }

    #[test]
    fn utf8_cut_at_the_window_edge_is_still_text() {
        let mut text = "a".repeat(TEXT_WINDOW - 1).into_bytes();
        text.extend_from_slice("é and more".as_bytes());
        assert_eq!(structured(&text), Some(StructuredFormat::PlainText));
    }

    #[test]
    fn unknown_binaries_fall_back_to_the_byte_map() {
        let IngestRoute::ByteMap(hint) = sniff_stream(b"PK\x03\x04\x14\0\0\0") else {
            panic!("zip should go to the byte map");
        };
        assert_eq!(hint.signature, Some(BinarySignature::Zip));

        let mut pe = b"MZ".to_vec();
        pe.resize(0x80, 0);
        pe[0x3C] = 0x40;
        pe[0x40..0x44].copy_from_slice(b"PE\0\0");
        assert!(matches!(
            sniff_stream(&pe),
            IngestRoute::ByteMap(ByteMapHint {
                signature: Some(BinarySignature::PortableExecutable),
                ..
            })
        ));

        // A xorshift stream: high entropy, no signature, control bytes.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let noise: Vec<u8> = (0..8192)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect();
        let route = sniff_stream(&noise);
        assert_eq!(route.tier(), IngestTier::ByteMap);
        assert_eq!(route.target_component(), ComponentKind::RasterTile2D);
        let IngestRoute::ByteMap(hint) = route else {
            unreachable!()
        };
        assert!(
            hint.likely_compressed(),
            "entropy {}",
            hint.entropy_bits_per_byte
        );
        assert_eq!(hint.signature, None);
    }

    #[test]
    fn routes_map_to_components() {
        assert_eq!(
            IngestRoute::Native(NativeFormat::Flac).target_component(),
            ComponentKind::PCMFloat1D
        );
        assert_eq!(
            IngestRoute::Native(NativeFormat::Glb).target_component(),
            ComponentKind::MeshBuffer3D
        );
        assert_eq!(
            IngestRoute::Structured(StructuredFormat::Svg).target_component(),
            ComponentKind::VectorSpline2D
        );
        assert_eq!(
            IngestRoute::Structured(StructuredFormat::Toml).target_component(),
            ComponentKind::ASTNode
        );
        assert_eq!(
            IngestRoute::Native(NativeFormat::Png).tier(),
            IngestTier::NativeMedia
        );
    }
}
