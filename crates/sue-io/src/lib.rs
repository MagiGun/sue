//! # sue-io: universal sniffer and first-class JPEG XL
//!
//! [`sniff_stream`] routes any byte stream by content alone to one of three
//! tiers: native media decoders, structured AST parsers, or the interactive
//! hex / byte-map fallback. [`ingest_bytes`] turns the routed bytes into an
//! arena entity. [`JxlPipeline`] is the JPEG XL interface; [`NativeJxlPipeline`]
//! decodes with `jxl-oxide`, rebuilds recompressed JPEGs bit for bit, and
//! encodes losslessly with `zune-jpegxl`. PNG and JPEG decode with `zune`,
//! audio (MP3, FLAC, WAV, Ogg Vorbis, AAC) with `symphonia`; [`encode_png`]
//! and [`encode_wav`] write the converter's other targets. [`TileCache`] keeps
//! encoded tiles under a byte budget, and [`byte_map_tile`] turns raw bytes
//! into a zero-copy `Gray8` raster.
//!
//! [`formats`] puts all of it behind the kernel's one `FormatRegistry`:
//! each format imports to entities of a well-known class and exports back,
//! and formats Sue cannot decode yet are kept byte for byte.
//!
//! Specification: `HUMANZ.md` §2.

mod audio;
mod bytemap;
mod formats;
mod ingest;
mod jxl;
pub mod pdf;
mod pictures;
pub mod publish;
mod raster;
mod sniff;
mod tile_cache;
pub mod zip;

pub use audio::{
    AudioInfo, DecodedAudio, SampleDepth, decode_audio, encode_aiff, encode_flac, encode_wav,
    encode_wav_as, probe_audio,
};
pub use bytemap::{byte_map_tile, shannon_entropy};
pub use formats::{
    CompressedAudioFormat, DelimitedFormat, JpegFormat, JxlFormat, KeptFormat, ObjFormat,
    PngFormat, PublishFormat, SueFormat, TextFormat, WavFormat, formats,
};
pub use ingest::{IngestOptions, IngestView, Ingested, ingest_bytes, meta, route_label};
pub use jxl::{
    DC_PREVIEW_SCALE, DecodedJxl, FourCc, JXL_CODESTREAM_SIGNATURE, JXL_CONTAINER_SIGNATURE,
    JxlError, JxlImageInfo, JxlPipeline, JxlProbe, NativeJxlPipeline, ProgressiveTarget, probe_jxl,
};
pub use pictures::{
    PICTURE_SAVE_TYPES, PictureFormat, decode_picture, encode_as, encode_gif_animation,
    encode_picture,
};
pub use raster::{
    DecodedRaster, RasterInfo, decode_jpeg, decode_png, encode_jpeg, encode_png, encode_png_rgba,
};
pub use sniff::{
    BinarySignature, ByteMapHint, IngestRoute, IngestTier, NativeFormat, StructuredFormat,
    sniff_stream,
};
pub use tile_cache::{TileCache, TileCacheStats, TileKey};

/// One codec compiled into this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodecInfo {
    /// Format family, e.g. `JPEG XL`.
    pub format: &'static str,
    /// What Sue can do with it.
    pub abilities: &'static str,
    /// The library that does it.
    pub library: &'static str,
}

/// Every codec built into Sue, for the Packages page.
#[must_use]
pub fn builtin_codecs() -> Vec<CodecInfo> {
    let codec = |format, abilities, library| CodecInfo {
        format,
        abilities,
        library,
    };
    vec![
        codec("JPEG XL", "decode, JPEG reconstruction", "jxl-oxide 0.12"),
        codec("JPEG XL", "lossless encode", "zune-jpegxl 0.5"),
        codec("PNG", "decode, encode", "zune-png 0.5"),
        codec("JPEG", "decode", "zune-jpeg 0.5"),
        codec("MP3, FLAC, WAV, Ogg Vorbis, AAC", "decode", "symphonia 0.5"),
        codec("WAV", "16-bit PCM encode", "built in"),
        codec("Anything else", "hex and byte map", "built in"),
    ]
}
