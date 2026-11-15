//! The canvas: tabs of open pictures, the turned and zoomed view, every
//! tool's gesture under the pointer or pen, the overlays (selection,
//! shape previews, brush outline, grids) and the status strip below.
//!
//! Pointer input is read as egui's raw event stream, in order, so a fast
//! stroke keeps every sample between frames and a pen's pressure arrives
//! with it (from touch events that carry force).
//!
//! The chrome around the view (the tab strip, the status strip, the
//! scroll bars, canvases in windows of their own and pictures dropped on
//! the canvas) is drawn by `tabs.rs`, `status.rs`, `view.rs` and
//! `window.rs`; what they keep between frames is [`Chrome`].

use egui::{Id, Key, Rect, Sense, Ui, pos2};
use sue_model::picture;
use sue_wm::ui::Density;
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::studio::SuePaint;
use crate::tools::Tool;
use view::{SCROLLBAR, scrollbar};

pub(crate) use window::Chrome;

pub(crate) mod engine;
mod fill;
mod input;
mod lay;
pub(crate) mod marks;
pub(crate) mod lines;
mod overlays;
mod pick;
pub(crate) mod puppet;
mod shape;
pub(crate) use shape::FigureDraft;
mod status;
mod tabs;
mod tools;
mod view;
mod window;

/// What surrounds the view in the canvas column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
    /// Sue Paint's own column: the tab strip over the view.
    Main,
    /// A canvas in a window of its own: no tabs, the window's title names
    /// it.
    Window,
}

impl SuePaint {
    /// The canvas column: tabs, view and status strip.
    pub(crate) fn canvas_area(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        self.keep_windows(cx);
        // While the studio comes in or goes out, the tabs and the status
        // strip slide off with the panels and the view takes their room.
        // Simple Mode shows the canvas bare: no tabs, no status strip.
        let chrome = if self.bare { 0.0 } else { chrome_height(ui) };
        let slide = self.hide * chrome;
        let tabs = Rect::from_min_max(
            pos2(rect.left(), rect.top() - slide),
            pos2(rect.right(), rect.top() - slide + chrome),
        );
        self.take_drops(ui, tabs, cx);
        if !self.bare {
            self.tabs(ui, tabs, cx, theme);
        }
        let below = Rect::from_min_max(pos2(rect.left(), tabs.bottom()), rect.max);
        match self.column_doc() {
            Some(index) if index != self.active => {
                // The current canvas is in a window of its own: the column
                // shows the one it showed last, which a press makes
                // current again.
                self.column_for(index, ui, below, slide, cx, theme);
            }
            Some(_) => self.canvas_column(ui, below, slide, Column::Main, cx, theme),
            None => {
                let status = status_rect(below, chrome, slide);
                let viewport = Rect::from_min_max(below.min, pos2(below.right(), status.top()));
                ui.painter()
                    .rect_filled(viewport, egui::CornerRadius::ZERO, theme.void);
                if self.docs.is_empty() {
                    self.viewport = viewport;
                    self.empty_canvas(ui, viewport, cx, theme);
                } else {
                    self.all_in_windows(ui, viewport, cx, theme);
                }
                if !self.bare {
                    self.status_strip(ui, status, None, Column::Main, cx, theme);
                }
            }
        }
        self.keep_window_viewport();
    }

    /// The view of the current canvas in `rect` with the status strip
    /// under it (`slide` points of it below `rect` while the studio comes
    /// in) and the scroll bars, in Sue Paint's column or in a window of
    /// its own.
    fn canvas_column(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        slide: f32,
        column: Column,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let chrome = if self.bare { 0.0 } else { chrome_height(ui) };
        let status = status_rect(rect, chrome, slide);
        let bars = self.scroll_bars && !self.bare;
        let scroll = if bars { SCROLLBAR } else { 0.0 };
        let mut viewport = Rect::from_min_max(rect.min, pos2(rect.right() - scroll, status.top()));
        if column == Column::Main && self.tiled && self.main_docs().len() > 1 {
            viewport = self.tile_others(ui, viewport, cx, theme);
        }
        self.viewport = viewport;
        ui.painter().rect_filled(
            viewport,
            egui::CornerRadius::ZERO,
            theme.void.gamma_multiply(self.cover),
        );
        let size = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(picture::size);
        let Some(size) = size else {
            self.empty_canvas(ui, viewport, cx, theme);
            if !self.bare {
                self.status_strip(ui, status, None, column, cx, theme);
            }
            return;
        };
        let (hide, ready, leaving) = (self.hide, self.ready, self.leaving());
        if let Some(doc) = self.doc_mut() {
            if !ready && hide > 0.0 && !leaving {
                // Coming in, the picture fits the room the palettes leave
                // as they slide in; going out, it stays as it was.
                doc.view.fit(viewport, size);
            } else if !doc.view.fitted {
                doc.view.fit(viewport, size);
            }
        }
        let salt = match column {
            Column::Main => Id::unique("sue-paint-canvas"),
            Column::Window => Id::unique((
                "sue-paint-canvas-window",
                self.doc().and_then(|doc| doc.window),
            )),
        };
        let response = ui.interact(viewport, salt, Sense::click_and_drag());
        self.canvas_input(ui, viewport, &response, size, cx);
        self.draw_picture(ui, viewport, size, cx, theme);
        self.tone_overlay(ui, viewport, size, cx);
        self.page_frames(ui, viewport, size, cx, theme);
        self.overlays(ui, viewport, size, &response, cx, theme);
        self.view_marks(ui, viewport, size, cx, theme);
        self.typing_ui(ui, viewport, size, cx);
        self.warp_ui(ui, viewport, size, cx, theme);
        self.selection_launcher(ui, viewport, size, cx, theme);
        self.text_launcher(ui, viewport, size, cx, theme);
        self.object_launcher(ui, viewport, size, cx, theme);
        self.light_table_launcher(ui, viewport, size, cx, theme);
        if self.ruler_bar {
            self.ruler_bars(ui, viewport, size, theme);
        }
        if !self.polyline.is_empty() {
            let (enter, escape) = ui.input(|input| {
                (
                    input.key_pressed(Key::Enter),
                    input.key_pressed(Key::Escape),
                )
            });
            // The corners belong to the sub tool that started them.
            let owner = self.polyline_for.unwrap_or((Tool::Select, 3));
            let now = (self.toolbox.tool(), self.toolbox.current().mode);
            let least = if owner.0 == Tool::Figure { 2 } else { 3 };
            if enter && self.polyline.len() >= least {
                self.finish_corners(self.select_op, cx);
            } else if escape || now != owner {
                self.polyline.clear();
                self.polyline_for = None;
            }
        }
        if let Some(bend) = &self.figure_draft.bend {
            // A Curve waiting for its bend: Esc, another sub tool, or
            // another picture drops it.
            let escape = ui.input(|input| input.key_pressed(Key::Escape));
            let now = (self.toolbox.tool(), self.toolbox.current().mode);
            let elsewhere = self.picture() != Some(bend.picture);
            if escape || elsewhere || now != (Tool::Figure, crate::tools::CURVE) {
                self.figure_draft.bend = None;
            }
        }
        if !self.curve_points.is_empty() {
            // Enter lays the curve ruler, Esc drops it.
            let (enter, escape) = ui.input(|input| {
                (
                    input.key_pressed(Key::Enter),
                    input.key_pressed(Key::Escape),
                )
            });
            if enter {
                self.finish_curve(cx);
            } else if escape || self.toolbox.tool() != Tool::Ruler {
                self.curve_points.clear();
            }
        }
        if self.transform.is_some() {
            self.transform_ui(ui, viewport, size, cx, theme);
        }
        if !self.bare {
            self.status_strip(ui, status, Some(size), column, cx, theme);
        }
        if bars {
            let bar = Rect::from_min_max(
                pos2(viewport.right(), viewport.top()),
                pos2(viewport.right() + SCROLLBAR, viewport.bottom()),
            );
            let salt = match column {
                Column::Main => "sue-paint-scroll-v",
                Column::Window => "sue-paint-scroll-v-window",
            };
            if let Some(doc) = self.docs.get_mut(self.active) {
                let extent = size[1] as f32 * doc.view.zoom;
                if let Some(pan) = scrollbar(
                    ui,
                    bar,
                    false,
                    doc.view.pan.y,
                    extent,
                    viewport.height(),
                    salt,
                    theme,
                ) {
                    doc.view.pan.y = pan;
                }
            }
        }
    }

    /// What the canvas area shows while no canvas is open: the empty
    /// state of Sue's design system, with New and Open. On a touch
    /// screen New opens the New canvas page; on the desktop the New
    /// dialog.
    fn empty_canvas(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = child(ui, viewport, "empty-canvas");
        inner.multiply_opacity(self.cover);
        if !self.ready {
            inner.disable();
        }
        let title = tr!("no-canvas-is-open");
        let detail = tr!("pictures-you-make-here-are-saved");
        let (new, open) = (tr!("new-ellipsis"), tr!("open-ellipsis"));
        let mut run = None;
        sue_wm::ui::empty_state("sue-paint-no-canvas", &title)
            .icon(sue_wm::icons::Icon::Picture)
            .detail(&detail)
            .show_actions(&mut inner, theme, |ui| {
                if sue_wm::ui::button("sue-paint-empty-new", &new)
                    .primary()
                    .icon(sue_wm::icons::Icon::FileNew)
                    .show(ui, theme)
                    .clicked()
                {
                    run = Some("new");
                }
                if sue_wm::ui::button("sue-paint-empty-open", &open)
                    .icon(sue_wm::icons::Icon::FileOpen)
                    .show(ui, theme)
                    .clicked()
                {
                    run = Some("file.open");
                }
            });
        match run {
            Some("new") if cx.view.form_factor != sue_platform::FormFactor::Desktop => {
                self.new_canvas.category = Some(0);
            }
            Some("new") => self.run("file.new", cx),
            Some(command) => self.run(command, cx),
            None => {}
        }
    }
}

/// How tall the tab strip and the status strip are: a compact strip on
/// the desktop, a finger's on a touch screen.
fn chrome_height(ui: &Ui) -> f32 {
    density(ui).tab_strip()
}

/// The density of the canvas chrome: compact as the palettes around it,
/// finger sized on a touch screen.
fn density(ui: &Ui) -> Density {
    if sue_wm::widgets::is_touch(ui.ctx()) {
        Density::Touch
    } else {
        Density::Compact
    }
}

/// The status strip at the bottom of `column`, `chrome` tall, `slide`
/// points of it below while the studio comes in.
fn status_rect(column: Rect, chrome: f32, slide: f32) -> Rect {
    Rect::from_min_max(
        pos2(column.left(), column.bottom() - chrome + slide),
        pos2(column.right(), column.bottom() + slide),
    )
}

#[cfg(test)]
mod tests {
    use super::input::{lean_in_picture, tilt_of};
    use egui::Pos2;

    #[test]
    fn the_pens_lean_turns_and_mirrors_with_the_view() {
        let near =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        let at = egui::pos2(40.0, 30.0);
        // Zoomed only: the same way, the same length.
        let zoomed = |p: Pos2| egui::pos2(p.x * 3.0, p.y * 3.0);
        assert!(near(
            lean_in_picture(Some([0.3, -0.4]), at, zoomed),
            [0.3, -0.4]
        ));
        // A view flipped left to right mirrors it.
        let flipped = |p: Pos2| egui::pos2(500.0 - p.x, p.y);
        assert!(near(
            lean_in_picture(Some([0.5, 0.0]), at, flipped),
            [-0.5, 0.0]
        ));
        // A view turned a quarter: leaning down on screen leans the
        // picture's way that the screen's down lands on.
        let turned = |p: Pos2| egui::pos2(-p.y, p.x);
        assert!(near(
            lean_in_picture(Some([0.0, 0.5]), at, turned),
            [-0.5, 0.0]
        ));
        // Upright or unknown: no way at all; how far is tilt's.
        assert_eq!(lean_in_picture(None, at, zoomed), [0.0, 0.0]);
        assert_eq!(lean_in_picture(Some([0.0, 0.0]), at, zoomed), [0.0, 0.0]);
        assert_eq!(tilt_of(None), 0.0);
        assert!((tilt_of(Some([0.6, 0.8])) - 1.0).abs() < 1e-6);
    }
}
