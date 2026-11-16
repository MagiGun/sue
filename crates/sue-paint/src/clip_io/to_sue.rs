//! A [`ClipDocument`] into Sue's picture model: the canvas as a picture,
//! each layer and folder as an entity with its style, pixels and mask,
//! and on each the `clip.foreign` component that keeps the layer's row,
//! bitmaps and every column Sue does not map, with the fingerprint of
//! the entity as it was made here (see [`super::print`]).

use sue_core::txn::{Relate, SpawnEntity};
use sue_core::{CHILD, Color, Entity, EntityId, OrderKey, PixelFormat, RasterTile2D, Relation};
use sue_io::clip::{ClipContent, ClipDocument, ClipLayer, ClipLayerPath, Columns, RASTER, column};
use sue_io::sqlite::Value;
use sue_model::picture;
use sue_model::{BlendMode, Foreign, class};

use super::report::{Line, Outcome, Report};
use super::{FORMAT, blend_of, extras, keep, layers, print};
use crate::doc::{self, Edit, MEMBER};

/// The layers made from a document, and where they go.
struct Making<'a> {
    doc: &'a ClipDocument,
    /// The picture they go in.
    picture: EntityId,
    /// The next order key, and the one they stay under.
    low: Option<OrderKey>,
    high: Option<OrderKey>,
    edit: Edit,
    report: Report,
    /// Where each layer made was in the file's tree.
    paths: Vec<(ClipLayerPath, EntityId)>,
    /// The file's timeline comes along, so its animation and camera
    /// folders are made as such.
    timeline: bool,
}

impl Making<'_> {
    fn next_key(&mut self) -> OrderKey {
        let key = match (&self.low, &self.high) {
            (Some(low), Some(high)) if low < high => OrderKey::between(Some(low), Some(high)),
            (low, _) => OrderKey::between(low.as_ref(), None),
        };
        self.low = Some(key.clone());
        key
    }

    /// Spawns `entity` (and its mask) in the stack; answers its id.
    fn spawn(&mut self, entity: Entity, mask: Option<Entity>) -> EntityId {
        let id = entity.id();
        let order = self.next_key();
        let mut spawn = SpawnEntity::new(entity)
            .with_relation(Relation::new(self.picture, CHILD, id).ordered(order));
        let mut edit = std::mem::replace(&mut self.edit, Edit::new(""));
        if let Some(mask) = mask {
            let mask_id = mask.id();
            edit = edit.then(spawn);
            spawn = SpawnEntity::new(mask).with_relation(Relation::new(id, doc::MASK, mask_id));
        }
        self.edit = edit.then(spawn);
        id
    }

    /// The layers `layers` (bottom first); answers their ids.
    fn layers(&mut self, layers: &[ClipLayer], at: &mut Vec<u32>) -> Result<Vec<EntityId>, String> {
        let depth = at.len();
        let mut ids = Vec::with_capacity(layers.len());
        for (index, layer) in (0_u32..).zip(layers) {
            at.push(index);
            let path = ClipLayerPath(at.clone());
            if layer.is_folder() {
                // Clip Studio nests folders far less deep; a file that
                // claims more is broken.
                if depth > 64 {
                    return Err(tr!("clip-folders-too-deep"));
                }
                let members = self.layers(&layer.children, at)?;
                let (entity, mask) = self.folder(layer)?;
                let id = self.spawn(entity, mask);
                let mut edit = std::mem::replace(&mut self.edit, Edit::new(""));
                for member in members {
                    edit = edit.then(Relate::new(Relation::new(id, MEMBER, member)));
                }
                self.edit = edit;
                self.paths.push((path, id));
                ids.push(id);
            } else {
                let (entity, mask) = self.layer(layer)?;
                let id = self.spawn(entity, mask);
                self.paths.push((path, id));
                ids.push(id);
            }
            at.pop();
        }
        Ok(ids)
    }

    /// A folder and its mask.
    fn folder(&mut self, layer: &ClipLayer) -> Result<(Entity, Option<Entity>), String> {
        let mut entity = Entity::empty(layer.name.clone(), class::FOLDER)
            .with_property(doc::OPEN, layer.is_open());
        let mut outcome = Outcome::Exact;
        match blend_of(layer.composite) {
            Some(BlendMode::Through) => {}
            Some(mode) => entity = entity.with_property(picture::BLEND, mode.name()),
            None => {
                entity = entity.with_property(picture::BLEND, BlendMode::Normal.name());
                outcome = Outcome::Approximated;
            }
        }
        entity = styled(entity, layer);
        let entity = if self.timeline {
            extras::folder_in(entity, layer.content.as_ref())
        } else {
            entity
        };
        let (entity, mut lost) = layers::styles_in(entity, layer);
        // Without the file's timeline, an animation or camera folder is
        // an ordinary folder.
        lost |= !self.timeline
            && matches!(
                layer.content,
                Some(ClipContent::AnimationFolder(_) | ClipContent::Camera(_))
            );
        let canvas = [0, 0, i64::from(self.doc.width), i64::from(self.doc.height)];
        let mask = self.mask(layer, canvas)?;
        Ok(self.finish(layer, entity, mask, outcome, lost))
    }

    /// A layer that is not a folder, and its mask.
    fn layer(&mut self, layer: &ClipLayer) -> Result<(Entity, Option<Entity>), String> {
        let (width, height) = (self.doc.width, self.doc.height);
        let mut outcome = Outcome::Exact;
        let mut lost = false;
        let canvas = [0, 0, i64::from(width), i64::from(height)];
        let made = layer
            .content
            .as_ref()
            .filter(|_| !layer.is_paper())
            .and_then(|content| layers::generated_in(&layer.name, content));
        let (mut entity, area) = if let Some((entity, generated_lost)) = made {
            // A fill, a gradient or a correction: drawn by Sue from its
            // settings, the file's pixels being only what it showed.
            lost = generated_lost;
            (entity, canvas)
        } else if layer.is_paper() {
            let [r, g, b] = if layer.draw_color.enabled() {
                layer.draw_color.color()
            } else {
                [255; 3]
            };
            let paper = Entity::empty(layer.name.clone(), class::PAPER)
                .with_property(picture::COLOR, Color::srgb8(r, g, b));
            (paper, [0, 0, i64::from(width), i64::from(height)])
        } else {
            let generated = drawn_by_clip_studio(&layer.columns) && layer.content.is_none();
            let decoded = match &layer.pixels {
                Some(bitmap) if !generated => Some(bitmap.decode()?),
                _ => None,
            };
            let sue_reads = matches!(
                layer.content,
                Some(ClipContent::Text(_) | ClipContent::Vector(_))
            );
            if generated
                || (layer.layer_type != RASTER && !layer.is_paper())
                || (layer.content.is_some() && !sue_reads)
            {
                outcome = Outcome::Kept;
            }
            let [left, top] = layer.pixels_at();
            // At least the canvas, and every pixel the file has beyond it.
            let (x0, y0) = (left.min(0), top.min(0));
            let (x1, y1) = match &decoded {
                Some(pixels) => (
                    (left + i64::from(pixels.width)).max(i64::from(width)),
                    (top + i64::from(pixels.height)).max(i64::from(height)),
                ),
                None => (i64::from(width), i64::from(height)),
            };
            let (w, h) = (
                u32::try_from(x1 - x0).map_err(|_| tr!("clip-layer-too-big"))?,
                u32::try_from(y1 - y0).map_err(|_| tr!("clip-layer-too-big"))?,
            );
            if u64::from(w) * u64::from(h) > 1 << 28 {
                return Err(tr!("clip-layer-too-big"));
            }
            let mut rgba = vec![0_u8; w as usize * h as usize * 4];
            if let Some(pixels) = &decoded {
                if pixels.channels != 4 {
                    // A gray or monochrome layer: Sue paints in color.
                    outcome = outcome.max(Outcome::Approximated);
                }
                let step = usize::from(pixels.channels);
                let (pw, dx, dy) = (pixels.width as usize, left - x0, top - y0);
                for y in 0..pixels.height as usize {
                    let row = (dy as usize + y) * w as usize + dx as usize;
                    for x in 0..pw {
                        let from = (y * pw + x) * step;
                        let to = (row + x) * 4;
                        if step == 4 {
                            rgba[to..to + 4].copy_from_slice(&pixels.data[from..from + 4]);
                        } else {
                            let value = pixels.data[from];
                            rgba[to..to + 4].copy_from_slice(&[value, value, value, 255]);
                        }
                    }
                }
            }
            let raster = RasterTile2D::from_bytes(w, h, PixelFormat::Rgba8, rgba)
                .map_err(|error| error.to_string())?;
            let name = if layer.name.is_empty() {
                tr!("layer-2")
            } else {
                layer.name.clone()
            };
            let mut entity = Entity::new(name, raster).with_class(class::LAYER);
            if x0 != 0 {
                entity = entity.with_property(picture::X, x0);
            }
            if y0 != 0 {
                entity = entity.with_property(picture::Y, y0);
            }
            if let Some(content) = &layer.content {
                let (made, content_lost) =
                    layers::on_pixels_in(entity, content, [x0, y0], self.doc.dpi);
                lost |= content_lost;
                entity = made;
            }
            (entity, [x0, y0, x1, y1])
        };
        match blend_of(layer.composite) {
            Some(BlendMode::Normal | BlendMode::Through) => {}
            Some(mode) => entity = entity.with_property(picture::BLEND, mode.name()),
            None => outcome = outcome.max(Outcome::Approximated),
        }
        entity = styled(entity, layer);
        let (entity, styles_lost) = layers::styles_in(entity, layer);
        let mask = self.mask(layer, area)?;
        Ok(self.finish(layer, entity, mask, outcome, lost || styles_lost))
    }

    /// `layer`'s mask, for a layer whose pixels cover `area` (`x0, y0,
    /// x1, y1`, picture pixels): as Sue paints masks, gray whose value
    /// is the cover, over at least the layer, with what the file's mask
    /// does not cover as the file fills it.
    fn mask(&self, layer: &ClipLayer, area: [i64; 4]) -> Result<Option<Entity>, String> {
        let offset = [area[0], area[1]];
        let (Some(mask), Some([mx, my])) = (&layer.mask, layer.mask_at()) else {
            return Ok(None);
        };
        let values = mask.bitmap.decode()?;
        let step = usize::from(values.channels);
        let outside = if mask.bitmap.white() { 255 } else { 0 };
        // At least the layer's area, and all of the file's mask.
        let x0 = area[0].min(mx);
        let y0 = area[1].min(my);
        let x1 = area[2].max(mx + i64::from(values.width));
        let y1 = area[3].max(my + i64::from(values.height));
        let (w, h) = (
            u32::try_from(x1 - x0).map_err(|_| tr!("clip-layer-too-big"))?,
            u32::try_from(y1 - y0).map_err(|_| tr!("clip-layer-too-big"))?,
        );
        if u64::from(w) * u64::from(h) > 1 << 28 {
            return Err(tr!("clip-layer-too-big"));
        }
        let mut gray = vec![outside; w as usize * h as usize];
        let (vw, dx, dy) = (values.width as usize, mx - x0, my - y0);
        for y in 0..values.height as usize {
            let row = (dy as usize + y) * w as usize + dx as usize;
            for x in 0..vw {
                gray[row + x] = values.data[(y * vw + x) * step + step - 1];
            }
        }
        let raster = RasterTile2D::from_bytes(w, h, PixelFormat::Gray8, gray)
            .map_err(|error| error.to_string())?;
        // Linked to its layer: its place is from the layer's.
        let mut entity = Entity::new(tr!("mask"), raster).with_class(doc::MASK_CLASS);
        let at = [x0 - offset[0], y0 - offset[1]];
        if at[0] != 0 {
            entity = entity.with_property(picture::X, at[0]);
        }
        if at[1] != 0 {
            entity = entity.with_property(picture::Y, at[1]);
        }
        Ok(Some(entity))
    }

    /// Puts the mask switch, the kept data and the fingerprint on
    /// `entity`, and says how it went.
    fn finish(
        &mut self,
        layer: &ClipLayer,
        mut entity: Entity,
        mask: Option<Entity>,
        outcome: Outcome,
        lost: bool,
    ) -> (Entity, Option<Entity>) {
        if mask.is_some() {
            entity = entity.with_property(doc::MASK_ON, layer.mask_enabled());
        }
        let fingerprint = print::layer(&entity, mask.as_ref());
        let mut alone = layer.clone();
        alone.children.clear();
        entity = entity.with_ext(Foreign::new(
            FORMAT,
            "layer",
            fingerprint,
            keep::layer_bytes(&alone),
        ));
        self.report
            .lines
            .push(Line::new(layer.name.clone(), outcome).lost(lost));
        (entity, mask)
    }
}

/// Whether Clip Studio draws this layer from settings Sue cannot redo yet
/// (a correction, a gradient or a fill layer).
pub(super) fn drawn_by_clip_studio(columns: &Columns) -> bool {
    ["FilterLayerInfo", "GradationFillInfo"]
        .iter()
        .any(|name| column(columns, name).and_then(Value::bytes).is_some())
}

/// `entity` with `layer`'s opacity, visibility, clipping and marks.
fn styled(mut entity: Entity, layer: &ClipLayer) -> Entity {
    if layer.opacity != 256 {
        entity = entity.with_property(picture::OPACITY, layer.opacity.clamp(0, 256) as f64 / 256.0);
    }
    if !layer.visible() {
        entity = entity.with_property(picture::VISIBLE, false);
    }
    if layer.clipped() {
        entity = entity.with_property(picture::CLIP, true);
    }
    if layer.draft {
        entity = entity.with_property(picture::DRAFT, true);
    }
    if layer.reference {
        entity = entity.with_property(doc::REFERENCE, true);
    }
    entity
}

/// A new picture named `name` made from `doc`: the edit (labelled
/// `label`), the picture and what became of each layer.
///
/// # Errors
/// When the document's canvas is empty or too large, or a bitmap is
/// broken.
pub fn new_picture(
    doc: &ClipDocument,
    name: &str,
    label: &str,
) -> Result<(Edit, EntityId, Report), String> {
    if doc.width == 0 || doc.height == 0 || doc.width > doc::MAX_SIDE || doc.height > doc::MAX_SIDE
    {
        return Err(tr!(
            "picture-must-most-pixels-side",
            MAX_SIDE = doc::MAX_SIDE
        ));
    }
    let mut canvas = doc.clone();
    canvas.root.children.clear();
    canvas.sue = None;
    let image = Entity::empty(name.to_owned(), class::IMAGE)
        .with_property(picture::WIDTH, i64::from(doc.width))
        .with_property(picture::HEIGHT, i64::from(doc.height))
        .with_property(picture::DPI, doc.dpi);
    let fingerprint = print::picture(&image);
    let image = image.with_ext(Foreign::new(
        FORMAT,
        "picture",
        fingerprint,
        keep::picture_bytes(&canvas, None),
    ));
    let picture = image.id();
    let mut making = Making {
        doc,
        picture,
        low: None,
        high: None,
        edit: Edit::new(label).then(SpawnEntity::new(image)),
        report: Report::default(),
        paths: Vec::new(),
        timeline: doc.timeline.is_some(),
    };
    making.layers(&doc.root.children, &mut Vec::new())?;
    let Making {
        edit,
        mut report,
        paths,
        ..
    } = making;
    // What belongs to the picture as a whole, once its layers are there.
    let mut edit = extras::into(doc, picture, &paths, edit, &mut report.parts);
    // Made once, the picture says what Sue makes of its comps, rulers and
    // timeline: kept with the canvas, so an export that still makes the
    // same writes back what the file held.
    let mut made = sue_core::Arena::new();
    sue_core::ReversibleTransaction::apply(&mut edit, &mut made)
        .map_err(|error| error.to_string())?;
    let written: Vec<(EntityId, ClipLayerPath)> =
        paths.iter().map(|(path, id)| (*id, path.clone())).collect();
    let parts = extras::parts(&made, picture, &written).print();
    // The timeline may have keyed a layer after its fingerprint was taken:
    // each is taken again from the layer as it ends up.
    for (_, id) in &paths {
        let mask = doc::mask_of(&made, *id).and_then(|mask| made.get_by_id(mask).ok());
        let Ok(entity) = made.get_by_id(*id) else {
            continue;
        };
        let now = print::layer(entity, mask);
        let Some(foreign) = entity
            .ext::<Foreign>()
            .filter(|foreign| foreign.fingerprint != now)
            .cloned()
        else {
            continue;
        };
        if let Ok(entity) = made.get_by_id_mut(*id) {
            entity.set_ext(Box::new(Foreign {
                fingerprint: now,
                ..foreign
            }));
        }
    }
    if let Ok(entity) = made.get_by_id_mut(picture) {
        entity.set_ext(Box::new(Foreign::new(
            FORMAT,
            "picture",
            fingerprint,
            keep::picture_bytes(&canvas, Some(&parts)),
        )));
    }
    Ok((settled(made, label), picture, report))
}

/// The edit that brings everything in `arena` into another arena, with
/// its ids and relations, as one step named `label`.
fn settled(mut arena: sue_core::Arena, label: &str) -> Edit {
    let relations: Vec<_> = arena.relations().iter().collect();
    let handles: Vec<_> = arena.iter().map(|(handle, _)| handle).collect();
    let mut edit = Edit::new(label);
    for handle in handles {
        if let Ok(entity) = arena.despawn(handle) {
            edit = edit.then(SpawnEntity::new(entity));
        }
    }
    for relation in relations {
        edit = edit.then(Relate::new(relation));
    }
    edit
}

/// The edit that brings `doc`'s layers into `picture` where a new layer
/// lands with `current` picked (in its place and its folder), and what
/// became of each layer. The canvas of the file is not kept: the picture
/// has its own.
///
/// # Errors
/// When a bitmap is broken.
pub fn into_picture(
    arena: &sue_core::Arena,
    picture: EntityId,
    current: Option<EntityId>,
    doc: &ClipDocument,
    label: &str,
) -> Result<(Edit, Report), String> {
    let (keys, folder) = doc::landing(arena, picture, current, 1);
    let low = doc::layers(arena, picture)
        .into_iter()
        .map(|info| info.order)
        .filter(|order| keys.first().is_some_and(|key| order < key))
        .max();
    let high = doc::layers(arena, picture)
        .into_iter()
        .map(|info| info.order)
        .filter(|order| keys.first().is_some_and(|key| order > key))
        .min();
    let mut making = Making {
        doc,
        picture,
        low,
        high,
        edit: Edit::new(label),
        report: Report::default(),
        paths: Vec::new(),
        timeline: false,
    };
    let top = making.layers(&doc.root.children, &mut Vec::new())?;
    let mut edit = making.edit;
    if let Some(folder) = folder {
        for id in top {
            edit = edit.then(Relate::new(Relation::new(folder, MEMBER, id)));
        }
    }
    Ok((edit, making.report))
}
