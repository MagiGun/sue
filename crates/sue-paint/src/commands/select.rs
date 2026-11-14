//! The Select menu's commands.

use sue_core::txn::SetProperty;
use sue_core::{Arena, UndoStack, Value};
use sue_model::picture;
use sue_wm::{AppletContext, Call, Command, Facts, ItemState, World};

use super::{Basis, flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::select::{Op, Selection};
use crate::studio::{SELECTION_LAYER, SuePaint};

/// Does what a Select command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_select(call.name, cx);
}

/// How an item for a Select command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_select(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Select command looks right now.
    fn state_select(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis {
            has_doc,
            info,
            raster,
            selection,
        } = self.basis(arena);
        match command {
            "select.from_selection_layer" => when(
                info.as_ref()
                    .and_then(|info| arena.get_by_id(info.id).ok())
                    .is_some_and(|entity| {
                        entity.property(SELECTION_LAYER).and_then(Value::as_bool) == Some(true)
                    }),
            ),
            "select.none" | "select.invert" => when(selection),
            "select.again" => when(self.doc().is_some_and(|doc| doc.last_selection.is_some())),
            "select.expand" | "select.shrink" | "select.feather" | "select.to_layer" => {
                when(selection)
            }
            "select.add_layer" | "select.subtract_layer" => when(raster),
            "select.quick_mask" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.quick_mask.is_some())),
            "select.launcher" => flag(self.launcher),
            "select.from_layer" => when(raster),
            _ => when(has_doc),
        }
    }

    /// Does what a Select command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_select(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "select.all" => {
                let all = self.doc().and_then(|doc| {
                    let [w, h] = picture::size(cx.world.arena.get_by_id(doc.picture).ok()?)?;
                    Some(Selection::all(w, h))
                });
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(all);
                }
            }
            "select.none" => {
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(None);
                }
            }
            "select.again" => {
                if let Some(doc) = self.doc_mut() {
                    let again = doc.last_selection.clone();
                    doc.set_selection(again);
                }
            }
            "select.invert" => {
                if let Some(doc) = self.doc_mut()
                    && let Some(mut selection) = doc.selection.clone()
                {
                    selection.invert();
                    doc.set_selection(Some(selection));
                }
            }
            "select.expand" | "select.shrink" | "select.feather" => {
                let kind = match command {
                    "select.expand" => 0,
                    "select.shrink" => 1,
                    _ => 2,
                };
                if self.doc().is_some_and(|doc| doc.selection.is_some()) {
                    self.dialog = Some(Dialog::SelectBy(kind, 4.0));
                }
            }
            "select.gamut" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Gamut(0.1, false));
                }
            }
            "select.add_layer" | "select.subtract_layer" => {
                let Some(raster) = self
                    .layer()
                    .and_then(|layer| doc::raster(&cx.world.arena, layer))
                else {
                    return;
                };
                let from = Selection::from_alpha(raster);
                let op = if command == "select.add_layer" {
                    Op::Add
                } else {
                    Op::Subtract
                };
                if let Some(doc) = self.doc_mut() {
                    let [width, height] = from.size();
                    let mut selection = doc
                        .selection
                        .clone()
                        .unwrap_or_else(|| Selection::new(width, height));
                    selection.apply_mask(from.mask(), op);
                    doc.set_selection(Some(selection));
                }
            }
            "select.to_layer" => {
                let (Some(picture), Some(selection)) = (
                    self.picture(),
                    self.doc().and_then(|doc| doc.selection.clone()),
                ) else {
                    return;
                };
                // A selection layer: the selection in green, left out of
                // exports; Add Selection takes it back.
                let [width, height] = selection.size();
                let pixels: Vec<u8> = selection
                    .mask()
                    .iter()
                    .flat_map(|alpha| [60, 200, 90, *alpha])
                    .collect();
                let Ok(raster) = sue_core::RasterTile2D::from_bytes(
                    width,
                    height,
                    sue_core::PixelFormat::Rgba8,
                    pixels,
                ) else {
                    return;
                };
                match doc::add_layer_pixels(
                    &cx.world.arena,
                    picture,
                    self.layer(),
                    tr!("selection"),
                    raster,
                ) {
                    Ok((spawn, id)) => {
                        let edit = doc::Edit::new(tr!("convert-to-selection-layer"))
                            .then(spawn)
                            .then(SetProperty::new(id, SELECTION_LAYER, true))
                            .then(SetProperty::new(id, picture::DRAFT, true))
                            .then(SetProperty::new(
                                id,
                                picture::OPACITY,
                                cx.rice()
                                    .get_i64("sue_paint.selection_layer_opacity")
                                    .unwrap_or(50)
                                    .clamp(0, 100) as f64
                                    / 100.0,
                            ));
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "select.quick_mask" => self.toggle_quick_mask(cx),
            "select.from_selection_layer" => {
                if let Some(raster) = self
                    .layer()
                    .and_then(|layer| doc::raster(&cx.world.arena, layer))
                {
                    let selection = Selection::from_alpha(raster);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(Some(selection));
                    }
                }
            }
            "select.launcher" => self.launcher = !self.launcher,
            "select.from_layer" => {
                let arena = &cx.world.arena;
                let Some(info) = self.layer_info(arena) else {
                    return;
                };
                let Some(size) = self
                    .picture()
                    .and_then(|picture| arena.get_by_id(picture).ok())
                    .and_then(picture::size)
                else {
                    return;
                };
                let selection = doc::raster(arena, info.id).map(|raster| {
                    let alpha = Selection::from_alpha(raster);
                    let mut mask = vec![0_u8; size[0] as usize * size[1] as usize];
                    let [ox, oy] = info.style.offset;
                    for y in 0..raster.height() {
                        for x in 0..raster.width() {
                            let (px, py) = (i64::from(x) + ox, i64::from(y) + oy);
                            if px >= 0
                                && py >= 0
                                && px < i64::from(size[0])
                                && py < i64::from(size[1])
                            {
                                mask[(py as u32 * size[0] + px as u32) as usize] =
                                    (alpha.at(x, y) * 255.0).round() as u8;
                            }
                        }
                    }
                    Selection::from_mask(size[0], size[1], mask)
                });
                if let Some(doc) = self.doc_mut() {
                    doc.set_selection(selection.flatten());
                }
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "select", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("select.from_layer", "create-selection-from-layer"),
    c("select.all", "select-all").keys("Ctrl+A"),
    c("select.none", "deselect").keys("Ctrl+D"),
    c("select.again", "reselect").keys("Ctrl+Shift+D"),
    c("select.invert", "invert-selected-area-2").keys("Ctrl+Shift+I or Shift+F7"),
    c("select.expand", "expand-selected-area-ellipsis"),
    c("select.shrink", "shrink-selected-area-ellipsis"),
    c("select.feather", "blur-border-ellipsis"),
    c("select.gamut", "select-color-gamut-ellipsis"),
    c("select.add_layer", "add-selection"),
    c("select.subtract_layer", "remove-selection"),
    c("select.to_layer", "convert-to-selection-layer"),
    c(
        "select.from_selection_layer",
        "convert-selection-layer-to-selection",
    ),
    c("select.quick_mask", "quick-mask"),
    c("select.launcher", "selection-launcher"),
];
