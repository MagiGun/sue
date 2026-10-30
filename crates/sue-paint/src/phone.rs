//! The sheets Sue Paint's touch layouts (Simple Mode and Studio Mode)
//! bring up from the bottom of the canvas, one at a time: the tools, the
//! sub tools, the layers, the color, the timeline, the reference pictures,
//! the navigator, the materials and the menus. They are the same palettes
//! the desktop layout docks; only where they sit changes.

use egui::{Align2, Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::panels::{icon_button, tool_icon};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// What a sheet shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sheet {
    /// Studio Mode's Menu: home, Simple Mode, Settings, then the menus.
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
    /// The navigator.
    Navigator,
    /// The current layer's settings.
    LayerProperty,
    /// The materials.
    Materials,
    /// The reference pictures (Sub View).
    Reference,
    /// Simple Mode's "..." menu.
    More,
    /// Simple Mode's pens and brushes.
    Pens,
}

/// A thumb-sized button.
const BUTTON: f32 = 44.0;

impl SuePaint {
    pub(crate) fn toggle_sheet(&mut self, sheet: Sheet) {
        self.sheet = if self.sheet == Some(sheet) {
            None
        } else {
            Some(sheet)
        };
    }

    /// A sheet over the bottom of `area`, with a grabber to close it.
    pub(crate) fn phone_sheet(
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
            Sheet::Tools | Sheet::Pens => (area.height() * 0.5).min(320.0),
            Sheet::More => (area.height() * 0.8).min(520.0),
            Sheet::Navigator | Sheet::LayerProperty => (area.height() * 0.55).min(420.0),
            Sheet::Materials => area.height() * 0.7,
            Sheet::Reference => area.height() * 0.6,
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
            Sheet::Reference => self.sub_view(&mut inner, body, cx, theme),
            Sheet::Navigator => self.navigator(&mut inner, body, cx, theme),
            Sheet::Materials => self.material_panel(&mut inner, body, cx, theme),
            Sheet::LayerProperty => {
                let info = self.layer_info(&cx.world.arena);
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-touch-layer-property")
                    .show(&mut inner, |ui| {
                        self.layer_property(ui, info.as_ref(), cx, theme);
                    });
            }
            Sheet::More => {
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-simple-more")
                    .show(&mut inner, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if let Some(command) = self.simple_more(ui, cx, theme) {
                            *run = Some(command);
                        }
                    });
            }
            Sheet::Tools | Sheet::Pens => {
                let columns = ((body.width() - 8.0) / (BUTTON + 8.0)).floor().max(1.0) as usize;
                let current = self.toolbox.tool();
                let tools: &[Tool] = if sheet == Sheet::Pens {
                    &crate::simple::PENS
                } else {
                    &Tool::ALL
                };
                for (index, tool) in tools.iter().enumerate() {
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
                        &tool.label(),
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
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-touch-menu")
                    .show(&mut inner, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.studio_menu(ui, cx, theme, run);
                    });
                if run.is_some() {
                    self.sheet = None;
                }
            }
        }
    }
}

impl SuePaint {
    /// Sue Paint's menus as a list, each folded under its title.
    pub(crate) fn menu_list(
        &mut self,
        ui: &mut Ui,
        cx: &AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let arena = &cx.world.arena;
        let history = &*cx.history;
        let menus = self.menus.clone();
        ui.spacing_mut().item_spacing.y = 4.0;
        for menu in &menus {
            egui::CollapsingHeader::new(egui::RichText::new(&menu.title).size(theme.ui_size + 1.0))
                .id_salt(("sue-paint-touch-menu", menu.title.as_str()))
                .show(ui, |ui| {
                    menu_items(
                        ui,
                        &menu.items,
                        &|command| self.state(command, arena, history),
                        run,
                    );
                });
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
                    Some(true) => tr!("on-2", label = label),
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
