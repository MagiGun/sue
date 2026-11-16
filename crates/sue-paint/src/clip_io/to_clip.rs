//! A picture of Sue's into a [`ClipDocument`]: every folder and layer as
//! it stacks, in its folder, with its style, pixels and mask. A layer
//! imported from a `.clip` and not changed since (its fingerprint is the
//! one its `clip.foreign` component was made with) goes back exactly as
//! it was read, with everything Sue does not map; a changed one is
//! written from Sue's model and its kept data is left out. What the file
//! held beyond its layers comes back while the canvas keeps its size.

use sue_core::{Arena, ComponentKind, Entity, EntityId, PixelFormat, RasterTile2D};
use sue_io::clip::{
    ClipBitmap, ClipContent, ClipDocument, ClipLayer, ClipLayerPath, ClipMask, RASTER,
};
use sue_model::picture::{self, LayerKind};
use sue_model::{BlendMode, Foreign, class};

use super::report::{Line, Outcome, Report};
use super::{FORMAT, composite_of, extras, keep, layers, print};
use crate::doc;

/// The kept data of `part` on `entity`, when it is the `.clip`
/// converter's.
fn kept<'a>(entity: &'a Entity, part: &str) -> Option<&'a Foreign> {
    entity
        .ext::<Foreign>()
        .filter(|foreign| foreign.format == FORMAT && foreign.part == part)
}

/// `value` 0 to 1 as 0 to 255.
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A raster's pixels as straight RGBA8.
fn rgba(raster: &RasterTile2D) -> Option<Vec<u8>> {
    match raster.format() {
        PixelFormat::Rgba8 => raster.bytes().map(<[u8]>::to_vec),
        PixelFormat::Gray8 => raster
            .bytes()
            .map(|bytes| bytes.iter().flat_map(|v| [*v, *v, *v, 255]).collect()),
        PixelFormat::RgbaF32 => raster
            .f32_samples()
            .map(|samples| samples.iter().map(|s| unit_byte(*s)).collect()),
    }
}

/// How a layer changed in Sue comes out, by what its kept data was: one
/// Clip Studio drew from more than pixels (text, vector lines, a
/// correction, a fill) is written as its pixels only, and its own data
/// is gone from the file.
fn lost(entity: &Entity) -> Outcome {
    let Some(layer) =
        kept(entity, "layer").and_then(|foreign| keep::layer_from(&foreign.data).ok())
    else {
        return Outcome::Exact;
    };
    if super::to_sue::drawn_by_clip_studio(&layer.columns)
        || !(layer.layer_type == RASTER || layer.is_paper() || layer.is_folder())
    {
        Outcome::Flattened
    } else {
        Outcome::Exact
    }
}

struct Writing<'a> {
    arena: &'a Arena,
    picture: EntityId,
    size: [u32; 2],
    dpi: f64,
    report: Report,
    /// Where each layer written went in the file's tree.
    paths: Vec<(EntityId, ClipLayerPath)>,
    /// Every layer went back exactly as it was read.
    all_kept: bool,
}

impl Writing<'_> {
    /// The layers whose folder is `parent` (at `at` in the file's tree),
    /// bottom first.
    fn layers(
        &mut self,
        stack: &[doc::LayerInfo],
        parent: Option<EntityId>,
        at: &mut Vec<u32>,
    ) -> Vec<ClipLayer> {
        let mut out = Vec::new();
        for info in stack
            .iter()
            .filter(|info| doc::folder_of(self.arena, info.id) == parent)
        {
            let Ok(entity) = self.arena.get_by_id(info.id) else {
                continue;
            };
            at.push(u32::try_from(out.len()).unwrap_or(u32::MAX));
            let folder = doc::is_folder(self.arena, info.id);
            let children = if folder {
                self.layers(stack, Some(info.id), at)
            } else {
                Vec::new()
            };
            let path = ClipLayerPath(at.clone());
            at.pop();
            let mask =
                doc::mask_of(self.arena, info.id).and_then(|mask| self.arena.get_by_id(mask).ok());
            let now = print::layer(entity, mask);
            if let Some(foreign) = kept(entity, "layer").filter(|foreign| foreign.unchanged(&now))
                && let Ok(mut layer) = keep::layer_from(&foreign.data)
            {
                layer.children = children;
                self.report
                    .lines
                    .push(Line::new(info.name.clone(), Outcome::Exact));
                self.paths.push((info.id, path));
                out.push(layer);
                continue;
            }
            self.all_kept = false;
            let made = if folder {
                Some((self.folder(info, children), Outcome::Exact, false))
            } else {
                self.layer(entity, info)
            }
            .map(|(layer, outcome, lost)| (layer, outcome.max(self::lost(entity)), lost));
            let Some((mut layer, mut outcome, mut approximated)) = made else {
                self.report
                    .lines
                    .push(Line::new(info.name.clone(), Outcome::LeftOut));
                continue;
            };
            let (effects, lost) = layers::effects_out(entity);
            approximated |= lost;
            if effects.is_some() && outcome == Outcome::Exact {
                outcome = Outcome::Live;
            }
            layer.effects = effects;
            if !layer.is_paper()
                && let Some(color) = layers::draw_color_out(entity)
            {
                layer.draw_color = color;
            }
            layer.palette_color = layers::palette_out(entity);
            if let Some(mask) = self.mask(info, &layer) {
                layer.mask = Some(mask);
                layer.set_mask_enabled(doc::mask_on(self.arena, info.id));
            }
            self.report
                .lines
                .push(Line::new(info.name.clone(), outcome).lost(approximated));
            self.paths.push((info.id, path));
            out.push(layer);
        }
        out
    }

    fn style(&self, info: &doc::LayerInfo, layer: &mut ClipLayer) {
        layer.opacity = (f64::from(info.style.opacity.clamp(0.0, 1.0)) * 256.0).round() as i64;
        layer.set_visible(info.style.visible);
        layer.clip = i64::from(info.style.clip);
        layer.draft = info.style.draft;
        layer.reference = doc::is_reference(self.arena, info.id);
    }

    fn folder(&self, info: &doc::LayerInfo, children: Vec<ClipLayer>) -> ClipLayer {
        let mut layer = ClipLayer::folder(&info.name);
        layer.set_open(doc::is_open(self.arena, info.id));
        layer.composite = composite_of(doc::folder_blend(self.arena, info.id));
        self.style(info, &mut layer);
        layer.children = children;
        layer
    }

    /// A layer that is not a folder, how it came out and whether
    /// something of it was lost; `None` for one Clip Studio has nothing
    /// for.
    fn layer(&self, entity: &Entity, info: &doc::LayerInfo) -> Option<(ClipLayer, Outcome, bool)> {
        let [width, height] = self.size;
        let (mut layer, outcome, lost) = match LayerKind::of(entity) {
            LayerKind::Paper => {
                let rgb = picture::paper_color(entity)
                    .to_space(sue_core::ColorSpace::Srgb)
                    .rgb;
                let color = rgb.map(unit_byte);
                (
                    ClipLayer::paper(&info.name, color, width, height),
                    Outcome::Exact,
                    false,
                )
            }
            LayerKind::Raster => {
                let raster = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
                let pixels = rgba(raster)?;
                let [x, y] = info.style.offset;
                let mut layer = ClipLayer::raster(
                    &info.name,
                    x,
                    y,
                    ClipBitmap::color(raster.width(), raster.height(), &pixels),
                );
                let format_lost = raster.format() != PixelFormat::Rgba8;
                if let Some((content, lost)) = layers::content_out(entity, [x, y], self.dpi) {
                    // Text and vector lines: Clip Studio gets what they
                    // drew, Sue the words and the lines.
                    layer.content = Some(content);
                    (layer, Outcome::Live, lost || format_lost)
                } else {
                    // Frames, balloons, figures and the like ride on a
                    // raster: Clip Studio gets what they drew.
                    let more = entity.class() != class::LAYER
                        || entity.exts().any(|component| {
                            component.type_name() != <Foreign as sue_core::ComponentType>::NAME
                        });
                    let outcome = if more {
                        Outcome::Flattened
                    } else {
                        Outcome::Exact
                    };
                    (layer, outcome, format_lost)
                }
            }
            LayerKind::Other => {
                let shown = layers::shown_pixels(self.arena, self.picture, info.id);
                let (pixels, [x, y]) = match &shown {
                    Some((raster, at)) => (
                        ClipBitmap::color(raster.width(), raster.height(), &rgba(raster)?),
                        *at,
                    ),
                    None => (ClipBitmap::color(width, height, &[]), [0, 0]),
                };
                if let Some((content, lost)) =
                    layers::content_out(entity, info.style.offset, self.dpi)
                {
                    let correction = matches!(content, ClipContent::Correction(_));
                    let outcome = if correction || shown.is_none() {
                        Outcome::SueOnly
                    } else {
                        Outcome::Live
                    };
                    (
                        ClipLayer::generated(&info.name, content, x, y, pixels),
                        outcome,
                        lost,
                    )
                } else {
                    // A layer kind Clip Studio has no place for (an image
                    // material, a kind from a newer Sue): what it shows.
                    shown.as_ref()?;
                    (
                        ClipLayer::raster(&info.name, x, y, pixels),
                        Outcome::Flattened,
                        false,
                    )
                }
            }
        };
        let blend = info.style.blend;
        layer.composite = composite_of(if blend == BlendMode::Through {
            BlendMode::Normal
        } else {
            blend
        });
        self.style(info, &mut layer);
        Some((layer, outcome, lost))
    }

    /// `layer`'s mask, as a one channel bitmap where it sits.
    fn mask(&self, info: &doc::LayerInfo, layer: &ClipLayer) -> Option<ClipMask> {
        let mask = doc::mask_of(self.arena, info.id)?;
        let raster = doc::raster(self.arena, mask)?;
        let at = doc::mask_offset(self.arena, info.id)?;
        let (w, h) = (raster.width(), raster.height());
        let mut values = Vec::with_capacity(w as usize * h as usize);
        for y in 0..i64::from(h) {
            for x in 0..i64::from(w) {
                values.push(unit_byte(doc::cover(raster, x, y)));
            }
        }
        Some(ClipMask {
            offset: [at[0] - layer.offset[0], at[1] - layer.offset[1]],
            offscreen_offset: [0, 0],
            bitmap: ClipBitmap::gray(w, h, &values, false),
        })
    }
}

/// `picture` as a [`ClipDocument`], and what became of each layer.
///
/// # Errors
/// When the picture has no size.
pub fn document(arena: &Arena, picture: EntityId) -> Result<(ClipDocument, Report), String> {
    let entity = arena
        .get_by_id(picture)
        .map_err(|_| tr!("this-picture-has-no-size"))?;
    let [width, height] = picture::size(entity).ok_or_else(|| tr!("this-picture-has-no-size"))?;
    let dpi = entity
        .property(picture::DPI)
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(72.0);
    let mut writing = Writing {
        arena,
        picture,
        size: [width, height],
        dpi,
        report: Report::default(),
        paths: Vec::new(),
        all_kept: true,
    };
    let kept_canvas = kept(entity, "picture");
    let (mut doc, kept_parts) = match kept_canvas
        .filter(|foreign| foreign.unchanged(&print::picture(entity)))
        .and_then(|foreign| keep::picture_and_print(&foreign.data).ok())
    {
        Some(kept) => kept,
        None => {
            writing.report.canvas_dropped = kept_canvas.is_some();
            (ClipDocument::new(width, height, dpi), None)
        }
    };
    doc.dpi = dpi;
    let stack = doc::layers(arena, writing.picture);
    doc.root.children = writing.layers(&stack, None, &mut Vec::new());
    // What belongs to the picture as a whole: while Sue makes of it what
    // it made when the file was opened, it goes back as the file held it;
    // otherwise Sue's model says it.
    let parts = extras::parts(arena, picture, &writing.paths);
    let unchanged = kept_parts == Some(parts.print());
    if !unchanged {
        writing.report.parts = parts.lines.clone();
        parts.put(&mut doc);
    }
    if !(unchanged && writing.all_kept) {
        doc.preview = extras::preview(arena, picture);
    }
    // Sue's own style is in the layers now: no hidden copy of the project.
    doc.sue = None;
    Ok((doc, writing.report))
}
