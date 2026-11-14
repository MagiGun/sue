//! The View menu's commands.

use sue_core::{Arena, UndoStack, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, ItemState, World};

use super::{Basis, flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::studio::SuePaint;

/// Does what a View command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_view(call.name, cx);
}

/// How an item for a View command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_view(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a View command looks right now.
    fn state_view(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "view.transform_launcher" => flag(self.transform_launcher),
            "view.text_launcher" => flag(self.text_launcher),
            "view.object_launcher" => flag(self.object_launcher),
            "view.grid" => flag(self.grid),
            "view.snap_special" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.ruler_on)),
            "view.snap_ruler" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.snap_ruler)),
            "view.pixel_grid" => flag(self.pixel_grid),
            "view.selection" => flag(self.show_selection),
            "view.flip_h" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.view.flip_h)),
            "view.flip_v" => self
                .doc()
                .map_or(ItemState::off(), |doc| flag(doc.view.flip_v)),
            "view.ruler_bar" => flag(self.ruler_bar),
            "view.scroll_bar" => flag(self.scroll_bars),
            "view.snap_grid" => flag(self.snap_grid),
            "view.paper" => match self.picture().and_then(|picture| {
                doc::layers(arena, picture)
                    .into_iter()
                    .find(|info| info.kind == LayerKind::Paper)
            }) {
                Some(paper) => flag(paper.style.visible),
                None => ItemState::off(),
            },
            "view.crop_frame" => flag(self.crop_frame),
            "view.bleed" => flag(self.bleed),
            "view.page_image" => flag(self.page_image),
            "view.tone_area" => flag(self.tone_area),
            "view.profile_preview" => flag(self.proof_on),
            _ => when(has_doc),
        }
    }

    /// Does what a View command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_view(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "view.snap_special" => {
                if let Some(doc) = self.doc_mut() {
                    doc.ruler_on = !doc.ruler_on;
                }
            }
            "view.snap_ruler" => {
                if let Some(doc) = self.doc_mut() {
                    doc.snap_ruler = !doc.snap_ruler;
                }
            }
            "view.rotate_90" | "view.rotate_180" | "view.rotate_270" => {
                let angle = match command {
                    "view.rotate_90" => 90.0,
                    "view.rotate_180" => 180.0,
                    _ => 270.0,
                };
                self.with_view(cx, |view, rect, size| view.set_rotation(rect, size, angle));
            }
            "view.reset_turn" | "view.reset_display" => {
                let fit = command == "view.reset_display";
                self.with_view(cx, |view, rect, size| {
                    view.set_rotation(rect, size, 0.0);
                    if view.flip_h {
                        view.flip(rect, size, true);
                    }
                    if view.flip_v {
                        view.flip(rect, size, false);
                    }
                    if fit {
                        view.fit(rect, size);
                    }
                });
            }
            "view.paper" => {
                let paper = self.picture().and_then(|picture| {
                    doc::layers(&cx.world.arena, picture)
                        .into_iter()
                        .find(|info| info.kind == LayerKind::Paper)
                });
                if let Some(paper) = paper {
                    Self::apply(
                        cx,
                        doc::set(paper.id, picture::VISIBLE, !paper.style.visible),
                    );
                }
            }
            "view.transform_launcher" => self.transform_launcher = !self.transform_launcher,
            "view.text_launcher" => self.text_launcher = !self.text_launcher,
            "view.object_launcher" => self.object_launcher = !self.object_launcher,
            "view.rotate_left" | "view.rotate_right" => {
                // Preferences, Canvas: the angle each turn takes.
                let step = cx
                    .rice()
                    .get_i64("sue_paint.rotate_step")
                    .unwrap_or(15)
                    .clamp(1, 90) as f32;
                let step = if command == "view.rotate_left" {
                    -step
                } else {
                    step
                };
                self.with_view(cx, |view, rect, size| view.rotate_by(rect, size, step));
            }
            "view.rotate_reset" => {
                self.with_view(cx, |view, rect, size| view.set_rotation(rect, size, 0.0))
            }
            "view.flip_h" => self.with_view(cx, |view, rect, size| view.flip(rect, size, true)),
            "view.flip_v" => self.with_view(cx, |view, rect, size| view.flip(rect, size, false)),
            "view.zoom_in" => self.with_view(cx, |view, rect, size| {
                view.zoom_at(rect, size, rect.center(), 1.25);
            }),
            "view.zoom_out" => self.with_view(cx, |view, rect, size| {
                view.zoom_at(rect, size, rect.center(), 0.8);
            }),
            "view.actual" => self.with_view(cx, |view, rect, size| {
                view.set_zoom_at(rect, size, rect.center(), 1.0);
            }),
            "view.double" => self.with_view(cx, |view, rect, size| {
                view.set_zoom_at(rect, size, rect.center(), 2.0);
            }),
            "view.fit" => self.with_view(cx, |view, rect, size| view.fit(rect, size)),
            "view.grid" => self.grid = !self.grid,
            "view.ruler_bar" => self.ruler_bar = !self.ruler_bar,
            "view.scroll_bar" => self.scroll_bars = !self.scroll_bars,
            "view.snap_grid" => self.snap_grid = !self.snap_grid,
            "view.crop_frame" => self.crop_frame = !self.crop_frame,
            "view.bleed" => self.bleed = !self.bleed,
            "view.page_image" => self.page_image = !self.page_image,
            "view.tone_area" => self.tone_area = !self.tone_area,
            "view.profile_preview" => self.proof_on = !self.proof_on,
            "view.profile_settings" => self.dialog = Some(Dialog::Proof(self.proof_profile)),
            "view.fit_navigator" => self.with_view(cx, |view, rect, size| {
                // The canvas fills the window, as the navigator shows it.
                let zoom = (rect.width() / size[0].max(1) as f32)
                    .max(rect.height() / size[1].max(1) as f32);
                view.set_rotation(rect, size, 0.0);
                view.set_zoom_at(rect, size, rect.center(), zoom);
                let middle = egui::pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
                view.center_on(rect, size, middle);
            }),
            "view.print_size" => {
                let dpi = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .and_then(|entity| entity.property(picture::DPI))
                    .and_then(Value::as_f64)
                    .unwrap_or(72.0) as f32;
                // A screen shows about 96 points an inch.
                let zoom = 96.0 / dpi.max(1.0);
                self.with_view(cx, |view, rect, size| {
                    view.set_zoom_at(rect, size, rect.center(), zoom)
                });
            }
            "view.pixel_grid" => self.pixel_grid = !self.pixel_grid,
            "view.selection" => self.show_selection = !self.show_selection,
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "view", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("view.reset_turn", "reset-rotation-inversion"),
    c("view.rotate_reset", "reset-rotation-2").keys("Ctrl+Shift+2"),
    c("view.rotate_90", "rotate-90"),
    c("view.rotate_180", "rotate-180"),
    c("view.rotate_270", "rotate-270"),
    c("view.flip_h", "flip-horizontal"),
    c("view.flip_v", "flip-vertical"),
    c("view.rotate_left", "rotate-left-2").keys("-"),
    c("view.rotate_right", "rotate-right-2").keys("="),
    c("view.zoom_in", "zoom-in-2").keys("Ctrl++ or Ctrl+= or Ctrl+;"),
    c("view.zoom_out", "zoom-out-2").keys("Ctrl+-"),
    c("view.actual", "zoom-100-percent").keys("Ctrl+Alt+0"),
    c("view.double", "zoom-200-percent"),
    c("view.fit", "fit-to-screen").keys("Ctrl+0"),
    c("view.reset_display", "reset-display"),
    c("view.fit_navigator", "fit-to-navigator"),
    c("view.print_size", "print-size-2"),
    c("view.scroll_bar", "scroll-bar"),
    c("view.selection", "selection-border"),
    c("view.transform_launcher", "transform-launcher"),
    c("view.text_launcher", "text-launcher"),
    c("view.object_launcher", "object-launcher"),
    c("view.grid", "grid"),
    c("view.ruler_bar", "ruler-bar").keys("Ctrl+R"),
    c("view.pixel_grid", "pixel-grid"),
    c("view.crop_frame", "crop-marks-inner-border"),
    c("view.bleed", "bleed-2"),
    c("view.page_image", "show-page-image"),
    c("view.paper", "paper-2"),
    c("view.tone_area", "show-tone-area"),
    c("view.snap_ruler", "snap-to-ruler").keys("Ctrl+1"),
    c("view.snap_special", "snap-to-special-ruler").keys("Ctrl+2"),
    c("view.snap_grid", "snap-to-grid").keys("Ctrl+3"),
    c("view.profile_preview", "preview"),
    c("view.profile_settings", "preview-settings-ellipsis"),
];
