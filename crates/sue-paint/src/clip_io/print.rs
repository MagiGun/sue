//! Fingerprints of what the `.clip` converter maps of an entity, so an
//! export can tell an entity nobody changed since the import from one
//! that was changed (see `sue_model::foreign`).
//!
//! A layer's fingerprint covers its name, class, every property, its
//! pixels (size, format and bytes) and its mask's; a picture's covers its
//! size. Where a layer sits in the stack and which folder holds it are
//! left out: the export writes the tree from Sue's model, so moving a
//! layer keeps its kept data.

use sue_core::codec::Writer;
use sue_core::{ComponentKind, Entity, RasterTile2D};
use sue_model::foreign::Fingerprint;
use sue_model::picture;

fn raster(print: &mut Fingerprint, raster: &RasterTile2D) {
    print.word(u64::from(raster.width()));
    print.word(u64::from(raster.height()));
    print.text(&format!("{:?}", raster.format()));
    if let Some(bytes) = raster.bytes() {
        print.bytes(bytes);
    } else if let Some(samples) = raster.f32_samples() {
        print.word(samples.len() as u64);
        for sample in samples {
            print.word(u64::from(sample.to_bits()));
        }
    }
}

fn entity(print: &mut Fingerprint, entity: &Entity) {
    print.text(entity.name());
    print.text(entity.class());
    let mut properties = Writer::new();
    entity.properties().encode(&mut properties);
    print.bytes(properties.as_bytes());
    match entity
        .component(ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
    {
        Some(pixels) => {
            print.word(1);
            raster(print, pixels);
        }
        None => print.word(0),
    }
}

/// The fingerprint of layer `layer` with `mask`.
#[must_use]
pub fn layer(layer: &Entity, mask: Option<&Entity>) -> [u8; 32] {
    let mut print = Fingerprint::new();
    print.text("layer");
    entity(&mut print, layer);
    match mask {
        Some(mask) => {
            print.word(1);
            entity(&mut print, mask);
        }
        None => print.word(0),
    }
    print.finish()
}

/// The fingerprint of picture `picture`: its size.
#[must_use]
pub fn picture(picture: &Entity) -> [u8; 32] {
    let mut print = Fingerprint::new();
    print.text("picture");
    let [width, height] = picture::size(picture).unwrap_or_default();
    print.word(u64::from(width));
    print.word(u64::from(height));
    print.finish()
}
