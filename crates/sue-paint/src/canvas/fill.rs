//! Fills and selections by area: the flood mask, the bucket, lasso
//! fill, auto select and gradients.
//!
//! What a fill looks at is what the canvas shows at the frame shown (an
//! animation's other cels stay out of it); where it lays its color is
//! [`AreaTarget`]: the editing layer, or its mask while the mask is being
//! edited, and the transparent color clears instead of painting.

use egui::Pos2;
use sue_core::{Arena, EntityId};
use sue_wm::AppletContext;

use crate::color;
use crate::composite::{self, FrameView, RenderOptions};
use crate::doc;
use crate::fill;
use crate::select::{Op, Selection};
use crate::studio::SuePaint;
use crate::tools::SubTool;

/// Where a fill, a lasso fill or a gradient lays its color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AreaTarget {
    /// The raster written: the editing layer, or its mask.
    pub(crate) id: EntityId,
    /// Where the raster's top left sits in the picture.
    pub(crate) offset: [i64; 2],
    /// The color laid (sRGB 0 to 1); `None` clears (the transparent
    /// color, or hiding on a mask).
    pub(crate) color: Option<[f32; 3]>,
    /// The layer's alpha is kept (Lock Transparent Pixels).
    pub(crate) alpha_lock: bool,
    /// The raster is the layer's mask.
    pub(crate) mask: bool,
}

/// The pixels of raster `id` laid at `offset` on a picture of `size`
/// (straight RGBA8), clear around them.
fn placed(arena: &Arena, id: EntityId, offset: [i64; 2], size: [u32; 2]) -> Option<Vec<u8>> {
    let raster = doc::raster(arena, id)?;
    let (width, height) = (raster.width(), raster.height());
    // A gray mask reads as white whose alpha is how much shows.
    let bytes: std::borrow::Cow<'_, [u8]> = if raster.format() == sue_core::PixelFormat::Gray8 {
        raster
            .bytes()?
            .iter()
            .flat_map(|value| [u8::MAX, u8::MAX, u8::MAX, *value])
            .collect::<Vec<u8>>()
            .into()
    } else {
        raster.bytes()?.into()
    };
    if offset == [0, 0] && [width, height] == size {
        return Some(bytes.into_owned());
    }
    let mut out = vec![0_u8; size[0] as usize * size[1] as usize * 4];
    for y in 0..height {
        let py = i64::from(y) + offset[1];
        if py < 0 || py >= i64::from(size[1]) {
            continue;
        }
        for x in 0..width {
            let px = i64::from(x) + offset[0];
            if px < 0 || px >= i64::from(size[0]) {
                continue;
            }
            let from = ((y * width + x) * 4) as usize;
            let to = ((py as u32 * size[0] + px as u32) * 4) as usize;
            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
        }
    }
    Some(out)
}

/// `true` if pixel `b` of `reference` (straight RGBA8) is like pixel `a`
/// within `tolerance` (0 to 1), compared premultiplied as a fill compares.
fn like(reference: &[u8], a: usize, b: usize, tolerance: f32) -> bool {
    let premultiplied = |index: usize| {
        reference.get(index * 4..index * 4 + 4).map(|p| {
            let alpha = u32::from(p[3]);
            [
                u32::from(p[0]) * alpha / 255,
                u32::from(p[1]) * alpha / 255,
                u32::from(p[2]) * alpha / 255,
                alpha,
            ]
        })
    };
    let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as u32;
    match (premultiplied(a), premultiplied(b)) {
        (Some(a), Some(b)) => a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= limit),
        _ => false,
    }
}

impl SuePaint {
    /// Where a fill lays its color now, or why it cannot: the editing
    /// layer, or its mask while Edit Mask is on. On a mask any color
    /// shows the layer and the transparent color hides it, as brushes do
    /// there.
    pub(crate) fn area_target(&self, arena: &Arena) -> Result<AreaTarget, String> {
        let info = self.paintable(arena)?;
        let clear = self.transparent;
        let editing_mask = self.doc().is_some_and(|doc| doc.mask_edit);
        if editing_mask && let Some(mask) = doc::mask_of(arena, info.id) {
            return Ok(AreaTarget {
                id: mask,
                offset: doc::mask_offset(arena, info.id).unwrap_or(info.style.offset),
                color: (!clear).then_some([1.0; 3]),
                alpha_lock: false,
                mask: true,
            });
        }
        Ok(AreaTarget {
            id: info.id,
            offset: info.style.offset,
            color: (!clear).then(|| color::unit(self.drawing_color())),
            alpha_lock: info.alpha_locked,
            mask: false,
        })
    }

    /// What the frame shown shows of an animation (its cels, and the
    /// layers its keyframes move or fade), while the timeline is on;
    /// `None` for a still picture.
    pub(crate) fn frame_view(&self, arena: &Arena) -> Option<FrameView> {
        let (picture, sequence, _) = self.timeline_here(arena).filter(|_| self.timeline_on)?;
        let frame = self.doc()?.frame;
        let mut view = crate::anim::view_in(arena, picture, &sequence, frame, None);
        // The camera frames what is shown; a fill works in the picture.
        view.cameras.clear();
        Some(view)
    }

    /// What Fill and Auto Select look at, as straight RGBA8 picture
    /// pixels: the editing layer (or the mask being edited) alone, or the
    /// layers `sub` refers to as the frame shown shows them.
    pub(in crate::canvas) fn fill_reference(
        &self,
        sub: &SubTool,
        size: [u32; 2],
        arena: &Arena,
    ) -> Option<Vec<u8>> {
        let picture_id = self.picture()?;
        match sub.referred() {
            fill::Refer::Editing => match self.area_target(arena) {
                Ok(target) if target.mask => placed(arena, target.id, target.offset, size),
                _ => self.layer_on_picture(arena, size),
            },
            // Fill up to vector path: vector layers as their lines' paths.
            refer if self.fill_vector_path && sub.tool == crate::tools::Tool::Fill => {
                let picked = self.doc().map(|doc| doc.picked.clone()).unwrap_or_default();
                crate::vector::path_reference(
                    arena,
                    picture_id,
                    size,
                    self.layer(),
                    &picked,
                    refer,
                    sub.exclude,
                )
            }
            refer => {
                let picked = self.doc().map(|doc| doc.picked.clone()).unwrap_or_default();
                fill::reference_in(
                    arena,
                    picture_id,
                    size,
                    self.layer(),
                    &picked,
                    refer,
                    sub.exclude,
                    self.frame_view(arena).as_ref(),
                )
            }
        }
    }

    /// The area a flood fill from each of `seeds` (picture pixels)
    /// reaches together, as a picture-sized mask; `clip` keeps it inside
    /// the selection (a fill does, Auto Select makes the next selection).
    /// A seed inside the area already taken adds nothing.
    pub(in crate::canvas) fn flood_mask(
        &self,
        seeds: &[[f32; 2]],
        sub: &SubTool,
        size: [u32; 2],
        arena: &Arena,
        clip: bool,
    ) -> Option<Vec<u8>> {
        let inside = |p: &&[f32; 2]| {
            p[0] >= 0.0 && p[1] >= 0.0 && p[0] < size[0] as f32 && p[1] < size[1] as f32
        };
        let seeds: Vec<[u32; 2]> = seeds
            .iter()
            .filter(inside)
            .map(|p| [p[0] as u32, p[1] as u32])
            .collect();
        if seeds.is_empty() {
            return None;
        }
        let reference = self.fill_reference(sub, size, arena)?;
        let settings = sub.area();
        // A drag fills the areas like the one it started in: crossing a
        // line on the way to the next area leaves the line alone.
        let first = (seeds[0][1] * size[0] + seeds[0][0]) as usize;
        let mut mask: Option<Vec<u8>> = None;
        for seed in seeds {
            let index = (seed[1] * size[0] + seed[0]) as usize;
            if mask.as_ref().is_some_and(|mask| mask[index] > 0)
                || !like(&reference, first, index, settings.tolerance)
            {
                continue;
            }
            let area = fill::area(&reference, size, seed, &settings);
            match mask.as_mut() {
                None => mask = Some(area),
                Some(mask) => {
                    for (into, value) in mask.iter_mut().zip(area) {
                        *into = (*into).max(value);
                    }
                }
            }
        }
        let mut mask = mask?;
        if let Some(selection) = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .filter(|_| clip)
        {
            for (index, value) in mask.iter_mut().enumerate() {
                let (x, y) = (index as u32 % size[0], index as u32 / size[0]);
                *value = (f32::from(*value) * selection.at(x, y)).round() as u8;
            }
        }
        Some(mask)
    }

    /// Lays `target`'s color (or clears) at `opacity` over the picture
    /// pixels `mask` covers, as one undo step named `label`.
    fn lay_area(
        &mut self,
        target: AreaTarget,
        mask: &[u8],
        size: [u32; 2],
        opacity: f32,
        label: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let AreaTarget {
            id,
            offset,
            color,
            alpha_lock,
            mask: on_mask,
        } = target;
        match doc::rewrite(
            &cx.world.arena,
            id,
            label,
            |pixels, width, height| match color {
                Some(rgb) => fill::paint_mask(
                    pixels,
                    [width, height],
                    offset,
                    mask,
                    size,
                    rgb,
                    opacity,
                    alpha_lock,
                ),
                None => fill::paint_blended(
                    pixels,
                    [width, height],
                    offset,
                    mask,
                    size,
                    None,
                    opacity,
                    sue_model::BlendMode::Normal,
                    alpha_lock,
                ),
            },
        ) {
            Ok(Some(edit)) => {
                if Self::apply(cx, edit) && color.is_some() && !on_mask {
                    self.recent.push(self.drawing_color());
                }
            }
            Ok(None) => {}
            Err(error) => cx.fail(error),
        }
    }

    /// The paint bucket: the areas a click (or a drag, every area it
    /// crosses) at `seeds` reaches fill, as one undo step.
    pub(super) fn bucket(
        &mut self,
        seeds: &[[f32; 2]],
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let target = match self.area_target(&cx.world.arena) {
            Ok(target) => target,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(mask) = self.flood_mask(seeds, sub, size, &cx.world.arena, true) else {
            return;
        };
        self.lay_area(target, &mask, size, sub.tip.opacity, &tr!("fill"), cx);
    }

    /// Lasso fill and Close and fill: the area drawn round (or, closing,
    /// the areas inside it that lines close off) filled with the drawing
    /// color, within the selection.
    pub(super) fn lasso_fill(
        &mut self,
        sub: &SubTool,
        close: bool,
        points: &[[f32; 2]],
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        if points.len() < 3 {
            return;
        }
        let Ok(target) = self.area_target(&cx.world.arena) else {
            return;
        };
        let Some(picture_id) = self.picture() else {
            return;
        };
        let mut shape = Selection::new(size[0], size[1]);
        shape.polygon(points, Op::Replace);
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let lasso: Vec<u8> = (0..size[0] * size[1])
            .map(|index| {
                let (x, y) = (index % size[0], index / size[0]);
                let inside = shape.at(x, y)
                    * selection
                        .as_ref()
                        .map_or(1.0, |selection| selection.at(x, y));
                (inside * 255.0).round() as u8
            })
            .collect();
        let mask = if close {
            let reference = composite::render_view(
                &cx.world.arena,
                picture_id,
                [0, 0, size[0], size[1]],
                RenderOptions {
                    paper: false,
                    drafts: true,
                },
                self.frame_view(&cx.world.arena).as_ref(),
            );
            if sub.mode == crate::tools::PAINT_UNPAINTED {
                let editing = if target.mask {
                    placed(&cx.world.arena, target.id, target.offset, size)
                } else {
                    self.layer_on_picture(&cx.world.arena, size)
                };
                let Some(layer) = editing else {
                    return;
                };
                fill::unpainted(&layer, &reference, size, &lasso)
            } else {
                // Target color: the lines that close the areas are the
                // other layers' (so what the editing layer has painted
                // can be filled again), and of the closed areas only the
                // pixels the editing layer has in that color fill.
                let others = (sub.close_target != fill::Target::All)
                    .then(|| {
                        fill::reference_in(
                            &cx.world.arena,
                            picture_id,
                            size,
                            self.layer(),
                            &[],
                            fill::Refer::All,
                            fill::Exclude {
                                editing: true,
                                ..fill::Exclude::default()
                            },
                            self.frame_view(&cx.world.arena).as_ref(),
                        )
                    })
                    .flatten();
                let mut enclosed =
                    fill::enclosed(others.as_ref().unwrap_or(&reference), size, &lasso);
                if sub.close_target != fill::Target::All {
                    let editing = if target.mask {
                        placed(&cx.world.arena, target.id, target.offset, size)
                    } else {
                        self.layer_on_picture(&cx.world.arena, size)
                    };
                    let Some(layer) = editing else {
                        return;
                    };
                    for (index, value) in enclosed.iter_mut().enumerate() {
                        let pixel = layer
                            .get(index * 4..index * 4 + 4)
                            .map_or([0; 4], |p| [p[0], p[1], p[2], p[3]]);
                        if !sub.close_target.matches(pixel, sub.tolerance) {
                            *value = 0;
                        }
                    }
                }
                enclosed
            }
        } else {
            lasso
        };
        if mask.iter().all(|value| *value == 0) {
            return;
        }
        let label = crate::localize(&sub.name);
        self.lay_area(target, &mask, size, sub.tip.opacity, &label, cx);
    }

    /// The auto select tool: a flood fill that makes a selection.
    pub(super) fn auto_select(
        &mut self,
        p: Pos2,
        sub: &SubTool,
        op: Op,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(mask) = self.flood_mask(&[[p.x, p.y]], sub, size, &cx.world.arena, false) else {
            return;
        };
        if let Some(doc) = self.doc_mut() {
            let mut selection = doc
                .selection
                .clone()
                .unwrap_or_else(|| Selection::new(size[0], size[1]));
            selection.apply_mask(&mask, op);
            doc.set_selection(Some(selection));
        }
    }
}

#[cfg(test)]
pub(in crate::canvas) mod tests {
    //! The canvas side of settings whose controls live in Tool Property
    //! (P4-2): each test sets the sub tool as its control does and runs
    //! the tool's own canvas code.

    use super::*;
    use crate::tools::Tool;
    use sue_core::{PixelFormat, RasterTile2D};

    pub(in crate::canvas) const SIZE: [u32; 2] = [80, 40];

    /// Sue Paint with a picture of 80 by 40 whose layers, bottom first,
    /// hold `layers` (pixels, and whether it is a draft layer), the top
    /// one picked.
    pub(in crate::canvas) fn with_layers(
        layers: Vec<(Vec<u8>, bool)>,
        work: impl FnOnce(&mut SuePaint, &mut AppletContext<'_>, Vec<EntityId>),
    ) {
        let mut rice =
            sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped settings");
        sue_wm::register_rice(&mut rice).expect("the shell's settings register");
        let theme = sue_wm::Theme::from_rice(&rice);
        let mut world = sue_wm::World {
            arena: Arena::new(),
            rice,
        };
        let mut history = sue_core::UndoStack::new();
        let mut typing = sue_wm::Typing::default();
        let mut requests = Vec::new();
        let catalog = sue_wm::Catalog::default();
        let mut textures = sue_platform::NoTextures;
        let mut transport = sue_wm::Transport::default();
        let work_list = sue_wm::WorkList::default();
        let mut transfers = sue_wm::Transfers::default();
        let mut cx = AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: sue_wm::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        let (edit, picture) = doc::create(&doc::NewPicture {
            name: "Fill".to_owned(),
            width: SIZE[0],
            height: SIZE[1],
            ..doc::NewPicture::default()
        })
        .expect("a picture");
        assert!(SuePaint::apply(&mut cx, edit));
        let mut ids = Vec::new();
        let mut below = doc::layers(&cx.world.arena, picture)
            .last()
            .map(|info| info.id);
        for (index, (pixels, draft)) in layers.into_iter().enumerate() {
            let raster =
                RasterTile2D::from_bytes(SIZE[0], SIZE[1], PixelFormat::Rgba8, pixels).unwrap();
            let (spawn, id) =
                doc::add_layer_pixels(&cx.world.arena, picture, below, format!("{index}"), raster)
                    .unwrap();
            assert!(SuePaint::apply(&mut cx, spawn));
            if draft {
                let mark = sue_core::txn::SetProperty::new(id, sue_model::picture::DRAFT, true);
                assert!(SuePaint::apply(&mut cx, mark));
            }
            below = Some(id);
            ids.push(id);
        }
        let mut paint = SuePaint::default();
        paint.sync_docs(&cx.world.arena);
        if let Some(doc) = paint.doc_mut() {
            doc.layer = ids.last().copied();
        }
        work(&mut paint, &mut cx, ids);
    }

    /// A clear picture of [`SIZE`] with the pixels `ink` says inked black.
    pub(in crate::canvas) fn picture(ink: impl Fn(u32, u32) -> bool) -> Vec<u8> {
        let mut out = vec![0_u8; (SIZE[0] * SIZE[1] * 4) as usize];
        for y in 0..SIZE[1] {
            for x in 0..SIZE[0] {
                if ink(x, y) {
                    out[((y * SIZE[0] + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        out
    }

    pub(in crate::canvas) fn selection(paint: &SuePaint) -> Selection {
        paint
            .doc()
            .and_then(|doc| doc.selection.clone())
            .expect("a selection")
    }

    pub(in crate::canvas) fn sub(tool: Tool, mode: u8) -> SubTool {
        SubTool::new(tool, "test", mode, crate::brush::Tip::default())
    }

    /// The alpha of `layer` at picture pixel `(x, 10)`.
    fn alpha(cx: &AppletContext<'_>, layer: EntityId, x: u32) -> u8 {
        doc::raster(&cx.world.arena, layer)
            .and_then(|raster| {
                raster
                    .bytes()
                    .map(|bytes| bytes[((10 * SIZE[0] + x) * 4 + 3) as usize])
            })
            .unwrap_or_default()
    }

    #[test]
    fn auto_select_and_fill_take_every_like_pixel_when_not_connected_only() {
        // FGV-21, ABV-26, A25V-23: Apply to connected pixels only.
        let wall = picture(|x, _| (36..45).contains(&x));
        with_layers(vec![(wall, false)], |paint, cx, ids| {
            let mut auto = sub(Tool::AutoSelect, 0);
            paint.auto_select(Pos2::new(10.0, 10.0), &auto, Op::Replace, SIZE, cx);
            let joined = selection(paint);
            assert!(joined.at(10, 10) > 0.0 && joined.at(70, 10) == 0.0);
            auto.connected_only = false;
            paint.auto_select(Pos2::new(10.0, 10.0), &auto, Op::Replace, SIZE, cx);
            let every = selection(paint);
            assert!(every.at(70, 10) > 0.0, "the other side is like it");
            assert_eq!(
                every.at(40, 10),
                0.0,
                "the wall is not (the area tucks a pixel under it)"
            );
            // Fill the same way, as one undo step that undoes whole.
            if let Some(doc) = paint.doc_mut() {
                doc.set_selection(None);
            }
            let mut fill = sub(Tool::Fill, 0);
            fill.connected_only = false;
            let before = doc::raster(&cx.world.arena, ids[0])
                .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
                .unwrap();
            let depth = cx.history.depth();
            paint.bucket(&[[10.0, 10.0]], &fill, SIZE, cx);
            assert_eq!(alpha(cx, ids[0], 10), 255);
            assert_eq!(alpha(cx, ids[0], 70), 255, "both sides filled");
            assert_eq!(cx.history.depth(), depth + 1);
            assert!(cx.history.undo(cx.world).is_ok());
            let undone = doc::raster(&cx.world.arena, ids[0])
                .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
                .unwrap();
            assert_eq!(undone, before);
        });
    }

    #[test]
    fn a_fill_that_refers_to_every_layer_leaves_out_the_draft_layers() {
        // A25-4, FG-25: Refer multiple with its exclusions.
        let draft_wall = picture(|x, _| x == 40);
        with_layers(
            vec![(draft_wall, true), (picture(|_, _| false), false)],
            |paint, cx, ids| {
                let mut fill = sub(Tool::Fill, 0).referring(fill::Refer::All);
                fill.exclude.drafts = false;
                paint.bucket(&[[10.0, 10.0]], &fill, SIZE, cx);
                assert_eq!(alpha(cx, ids[1], 10), 255);
                assert_eq!(alpha(cx, ids[1], 70), 0, "the draft's line holds the fill");
                assert!(cx.history.undo(cx.world).is_ok());
                fill.exclude.drafts = true;
                paint.bucket(&[[10.0, 10.0]], &fill, SIZE, cx);
                assert_eq!(alpha(cx, ids[1], 70), 255, "left out, it holds nothing");
            },
        );
    }

    /// The pixel of `layer` at picture pixel `(x, 10)`.
    fn pixel(cx: &AppletContext<'_>, layer: EntityId, x: u32) -> [u8; 4] {
        let bytes = doc::raster(&cx.world.arena, layer)
            .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
            .unwrap();
        let at = ((10 * SIZE[0] + x) * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    }

    #[test]
    fn a_drag_fill_across_a_line_fills_both_sides_and_leaves_the_line() {
        // AB-20: the drag crosses the line to reach the next area; the
        // line itself is not an area the drag means to fill.
        let wall = picture(|x, _| (38..42).contains(&x));
        with_layers(vec![(wall, false)], |paint, cx, ids| {
            paint.main = [255, 0, 0];
            let fill = sub(Tool::Fill, 0);
            let seeds: Vec<[f32; 2]> = (10..=70).map(|x| [x as f32, 10.0]).collect();
            paint.bucket(&seeds, &fill, SIZE, cx);
            assert_eq!(pixel(cx, ids[0], 10), [255, 0, 0, 255]);
            assert_eq!(pixel(cx, ids[0], 70), [255, 0, 0, 255], "both sides");
            assert_eq!(pixel(cx, ids[0], 40), [0, 0, 0, 255], "the line stays");
            // Started on the line, a drag fills the line alone.
            assert!(cx.history.undo(cx.world).is_ok());
            paint.bucket(&[[40.0, 10.0], [60.0, 10.0]], &fill, SIZE, cx);
            assert_eq!(pixel(cx, ids[0], 40), [255, 0, 0, 255]);
            assert_eq!(pixel(cx, ids[0], 60), [0, 0, 0, 0]);
        });
    }

    #[test]
    fn a_clearing_gradient_keeps_a_layer_whose_transparent_pixels_are_locked() {
        // The transparent color clears like the bucket does: not on a
        // layer with Lock Transparent Pixels.
        let full = picture(|_, _| true);
        with_layers(vec![(full, false)], |paint, cx, ids| {
            let lock =
                sue_core::txn::SetProperty::new(ids[0], sue_model::picture::ALPHA_LOCKED, true);
            assert!(SuePaint::apply(cx, lock));
            paint.transparent = true;
            let gradient = sub(Tool::Gradient, 0);
            let depth = cx.history.depth();
            paint.gradient(Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0), &gradient, cx);
            assert_eq!(cx.history.depth(), depth, "nothing changed");
            assert_eq!(alpha(cx, ids[0], 5), 255);
            // Unlocked, it clears from full at the start to none at the end.
            assert!(cx.history.undo(cx.world).is_ok());
            paint.gradient(Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0), &gradient, cx);
            assert_eq!(
                cx.history.depth(),
                depth,
                "the gradient took the lock's place"
            );
            assert!(alpha(cx, ids[0], 1) < 10);
            assert!(alpha(cx, ids[0], 78) > 245);
        });
    }
}
