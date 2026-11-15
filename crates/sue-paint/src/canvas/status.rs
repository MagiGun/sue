//! The status strip below the canvas: the zoom and the turn of the view,
//! each a field, a wedge and its buttons, then the horizontal scroll bar,
//! and at the end the button that moves the canvas into a window of its
//! own (or, in that window, back into the tabs).

use egui::{Rect, Ui, Vec2, pos2};
use sue_wm::ui::{Space, Tip, number_field, tooltip, wedge};
use sue_wm::{AppletContext, Theme};

use super::tabs::keyed_icon_button;
use super::view::{SCROLLBAR, scrollbar};
use super::{Column, density};
use crate::commands::registry;
use crate::studio::SuePaint;

/// The buttons after the zoom: their command and icon.
const ZOOM_BUTTONS: [(&str, sue_wm::icons::Icon); 3] = [
    ("view.zoom_out", sue_wm::icons::Icon::ZoomOut),
    ("view.zoom_in", sue_wm::icons::Icon::ZoomIn),
    ("view.fit", sue_wm::icons::Icon::ZoomFit),
];

/// The buttons after the turn: their command and icon.
const TURN_BUTTONS: [(&str, sue_wm::icons::Icon); 4] = [
    ("view.rotate_left", sue_wm::icons::Icon::RotateLeft),
    ("view.rotate_right", sue_wm::icons::Icon::RotateRight),
    ("view.rotate_reset", sue_wm::icons::Icon::Reset),
    ("view.flip_h", sue_wm::icons::Icon::FlipHorizontal),
];

impl SuePaint {
    #[allow(clippy::too_many_arguments, reason = "a strip of the column")]
    pub(super) fn status_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        size: Option<[u32; 2]>,
        column: Column,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(strip, egui::CornerRadius::ZERO, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.top(), theme.hairline());
        let Some(size) = size else {
            return;
        };
        let viewport = self.viewport;
        let center = viewport.center();
        let density = density(ui);
        let gap = theme.space(Space::S2);
        let side = density.icon_button().min(strip.height());
        let height = density.control_height().min(strip.height());
        let field = side * 2.5;
        let reach = side * 2.5;
        let mut inner = crate::panels::child(ui, strip, "status-strip");
        let mut x = strip.left() + theme.space(Space::S4);
        let mut next = |width: f32, tall: f32| {
            let rect = Rect::from_min_size(
                pos2(x, strip.center().y - tall / 2.0),
                Vec2::new(width, tall),
            );
            x = rect.right() + gap;
            rect
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let (mut percent, mut rotation, flipped) = (
            f64::from(doc.view.zoom * 100.0),
            f64::from(doc.view.rotation),
            doc.view.flip_h,
        );
        let (zoom_name, turn_name) = (tr!("zoom"), tr!("rotation"));
        let mut run = None;
        let (zoomed, turned) = density.scope(&mut inner, |ui| {
            let mut zoomed = number_field("sue-paint-status-zoom", &mut percent)
                .range(1.0..=6400.0)
                .decimals(1)
                .suffix("%")
                .label(&zoom_name)
                .show_at(ui, next(field, height), theme)
                .changed();
            let response = wedge("sue-paint-status-zoom-wedge", &mut percent, 1.0..=6400.0)
                .logarithmic()
                .label(&zoom_name)
                .show_at(ui, next(reach, height), theme);
            zoomed |= tooltip(response, &Tip::new(&zoom_name), theme).changed();
            for (command, icon) in ZOOM_BUTTONS {
                if self.status_button(ui, next(side, side), command, icon, None, theme) {
                    run = Some(command);
                }
            }
            let _ = next(0.0, height);
            let mut turned = number_field("sue-paint-status-turn", &mut rotation)
                .range(-180.0..=180.0)
                .decimals(1)
                .suffix("\u{b0}")
                .label(&turn_name)
                .show_at(ui, next(field, height), theme)
                .changed();
            let response = wedge("sue-paint-status-turn-wedge", &mut rotation, -180.0..=180.0)
                .label(&turn_name)
                .show_at(ui, next(reach, height), theme);
            turned |= tooltip(response, &Tip::new(&turn_name), theme).changed();
            for (command, icon) in TURN_BUTTONS {
                let on = (command == "view.flip_h").then_some(flipped);
                if self.status_button(ui, next(side, side), command, icon, on, theme) {
                    run = Some(command);
                }
            }
            (zoomed, turned)
        });
        let controls = x;
        if let Some(doc) = self.docs.get_mut(self.active) {
            if zoomed {
                doc.view
                    .set_zoom_at(viewport, size, center, percent as f32 / 100.0);
            }
            if turned {
                doc.view.set_rotation(viewport, size, rotation as f32);
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
        // At the end: the canvas into a window of its own, or back.
        let float = Rect::from_center_size(
            pos2(
                strip.right() - theme.space(Space::S2) - side / 2.0,
                strip.center().y,
            ),
            Vec2::splat(side),
        );
        let (name, icon) = match column {
            Column::Main => (
                registry().label("window.float").unwrap_or_default(),
                sue_wm::icons::Icon::External,
            ),
            Column::Window => (tr!("back-to-tabs"), sue_wm::icons::Icon::WindowRestore),
        };
        let keys = self.shortcut_of("window.float");
        if !self.bare
            && keyed_icon_button(
                ui,
                float,
                "sue-paint-status-float",
                icon,
                (&name, &keys),
                theme,
            )
            .clicked()
        {
            self.toggle_canvas_window(cx);
        }
        let bar = Rect::from_min_max(
            pos2(controls + gap, strip.center().y - SCROLLBAR / 2.0),
            pos2(float.left() - gap, strip.center().y + SCROLLBAR / 2.0),
        );
        let salt = match column {
            Column::Main => "sue-paint-scroll-h",
            Column::Window => "sue-paint-scroll-h-window",
        };
        if bar.width() > side * 2.0
            && self.scroll_bars
            && let Some(doc) = self.docs.get_mut(self.active)
        {
            let extent = size[0] as f32 * doc.view.zoom;
            if let Some(pan) = scrollbar(
                ui,
                bar,
                true,
                doc.view.pan.x,
                extent,
                viewport.width(),
                salt,
                theme,
            ) {
                doc.view.pan.x = pan;
            }
        }
    }

    /// A button of the status strip that runs `command`, named and keyed
    /// as the menus name it; lit while `on` says so. `true` when clicked.
    fn status_button(
        &self,
        ui: &Ui,
        rect: Rect,
        command: &str,
        icon: sue_wm::icons::Icon,
        on: Option<bool>,
        theme: &Theme,
    ) -> bool {
        let name = registry().label(command).unwrap_or_default();
        let keys = self.shortcut_of(command);
        let salt = format!("sue-paint-status-{command}");
        match on {
            Some(on) => {
                let response = sue_wm::ui::icon_button(salt.as_str(), icon, "")
                    .selected(on)
                    .show_at(ui, rect, theme);
                let label = name.clone();
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), on, &label)
                });
                tooltip(response, &Tip::new(&name).shortcut(&keys), theme).clicked()
            }
            None => keyed_icon_button(ui, rect, &salt, icon, (&name, &keys), theme).clicked(),
        }
    }
}
