//! Universal ingest: turns any bytes into an arena entity, routed by content.

use std::time::Instant;

use sue_core::{Arena, AssetHandle, CoreError, StringBuffer};

use crate::{
    IngestRoute, JxlImageInfo, JxlPipeline, NativeFormat, ProgressiveTarget, byte_map_tile,
    shannon_entropy, sniff_stream,
};

/// Metadata keys set on ingested entities.
pub mod meta {
    /// Original size in bytes.
    pub const BYTES: &str = "bytes";
    /// Human-readable route, e.g. `Tier 1 · JPEG XL`.
    pub const ROUTE: &str = "route";
    /// Detected format name.
    pub const FORMAT: &str = "format";
    /// Decode or load time in microseconds.
    pub const MICROS: &str = "micros";
    /// Image width in pixels.
    pub const WIDTH: &str = "width";
    /// Image height in pixels.
    pub const HEIGHT: &str = "height";
    /// Colour model of a decoded image.
    pub const COLOR: &str = "color";
    /// Bits per sample of a decoded image.
    pub const BITS: &str = "bits";
    /// Shannon entropy of raw bytes, in bits per byte.
    pub const ENTROPY: &str = "entropy";
    /// Why the bytes were shown raw.
    pub const NOTE: &str = "note";
}

/// Which kind of view suits the ingested entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IngestView {
    /// A decoded image (`RasterTile2D`, `Rgba8`).
    Raster,
    /// UTF-8 text (`StringBuffer`).
    Text,
    /// Raw bytes as a `Gray8` byte map (`RasterTile2D`), for the hex view.
    Bytes,
}

/// Ingest settings, read from rice by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngestOptions {
    /// Columns of the byte map.
    pub byte_map_width: u32,
}

/// The result of ingesting one stream.
#[derive(Clone, Debug, PartialEq)]
pub struct Ingested {
    /// The new entity.
    pub handle: AssetHandle,
    /// Where the sniffer sent the bytes.
    pub route: IngestRoute,
    /// The view that suits the entity.
    pub view: IngestView,
    /// Facts about a decoded image.
    pub image: Option<JxlImageInfo>,
    /// Why the bytes ended up in a different view than their route suggests.
    pub note: Option<String>,
    /// Time spent decoding or loading, in microseconds.
    pub micros: u64,
}

/// Sniffs `bytes`, decodes them when Sue has a native decoder, and spawns one
/// entity named `name`: an `Rgba8` raster for JPEG XL, a `StringBuffer` for
/// UTF-8 text, and a `Gray8` byte map for everything else. The byte vector is
/// moved into the arena, not copied.
///
/// # Errors
/// Only if the byte map cannot be built (more than `u32::MAX` rows).
pub fn ingest_bytes(
    arena: &mut Arena,
    name: &str,
    bytes: Vec<u8>,
    pipeline: &dyn JxlPipeline,
    options: IngestOptions,
) -> Result<Ingested, CoreError> {
    let started = Instant::now();
    let route = sniff_stream(&bytes);
    let length = bytes.len();
    let label = route_label(&route);
    let elapsed =
        |started: Instant| u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);

    let (bytes, note) = match route {
        IngestRoute::Native(NativeFormat::JpegXl { .. }) => {
            match pipeline.decode(&bytes, ProgressiveTarget::Full) {
                Ok(decoded) => {
                    let micros = elapsed(started);
                    let info = decoded.info;
                    let handle = arena.spawn(name, decoded.tile);
                    let entity = arena.get_mut(handle)?;
                    for (key, value) in [
                        (meta::BYTES, length.to_string()),
                        (meta::ROUTE, label),
                        (meta::FORMAT, "JPEG XL".to_owned()),
                        (meta::MICROS, micros.to_string()),
                        (meta::WIDTH, info.width.to_string()),
                        (meta::HEIGHT, info.height.to_string()),
                        (meta::COLOR, info.color_model.to_owned()),
                        (meta::BITS, info.bits_per_sample.to_string()),
                    ] {
                        entity.set_meta(key, value);
                    }
                    return Ok(Ingested {
                        handle,
                        route,
                        view: IngestView::Raster,
                        image: Some(info),
                        note: None,
                        micros,
                    });
                }
                Err(error) => (bytes, Some(format!("{error}; showing the raw bytes"))),
            }
        }
        IngestRoute::Native(format) => (
            bytes,
            Some(format!(
                "no {} decoder is installed yet; showing the raw bytes",
                format.name()
            )),
        ),
        IngestRoute::Structured(format) => match String::from_utf8(bytes) {
            Ok(text) => {
                let handle = arena.spawn(name, StringBuffer::from(text));
                let micros = elapsed(started);
                let entity = arena.get_mut(handle)?;
                entity.set_meta(meta::BYTES, length.to_string());
                entity.set_meta(meta::ROUTE, label);
                entity.set_meta(meta::FORMAT, format.name());
                entity.set_meta(meta::MICROS, micros.to_string());
                return Ok(Ingested {
                    handle,
                    route,
                    view: IngestView::Text,
                    image: None,
                    note: None,
                    micros,
                });
            }
            Err(error) => (
                error.into_bytes(),
                Some("not valid UTF-8; showing the raw bytes".to_owned()),
            ),
        },
        IngestRoute::ByteMap(_) => (bytes, None),
    };

    let entropy = shannon_entropy(&bytes[..bytes.len().min(64 * 1024)]);
    let tile = byte_map_tile(bytes, options.byte_map_width)?;
    let handle = arena.spawn(name, tile);
    let micros = elapsed(started);
    let entity = arena.get_mut(handle)?;
    entity.set_meta(meta::BYTES, length.to_string());
    entity.set_meta(meta::ROUTE, label);
    entity.set_meta(meta::ENTROPY, format!("{entropy:.2}"));
    entity.set_meta(meta::MICROS, micros.to_string());
    if let Some(note) = &note {
        entity.set_meta(meta::NOTE, note.clone());
    }
    Ok(Ingested {
        handle,
        route,
        view: IngestView::Bytes,
        image: None,
        note,
        micros,
    })
}

/// `Tier 1 · JPEG XL`, `Tier 2 · TOML`, `Tier 3 · byte map (zip)`.
#[must_use]
pub fn route_label(route: &IngestRoute) -> String {
    match route {
        IngestRoute::Native(format) => format!("Tier 1 · {}", format.name()),
        IngestRoute::Structured(format) => format!("Tier 2 · {}", format.name()),
        IngestRoute::ByteMap(hint) => match hint.signature {
            Some(signature) => format!("Tier 3 · byte map ({})", signature.name()),
            None => "Tier 3 · byte map".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NativeJxlPipeline;
    use sue_core::{ComponentKind, PixelFormat, RasterTile2D};

    const OPTIONS: IngestOptions = IngestOptions { byte_map_width: 64 };

    #[test]
    fn jxl_files_become_decoded_rasters() {
        let pipeline = NativeJxlPipeline;
        let tile = RasterTile2D::from_bytes(4, 3, PixelFormat::Rgba8, vec![200; 48]).unwrap();
        let encoded = pipeline.encode_layers(&[&tile], 1).unwrap();
        let mut arena = Arena::new();
        let ingested = ingest_bytes(&mut arena, "card.jxl", encoded, &pipeline, OPTIONS).unwrap();
        assert_eq!(ingested.view, IngestView::Raster);
        let entity = arena.get(ingested.handle).unwrap();
        assert_eq!(entity.base_kind(), ComponentKind::RasterTile2D);
        assert_eq!(entity.meta(meta::WIDTH), Some("4"));
        assert_eq!(entity.meta(meta::FORMAT), Some("JPEG XL"));
        assert_eq!(entity.meta(meta::ROUTE), Some("Tier 1 · JPEG XL"));
    }

    #[test]
    fn text_becomes_a_string_buffer() {
        let mut arena = Arena::new();
        let bytes = b"[layout]\ngap = 8\n".to_vec();
        let ingested =
            ingest_bytes(&mut arena, "my.suerice", bytes, &NativeJxlPipeline, OPTIONS).unwrap();
        assert_eq!(ingested.view, IngestView::Text);
        let entity = arena.get(ingested.handle).unwrap();
        assert_eq!(
            entity.string_buffer().unwrap().as_str(),
            "[layout]\ngap = 8\n"
        );
        assert_eq!(entity.meta(meta::FORMAT), Some("TOML"));
    }

    #[test]
    fn binaries_and_undecodable_media_become_byte_maps() {
        let mut arena = Arena::new();
        let zip = b"PK\x03\x04\x14\0\0\0\x08\0".to_vec();
        let ingested = ingest_bytes(&mut arena, "a.zip", zip, &NativeJxlPipeline, OPTIONS).unwrap();
        assert_eq!(ingested.view, IngestView::Bytes);
        let entity = arena.get(ingested.handle).unwrap();
        assert_eq!(entity.meta(meta::BYTES), Some("10"));
        assert_eq!(entity.meta(meta::ROUTE), Some("Tier 3 · byte map (ZIP)"));

        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        let ingested = ingest_bytes(&mut arena, "a.png", png, &NativeJxlPipeline, OPTIONS).unwrap();
        assert_eq!(ingested.view, IngestView::Bytes);
        assert!(ingested.note.unwrap().contains("no PNG decoder"));

        let broken = vec![0xFF, 0x0A, 0x41, 0x00];
        let ingested =
            ingest_bytes(&mut arena, "cut.jxl", broken, &NativeJxlPipeline, OPTIONS).unwrap();
        assert_eq!(ingested.view, IngestView::Bytes);
        assert!(ingested.note.is_some());
    }
}
