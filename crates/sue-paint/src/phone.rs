//! Sue Paint on a phone (and on any device set to the phone interface):
//! the canvas fills the screen between a top bar and a bottom bar of large
//! buttons, and the palettes come up as sheets from the bottom, one at a
//! time. The tools, layers and colors are the same ones the desktop
//! layout shows; only where they sit changes.

use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::icons::Icon;
use crate::panels::{icon_button, tool_icon};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// What a sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sheet {
    /// The menus, as a list.
    Menu,
    /// Every tool, as a grid.
    Tools,
    /// Sub tools and their properties.
    ToolSettings,
    /// Layers.
    Layers,
    /// The color wheel and the color set.
    Color,
    /// The animation timeline.
    Timeline,
}

/// The tools the bottom bar shows first, the way people reach for them.
const BAR_TOOLS: [Tool; 9] = [
    Tool::Pen,
    Tool::Pencil,
    Tool::Brush,
    Tool::Eraser,
    Tool::Fill,
    Tool::Select,
    Tool::MoveLayer,
    Tool::Eyedropper,
    Tool::Hand,
];

/// A thumb-sized button.
const BUTTON: f32 = 44.0;

impl SuePaint {
    /// The whole studio laid out for a phone.
    pub(crate) fn phone_frame(
        &mut self,
        ui: &mut Ui,
        full: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let top = Rect::from_min_size(full.min, vec2(full.width(), BUTTON + 8.0));
        let bottom = Rect::from_min_max(pos2(full.left(), full.bottom() - BUTTON - 16.0), full.max);
        let middle = Rect::from_min_max(
            pos2(full.left(), top.bottom()),
            pos2(full.right(), bottom.top()),
        );
        self.canvas_area(ui, middle, cx, theme);
        let mut run: Option<String> = None;

        // The top bar: menus, undo and redo; the palettes; the way out.
        ui.painter().rect_filled(top, 0.0, theme.panel);
        ui.painter()
            .hline(top.x_range(), top.bottom(), theme.hairline());
        let slot = |index: usize, from_right: bool| {
            let x = if from_right {
                top.right() - 6.0 - (index as f32 + 1.0) * (BUTTON + 2.0)
            } else {
                top.left() + 6.0 + index as f32 * (BUTTON + 2.0)
            };
            Rect::from_min_size(pos2(x, top.top() + 4.0), vec2(BUTTON, BUTTON))
        };
        let history = (cx.history.can_undo(), cx.history.can_redo());
        if icon_button(
            ui,
            slot(0, false),
            Icon::Menu,
            true,
            self.sheet == Some(Sheet::Menu),
            "Menus",
            theme,
        ) {
            self.toggle_sheet(Sheet::Menu);
        }
        if icon_button(
            ui,
            slot(1, false),
            Icon::Undo,
            history.0,
            false,
            "Undo (two-finger tap)",
            theme,
        ) {
            run = Some("edit.undo".to_owned());
        }
        if icon_button(
            ui,
            slot(2, false),
            Icon::Redo,
            history.1,
            false,
            "Redo (three-finger tap)",
            theme,
        ) {
            run = Some("edit.redo".to_owned());
        }
        let right = [
            (Icon::Canvas, None, "Back to the canvas"),
            (Icon::Layers, Some(Sheet::Layers), "Layers"),
            (Icon::Wheel, Some(Sheet::Color), "Color"),
            (Icon::Wrench, Some(Sheet::ToolSettings), "Tool settings"),
            (Icon::Play, Some(Sheet::Timeline), "Timeline"),
        ];
        let fits = ((top.width() - 3.0 * (BUTTON + 2.0) - 12.0) / (BUTTON + 2.0))
            .floor()
            .max(1.0) as usize;
        for (index, (icon, sheet, tip)) in right.iter().enumerate().take(fits) {
            let active = sheet.is_some() && self.sheet == *sheet;
            if icon_button(ui, slot(index, true), *icon, true, active, tip, theme) {
                match sheet {
                    Some(sheet) => self.toggle_sheet(*sheet),
                    None => cx.request(ShellRequest::CloseStudio),
                }
            }
        }

        // The bottom bar: the main tools, then More, the color and the size.
        ui.painter().rect_filled(bottom, 0.0, theme.panel);
        ui.painter()
            .hline(bottom.x_range(), bottom.top(), theme.hairline());
        let chip = Rect::from_min_size(
            pos2(bottom.right() - BUTTON - 8.0, bottom.top() + 8.0),
            vec2(BUTTON, BUTTON),
        );
        let room = ((chip.left() - bottom.left() - 8.0) / (BUTTON + 2.0))
            .floor()
            .max(2.0) as usize;
        let shown = room.saturating_sub(1).min(BAR_TOOLS.len());
        let current = self.toolbox.tool();
        for (index, tool) in BAR_TOOLS.iter().take(shown).enumerate() {
            let rect = Rect::from_min_size(
                pos2(
                    bottom.left() + 8.0 + index as f32 * (BUTTON + 2.0),
                    bottom.top() + 8.0,
                ),
                vec2(BUTTON, BUTTON),
            );
            if icon_button(
                ui,
                rect,
                tool_icon(*tool),
                tool.available(),
                *tool == current,
                tool.label(),
                theme,
            ) {
                if *tool == current {
                    self.toggle_sheet(Sheet::ToolSettings);
                } else {
                    self.toolbox.pick(*tool);
                }
            }
        }
        let more = Rect::from_min_size(
            pos2(
                bottom.left() + 8.0 + shown as f32 * (BUTTON + 2.0),
                bottom.top() + 8.0,
            ),
            vec2(BUTTON, BUTTON),
        );
        let others = !BAR_TOOLS[..shown].contains(&current);
        if icon_button(
            ui,
            more,
            Icon::List,
            true,
            others || self.sheet == Some(Sheet::Tools),
            "Every tool",
            theme,
        ) {
            self.toggle_sheet(Sheet::Tools);
        }
        // The main color, which opens the color sheet.
        let [r, g, b] = self.main;
        ui.painter()
            .rect_filled(chip.shrink(4.0), 6.0, Color32::from_rgb(r, g, b));
        ui.painter().rect_stroke(
            chip.shrink(4.0),
            6.0,
            Stroke::new(1.0, theme.border),
            egui::StrokeKind::Outside,
        );
        if ui
            .interact(chip, Id::unique("sue-paint-phone-color"), Sense::click())
            .on_hover_text("Color")
            .clicked()
        {
            self.toggle_sheet(Sheet::Color);
        }

        if let Some(sheet) = self.sheet {
            self.phone_sheet(ui, middle, sheet, cx, theme, &mut run);
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    fn toggle_sheet(&mut self, sheet: Sheet) {
        self.sheet = if self.sheet == Some(sheet) {
            None
        } else {
            Some(sheet)
        };
    }

    /// A sheet over the bottom of `area`, with a grabber to close it.
    fn phone_sheet(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        sheet: Sheet,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let height = match sheet {
            Sheet::Menu | Sheet::Layers | Sheet::ToolSettings => area.height() * 0.75,
            Sheet::Tools => (area.height() * 0.5).min(320.0),
            Sheet::Color | Sheet::Timeline => (area.height() * 0.55).min(420.0),
        };
        let rect = Rect::from_min_max(pos2(area.left(), area.bottom() - height), area.max);
        // Above the canvas: a tap outside closes the sheet.
        let scrim = Rect::from_min_max(area.min, pos2(area.right(), rect.top()));
        let outside = ui.interact(scrim, Id::unique("sue-paint-sheet-scrim"), Sense::click());
        ui.painter()
            .rect_filled(scrim, 0.0, Color32::from_black_alpha(60));
        if outside.clicked() {
            self.sheet = None;
            return;
        }
        ui.painter().rect_filled(rect, 12.0, theme.panel);
        ui.painter().rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0, theme.border),
            egui::StrokeKind::Inside,
        );
        let grab =
            Rect::from_center_size(pos2(rect.center().x, rect.top() + 10.0), vec2(44.0, 5.0));
        ui.painter().rect_filled(grab, 3.0, theme.muted);
        let handle = Rect::from_min_size(rect.min, vec2(rect.width(), 22.0));
        if ui
            .interact(
                handle,
                Id::unique("sue-paint-sheet-grab"),
                Sense::click_and_drag(),
            )
            .clicked()
        {
            self.sheet = None;
            return;
        }
        let body = Rect::from_min_max(
            pos2(rect.left() + 4.0, rect.top() + 22.0),
            pos2(rect.right() - 4.0, rect.bottom() - 4.0),
        );
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body));
        inner.set_clip_rect(body);
        // Bigger targets for fingers.
        inner.spacing_mut().interact_size.y = 36.0;
        inner.spacing_mut().button_padding = vec2(10.0, 8.0);
        match sheet {
            Sheet::Layers => self.layer_panel(&mut inner, body, cx, theme),
            Sheet::Color => self.color_panel(&mut inner, body, theme),
            Sheet::ToolSettings => self.tool_panel(&mut inner, body, cx, theme),
            Sheet::Timeline => self.timeline_panel(&mut inner, body, cx, theme),
            Sheet::Tools => {
                let columns = ((body.width() - 8.0) / (BUTTON + 8.0)).floor().max(1.0) as usize;
                let current = self.toolbox.tool();
                for (index, tool) in Tool::ALL.iter().enumerate() {
                    let (row, column) = (index / columns, index % columns);
                    let at = pos2(
                        body.left() + 4.0 + column as f32 * (BUTTON + 8.0),
                        body.top() + 4.0 + row as f32 * (BUTTON + 22.0),
                    );
                    let button = Rect::from_min_size(at, vec2(BUTTON, BUTTON));
                    if icon_button(
                        &mut inner,
                        button,
                        tool_icon(*tool),
                        tool.available(),
                        *tool == current,
                        tool.label(),
                        theme,
                    ) {
                        self.toolbox.pick(*tool);
                        self.sheet = None;
                    }
                    inner.painter().text(
                        pos2(button.center().x, button.bottom() + 2.0),
                        Align2::CENTER_TOP,
                        tool.label(),
                        theme.font(theme.small_size - 1.0),
                        theme.muted,
                    );
                }
            }
            Sheet::Menu => {
                let arena = &cx.world.arena;
                let history = &*cx.history;
                let menus = self.menus.clone();
                // The layout first, so switching to the desktop one is a tap away.
                inner.horizontal(|ui| {
                    ui.label(egui::RichText::new("Interface").color(theme.muted));
                    for (label, choice) in [
                        ("Phone", "phone"),
                        ("Tablet", "tablet"),
                        ("Desktop", "desktop"),
                        ("Automatic", "auto"),
                    ] {
                        if ui
                            .selectable_label(self.interface == choice, label)
                            .clicked()
                        {
                            *run = Some(format!("window.interface.{choice}"));
                        }
                    }
                });
                inner.separator();
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-phone-menus")
                    .show(&mut inner, |ui| {
                        for menu in &menus {
                            egui::CollapsingHeader::new(
                                egui::RichText::new(&menu.title).size(theme.ui_size + 1.0),
                            )
                            .id_salt(("sue-paint-phone-menu", menu.title.as_str()))
                            .show(ui, |ui| {
                                menu_items(
                                    ui,
                                    &menu.items,
                                    &|command| self.state(command, arena, history),
                                    run,
                                );
                            });
                        }
                    });
                if run.is_some() {
                    self.sheet = None;
                }
            }
        }
    }
}

/// `items` as a tappable list, submenus folded.
fn menu_items(
    ui: &mut Ui,
    items: &[sue_wm::MenuItem],
    state: &dyn Fn(&str) -> sue_wm::ItemState,
    run: &mut Option<String>,
) {
    for item in items {
        match item {
            sue_wm::MenuItem::Action { label, command, .. } => {
                let now = state(command);
                let text = match now.checked {
                    Some(true) => format!("{label}  (on)"),
                    _ => label.clone(),
                };
                if ui
                    .add_enabled(
                        now.enabled,
                        egui::Button::new(text)
                            .frame(false)
                            .min_size(vec2(ui.available_width(), 34.0)),
                    )
                    .clicked()
                {
                    *run = Some(command.clone());
                }
            }
            sue_wm::MenuItem::Sub { label, items } => {
                egui::CollapsingHeader::new(label.as_str())
                    .id_salt(("sue-paint-phone-sub", label.as_str()))
                    .show(ui, |ui| menu_items(ui, items, state, run));
            }
            _ => {
                ui.separator();
            }
        }
    }
}
