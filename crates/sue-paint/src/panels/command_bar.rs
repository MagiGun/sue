//! The command bar: its groups of buttons and how they are drawn.

use egui::{Id, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::icons::Icon;
use crate::panels::{Flow, icon_button};
use crate::studio::SuePaint;

/// The command bar's buttons, in groups: icon, command, tip.
pub(crate) const BAR_GROUPS: [&[(Icon, &str, &str)]; 5] = [
    &[
        (Icon::New, "file.new", "New (Ctrl+N)"),
        (Icon::Open, "file.open", "Open (Ctrl+O)"),
        (Icon::Save, "file.save", "Save Project (Ctrl+S)"),
    ],
    &[
        (Icon::Undo, "edit.undo", "Undo (Ctrl+Z)"),
        (Icon::Redo, "edit.redo", "Redo (Ctrl+Y)"),
    ],
    &[
        (Icon::Clear, "edit.clear", "Clear (Del)"),
        (
            Icon::ClearOutside,
            "edit.clear_outside",
            "Clear Outside Selection",
        ),
        (Icon::FillSelection, "edit.fill", "Fill (Alt+Del)"),
        (Icon::Deselect, "select.none", "Deselect (Ctrl+D)"),
        (
            Icon::MoveLayer,
            "edit.transform.scale",
            "Scale/Rotate (Ctrl+T)",
        ),
    ],
    &[
        (Icon::FlipH, "view.flip_h", "Flip View Horizontally"),
        (Icon::RotateLeft, "view.rotate_left", "Rotate View Left"),
        (Icon::RotateRight, "view.rotate_right", "Rotate View Right"),
        (Icon::Fit, "view.fit", "Fit to Screen (Ctrl+0)"),
    ],
    &[
        (Icon::Ruler, "view.snap_ruler", "Snap to Ruler"),
        (Icon::Frame, "view.snap_grid", "Snap to Grid"),
        (Icon::Reference, "help.shortcuts", "Keyboard Shortcuts"),
        (Icon::Help, "help.about", "About Sue Paint"),
        (Icon::Search, "app.palette", "Search every command"),
    ],
];

impl SuePaint {
    pub(crate) fn command_bar(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.panel);
        ui.painter()
            .hline(strip.x_range(), strip.bottom(), theme.hairline());
        // A palette like the others: its grip at the left (drawn by the
        // dock), its menu button at the right, and its buttons in lines
        // that wrap when it is narrower than they are (in a column, or a
        // taller top row).
        let groups = BAR_GROUPS;
        let mut flow = Flow::new(strip.shrink2(vec2(0.0, 3.0)), 16.0, 24.0, 34.0);
        let mut clicked: Option<String> = None;
        let mut save_at = None;
        for (index, group) in groups.iter().enumerate() {
            if index > 0
                && let Some(x) = flow.gap(8.0)
            {
                ui.painter()
                    .vline(x + 3.0, (flow.y + 5.0)..=(flow.y + 27.0), theme.hairline());
            }
            for (icon, command, tip) in group.iter() {
                if self.bar_hidden.contains(*command) {
                    continue;
                }
                let Some(rect) = flow.next(vec2(32.0, 32.0), 34.0) else {
                    break;
                };
                if *command == "file.save" {
                    save_at = Some(rect);
                }
                let state = self.state(command, &sue_wm::Facts::of(cx));
                if icon_button(
                    ui,
                    rect,
                    *icon,
                    state.enabled,
                    state.checked == Some(true),
                    &crate::localize(tip),
                    theme,
                ) {
                    clicked = Some((*command).to_owned());
                }
            }
        }
        // Commands people added (Command Bar Settings), as text buttons.
        let font = theme.font(theme.small_size + 0.5);
        for command in &self.bar_extra {
            let label = crate::menus::label_of(&self.menus, command)
                .unwrap_or(command)
                .trim_end_matches("...")
                .to_owned();
            let galley = ui.painter().layout_no_wrap(label, font.clone(), theme.text);
            let Some(rect) = flow.next(vec2(galley.size().x + 14.0, 26.0), galley.size().x + 20.0)
            else {
                break;
            };
            let rect = rect.translate(vec2(4.0, 1.0));
            let state = self.state(command, &sue_wm::Facts::of(cx));
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-bar", command.as_str())),
                Sense::click(),
            );
            if response.hovered() && state.enabled {
                ui.painter().rect_filled(rect, 3.0, theme.hover);
            }
            let color = if state.enabled {
                theme.text
            } else {
                theme.muted
            };
            ui.painter()
                .galley(rect.center() - galley.size() / 2.0, galley, color);
            if response.clicked() && state.enabled {
                clicked = Some(command.clone());
            }
        }
        // Save's menu: the project, or the picture as a file.
        let Some(save_button) = save_at else {
            match clicked.as_deref() {
                Some("app.palette") => cx.request(ShellRequest::Palette),
                Some(command) => self.run(command, cx),
                None => {}
            }
            return;
        };
        let save = Rect::from_min_size(
            pos2(save_button.right() - 2.0, save_button.top() + 6.0),
            vec2(10.0, 16.0),
        );
        let response = ui.interact(save, Id::unique("sue-paint-save-menu"), Sense::click());
        let painter = ui.painter();
        painter.add(Shape::convex_polygon(
            vec![
                pos2(save.left() + 1.0, save.center().y - 2.0),
                pos2(save.right() - 1.0, save.center().y - 2.0),
                pos2(save.center().x, save.center().y + 3.0),
            ],
            if response.hovered() {
                theme.text
            } else {
                theme.muted
            },
            Stroke::NONE,
        ));
        let response = response.on_hover_text(tr!("save-or-export"));
        egui::Popup::menu(&response).show(|ui| {
            for (label, command) in [
                (tr!("save-project-ctrl-s"), "file.save"),
                (tr!("save-project-as-ctrl-shift-s"), "file.save_as"),
                (tr!("export-as-png"), "file.export.png"),
                (tr!("export-as-jpeg"), "file.export.jpg"),
                (tr!("export-as-jpeg-xl"), "file.export.jxl"),
            ] {
                if ui.button(label).clicked() {
                    clicked = Some(command.to_owned());
                }
            }
        });
        match clicked.as_deref() {
            Some("app.palette") => cx.request(ShellRequest::Palette),
            Some(command) => self.run(command, cx),
            None => {}
        }
    }
}
