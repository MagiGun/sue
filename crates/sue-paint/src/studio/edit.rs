//! Edit's work on pixels: fills, filters, canvas transforms, clearing,
//! selections by color, Quick Mask and puppet warp.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, RasterTile2D};
use sue_model::picture;
use sue_wm::{AppletContext, ArenaEdit};

use super::{SuePaint, WarpSession};
use crate::color;
use crate::composite::{self, RenderOptions};
use crate::dialogs::{ColorizeDialog, Dialog, FilterDialog};
use crate::doc;
use crate::filters::Filter;
use crate::select::Selection;

impl SuePaint {
    /// Edit, Advanced Fill with its settings: in the selection (or the
    /// whole canvas), looking at the current layer or every shown one.
    pub(crate) fn advanced_fill_now(&mut self, cx: &mut AppletContext<'_>) {
        let Some(selection) = self.selection_or_all(&cx.world.arena) else {
            return;
        };
        let Some(picture) = self.picture() else {
            return;
        };
        let size = selection.size();
        let settings = self.advanced_fill.clone();
        let reference = if settings.refer_all {
            crate::composite::render(
                &cx.world.arena,
                picture,
                [0, 0, size[0], size[1]],
                crate::composite::RenderOptions {
                    paper: false,
                    drafts: true,
                },
            )
        } else {
            match self.layer_on_picture(&cx.world.arena, size) {
                Some(pixels) => pixels,
                None => return,
            }
        };
        let region: Vec<u8> = (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
            .map(|(x, y)| (selection.at(x, y) * 255.0).round() as u8)
            .collect();
        let mask = crate::fill::advanced_mask(&reference, size, &region, &settings.area);
        let color = match settings.color {
            0 => Some(self.main),
            1 => Some(self.sub),
            2 => None,
            _ => Some(settings.rgb),
        }
        .map(|rgb| rgb.map(|value| f32::from(value) / 255.0));
        let (offset, alpha_lock) = self
            .layer_info(&cx.world.arena)
            .map_or(([0, 0], false), |info| {
                (info.style.offset, info.alpha_locked)
            });
        self.rewrite(cx, &tr!("advanced-fill"), |pixels, width, height| {
            crate::fill::paint_blended(
                pixels,
                [width, height],
                offset,
                &mask,
                size,
                color,
                settings.opacity / 100.0,
                settings.blend,
                alpha_lock,
            );
        });
    }

    /// Starts puppet warp on the current layer.
    pub(crate) fn start_warp(&mut self, cx: &mut AppletContext<'_>) {
        if self.warp.is_some() {
            return;
        }
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(before) = raster.bytes().map(<[u8]>::to_vec) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        self.warp = Some(WarpSession {
            layer: info.id,
            size,
            before,
            pins: Vec::new(),
            shown: Vec::new(),
            offset: info.style.offset,
            shown_settings: None,
            mesh: Vec::new(),
            label: tr!("puppet-warp-2"),
        });
        cx.notify(tr!("puppet-warp-click-put-pins-drag"));
    }

    /// Shows the layer bent by the pins as they are now.
    pub(crate) fn warp_preview(&mut self, cx: &mut AppletContext<'_>) {
        self.puppet_preview(cx);
    }

    /// Ends puppet warp: keeps the bend as one undo step, or puts the
    /// layer back.
    pub(crate) fn finish_warp(&mut self, cx: &mut AppletContext<'_>, keep: bool) {
        let Some(warp) = self.warp.take() else {
            return;
        };
        self.gesture = None;
        let after = if keep {
            warp.bend(&self.transform_prefs.puppet)
        } else {
            warp.before.clone()
        };
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, warp.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == after.len())
        {
            bytes.copy_from_slice(&after);
        }
        if keep
            && let Some(patch) = doc::diff_patch(&warp.before, &after, warp.size[0], warp.size[1])
        {
            cx.record(ArenaEdit(sue_core::txn::RasterEdit::new(
                warp.layer,
                warp.label.clone(),
                vec![patch],
            )));
        }
    }

    /// Select Color Gamut: the pixels near the drawing color, of the current
    /// layer or (with `all`) of every layer as shown.
    pub(crate) fn select_gamut(&mut self, cx: &mut AppletContext<'_>, tolerance: f32, all: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let color = self.drawing_color();
        let found = if all {
            composite::flatten(&cx.world.arena, picture, RenderOptions::default()).and_then(
                |tile| {
                    Some(Selection::of_color(
                        tile.bytes()?,
                        tile.width(),
                        tile.height(),
                        color,
                        tolerance,
                    ))
                },
            )
        } else {
            self.layer()
                .and_then(|layer| doc::raster(&cx.world.arena, layer))
                .and_then(|raster| {
                    let offset = self
                        .layer_info(&cx.world.arena)
                        .map_or([0, 0], |info| info.style.offset);
                    let found = Selection::of_color(
                        raster.bytes()?,
                        raster.width(),
                        raster.height(),
                        color,
                        tolerance,
                    );
                    Some(found.shifted([-offset[0], -offset[1]]))
                })
        };
        match found.filter(|selection| !selection.is_empty()) {
            Some(selection) => {
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(Some(selection));
                }
            }
            None => cx.notify(tr!("no-pixels-color-there")),
        }
    }

    /// Quick Mask: the selection becomes a red layer to paint on, and back.
    pub(crate) fn toggle_quick_mask(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(layer) = self.doc().and_then(|doc| doc.quick_mask) {
            let selection = doc::raster(&cx.world.arena, layer).map(Selection::from_alpha);
            if Self::apply(
                cx,
                doc::Edit::new(tr!("quick-mask")).then(doc::delete(layer)),
            ) && let Some(doc) = self.doc_mut()
            {
                doc.quick_mask = None;
                doc.set_selection(selection);
                doc.layer = doc::layers(&cx.world.arena, picture)
                    .last()
                    .map(|info| info.id);
            }
            return;
        }
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        let alpha: Vec<u8> = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map_or_else(
                || vec![0; (size[0] * size[1]) as usize],
                |selection| selection.mask().to_vec(),
            );
        let pixels: Vec<u8> = alpha
            .iter()
            .flat_map(|alpha| [230, 40, 40, *alpha])
            .collect();
        let Ok(raster) = sue_core::RasterTile2D::from_bytes(
            size[0],
            size[1],
            sue_core::PixelFormat::Rgba8,
            pixels,
        ) else {
            return;
        };
        let top = doc::layers(&cx.world.arena, picture)
            .last()
            .map(|info| info.id);
        match doc::add_layer_pixels(&cx.world.arena, picture, top, tr!("quick-mask"), raster) {
            Ok((spawn, id)) => {
                let edit = doc::Edit::new(tr!("quick-mask"))
                    .then(spawn)
                    .then(SetProperty::new(id, picture::DRAFT, true))
                    .then(SetProperty::new(id, picture::OPACITY, 0.5));
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.quick_mask = Some(id);
                    doc.layer = Some(id);
                    doc.set_selection(None);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Runs a filter on the current layer at once.
    pub(crate) fn filter_now(&mut self, cx: &mut AppletContext<'_>, filter: Filter) {
        // A tonal correction with other layers picked reaches them all, as
        // one step.
        if filter.to_text().is_some()
            && self.doc().is_some_and(|doc| !doc.picked.is_empty())
            && let Ok(info) = self.paintable(&cx.world.arena)
            && let Some(raster) = doc::raster(&cx.world.arena, info.id)
        {
            let dialog = FilterDialog::new(
                filter,
                info.id,
                raster,
                self.doc().and_then(|doc| doc.selection.clone()),
                info.style.offset,
            );
            self.keep_filter(cx, &dialog);
            return;
        }
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, &filter.label(), |pixels, width, height| {
            filter.apply(pixels, width, height, selection.as_ref(), offset);
        });
    }

    /// Opens a filter's dialog over the current layer.
    pub(crate) fn filter_dialog(&mut self, cx: &mut AppletContext<'_>, filter: Filter) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        self.dialog = Some(Dialog::Filter(
            FilterDialog::new(
                filter,
                info.id,
                raster,
                self.doc().and_then(|doc| doc.selection.clone()),
                info.style.offset,
            )
            .with_colors(self.main, self.sub),
        ));
    }

    /// The selection, or everything when nothing is selected.
    pub(crate) fn selection_or_all(&self, arena: &Arena) -> Option<Selection> {
        let doc = self.doc()?;
        doc.selection.clone().or_else(|| {
            let [width, height] = picture::size(arena.get_by_id(doc.picture).ok()?)?;
            Some(Selection::all(width, height))
        })
    }

    /// Mirrors the canvas (and for 180 degrees, turns it): every layer's
    /// pixels, masks and vector lines go with it, as one undo step.
    pub(crate) fn transform_canvas(
        &mut self,
        cx: &mut AppletContext<'_>,
        flip_h: bool,
        flip_v: bool,
        label: &str,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        match doc::flip_canvas(&cx.world.arena, picture, flip_h, flip_v, label) {
            Ok(edit) if !edit.is_empty() => {
                Self::apply(cx, edit);
            }
            Ok(_) => {}
            Err(error) => cx.fail(error),
        }
    }

    /// Edit, Outline Selection: a line `width` pixels wide along the
    /// selection's edge, half inside and half outside, in the drawing
    /// color on the current layer.
    pub(crate) fn outline_selection(&mut self, cx: &mut AppletContext<'_>, width: f32) {
        let Some(selection) = self.doc().and_then(|doc| doc.selection.clone()) else {
            return;
        };
        let half = (width / 2.0).max(0.5);
        let outer = selection.expanded(half);
        let inner = selection.shrunk(half);
        let size = selection.size();
        let mask: Vec<u8> = outer
            .mask()
            .iter()
            .zip(inner.mask())
            .map(|(out, inside)| out.saturating_sub(*inside))
            .collect();
        let rgb = color::unit(self.drawing_color());
        let alpha_lock = self
            .layer_info(&cx.world.arena)
            .is_some_and(|info| info.alpha_locked);
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, &tr!("outline-selection"), |pixels, w, h| {
            crate::fill::paint_mask(pixels, [w, h], offset, &mask, size, rgb, 1.0, alpha_lock);
        });
    }

    /// Clears the selected pixels (or, `outside`, the unselected ones).
    pub(crate) fn run_clear(&mut self, cx: &mut AppletContext<'_>, outside: bool, label: &str) {
        let Some(mut selection) = self.selection_or_all(&cx.world.arena) else {
            return;
        };
        if outside {
            selection.invert();
        }
        let offset = self
            .layer_info(&cx.world.arena)
            .map_or([0, 0], |info| info.style.offset);
        self.rewrite(cx, label, |pixels, width, height| {
            for y in 0..height {
                for x in 0..width {
                    let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                    if px < 0 || py < 0 {
                        continue;
                    }
                    let cover = selection.at(px as u32, py as u32);
                    if cover > 0.0 {
                        let at = ((y * width + x) * 4 + 3) as usize;
                        pixels[at] = (f32::from(pixels[at]) * (1.0 - cover)).round() as u8;
                    }
                }
            }
        });
    }

    /// Edit, Pick Screen Color (and, with `hide`, Hide Windows and Pick
    /// Screen Color): the next click anywhere on any screen, also outside
    /// Sue, sets the drawing color; a right click or Esc cancels. Where
    /// the system lets no program read the screen it says so.
    fn start_screen_pick(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>, hide: bool) {
        if self.screen_pick.is_some() {
            return;
        }
        let title = if hide {
            tr!("hide-windows-and-pick-screen-color")
        } else {
            tr!("pick-screen-color")
        };
        let wake = ctx.clone();
        match sue_platform::screen::ScreenPick::start(
            hide,
            title,
            Box::new(move || wake.request_repaint()),
        ) {
            Ok(pick) => {
                self.screen_pick = Some(pick);
                cx.notify(tr!("pick-screen-color-hint"));
            }
            Err(_) => cx.notify(tr!("pick-screen-color-unavailable")),
        }
    }

    /// Follows a screen pick under way, once a frame: the picked color
    /// becomes the drawing color, as the Eyedropper sets it.
    pub(crate) fn screen_pick_frame(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        use sue_platform::screen::PickState;
        if let Some(hide) = self.screen_pick_asked.take() {
            self.start_screen_pick(ctx, cx, hide);
        }
        let Some(pick) = self.screen_pick.as_ref() else {
            return;
        };
        match pick.state() {
            PickState::Waiting(_) => {
                // The picker wakes the frame on each change; this is the
                // net under it.
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            PickState::Picked(rgb) => {
                self.screen_pick = None;
                self.set_drawing_color(rgb);
                ctx.request_repaint();
            }
            PickState::Cancelled => self.screen_pick = None,
            PickState::Failed(_) => {
                self.screen_pick = None;
                cx.notify(tr!("pick-screen-color-failed"));
            }
        }
    }

    /// Edit, Colorize: opens its dialog with the current layer as the hint
    /// layer and, as the line art, the layer right above it (else the
    /// topmost other one).
    pub(crate) fn open_colorize(&mut self, cx: &mut AppletContext<'_>) {
        let info = match self.paintable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let Some(picture) = self.picture() else {
            return;
        };
        let layers = doc::layers(&cx.world.arena, picture);
        let choices: Vec<(EntityId, String)> = layers
            .iter()
            .filter(|layer| layer.id != info.id && doc::raster(&cx.world.arena, layer.id).is_some())
            .rev()
            .map(|layer| (layer.id, layer.name.clone()))
            .collect();
        if choices.is_empty() {
            cx.notify(tr!("colorize-needs-line-art"));
            return;
        }
        let above = layers
            .iter()
            .position(|layer| layer.id == info.id)
            .and_then(|at| layers.get(at + 1))
            .and_then(|layer| choices.iter().position(|(id, _)| *id == layer.id))
            .unwrap_or(0);
        self.dialog = Some(Dialog::Colorize(ColorizeDialog {
            hints: info.id,
            choices,
            lines: above,
            blur: 1.0,
        }));
    }

    /// OK on Colorize: a new layer of flat colors right under the line
    /// art, as one undo step.
    pub(crate) fn colorize_ok(&mut self, cx: &mut AppletContext<'_>, dialog: &ColorizeDialog) {
        let (Some(picture), Some(lines)) = (self.picture(), dialog.line_layer()) else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        let on_canvas = |layer: EntityId| -> Option<Vec<u8>> {
            let raster = doc::raster(arena, layer)?;
            let offset = doc::layers(arena, picture)
                .into_iter()
                .find(|info| info.id == layer)
                .map_or([0, 0], |info| info.style.offset);
            let bytes = raster.bytes()?;
            let mut out = vec![0_u8; (width * height * 4) as usize];
            for y in 0..raster.height() {
                for x in 0..raster.width() {
                    let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                    if px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
                        continue;
                    }
                    let from = ((y * raster.width() + x) * 4) as usize;
                    let to = ((py as u32 * width + px as u32) * 4) as usize;
                    out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
                }
            }
            Some(out)
        };
        let (Some(line_art), Some(hints)) = (on_canvas(lines), on_canvas(dialog.hints)) else {
            return;
        };
        let Some(colors) = crate::colorize::colorize(&line_art, &hints, width, height, dialog.blur)
        else {
            cx.notify(tr!("colorize-needs-hints"));
            return;
        };
        let Ok(raster) =
            RasterTile2D::from_bytes(width, height, sue_core::PixelFormat::Rgba8, colors)
        else {
            return;
        };
        // Right under the line art: above the layer below it.
        let layers = doc::layers(arena, picture);
        let below = layers
            .iter()
            .position(|layer| layer.id == lines)
            .and_then(|at| at.checked_sub(1))
            .and_then(|at| layers.get(at))
            .map(|layer| layer.id);
        match doc::add_layer_pixels(arena, picture, below, tr!("colorize"), raster) {
            Ok((spawn, id)) => {
                if Self::apply(cx, doc::Edit::new(tr!("colorize")).then(spawn))
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            Err(error) => cx.fail(error),
        }
    }
}
