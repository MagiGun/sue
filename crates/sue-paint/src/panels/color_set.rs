//! The Color Set palette and its menu.

use egui::{Align2, Color32, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::color;
use crate::icons::Icon;
use crate::panels::{child, icon_button};
use crate::studio::SuePaint;

impl SuePaint {
    pub(crate) fn color_set_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let tabs = [
            (tr!("color-set"), Icon::Swatches, true, None),
            (
                tr!("color-history"),
                Icon::History,
                true,
                Some(Palette::ColorHistory),
            ),
            (
                tr!("material"),
                Icon::Material,
                true,
                Some(Palette::Material),
            ),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.set_tab, theme);
        self.set_tab = tab;
        self.color_set_body(ui, body, cx, theme);
    }

    /// The Color Set palette's tab showing (`set_tab`), in `body`.
    #[allow(clippy::too_many_lines, reason = "the color set and its controls")]
    pub(crate) fn color_set_body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if self.set_tab == 2 {
            self.material_panel(ui, body.shrink(6.0), cx, theme);
            return;
        }
        let body = body.shrink(6.0);
        let mut top = body.top();
        let mut bottom = body.bottom();
        let editing = self.set_tab == 0;
        if editing {
            self.color_sets
                .load(crate::colorsets::folder(cx).as_deref());
            // The set, its settings, import and export.
            let row = Rect::from_min_size(body.min, vec2(body.width(), 22.0));
            let mut inner = child(
                ui,
                Rect::from_min_max(row.min, pos2(row.right() - 78.0, row.bottom())),
                "color-set-pick",
            );
            let names: Vec<String> = self
                .color_sets
                .sets
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            let current = self.color_sets.current.min(names.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("sue-paint-color-set")
                .selected_text(names.get(current).cloned().unwrap_or_default())
                .width((row.width() - 90.0).max(60.0))
                .show_ui(&mut inner, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        if ui
                            .add(egui::Button::selectable(index == current, name))
                            .clicked()
                        {
                            self.color_sets.current = index;
                            self.color_sets.selected = None;
                        }
                    }
                });
            let icons_at = |index: f32| {
                Rect::from_min_size(
                    pos2(row.right() - 76.0 + index * 26.0, row.top()),
                    vec2(22.0, 22.0),
                )
            };
            if icon_button(
                ui,
                icons_at(0.0),
                Icon::Wrench,
                true,
                self.set_menu,
                &tr!("color-set-settings"),
                theme,
            ) {
                self.set_menu = !self.set_menu;
            }
            let me = || sue_wm::AppletId::new(crate::studio::ID).expect("a valid id");
            if icon_button(
                ui,
                icons_at(1.0),
                Icon::Import,
                true,
                false,
                &tr!("import-color-set-gimp-gpl-hex"),
                theme,
            ) {
                cx.request(ShellRequest::PickFile(
                    sue_wm::FilePick::open(
                        Some(me()),
                        "colorset-import",
                        &tr!("import-a-color-set"),
                    )
                    .extensions(&["gpl", "txt", "hex"]),
                ));
            }
            if icon_button(
                ui,
                icons_at(2.0),
                Icon::Export,
                true,
                false,
                &tr!("export-color-set-gimp-gpl"),
                theme,
            ) {
                let name = names
                    .get(current)
                    .cloned()
                    .unwrap_or_else(|| "Colors".to_owned());
                cx.request(ShellRequest::PickFile(
                    sue_wm::FilePick::save(
                        Some(me()),
                        "colorset-export",
                        &tr!("export-the-color-set"),
                        &format!("{name}.gpl"),
                    )
                    .extensions(&["gpl"]),
                ));
            }
            if self.set_menu {
                self.color_set_menu(ui, icons_at(0.0), cx, theme);
            }
            top += 28.0;
            bottom -= 26.0;
        }
        let swatches: Vec<[u8; 3]> = if editing {
            self.color_sets.colors().to_vec()
        } else {
            self.recent.colors().to_vec()
        };
        if swatches.is_empty() {
            ui.painter().text(
                pos2(body.left() + 2.0, top + 10.0),
                Align2::LEFT_CENTER,
                if editing {
                    tr!("empty-add-drawing-color")
                } else {
                    tr!("colors-you-paint-show-here")
                },
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
        }
        let tile = if self.color_sets.tile > 0.0 {
            self.color_sets.tile
        } else {
            (body.width() / 8.0).floor()
        };
        let columns = ((body.width() / tile).floor() as usize).max(1);
        let cell = (body.width() / columns as f32).floor();
        let mut swatch_menu = None;
        for (index, rgb) in swatches.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            let rect = Rect::from_min_size(
                pos2(body.left() + column as f32 * cell, top + row as f32 * cell),
                vec2(cell - 1.0, cell - 1.0),
            );
            if rect.bottom() > bottom {
                break;
            }
            let [r, g, b] = *rgb;
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-swatch", self.set_tab, index)),
                Sense::click(),
            );
            let chosen = editing && self.color_sets.selected == Some(index);
            if response.hovered() || chosen {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(if chosen { 2.0 } else { 1.5 }, theme.border_active),
                    StrokeKind::Inside,
                );
            }
            let response = response.on_hover_text(color::hex(*rgb));
            if response.clicked() {
                self.set_drawing_color(*rgb);
                self.transparent = false;
                if editing {
                    self.color_sets.selected = Some(index);
                }
            }
            if editing {
                response.context_menu(|ui| {
                    for (what, label) in [
                        (0, tr!("add-color")),
                        (1, tr!("replace-color")),
                        (2, tr!("delete-color")),
                    ] {
                        if ui.button(label).clicked() {
                            swatch_menu = Some((what, index));
                            ui.close();
                        }
                    }
                });
            }
        }
        if let Some((what, index)) = swatch_menu {
            let drawing = self.drawing_color();
            if let Some(colors) = self.color_sets.colors_mut() {
                match what {
                    0 => {
                        colors.push(drawing);
                        self.color_sets.selected = Some(colors.len() - 1);
                    }
                    1 if index < colors.len() => colors[index] = drawing,
                    2 if index < colors.len() => {
                        colors.remove(index);
                        self.color_sets.selected = None;
                    }
                    _ => {}
                }
                self.save_color_sets(cx);
            }
        }
        if !editing {
            return;
        }
        // The drawing colors, and adding, replacing and deleting swatches.
        let row = Rect::from_min_max(pos2(body.left(), bottom + 2.0), body.max);
        let main = Rect::from_min_size(row.min + vec2(0.0, 3.0), vec2(14.0, 14.0));
        let sub = Rect::from_min_size(row.min + vec2(18.0, 3.0), vec2(14.0, 14.0));
        for (rect, rgb, chosen) in [
            (main, self.main, !self.sub_active),
            (sub, self.sub, self.sub_active),
        ] {
            let [r, g, b] = rgb;
            ui.painter()
                .rect_filled(rect, 1.0, Color32::from_rgb(r, g, b));
            ui.painter().rect_stroke(
                rect,
                1.0,
                Stroke::new(
                    1.0,
                    if chosen {
                        theme.border_active
                    } else {
                        theme.border
                    },
                ),
                StrokeKind::Outside,
            );
        }
        let at = |index: f32| {
            Rect::from_min_size(
                pos2(row.right() - 74.0 + index * 25.0, row.top()),
                vec2(22.0, 22.0),
            )
        };
        let has = self
            .color_sets
            .selected
            .filter(|index| *index < swatches.len());
        let drawing = self.drawing_color();
        let mut changed = false;
        if icon_button(
            ui,
            at(0.0),
            Icon::Add,
            true,
            false,
            &tr!("add-the-drawing-color"),
            theme,
        ) && let Some(colors) = self.color_sets.colors_mut()
        {
            colors.push(drawing);
            self.color_sets.selected = Some(colors.len() - 1);
            changed = true;
        }
        if icon_button(
            ui,
            at(1.0),
            Icon::Replace,
            has.is_some(),
            false,
            &tr!("replace-picked-swatch-drawing-color"),
            theme,
        ) && let (Some(index), Some(colors)) = (has, self.color_sets.colors_mut())
        {
            colors[index] = drawing;
            changed = true;
        }
        if icon_button(
            ui,
            at(2.0),
            Icon::Delete,
            has.is_some(),
            false,
            &tr!("delete-the-picked-swatch"),
            theme,
        ) && let (Some(index), Some(colors)) = (has, self.color_sets.colors_mut())
        {
            colors.remove(index);
            self.color_sets.selected = None;
            changed = true;
        }
        if changed {
            self.save_color_sets(cx);
        }
    }

    /// The color set's settings: sets and swatch sizes.
    fn color_set_menu(
        &mut self,
        ui: &mut Ui,
        button: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut chosen = None;
        let shown = egui::Area::new(Id::unique("sue-paint-color-set-menu"))
            .order(egui::Order::Tooltip)
            .fade_in(false)
            .fixed_pos(pos2(button.right() - 190.0, button.bottom() + 2.0))
            .show(ui.ctx(), |ui| {
                sue_wm::widgets::window_frame(theme)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(178.0);
                        for (label, key) in [
                            (tr!("new-color-set"), "new"),
                            (tr!("duplicate-this-set"), "duplicate"),
                            (tr!("delete-this-set"), "delete"),
                            (tr!("small-swatches"), "small"),
                            (tr!("medium-swatches"), "medium"),
                            (tr!("large-swatches"), "large"),
                            (tr!("restore-the-built-in-sets"), "restore"),
                        ] {
                            if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                chosen = Some(key);
                            }
                        }
                    });
            });
        let inside = shown.response.rect;
        let outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|point| !inside.contains(point) && !button.contains(point))
        });
        let sets = &mut self.color_sets;
        match chosen {
            Some("new") => sets.add_set(&tr!("my-colors"), Vec::new()),
            Some("duplicate") => {
                let (name, colors) = sets.sets.get(sets.current).cloned().unwrap_or_default();
                sets.add_set(&name, colors);
            }
            Some("delete") if sets.sets.len() > 1 => {
                sets.sets.remove(sets.current);
                sets.current = sets.current.min(sets.sets.len() - 1);
                sets.selected = None;
            }
            Some("small") => sets.tile = 14.0,
            Some("medium") => sets.tile = 0.0,
            Some("large") => sets.tile = 40.0,
            Some("restore") => {
                for builtin in crate::colorsets::builtin() {
                    if !sets.sets.iter().any(|(name, _)| *name == builtin.0) {
                        sets.sets.push(builtin);
                    }
                }
            }
            _ => {}
        }
        if chosen.is_some() {
            self.save_color_sets(cx);
        }
        if chosen.is_some() || outside {
            self.set_menu = false;
        }
    }
}
