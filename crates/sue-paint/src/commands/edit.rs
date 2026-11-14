//! The Edit menu's commands: undo, the clipboard, fills, transforms,
//! alignment, the canvas's size and materials.

use sue_core::{Arena, UndoStack, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState, World};

use super::{Basis, when};
use crate::color;
use crate::dialogs::Dialog;
use crate::doc;
use crate::filters::Filter;
use crate::select::Selection;
use crate::studio::{SuePaint, mirror};

/// Does what a Edit command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_edit(call.name, cx);
}

/// How an item for a Edit command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_edit(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Edit command looks right now.
    fn state_edit(&self, command: &str, arena: &Arena, history: &UndoStack<World>) -> ItemState {
        let Basis {
            has_doc,
            raster,
            selection,
            ..
        } = self.basis(arena);
        match command {
            "edit.undo" => when(history.can_undo()),
            "edit.redo" => when(history.can_redo()),
            "edit.paste" | "edit.paste_here" => when(has_doc && self.clipboard.is_some()),
            "edit.brush_tip" | "edit.material.tip" => when(raster),
            "edit.clear_outside" => when(selection),
            "edit.cut" | "edit.copy" | "edit.clear" | "edit.fill" | "edit.recolor"
            | "edit.luma_to_alpha" | "edit.align.left" | "edit.align.hcenter"
            | "edit.align.right" | "edit.align.top" | "edit.align.vcenter"
            | "edit.align.bottom" | "edit.flip_h" | "edit.flip_v" | "edit.rotate180" => {
                when(raster)
            }
            _ => when(has_doc),
        }
    }

    /// Does what a Edit command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_edit(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        let rgb = color::unit(self.drawing_color());
        match command {
            "edit.transform.puppet" => self.start_warp(cx),
            "edit.transform.mesh" => {
                self.start_warp(cx);
                // Mesh Transformation: a 4 by 4 grid of points over the
                // layer's pixels, each dragged to bend what is near it.
                if let Some(warp) = self.warp.as_mut()
                    && warp.pins.is_empty()
                    && let Some([x0, y0, x1, y1]) = warp.bounds
                {
                    for row in 0..4 {
                        for column in 0..4 {
                            let at = [
                                x0 as f32 + (x1 - x0) as f32 * column as f32 / 3.0,
                                y0 as f32 + (y1 - y0) as f32 * row as f32 / 3.0,
                            ];
                            warp.pins.push(crate::warp::Pin::at(at));
                        }
                    }
                    cx.notify(tr!("mesh-transformation-drag-points-bend-layer"));
                }
            }
            "edit.transform.scale" | "edit.transform.scale_only" | "edit.transform.rotate" => {
                self.start_transform(cx, false);
            }
            "edit.transform.free"
            | "edit.transform.distort"
            | "edit.transform.skew"
            | "edit.transform.perspective" => {
                self.start_transform(cx, true);
            }
            "edit.undo" | "edit.redo" => {
                cx.typing.flush(cx.history);
                let result = if command == "edit.undo" {
                    cx.history.undo(cx.world)
                } else {
                    cx.history.redo(cx.world)
                };
                if let Err(error) = result {
                    cx.fail(tr!("undo-failed", error = error));
                }
            }
            "edit.copy" => {
                if self.copy(cx) {
                    cx.notify(tr!("copied"));
                }
            }
            "edit.cut" => {
                if self.copy(cx) {
                    self.run_clear(cx, false, &tr!("cut"));
                }
            }
            "edit.paste" => self.paste(cx, &tr!("pasted")),
            "edit.clear"
                if self.toolbox.tool() == crate::tools::Tool::Operation
                    && self
                        .picked_lines
                        .as_ref()
                        .is_some_and(|(_, picked)| !picked.is_empty()) =>
            {
                self.delete_picked_lines(cx);
            }
            "edit.clear" => self.run_clear(cx, false, &tr!("clear")),
            "edit.clear_outside" => self.run_clear(cx, true, &tr!("clear-outside-selection-2")),
            "edit.fill" => {
                let Some(selection) = self.selection_or_all(&cx.world.arena) else {
                    return;
                };
                let size = selection.size();
                let alpha_lock = self
                    .layer_info(&cx.world.arena)
                    .is_some_and(|info| info.alpha_locked);
                let offset = self
                    .layer_info(&cx.world.arena)
                    .map_or([0, 0], |info| info.style.offset);
                let mask: Vec<u8> = (0..size[1])
                    .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
                    .map(|(x, y)| (selection.at(x, y) * 255.0).round() as u8)
                    .collect();
                self.rewrite(cx, &tr!("fill"), |pixels, width, height| {
                    crate::fill::paint_mask(
                        pixels,
                        [width, height],
                        offset,
                        &mask,
                        size,
                        rgb,
                        1.0,
                        alpha_lock,
                    );
                });
            }
            "edit.clear_history" => {
                // Edit, Clear work history: this canvas's steps only, as
                // each canvas keeps its own.
                cx.typing.flush(cx.history);
                *cx.history = sue_core::UndoStack::new();
                cx.notify(tr!("work-history-cleared"));
            }
            "edit.outline_selection" => {
                if self.doc().is_some_and(|doc| doc.selection.is_some()) {
                    self.dialog = Some(Dialog::SelectBy(3, 4.0));
                } else {
                    cx.fail(tr!("select-area-keep-first"));
                }
            }
            "edit.recolor" => self.filter_now(
                cx,
                Filter::Recolor {
                    rgb: self.drawing_color(),
                },
            ),
            // Clip Studio Paint's Change canvas height: the canvas's size,
            // for a webtoon's length.
            "edit.canvas_height" => self.run("edit.canvas_size", cx),
            // Puts the layer back as it was before the transform.
            "edit.transform.cancel" => {
                if self.transform.is_some() {
                    self.finish_transform(cx, false);
                }
            }
            align if align.starts_with("edit.align.") => {
                self.align_layer(&align["edit.align.".len()..], cx);
            }
            "edit.luma_to_alpha" => self.filter_now(
                cx,
                Filter::BrightnessToOpacity {
                    rgb: self.drawing_color(),
                },
            ),
            "edit.flip_h" => self.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
                mirror(p, w, h, true, false)
            }),
            "edit.flip_v" => self.rewrite(cx, &tr!("flip-layer"), |p, w, h| {
                mirror(p, w, h, false, true)
            }),
            "edit.rotate180" => {
                self.rewrite(cx, &tr!("rotate-layer"), |p, w, h| {
                    mirror(p, w, h, true, true)
                });
            }
            "edit.advanced_fill" => {
                if let Err(error) = self.paintable(&cx.world.arena) {
                    cx.fail(error);
                    return;
                }
                self.dialog = Some(Dialog::AdvancedFill);
            }
            "edit.smart_smoothing" => {
                self.filter_dialog(cx, Filter::SmartSmooth { strength: 0.5 });
            }
            "edit.material.paste" => {
                let library = self
                    .material_kinds
                    .scan(crate::materials::location(cx).as_deref());
                if let Some(material) = library
                    .into_iter()
                    .find(|material| self.material_pick.as_deref() == Some(material.name.as_str()))
                {
                    self.paste_material(cx, &material);
                } else {
                    cx.notify(tr!("pick-material-material-palette-first"));
                }
            }
            "edit.material.image" => {
                let name = self
                    .layer_info(&cx.world.arena)
                    .map_or_else(|| tr!("material"), |info| info.name);
                if self.paintable(&cx.world.arena).is_ok() {
                    self.dialog = Some(Dialog::Material(name));
                } else {
                    cx.fail(tr!("pick-raster-layer-register-as-material"));
                }
            }
            "edit.canvas_size" | "edit.resolution" => {
                let Some(old) = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(|entity| {
                        let dpi = entity
                            .property(picture::DPI)
                            .and_then(Value::as_f64)
                            .unwrap_or(72.0);
                        picture::size(entity).map(|size| (size, dpi))
                    })
                else {
                    return;
                };
                let (size, dpi) = old;
                self.dialog = Some(if command == "edit.canvas_size" {
                    Dialog::Canvas(crate::dialogs::CanvasDialog {
                        size,
                        old: size,
                        anchor: 4,
                    })
                } else {
                    Dialog::Resolution(crate::dialogs::ResolutionDialog {
                        size,
                        old: size,
                        dpi,
                        keep: true,
                    })
                });
            }
            "edit.crop" => {
                let (Some(picture), Some(bounds)) = (
                    self.picture(),
                    self.doc()
                        .and_then(|doc| doc.selection.as_ref())
                        .and_then(Selection::bounds),
                ) else {
                    cx.fail(tr!("select-area-keep-first"));
                    return;
                };
                let [x0, y0, x1, y1] = bounds;
                match doc::resize_canvas(
                    &cx.world.arena,
                    picture,
                    [x1 - x0, y1 - y0],
                    [-i64::from(x0), -i64::from(y0)],
                ) {
                    Ok(edit) => {
                        let edit = sue_core::txn::Batch::new(tr!("crop")).then(edit);
                        Self::apply(cx, edit);
                        if let Some(doc) = self.doc_mut() {
                            doc.set_selection(None);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "edit.paste_here" => {
                let (Some(doc), Some(clip)) = (self.doc(), self.clipboard.as_ref()) else {
                    return;
                };
                let size = cx
                    .world
                    .arena
                    .get_by_id(doc.picture)
                    .ok()
                    .and_then(picture::size)
                    .unwrap_or([1, 1]);
                let center = doc
                    .view
                    .to_picture(self.viewport, size, self.viewport.center());
                let origin = [
                    (center.x - clip.raster.width() as f32 / 2.0)
                        .round()
                        .max(0.0) as u32,
                    (center.y - clip.raster.height() as f32 / 2.0)
                        .round()
                        .max(0.0) as u32,
                ];
                let saved = clip.origin;
                if let Some(clip) = self.clipboard.as_mut() {
                    clip.origin = origin;
                }
                self.paste(cx, &tr!("pasted"));
                if let Some(clip) = self.clipboard.as_mut() {
                    clip.origin = saved;
                }
            }
            "edit.canvas_properties" => {
                if let Some(picture) = self.picture() {
                    let arena = &cx.world.arena;
                    let entity = arena.get_by_id(picture).ok();
                    let paper = doc::layers(arena, picture)
                        .into_iter()
                        .find(|info| info.kind == LayerKind::Paper);
                    let paper_color = paper
                        .as_ref()
                        .and_then(|info| arena.get_by_id(info.id).ok())
                        .map(|entity| {
                            let rgb = picture::paper_color(entity)
                                .to_space(sue_core::ColorSpace::Srgb)
                                .rgb;
                            rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
                        });
                    self.dialog =
                        Some(Dialog::CanvasProperties(crate::dialogs::CanvasProperties {
                            picture,
                            name: entity
                                .map(|entity| entity.name().to_owned())
                                .unwrap_or_default(),
                            dpi: entity
                                .and_then(|entity| entity.property(picture::DPI))
                                .and_then(Value::as_f64)
                                .unwrap_or(72.0),
                            paper: paper.as_ref().map(|info| info.id),
                            paper_color: paper_color.unwrap_or([255, 255, 255]),
                            paper_shown: paper.as_ref().is_some_and(|info| info.style.visible),
                        }));
                }
            }
            "edit.brush_tip" | "edit.material.tip" => self.make_brush_tip(cx),
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "edit", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("edit.undo", "undo"),
    c("edit.redo", "redo"),
    c("edit.cut", "cut").keys("Ctrl+X or F2"),
    c("edit.copy", "copy-2").keys("Ctrl+C or F3"),
    c("edit.paste", "paste").keys("Ctrl+V or F4"),
    c("edit.paste_here", "paste-to-shown-position"),
    c("edit.clear", "delete").keys("Ctrl+Delete or Ctrl+Backspace or Delete or Backspace"),
    c("edit.clear_outside", "delete-outside-selection"),
    c("edit.smart_smoothing", "smart-smoothing-ellipsis"),
    c("edit.fill", "fill").keys("Shift+Delete or Shift+Backspace or Alt+Delete or Alt+Backspace"),
    c("edit.advanced_fill", "advanced-fill-ellipsis"),
    c("edit.recolor", "convert-to-drawing-color"),
    c("edit.outline_selection", "outline-selection-ellipsis"),
    c("edit.luma_to_alpha", "convert-brightness-to-opacity"),
    c("edit.transform.scale", "scale-rotate-ellipsis").keys("Ctrl+T"),
    c("edit.transform.scale_only", "scale-ellipsis"),
    c("edit.transform.rotate", "rotate-ellipsis"),
    c("edit.transform.free", "free-transform").keys("Ctrl+Shift+T"),
    c("edit.transform.distort", "distort"),
    c("edit.transform.skew", "skew"),
    c("edit.transform.perspective", "perspective"),
    c("edit.flip_h", "flip-horizontal"),
    c("edit.flip_v", "flip-vertical"),
    c("edit.rotate180", "rotate-180-degrees"),
    c("edit.transform.mesh", "mesh-transformation"),
    c("edit.transform.puppet", "puppet-warp-ellipsis"),
    c("edit.align.left", "align-left"),
    c("edit.align.hcenter", "align-horizontal-center"),
    c("edit.align.right", "align-right"),
    c("edit.align.top", "align-top"),
    c("edit.align.vcenter", "align-vertical-center"),
    c("edit.align.bottom", "align-bottom"),
    c("edit.resolution", "change-image-resolution-ellipsis"),
    c("edit.canvas_size", "change-canvas-size-ellipsis"),
    c("edit.canvas_height", "change-canvas-height-ellipsis"),
    c("edit.crop", "crop"),
    c("edit.canvas_properties", "canvas-properties-ellipsis"),
    c("edit.clear_history", "this-canvas"),
    c("edit.brush_tip", "create-brush-tip-from-image-ellipsis"),
    c("edit.material.image", "image-ellipsis"),
    c("edit.material.tip", "brush-tip-ellipsis"),
    c("edit.transform.cancel", "cancel-transform").with(Flags::HIDDEN),
    c("edit.material.paste", "paste-material").with(Flags::HIDDEN),
];
