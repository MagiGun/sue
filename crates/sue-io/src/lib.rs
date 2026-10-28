//! # sue-io: universal sniffer and first-class JPEG XL
//!
//! [`sniff_stream`] routes any byte stream by content alone to one of three
//! tiers: native media decoders, structured AST parsers, or the interactive
//! hex / byte-map fallback. [`ingest_bytes`] turns the routed bytes into an
//! arena entity. [`JxlPipeline`] is the JPEG XL interface; [`NativeJxlPipeline`]
//! decodes with `jxl-oxide`, rebuilds recompressed JPEGs bit for bit, and
//! encodes losslessly with `zune-jpegxl`. [`TileCache`] keeps encoded tiles
//! under a byte budget, and [`byte_map_tile`] turns raw bytes into a zero-copy
//! `Gray8` raster.
//!
//! Specification: `HUMANZ.md` §2.

mod bytemap;
mod ingest;
mod jxl;
mod sniff;
mod tile_cache;

pub use bytemap::{byte_map_tile, shannon_entropy};
pub use ingest::{IngestOptions, IngestView, Ingested, ingest_bytes, meta, route_label};
pub use jxl::{
    DC_PREVIEW_SCALE, DecodedJxl, FourCc, JXL_CODESTREAM_SIGNATURE, JXL_CONTAINER_SIGNATURE,
    JxlError, JxlImageInfo, JxlPipeline, JxlProbe, NativeJxlPipeline, ProgressiveTarget, probe_jxl,
};
pub use sniff::{
    BinarySignature, ByteMapHint, IngestRoute, IngestTier, NativeFormat, StructuredFormat,
    sniff_stream,
};
pub use tile_cache::{TileCache, TileCacheStats, TileKey};
