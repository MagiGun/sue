//! # sue-io: universal sniffer and first-class JPEG XL
//!
//! [`sniff_stream`] routes any byte stream by content alone to one of three
//! tiers: native media decoders, structured AST parsers, or the interactive
//! hex / byte-map fallback. [`JxlPipeline`] is the codec-facing JPEG XL trait;
//! [`probe_jxl`] parses containers and codestream headers natively.
//! [`TileCache`] keeps encoded tiles under a byte budget, and
//! [`byte_map_tile`] turns raw bytes into a zero-copy `Gray8` raster.
//!
//! Specification: `HUMANZ.md` §2.

mod bytemap;
mod jxl;
mod sniff;
mod tile_cache;

pub use bytemap::{byte_map_tile, shannon_entropy};
pub use jxl::{
    ContainerOnlyPipeline, FourCc, JXL_CODESTREAM_SIGNATURE, JXL_CONTAINER_SIGNATURE, JxlError,
    JxlPipeline, JxlProbe, ProgressiveTarget, probe_jxl,
};
pub use sniff::{
    BinarySignature, ByteMapHint, IngestRoute, IngestTier, NativeFormat, StructuredFormat,
    sniff_stream,
};
pub use tile_cache::{TileCache, TileCacheStats, TileKey};
