//! The palettes around the canvas: the command bar, the tool strip, Sub
//! Tool and Tool Property, the brush size sliders, the color wheel and
//! sliders, the color set and history, the navigator, the layers and the
//! timeline.

use egui::{
    Align2, Color32, Id, Mesh, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, pos2, vec2,
};
use sue_core::Value;
use sue_model::BlendMode;
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::brush::{AntiAlias, Stroke as BrushStroke};
use crate::color::{self, Hsv};
use crate::doc;
use crate::icons::{self, Icon};
use crate::select::Op;
use crate::studio::{LiveProperty, SuePaint, texture_key};
use crate::tools::Tool;

/// A child `Ui` confined to `rect`.
pub(crate) fn child(ui: &mut Ui, rect: Rect, salt: &str) -> Ui {
    let mut inner = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt(("sue-paint", salt))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    inner.set_clip_rect(rect.intersect(ui.clip_rect()));
    inner
}

/// A square icon button. Returns `true` when clicked while enabled.
pub(crate) fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    icon: Icon,
    enabled: bool,
    active: bool,
    tip: &str,
    theme: &Theme,
) -> bool {
    let response = ui.interact(
        rect,
        Id::unique(("sue-paint-icon", tip, rect.min.x as i32, rect.min.y as i32)),
        Sense::click(),
    );
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, theme.control_radius, theme.active);
    } else if enabled && response.hovered() {
        painter.rect_filled(rect, theme.control_radius, theme.hover);
    }
    let color = if !enabled {
        theme.muted.gamma_multiply(0.45)
    } else if active {
        theme.text
    } else {
        theme.text.gamma_multiply(0.85)
    };
    icons::paint(painter, rect.shrink(rect.width() * 0.2), icon, color);
    let response = if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_hover_text(format!("{tip} (not available yet)"))
    };
    enabled && response.clicked()
}

/// An icon button placed by the layout. Returns `true` when clicked.
pub(crate) fn icon_in(ui: &mut Ui, icon: Icon, active: bool, tip: &str, theme: &Theme) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    icon_button(ui, rect, icon, true, active, tip, theme)
}

/// A palette's tab strip. Returns the body below it and a clicked tab.
fn header(
    ui: &mut Ui,
    rect: Rect,
    tabs: &[(&str, bool)],
    selected: u8,
    theme: &Theme,
) -> (Rect, Option<u8>) {
    let strip = Rect::from_min_size(rect.min, vec2(rect.width(), 24.0));
    let painter = ui.painter().clone();
    painter.rect_filled(strip, 0.0, theme.pill);
    painter.hline(strip.x_range(), strip.bottom(), theme.hairline());
    let font = theme.font(theme.small_size + 0.5);
    let mut x = strip.left() + 4.0;
    let mut clicked = None;
    for (index, (title, enabled)) in tabs.iter().enumerate() {
        let galley = painter.layout_no_wrap((*title).to_owned(), font.clone(), theme.text);
        let tab = Rect::from_min_size(
            pos2(x, strip.top() + 2.0),
            vec2(galley.size().x + 16.0, 22.0),
        );
        x = tab.right() + 2.0;
        let chosen = index as u8 == selected;
        let response = ui.interact(
            tab,
            Id::unique(("sue-paint-tab", title, rect.min.x as i32, rect.min.y as i32)),
            Sense::click(),
        );
        if chosen {
            painter.rect_filled(tab, 0.0, theme.panel);
        } else if *enabled && response.hovered() {
            painter.rect_filled(tab, 0.0, theme.hover);
        }
        let color = if !enabled {
            theme.muted.gamma_multiply(0.5)
        } else if chosen {
            theme.text
        } else {
            theme.muted
        };
        painter.galley(tab.center() - galley.size() / 2.0, galley, color);
        if *enabled && response.clicked() {
            clicked = Some(index as u8);
        }
    }
    (
        Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max),
        clicked,
    )
}

/// How much of a menu bar `width` wide the way back to the canvas takes:
/// its label, or only its icon when the window is narrow.
pub(crate) fn back_width(width: f32) -> f32 {
    if width < 820.0 { 34.0 } else { 146.0 }
}

/// The button at the right of the menu bar that goes back to the canvas.
pub(crate) fn back_button(ui: &mut Ui, bar: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
    let wide = back_width(bar.width()) > 40.0;
    let rect = Rect::from_min_max(
        pos2(bar.right() - back_width(bar.width()) + 6.0, bar.top() + 3.0),
        pos2(bar.right() - 6.0, bar.bottom() - 3.0),
    );
    let response = ui.interact(rect, Id::unique("sue-paint-back"), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, theme.control_radius, theme.hover);
    }
    icons::paint(
        painter,
        Rect::from_min_size(rect.min + vec2(6.0, 2.0), vec2(16.0, 16.0)),
        Icon::Canvas,
        theme.muted,
    );
    if wide {
        painter.text(
            rect.left_center() + vec2(28.0, 0.0),
            Align2::LEFT_CENTER,
            "Back to Canvas",
            theme.font(theme.small_size + 0.5),
            theme.text,
        );
    }
    if response
        .on_hover_text("Leave Sue Paint for the canvas (the picture stays in your project)")
        .clicked()
    {
        cx.request(ShellRequest::CloseStudio);
    }
}

/// A labelled horizontal slider row.
fn slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    log: bool,
    theme: &Theme,
) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(78.0, 18.0),
            egui::Label::new(
                RichText::new(label)
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            ),
        );
        ui.spacing_mut().slider_width = (ui.available_width() - 62.0).max(36.0);
        ui.add(
            egui::Slider::new(value, range)
                .logarithmic(log)
                .max_decimals(if log { 1 } else { 0 })
                .suffix(suffix),
        )
        .changed()
    })
    .inner
}

impl SuePaint {
    // ----- Command bar ----------------------------------------------------------

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
        let groups: [&[(Icon, &str, &str)]; 5] = [
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
        let mut x = strip.left() + 8.0;
        let mut clicked = None;
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                ui.painter().vline(
                    x + 3.0,
                    strip.shrink2(vec2(0.0, 8.0)).y_range(),
                    theme.hairline(),
                );
                x += 8.0;
            }
            for (icon, command, tip) in group.iter() {
                let rect = Rect::from_min_size(pos2(x, strip.top() + 3.0), vec2(28.0, 28.0));
                x += 30.0;
                let state = self.state(command, &cx.world.arena, cx.history);
                if icon_button(
                    ui,
                    rect,
                    *icon,
                    state.enabled,
                    state.checked == Some(true),
                    tip,
                    theme,
                ) {
                    clicked = Some(*command);
                }
            }
        }
        // Save's menu: the project, or the picture as a file.
        let save = Rect::from_min_size(
            pos2(strip.left() + 8.0 + 2.0 * 30.0 + 26.0, strip.top() + 9.0),
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
        let response = response.on_hover_text("Save or export");
        egui::Popup::menu(&response).show(|ui| {
            for (label, command) in [
                ("Save Project (Ctrl+S)", "file.save"),
                ("Save Project As (Ctrl+Shift+S)", "file.save_as"),
                ("Export as PNG", "file.export.png"),
                ("Export as JPEG", "file.export.jpg"),
                ("Export as JPEG XL", "file.export.jxl"),
            ] {
                if ui.button(label).clicked() {
                    clicked = Some(command);
                }
            }
        });
        match clicked {
            Some("app.palette") => cx.request(ShellRequest::Palette),
            Some(command) => self.run(command, cx),
            None => {}
        }
    }

    // ----- Tool strip -------------------------------------------------------------

    pub(crate) fn tool_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let _ = cx;
        ui.painter().rect_filled(strip, 0.0, theme.panel);
        ui.painter()
            .vline(strip.right(), strip.y_range(), theme.hairline());
        let mut y = strip.top() + 6.0;
        let current = self.toolbox.tool();
        for tool in Tool::ALL {
            let rect = Rect::from_min_size(pos2(strip.left() + 5.0, y), vec2(30.0, 28.0));
            y += 30.0;
            if y > strip.bottom() - 70.0 {
                break;
            }
            let tip = match tool.key() {
                Some(key) => format!("{} ({key})", tool.label()),
                None => tool.label().to_owned(),
            };
            if icon_button(
                ui,
                rect,
                tool_icon(tool),
                tool.available(),
                tool == current,
                &tip,
                theme,
            ) {
                self.toolbox.pick(tool);
            }
        }
        // Main and sub color chips.
        let base = pos2(strip.left() + 6.0, strip.bottom() - 42.0);
        let main = Rect::from_min_size(base, vec2(20.0, 20.0));
        let sub = Rect::from_min_size(base + vec2(10.0, 12.0), vec2(20.0, 20.0));
        self.color_chips(ui, main, sub, theme);
        let swap = Rect::from_min_size(base + vec2(16.0, -20.0), vec2(16.0, 16.0));
        if icon_button(
            ui,
            swap,
            Icon::Swap,
            true,
            false,
            "Swap main and sub color (X)",
            theme,
        ) {
            std::mem::swap(&mut self.main, &mut self.sub);
            self.hex = color::hex(self.drawing_color());
        }
    }

    /// The main and sub color chips; a click picks which one brushes use.
    fn color_chips(&mut self, ui: &mut Ui, main: Rect, sub: Rect, theme: &Theme) {
        let chip = |ui: &mut Ui, rect: Rect, rgb: [u8; 3], chosen: bool, tip: &str| {
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-chip", tip, rect.min.x as i32, rect.min.y as i32)),
                Sense::click(),
            );
            let [r, g, b] = rgb;
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            ui.painter().rect_stroke(
                rect,
                2.0,
                Stroke::new(
                    if chosen { 2.0 } else { 1.0 },
                    if chosen {
                        theme.border_active
                    } else {
                        theme.border
                    },
                ),
                StrokeKind::Outside,
            );
            response.on_hover_text(tip).clicked()
        };
        if chip(
            ui,
            sub,
            self.sub,
            self.sub_active && !self.transparent,
            "Sub color",
        ) {
            self.sub_active = true;
            self.transparent = false;
            self.hex = color::hex(self.sub);
        }
        if chip(
            ui,
            main,
            self.main,
            !self.sub_active && !self.transparent,
            "Main color",
        ) {
            self.sub_active = false;
            self.transparent = false;
            self.hex = color::hex(self.main);
        }
        // The transparent color: brushes erase with it (C switches).
        let clear = Rect::from_min_size(sub.right_bottom() + vec2(4.0, -10.0), vec2(10.0, 10.0));
        let response = ui.interact(
            clear,
            Id::unique(("sue-paint-transparent", clear.min.x as i32)),
            Sense::click(),
        );
        let painter = ui.painter();
        for (index, cell) in [
            Rect::from_min_size(clear.min, vec2(5.0, 5.0)),
            Rect::from_min_size(clear.min + vec2(5.0, 5.0), vec2(5.0, 5.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let _ = index;
            painter.rect_filled(cell, 0.0, Color32::from_gray(200));
        }
        painter.rect_stroke(
            clear,
            0.0,
            Stroke::new(
                if self.transparent { 2.0 } else { 1.0 },
                if self.transparent {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Outside,
        );
        if response
            .on_hover_text("Transparent color: brushes erase (C)")
            .clicked()
        {
            self.transparent = !self.transparent;
        }
    }

    // ----- Left column ---------------------------------------------------------------

    pub(crate) fn tool_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let tool = self.toolbox.tool();
        let tabs = [
            ("Sub Tool", self.panels.sub_tool),
            ("Tool Property", self.panels.tool_property),
        ];
        let (body, clicked) = header(ui, rect, &tabs, self.tool_tab, theme);
        if let Some(tab) = clicked {
            self.tool_tab = tab;
        }
        if !self.panels.sub_tool {
            self.tool_tab = 1;
        } else if !self.panels.tool_property {
            self.tool_tab = 0;
        }
        let mut inner = child(ui, body.shrink2(vec2(8.0, 6.0)), "tool-panel");
        if self.tool_tab == 0 {
            inner.label(
                RichText::new(tool.label())
                    .color(theme.muted)
                    .size(theme.small_size + 0.5),
            );
            let picked = self.toolbox.picked(tool);
            let names: Vec<String> = self
                .toolbox
                .subs(tool)
                .iter()
                .map(|sub| sub.name.clone())
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("sue-paint-subs")
                .show(&mut inner, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        let chosen = index == picked;
                        if ui
                            .add_sized(
                                vec2(ui.available_width(), 24.0),
                                egui::Button::selectable(chosen, name.as_str()),
                            )
                            .clicked()
                        {
                            self.toolbox.pick_sub(tool, index);
                            self.tool_tab = 1;
                        }
                    }
                });
            return;
        }
        // Tool Property.
        let sub_name = self.toolbox.current().name.clone();
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(&sub_name)
                    .color(theme.text)
                    .size(theme.ui_size),
            );
            let count = self.toolbox.subs(tool).len();
            if count > 1 {
                egui::ComboBox::from_id_salt("sue-paint-sub-pick")
                    .selected_text("")
                    .width(24.0)
                    .show_ui(ui, |ui| {
                        let picked = self.toolbox.picked(tool);
                        let names: Vec<String> = self
                            .toolbox
                            .subs(tool)
                            .iter()
                            .map(|sub| sub.name.clone())
                            .collect();
                        for (index, name) in names.iter().enumerate() {
                            if ui.selectable_label(index == picked, name).clicked() {
                                self.toolbox.pick_sub(tool, index);
                            }
                        }
                    });
            }
        });
        if tool.paints() {
            self.stroke_preview(&mut inner, cx, theme);
        }
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-props")
            .show(&mut inner, |ui| {
                self.tool_properties(ui, tool, theme);
            });
    }

    /// A sample stroke drawn with the current brush.
    fn stroke_preview(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, theme.control_radius, Color32::from_gray(236));
        let sub = self.toolbox.current();
        let (width, height) = (200_u32, 44_u32);
        let mut tip = sub.tip;
        tip.size = tip.size.clamp(1.0, 30.0);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{tip:?}{:?}", self.drawing_color()).hash(&mut hasher);
            hasher.finish()
        };
        if self
            .preview
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.preview =
                Self::render_preview(tip, color::unit(self.drawing_color()), [width, height])
                    .map(|pixels| (key, pixels));
        }
        let Some((_, shown)) = &self.preview else {
            return;
        };
        if let Some(texture) =
            cx.textures
                .texture(0x5355_4550_5245_5649, key, [width, height], 4, shown, false)
        {
            let image = Rect::from_center_size(
                rect.center(),
                vec2(rect.width(), rect.width() * height as f32 / width as f32).min(rect.size()),
            );
            ui.painter().image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    /// A sample stroke with `tip` over the light preview ground.
    fn render_preview(
        tip: crate::brush::Tip,
        rgb: [f32; 3],
        [width, height]: [u32; 2],
    ) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        // An eraser shows what it takes out of a dark ground.
        if tip.eraser
            && let Some(bytes) = raster.bytes_mut()
        {
            for pixel in bytes.as_chunks_mut::<4>().0 {
                *pixel = [40, 40, 40, 255];
            }
        }
        let mut stroke = BrushStroke::new(sue_core::EntityId::NIL, tip, rgb, false);
        for step in 0..=40 {
            let t = step as f32 / 40.0;
            let x = 12.0 + t * (width as f32 - 24.0);
            let y = height as f32 * (0.5 - 0.28 * (t * std::f32::consts::TAU).sin());
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            stroke.to(&mut raster, [x, y], pressure, None);
        }
        stroke.end(&mut raster, crate::brush::Finish::default(), None);
        let bytes = raster.bytes()?;
        let mut shown = Vec::with_capacity(bytes.len());
        for pixel in bytes.as_chunks::<4>().0 {
            let alpha = f32::from(pixel[3]) / 255.0;
            for value in &pixel[..3] {
                shown.push((f32::from(*value) * alpha + 236.0 * (1.0 - alpha)).round() as u8);
            }
            shown.push(255);
        }
        Some(shown)
    }

    /// The settings of the current sub tool.
    #[allow(clippy::too_many_lines, reason = "one block per tool")]
    fn tool_properties(&mut self, ui: &mut Ui, tool: Tool, theme: &Theme) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        let sub = self.toolbox.current_mut();
        match tool {
            tool if tool.paints() => {
                let tip = &mut sub.tip;
                slider_row(
                    ui,
                    "Brush Size",
                    &mut tip.size,
                    0.5..=1000.0,
                    " px",
                    true,
                    theme,
                );
                let mut opacity = tip.opacity * 100.0;
                if slider_row(ui, "Opacity", &mut opacity, 0.0..=100.0, " %", false, theme) {
                    tip.opacity = opacity / 100.0;
                }
                let mut hardness = tip.hardness * 100.0;
                if slider_row(
                    ui,
                    "Hardness",
                    &mut hardness,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    tip.hardness = hardness / 100.0;
                }
                ui.horizontal(|ui| {
                    ui.add_sized(vec2(78.0, 18.0), egui::Label::new(small("Anti-aliasing")));
                    for level in AntiAlias::ALL {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
                        let chosen = tip.aa == level;
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            if chosen { theme.active } else { theme.pill },
                        );
                        let soft = level.edge();
                        let center = rect.center();
                        ui.painter().circle_filled(center, 5.0, theme.text);
                        if soft > 0.0 {
                            ui.painter().circle_stroke(
                                center,
                                5.0 + soft,
                                Stroke::new(soft, theme.text.gamma_multiply(0.35)),
                            );
                        }
                        if response.on_hover_text(level.label()).clicked() {
                            tip.aa = level;
                        }
                    }
                });
                let mut stabilization = sub.stabilization as f32;
                if slider_row(
                    ui,
                    "Stabilization",
                    &mut stabilization,
                    0.0..=100.0,
                    "",
                    false,
                    theme,
                ) {
                    sub.stabilization = stabilization.round() as u32;
                }
                ui.checkbox(&mut sub.adjust_by_speed, small("Adjust by speed"))
                    .on_hover_text(
                        "Less smoothing as the pen moves faster, so quick lines stay snappy",
                    );
                let mut correction = sub.correction as f32;
                if slider_row(
                    ui,
                    "Correction",
                    &mut correction,
                    0.0..=10.0,
                    "",
                    false,
                    theme,
                ) {
                    sub.correction = correction.round() as u32;
                }
                ui.checkbox(&mut sub.flick, small("Flick into a point"))
                    .on_hover_text(
                        "A quick stroke carries on along the pen's momentum into a fine point",
                    );
                ui.label(small("Starting and ending"));
                let tip = &mut sub.tip;
                slider_row(
                    ui,
                    "Taper start",
                    &mut tip.taper_start,
                    0.0..=200.0,
                    " px",
                    false,
                    theme,
                );
                slider_row(
                    ui,
                    "Taper end",
                    &mut tip.taper_end,
                    0.0..=200.0,
                    " px",
                    false,
                    theme,
                );
                if tip.dabbed() {
                    let mut spacing = tip.spacing * 100.0;
                    if slider_row(ui, "Spacing", &mut spacing, 1.0..=200.0, " %", true, theme) {
                        tip.spacing = spacing / 100.0;
                    }
                }
                ui.checkbox(&mut tip.pressure_size, small("Size by pen pressure"));
                if tip.pressure_size {
                    let mut min = tip.min_size * 100.0;
                    if slider_row(ui, "Min size", &mut min, 0.0..=100.0, " %", false, theme) {
                        tip.min_size = min / 100.0;
                    }
                }
                ui.checkbox(&mut tip.pressure_opacity, small("Opacity by pen pressure"));
                ui.checkbox(&mut tip.build_up, small("Build up (airbrush)"));
                if tip.build_up {
                    let mut flow = tip.flow * 100.0;
                    if slider_row(ui, "Flow", &mut flow, 1.0..=100.0, " %", false, theme) {
                        tip.flow = flow / 100.0;
                    }
                }
                if !tip.eraser {
                    let mut mix = tip.mix * 100.0;
                    if slider_row(
                        ui,
                        "Color mixing",
                        &mut mix,
                        0.0..=100.0,
                        " %",
                        false,
                        theme,
                    ) {
                        tip.mix = mix / 100.0;
                    }
                }
                let mut grain = tip.grain * 100.0;
                if slider_row(
                    ui,
                    "Paper grain",
                    &mut grain,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    tip.grain = grain / 100.0;
                }
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut false, small("Vector magnet")),
                );
                if tool == Tool::Figure {
                    ui.label(small("Drag on the canvas to draw the shape"));
                }
            }
            Tool::Fill | Tool::AutoSelect => {
                let mut tolerance = sub.tolerance * 100.0;
                if slider_row(
                    ui,
                    "Tolerance",
                    &mut tolerance,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    sub.tolerance = tolerance / 100.0;
                }
                ui.horizontal(|ui| {
                    ui.label(small("Refer"));
                    ui.selectable_value(&mut sub.mode, 0, "Editing layer");
                    ui.selectable_value(&mut sub.mode, 1, "All layers");
                });
                if tool == Tool::Fill {
                    let mut opacity = sub.tip.opacity * 100.0;
                    if slider_row(ui, "Opacity", &mut opacity, 0.0..=100.0, " %", false, theme) {
                        sub.tip.opacity = opacity / 100.0;
                    }
                    ui.add_enabled(false, egui::Checkbox::new(&mut false, small("Close gap")));
                } else {
                    self.select_ops(ui, theme);
                }
            }
            Tool::Gradient => {
                ui.horizontal(|ui| {
                    ui.label(small("Shape"));
                    let mut radial = sub.mode == 2;
                    ui.selectable_value(&mut radial, false, "Linear");
                    ui.selectable_value(&mut radial, true, "Radial");
                    if radial {
                        sub.mode = 2;
                    } else if sub.mode == 2 {
                        sub.mode = 0;
                    }
                });
                if sub.mode != 2 {
                    ui.horizontal(|ui| {
                        ui.label(small("Colors"));
                        ui.selectable_value(&mut sub.mode, 0, "To transparent");
                        ui.selectable_value(&mut sub.mode, 1, "Main to sub");
                    });
                }
                let mut opacity = sub.tip.opacity * 100.0;
                if slider_row(ui, "Opacity", &mut opacity, 0.0..=100.0, " %", false, theme) {
                    sub.tip.opacity = opacity / 100.0;
                }
            }
            Tool::Select => {
                ui.horizontal(|ui| {
                    ui.label(small("Shape"));
                    ui.selectable_value(&mut sub.mode, 0, "Rectangle");
                    ui.selectable_value(&mut sub.mode, 1, "Ellipse");
                    ui.selectable_value(&mut sub.mode, 2, "Lasso");
                });
                self.select_ops(ui, theme);
            }
            Tool::Eyedropper => {
                ui.selectable_value(&mut sub.mode, 0, "Obtain display color");
                ui.selectable_value(&mut sub.mode, 1, "Obtain layer color");
            }
            Tool::Zoom => {
                ui.selectable_value(&mut sub.mode, 0, "Zoom in on click");
                ui.selectable_value(&mut sub.mode, 1, "Zoom out on click");
                ui.label(small("Alt+click does the opposite"));
            }
            Tool::Hand => {
                ui.selectable_value(&mut sub.mode, 0, "Scroll");
                ui.selectable_value(&mut sub.mode, 1, "Rotate");
            }
            Tool::MoveLayer => {
                ui.label(small("Drag to move the current layer"));
            }
            _ => {
                ui.label(small("This tool arrives in a later Sue Paint"));
            }
        }
    }

    fn select_ops(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Mode")
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            );
            for op in Op::ALL {
                ui.selectable_value(&mut self.select_op, op, op.label());
            }
        });
        ui.label(
            RichText::new("Shift adds, Alt subtracts, both intersect")
                .size(theme.small_size)
                .color(theme.muted),
        );
    }

    // ----- Color ---------------------------------------------------------------------

    pub(crate) fn color_panel(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        let tabs = [("Color Wheel", true), ("Color Slider", true)];
        let (body, clicked) = header(ui, rect, &tabs, self.color_tab, theme);
        if let Some(tab) = clicked {
            self.color_tab = tab;
        }
        let body = body.shrink(8.0);
        if self.color_tab == 0 {
            self.color_wheel(ui, body, theme);
        } else {
            let mut inner = child(ui, body, "color-sliders");
            self.color_sliders(&mut inner, theme);
        }
    }

    fn color_wheel(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let side = (body.width()).min(body.height() - 34.0).max(60.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top()),
            vec2(side, side),
        );
        let center = area.center();
        let outer = side / 2.0;
        let inner_radius = outer * 0.8;
        let rgb = self.drawing_color();
        let mut hsv = Hsv::from_rgb(color::unit(rgb));
        if hsv.s <= 0.0 || hsv.v <= 0.0 {
            hsv.h = self.hue;
        }
        // The hue ring.
        let mut ring = Mesh::default();
        let steps = 96;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let angle = t * std::f32::consts::TAU;
            let direction = vec2(angle.cos(), angle.sin());
            let [r, g, b] = color::bytes(
                Hsv {
                    h: t * 360.0,
                    s: 1.0,
                    v: 1.0,
                }
                .to_rgb(),
            );
            let shade = Color32::from_rgb(r, g, b);
            ring.colored_vertex(center + direction * inner_radius, shade);
            ring.colored_vertex(center + direction * outer, shade);
            if step > 0 {
                let base = (step * 2) as u32;
                ring.add_triangle(base - 2, base - 1, base);
                ring.add_triangle(base - 1, base + 1, base);
            }
        }
        ui.painter().add(Shape::mesh(ring));
        // The saturation and value square.
        let half = inner_radius * 0.68;
        let square = Rect::from_center_size(center, vec2(half * 2.0, half * 2.0));
        let mut mesh = Mesh::default();
        let cells = 32;
        for row in 0..=cells {
            for column in 0..=cells {
                let (s, v) = (
                    column as f32 / cells as f32,
                    1.0 - row as f32 / cells as f32,
                );
                let [r, g, b] = color::bytes(Hsv { h: hsv.h, s, v }.to_rgb());
                mesh.colored_vertex(
                    pos2(
                        square.left() + s * square.width(),
                        square.top() + (1.0 - v) * square.height(),
                    ),
                    Color32::from_rgb(r, g, b),
                );
                if row > 0 && column > 0 {
                    let at = (row * (cells + 1) + column) as u32;
                    let up = at - (cells + 1) as u32;
                    mesh.add_triangle(up - 1, up, at);
                    mesh.add_triangle(up - 1, at, at - 1);
                }
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        // Markers.
        let angle = hsv.h.to_radians();
        let ring_mark = center + vec2(angle.cos(), angle.sin()) * (inner_radius + outer) / 2.0;
        ui.painter().circle_stroke(
            ring_mark,
            (outer - inner_radius) / 2.0,
            Stroke::new(1.5, Color32::WHITE),
        );
        let mark = pos2(
            square.left() + hsv.s * square.width(),
            square.top() + (1.0 - hsv.v) * square.height(),
        );
        ui.painter()
            .circle_stroke(mark, 4.5, Stroke::new(1.5, Color32::WHITE));
        ui.painter()
            .circle_stroke(mark, 5.5, Stroke::new(1.0, Color32::BLACK));
        // Dragging.
        let response = ui.interact(area, Id::unique("sue-paint-wheel"), Sense::click_and_drag());
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked() || response.drag_started())
        {
            let press = ui
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pointer);
            let on_ring = (press - center).length() > inner_radius * 0.95;
            let mut next = hsv;
            if on_ring {
                let offset = pointer - center;
                next.h = offset.y.atan2(offset.x).to_degrees().rem_euclid(360.0);
                self.hue = next.h;
            } else {
                next.s = ((pointer.x - square.left()) / square.width()).clamp(0.0, 1.0);
                next.v = 1.0 - ((pointer.y - square.top()) / square.height()).clamp(0.0, 1.0);
            }
            let hue = self.hue;
            self.set_drawing_color(color::bytes(next.to_rgb()));
            if on_ring || next.s <= 0.0 {
                self.hue = if on_ring { next.h } else { hue };
            }
        }
        // Chips and numbers below.
        let row = Rect::from_min_max(pos2(body.left(), area.bottom() + 6.0), body.max);
        let main = Rect::from_min_size(row.min, vec2(20.0, 20.0));
        let sub = Rect::from_min_size(row.min + vec2(12.0, 8.0), vec2(20.0, 20.0));
        self.color_chips(ui, main, sub, theme);
        let text = format!(
            "H {:.0}  S {:.0}  V {:.0}",
            hsv.h,
            hsv.s * 100.0,
            hsv.v * 100.0
        );
        ui.painter().text(
            pos2(row.left() + 44.0, row.top() + 14.0),
            Align2::LEFT_CENTER,
            text,
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
    }

    fn color_sliders(&mut self, ui: &mut Ui, theme: &Theme) {
        let rgb = self.drawing_color();
        let mut values = rgb.map(f32::from);
        let mut changed = false;
        for (index, name) in ["R", "G", "B"].iter().enumerate() {
            changed |= slider_row(ui, name, &mut values[index], 0.0..=255.0, "", false, theme);
        }
        let mut hsv = Hsv::from_rgb(color::unit(rgb));
        let mut hue = if hsv.s > 0.0 { hsv.h } else { self.hue };
        let mut saturation = hsv.s * 100.0;
        let mut value = hsv.v * 100.0;
        let mut hsv_changed = slider_row(ui, "H", &mut hue, 0.0..=359.0, "", false, theme);
        hsv_changed |= slider_row(ui, "S", &mut saturation, 0.0..=100.0, "", false, theme);
        hsv_changed |= slider_row(ui, "V", &mut value, 0.0..=100.0, "", false, theme);
        if changed {
            self.set_drawing_color(values.map(|value| value.round() as u8));
        } else if hsv_changed {
            hsv = Hsv {
                h: hue,
                s: saturation / 100.0,
                v: value / 100.0,
            };
            self.hue = hue;
            self.set_drawing_color(color::bytes(hsv.to_rgb()));
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Hex")
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            );
            let response = ui.add(egui::TextEdit::singleline(&mut self.hex).desired_width(90.0));
            if response.lost_focus()
                && let Some(rgb) = color::parse_hex(&self.hex)
            {
                self.set_drawing_color(rgb);
            }
        });
    }

    #[allow(clippy::too_many_lines, reason = "the color set and its controls")]
    pub(crate) fn color_set_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let tabs = [("Color Set", true), ("Color History", true)];
        let (body, clicked) = header(ui, rect, &tabs, self.set_tab, theme);
        if let Some(tab) = clicked {
            self.set_tab = tab;
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
                "Color set settings",
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
                "Import a color set (GIMP .gpl or hex list)",
                theme,
            ) {
                cx.request(ShellRequest::PickFile(
                    sue_wm::FilePick::open(Some(me()), "colorset-import", "Import a Color Set")
                        .extensions(&["gpl", "txt", "hex"]),
                ));
            }
            if icon_button(
                ui,
                icons_at(2.0),
                Icon::Export,
                true,
                false,
                "Export this color set (GIMP .gpl)",
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
                        "Export the Color Set",
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
                    "Empty: add the drawing color with +"
                } else {
                    "Colors you paint with show here"
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
            if response.on_hover_text(color::hex(*rgb)).clicked() {
                self.set_drawing_color(*rgb);
                self.transparent = false;
                if editing {
                    self.color_sets.selected = Some(index);
                }
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
            "Add the drawing color",
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
            "Replace the picked swatch with the drawing color",
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
            "Delete the picked swatch",
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
                            ("New Color Set", "new"),
                            ("Duplicate This Set", "duplicate"),
                            ("Delete This Set", "delete"),
                            ("Small Swatches", "small"),
                            ("Medium Swatches", "medium"),
                            ("Large Swatches", "large"),
                            ("Restore the Built-in Sets", "restore"),
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
            Some("new") => sets.add_set("My Colors", Vec::new()),
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

    // ----- Brush size sliders ---------------------------------------------------------

    pub(crate) fn size_column(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .vline(rect.right(), rect.y_range(), theme.hairline());
        let (body, _) = header(ui, rect, &[("Size", true)], 0, theme);
        let tool = self.toolbox.tool();
        if !tool.paints() && !matches!(tool, Tool::Fill | Tool::Gradient) {
            return;
        }
        let tip = &mut self.toolbox.current_mut().tip;
        let mut inner = child(ui, body.shrink2(vec2(4.0, 8.0)), "size-column");
        let height = (body.height() - 80.0).max(60.0);
        inner.spacing_mut().slider_width = height / 2.0 - 30.0;
        inner.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("{:.1}\npx", tip.size))
                    .size(theme.small_size)
                    .color(theme.text),
            );
            ui.add(
                egui::Slider::new(&mut tip.size, 0.5..=1000.0)
                    .logarithmic(true)
                    .vertical()
                    .show_value(false),
            );
            ui.add_space(12.0);
            let mut opacity = tip.opacity * 100.0;
            ui.label(
                RichText::new(format!("{opacity:.0}\n%"))
                    .size(theme.small_size)
                    .color(theme.text),
            );
            if ui
                .add(
                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                        .vertical()
                        .show_value(false),
                )
                .changed()
            {
                tip.opacity = opacity / 100.0;
            }
        });
    }

    // ----- Right column: navigator and layers ------------------------------------------

    pub(crate) fn navigator(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let (body, clicked) = header(
            ui,
            rect,
            &[("Navigator", true), ("Sub View", true)],
            self.nav_tab,
            theme,
        );
        if let Some(tab) = clicked {
            self.nav_tab = tab;
        }
        let body = body.shrink(6.0);
        if self.nav_tab == 1 {
            self.sub_view(ui, body, cx, theme);
            return;
        }
        let viewport = self.viewport;
        let controls = 58.0;
        let thumb_area = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - controls));
        ui.painter().rect_filled(thumb_area, 0.0, theme.void);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let (pixels, size, version) = doc.compositor.display();
        if size[0] == 0 {
            return;
        }
        let scale = (thumb_area.width() / size[0] as f32).min(thumb_area.height() / size[1] as f32);
        let thumb = Rect::from_center_size(
            thumb_area.center(),
            vec2(size[0] as f32, size[1] as f32) * scale,
        );
        if let Some(texture) = cx.textures.texture(
            texture_key(doc.picture, 0),
            version,
            size,
            4,
            pixels,
            doc.view.zoom >= 2.0,
        ) {
            ui.painter().image(
                texture,
                thumb,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // The part the canvas shows.
        let corners = [
            viewport.left_top(),
            viewport.right_top(),
            viewport.right_bottom(),
            viewport.left_bottom(),
        ]
        .map(|corner| {
            let p = doc.view.to_picture(viewport, size, corner);
            thumb.min + p.to_vec2() * scale
        });
        // Clipped: zoomed out, the view is larger than the picture.
        ui.painter()
            .with_clip_rect(thumb_area)
            .add(Shape::closed_line(
                corners.to_vec(),
                Stroke::new(1.5, theme.border_active),
            ));
        let response = ui.interact(
            thumb_area,
            Id::unique("sue-paint-navigator"),
            Sense::click_and_drag(),
        );
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            let p = ((pointer - thumb.min) / scale).to_pos2();
            doc.view.center_on(viewport, size, p);
        }
        // Zoom and rotation.
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), thumb_area.bottom() + 4.0), body.max),
            "navigator-controls",
        );
        inner.spacing_mut().slider_width = (body.width() - 170.0).max(40.0);
        let mut percent = doc.view.zoom * 100.0;
        inner.horizontal(|ui| {
            if ui
                .add(
                    egui::Slider::new(&mut percent, 1.0..=6400.0)
                        .logarithmic(true)
                        .max_decimals(1)
                        .suffix(" %"),
                )
                .changed()
            {
                doc.view
                    .set_zoom_at(viewport, size, viewport.center(), percent / 100.0);
            }
            if icon_in(ui, Icon::ZoomOut, false, "Zoom out", theme) {
                doc.view.zoom_at(viewport, size, viewport.center(), 0.8);
            }
            if icon_in(ui, Icon::ZoomIn, false, "Zoom in", theme) {
                doc.view.zoom_at(viewport, size, viewport.center(), 1.25);
            }
            if icon_in(ui, Icon::Actual, false, "100%", theme) {
                doc.view.set_zoom_at(viewport, size, viewport.center(), 1.0);
            }
            if icon_in(ui, Icon::Fit, false, "Fit to screen", theme) {
                doc.view.fit(viewport, size);
            }
        });
        let mut rotation = doc.view.rotation;
        inner.horizontal(|ui| {
            if ui
                .add(
                    egui::Slider::new(&mut rotation, -180.0..=180.0)
                        .max_decimals(1)
                        .suffix(" \u{b0}"),
                )
                .changed()
            {
                doc.view.set_rotation(viewport, size, rotation);
            }
            if icon_in(ui, Icon::RotateLeft, false, "Rotate left", theme) {
                doc.view.rotate_by(viewport, size, -15.0);
            }
            if icon_in(ui, Icon::RotateRight, false, "Rotate right", theme) {
                doc.view.rotate_by(viewport, size, 15.0);
            }
            if icon_in(ui, Icon::Reset, false, "Reset rotation", theme) {
                doc.view.set_rotation(viewport, size, 0.0);
            }
            if icon_in(ui, Icon::FlipH, doc.view.flip_h, "Flip horizontally", theme) {
                doc.view.flip(viewport, size, true);
            }
            if icon_in(ui, Icon::FlipV, doc.view.flip_v, "Flip vertically", theme) {
                doc.view.flip(viewport, size, false);
            }
        });
    }

    #[allow(clippy::too_many_lines, reason = "the layer palette's three tabs")]
    pub(crate) fn layer_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let tabs = [
            ("Layer", true),
            ("Layer Property", true),
            ("History", true),
            ("Animation Cels", true),
        ];
        let (body, clicked) = header(ui, rect, &tabs, self.layer_tab, theme);
        if let Some(tab) = clicked {
            self.layer_tab = tab;
        }
        let Some(picture_id) = self.picture() else {
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                "No canvas",
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            return;
        };
        let stack = doc::layers(&cx.world.arena, picture_id);
        let current = self.layer_info(&cx.world.arena);
        if self.layer_tab == 3 {
            self.cels_tab(ui, body, theme);
            return;
        }
        match self.layer_tab {
            1 => {
                let mut inner = child(ui, body.shrink(8.0), "layer-property");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
                return;
            }
            2 => {
                let mut inner = child(ui, body.shrink(8.0), "layer-history");
                inner.horizontal(|ui| {
                    if ui
                        .add_enabled(cx.history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.run("edit.undo", cx);
                    }
                    if ui
                        .add_enabled(cx.history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.run("edit.redo", cx);
                    }
                });
                let labels: Vec<String> = cx
                    .history
                    .history()
                    .iter()
                    .map(|label| (*label).to_owned())
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("sue-paint-history")
                    .stick_to_bottom(true)
                    .show(&mut inner, |ui| {
                        if labels.is_empty() {
                            ui.label(RichText::new("Nothing to undo yet").color(theme.muted));
                        }
                        for (index, label) in labels.iter().enumerate() {
                            let last = index + 1 == labels.len();
                            ui.label(RichText::new(label).color(if last {
                                theme.text
                            } else {
                                theme.muted
                            }));
                        }
                    });
                return;
            }
            _ => {}
        }
        // Blend mode and opacity of the current layer.
        let mut inner = child(ui, body.shrink2(vec2(6.0, 6.0)), "layer-top");
        let raster = current
            .as_ref()
            .is_some_and(|info| info.kind == LayerKind::Raster);
        inner.horizontal(|ui| {
            let blend = current
                .as_ref()
                .map_or(BlendMode::Normal, |info| info.style.blend);
            ui.add_enabled_ui(raster, |ui| {
                egui::ComboBox::from_id_salt("sue-paint-blend")
                    .selected_text(blend.label())
                    .width(108.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        let mut group = 0;
                        for mode in BlendMode::ALL {
                            if mode.group() != group {
                                ui.separator();
                                group = mode.group();
                            }
                            if ui.selectable_label(mode == blend, mode.label()).clicked()
                                && let Some(info) = &current
                            {
                                Self::apply(cx, doc::set(info.id, picture::BLEND, mode.name()));
                            }
                        }
                    });
            });
            let mut opacity = current
                .as_ref()
                .map_or(100.0, |info| info.style.opacity * 100.0);
            ui.spacing_mut().slider_width = (ui.available_width() - 52.0).max(40.0);
            ui.add_enabled_ui(current.is_some(), |ui| {
                let response = ui.add(
                    egui::Slider::new(&mut opacity, 0.0..=100.0)
                        .max_decimals(0)
                        .suffix(""),
                );
                if let Some(info) = &current {
                    self.live_property(
                        cx,
                        &response,
                        info.id,
                        picture::OPACITY,
                        Value::Float(f64::from(opacity) / 100.0),
                    );
                }
            });
        });
        // Layer switches and actions.
        let row_top = inner.min_rect().bottom() + 4.0;
        let flags: [(Icon, &str, &str); 6] = [
            (Icon::Clip, "layer.toggle.clip", "Clip to Layer Below"),
            (
                Icon::Reference,
                "layer.toggle.reference",
                "Set as Reference Layer",
            ),
            (Icon::Draft, "layer.toggle.draft", "Set as Draft Layer"),
            (Icon::Lock, "layer.toggle.lock", "Lock Layer"),
            (
                Icon::LockAlpha,
                "layer.toggle.lock_alpha",
                "Lock Transparent Pixels",
            ),
            (Icon::Mask, "layer.mask.outside", "Enable Mask"),
        ];
        let mut run = None;
        for (index, (icon, command, tip)) in flags.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(body.left() + 6.0 + index as f32 * 28.0, row_top),
                vec2(26.0, 24.0),
            );
            let state = self.state(command, &cx.world.arena, cx.history);
            let checked = match *command {
                "layer.toggle.lock_alpha" => current.as_ref().is_some_and(|info| info.alpha_locked),
                _ => state.checked == Some(true),
            };
            if icon_button(ui, rect, *icon, state.enabled, checked, tip, theme) {
                run = Some(*command);
            }
        }
        let actions: [(Icon, &str, &str); 8] = [
            (
                Icon::NewLayer,
                "layer.new",
                "New Raster Layer (Ctrl+Shift+N)",
            ),
            (Icon::NewVector, "layer.new.vector", "New Vector Layer"),
            (Icon::NewFolder, "layer.new.folder", "New Layer Folder"),
            (Icon::Duplicate, "layer.duplicate", "Duplicate Layer"),
            (
                Icon::MergeDown,
                "layer.merge_down",
                "Merge with Layer Below (Ctrl+E)",
            ),
            (Icon::Up, "layer.order.up", "Move Layer Up (Ctrl+])"),
            (Icon::Down, "layer.order.down", "Move Layer Down (Ctrl+[)"),
            (Icon::Delete, "layer.delete", "Delete Layer"),
        ];
        let action_top = row_top + 28.0;
        for (index, (icon, command, tip)) in actions.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(body.left() + 6.0 + index as f32 * 28.0, action_top),
                vec2(26.0, 24.0),
            );
            let state = self.state(command, &cx.world.arena, cx.history);
            if icon_button(ui, rect, *icon, state.enabled, false, tip, theme) {
                run = Some(*command);
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
            return;
        }
        // The list, top layer first.
        let list = Rect::from_min_max(pos2(body.left(), action_top + 30.0), body.max);
        ui.painter()
            .hline(list.x_range(), list.top(), theme.hairline());
        let mut list_ui = child(ui, list, "layer-list");
        let mut pick = None;
        let mut toggle = None;
        let mut rename_done = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-layers")
            .show(&mut list_ui, |ui| {
                for info in stack.iter().rev() {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                    let chosen = Some(info.id) == self.layer();
                    if chosen {
                        ui.painter().rect_filled(row, 0.0, theme.active);
                    } else if response.hovered() {
                        ui.painter().rect_filled(row, 0.0, theme.hover);
                    }
                    ui.painter()
                        .hline(row.x_range(), row.bottom(), theme.hairline());
                    // Visibility.
                    let eye = Rect::from_min_size(row.min + vec2(4.0, 13.0), vec2(20.0, 20.0));
                    let eye_response = ui.interact(
                        eye,
                        Id::unique(("sue-paint-eye", info.id.to_u128())),
                        Sense::click(),
                    );
                    icons::paint(
                        ui.painter(),
                        eye,
                        Icon::Eye,
                        if info.style.visible {
                            theme.text
                        } else {
                            theme.muted.gamma_multiply(0.35)
                        },
                    );
                    if eye_response.on_hover_text("Show or hide").clicked() {
                        toggle = Some((info.id, !info.style.visible));
                    }
                    // State marks: the pen on the current layer, locks.
                    let mark = Rect::from_min_size(row.min + vec2(26.0, 15.0), vec2(16.0, 16.0));
                    if info.locked {
                        icons::paint(ui.painter(), mark, Icon::Lock, theme.muted);
                    } else if chosen {
                        icons::paint(ui.painter(), mark, Icon::Pen, theme.text);
                    }
                    if info.style.clip {
                        ui.painter().vline(
                            row.left() + 46.0,
                            row.shrink2(vec2(0.0, 6.0)).y_range(),
                            Stroke::new(2.0, theme.border_active),
                        );
                    }
                    // Thumbnail.
                    let thumb_rect =
                        Rect::from_min_size(row.min + vec2(50.0, 5.0), vec2(48.0, 36.0));
                    self.layer_thumbnail(ui, thumb_rect, info, cx, theme);
                    // Text.
                    let text_left = thumb_rect.right() + 8.0;
                    let top_line = match info.kind {
                        LayerKind::Paper => "Paper".to_owned(),
                        _ => format!(
                            "{:.0} % {}",
                            info.style.opacity * 100.0,
                            info.style.blend.label()
                        ),
                    };
                    ui.painter().text(
                        pos2(text_left, row.top() + 14.0),
                        Align2::LEFT_CENTER,
                        top_line,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                    let renaming = self
                        .renaming
                        .as_ref()
                        .filter(|(id, _)| *id == info.id)
                        .map(|(_, name)| name.clone());
                    if let Some(mut name) = renaming {
                        let field = Rect::from_min_max(
                            pos2(text_left, row.top() + 22.0),
                            pos2(row.right() - 6.0, row.bottom() - 4.0),
                        );
                        let mut field_ui = child(ui, field, "rename");
                        let edit = field_ui.add(
                            egui::TextEdit::singleline(&mut name).desired_width(field.width()),
                        );
                        edit.request_focus();
                        if edit.lost_focus() {
                            rename_done = Some((
                                info.id,
                                name.clone(),
                                ui.input(|input| input.key_pressed(egui::Key::Escape)),
                            ));
                        }
                        self.renaming = Some((info.id, name));
                    } else {
                        let name = if info.style.draft {
                            format!("{} (draft)", info.name)
                        } else {
                            info.name.clone()
                        };
                        ui.painter().text(
                            pos2(text_left, row.top() + 31.0),
                            Align2::LEFT_CENTER,
                            name,
                            theme.font(theme.ui_size),
                            if info.style.visible {
                                theme.text
                            } else {
                                theme.muted
                            },
                        );
                    }
                    if response.double_clicked() {
                        self.renaming = Some((info.id, info.name.clone()));
                    } else if response.clicked() {
                        pick = Some(info.id);
                    }
                }
            });
        if let Some(id) = pick
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
        }
        if let Some((id, visible)) = toggle {
            Self::apply(cx, doc::set(id, picture::VISIBLE, visible));
        }
        if let Some((id, name, cancelled)) = rename_done {
            self.renaming = None;
            let name = name.trim().to_owned();
            let old = cx
                .world
                .arena
                .get_by_id(id)
                .map(|entity| entity.name().to_owned())
                .unwrap_or_default();
            if !cancelled && !name.is_empty() && name != old {
                Self::apply(cx, sue_core::txn::Rename::new(id, name));
            }
        }
    }

    /// The Layer Property tab: the current layer's settings.
    fn layer_property(
        &mut self,
        ui: &mut Ui,
        current: Option<&doc::LayerInfo>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(info) = current else {
            ui.label(RichText::new("No layer").color(theme.muted));
            return;
        };
        ui.label(
            RichText::new(&info.name)
                .size(theme.ui_size + 1.0)
                .color(theme.text),
        );
        ui.label(
            RichText::new(match info.kind {
                LayerKind::Raster => "Raster layer",
                LayerKind::Paper => "Paper",
                LayerKind::Other => "A layer kind this Sue cannot edit yet",
            })
            .color(theme.muted),
        );
        ui.add_space(6.0);
        match info.kind {
            LayerKind::Paper => {
                let paper = cx
                    .world
                    .arena
                    .get_by_id(info.id)
                    .map(picture::paper_color)
                    .unwrap_or_else(|_| sue_core::Color::srgb8(255, 255, 255));
                let [r, g, b] = color::bytes(paper.rgb);
                let mut rgba = [r, g, b, 255];
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Color").color(theme.muted));
                    let pick = sue_wm::color_button(
                        ui,
                        ("sue-paint-paper", info.id),
                        &mut rgba,
                        false,
                        theme,
                    );
                    let [r, g, b, _] = rgba;
                    self.live_value(
                        cx,
                        (
                            pick.changed,
                            pick.released || (pick.changed && !pick.dragging),
                        ),
                        info.id,
                        picture::COLOR,
                        Value::from(sue_core::Color::srgb8(r, g, b)),
                    );
                });
            }
            LayerKind::Raster => {
                let mut opacity = info.style.opacity * 100.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Opacity").color(theme.muted));
                    let response = ui.add(
                        egui::Slider::new(&mut opacity, 0.0..=100.0)
                            .max_decimals(0)
                            .suffix(" %"),
                    );
                    self.live_property(
                        cx,
                        &response,
                        info.id,
                        picture::OPACITY,
                        Value::Float(f64::from(opacity) / 100.0),
                    );
                });
                let mut offset = info.style.offset;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Offset").color(theme.muted));
                    let x = ui.add(egui::DragValue::new(&mut offset[0]).suffix(" px"));
                    self.live_property(cx, &x, info.id, picture::X, Value::Int(offset[0]));
                    let y = ui.add(egui::DragValue::new(&mut offset[1]).suffix(" px"));
                    self.live_property(cx, &y, info.id, picture::Y, Value::Int(offset[1]));
                });
                if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
                    ui.label(
                        RichText::new(format!(
                            "{} x {} px, {:?}",
                            raster.width(),
                            raster.height(),
                            raster.format()
                        ))
                        .size(theme.small_size)
                        .color(theme.muted),
                    );
                }
            }
            LayerKind::Other => {}
        }
        ui.add_space(6.0);
        ui.add_enabled_ui(false, |ui| {
            ui.label(RichText::new("Effects: border, tone, layer color").color(theme.muted));
        });
    }

    /// Applies a slider's value in place while it is dragged, and records
    /// one undo step (holding the value from before the drag) when it ends.
    fn live_property(
        &mut self,
        cx: &mut AppletContext<'_>,
        response: &egui::Response,
        id: sue_core::EntityId,
        key: &str,
        value: Value,
    ) {
        let finished = response.drag_stopped()
            || response.lost_focus()
            || (response.changed() && !response.dragged() && !response.has_focus());
        self.live_value(cx, (response.changed(), finished), id, key, value);
    }

    /// Sets property `key` of `id` live while it `changed`, and records
    /// one undo step from where it began once `finished`.
    fn live_value(
        &mut self,
        cx: &mut AppletContext<'_>,
        (changed, finished): (bool, bool),
        id: sue_core::EntityId,
        key: &str,
        value: Value,
    ) {
        let label = doc::label(key, &value);
        if changed {
            let Ok(entity) = cx.world.arena.get_by_id_mut(id) else {
                return;
            };
            if self
                .live
                .as_ref()
                .is_none_or(|live| live.id != id || live.key != key)
            {
                self.live = Some(LiveProperty {
                    id,
                    key: key.to_owned(),
                    old: entity.property(key).cloned(),
                });
            }
            entity.properties_mut().set(key, value);
        }
        if finished && let Some(live) = self.live.take_if(|live| live.id == id && live.key == key) {
            let step = match live.old {
                Some(old) => sue_core::txn::SetProperty::new(id, key, old),
                None => sue_core::txn::SetProperty::remove(id, key),
            };
            cx.record(sue_wm::ArenaEdit(doc::Edit::new(label).then(step)));
        }
    }

    fn layer_thumbnail(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        info: &doc::LayerInfo,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let key = texture_key(info.id, 0x7448_554d_4253);
        if self
            .thumbs
            .get(&info.id)
            .is_none_or(|(revision, _, _)| *revision != info.revision)
            && let Some((pixels, size)) =
                crate::composite::thumbnail(&cx.world.arena, info.id, [96, 72])
        {
            self.thumbs.insert(info.id, (info.revision, pixels, size));
        }
        let texture = self
            .thumbs
            .get(&info.id)
            .and_then(|(revision, pixels, size)| {
                cx.textures.texture(key, *revision, *size, 4, pixels, false)
            });
        ui.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(texture) = texture {
            let aspect = self
                .picture()
                .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                .and_then(picture::size)
                .map_or(4.0 / 3.0, |[w, h]| w as f32 / h.max(1) as f32);
            let size = if aspect > rect.width() / rect.height() {
                vec2(rect.width(), rect.width() / aspect)
            } else {
                vec2(rect.height() * aspect, rect.height())
            };
            let image = Rect::from_center_size(rect.center(), size);
            ui.painter().image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.painter()
                .rect_stroke(image, 0.0, theme.hairline(), StrokeKind::Outside);
        }
    }

    // ----- Timeline -----------------------------------------------------------------------

    /// The Animation Cels tab: its tools, which wait for animation.
    fn cels_tab(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let row = Rect::from_min_size(body.min + vec2(6.0, 6.0), vec2(body.width() - 12.0, 22.0));
        let buttons: [(Icon, &str); 6] = [
            (Icon::Cel, "New animation cel"),
            (Icon::Edit, "Specify cels"),
            (Icon::Delete, "Delete cel"),
            (Icon::Onion, "Onion skin"),
            (Icon::Layers, "Light table"),
            (Icon::Loop, "Cel order"),
        ];
        for (index, (icon, tip)) in buttons.iter().enumerate() {
            let at = Rect::from_min_size(
                pos2(row.left() + index as f32 * 26.0, row.top()),
                vec2(22.0, 22.0),
            );
            icon_button(ui, at, *icon, false, false, tip, theme);
        }
        ui.painter().text(
            pos2(body.left() + 8.0, row.bottom() + 16.0),
            Align2::LEFT_CENTER,
            "Animation cels arrive with animation",
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
    }

    pub(crate) fn timeline_panel(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (body, _) = header(ui, rect, &[("Timeline", true)], 0, theme);
        let bar = Rect::from_min_size(body.min, vec2(body.width(), 28.0));
        // The timeline to edit, then its tools and playback, as in Clip
        // Studio Paint; they wait for animation to arrive.
        let mut picker = child(
            ui,
            Rect::from_min_size(bar.min + vec2(6.0, 3.0), vec2(118.0, 22.0)),
            "timeline-pick",
        );
        picker.add_enabled_ui(false, |ui| {
            egui::ComboBox::from_id_salt("sue-paint-timeline-pick")
                .selected_text("Timeline 1")
                .width(110.0)
                .show_ui(ui, |_| {});
        });
        let icons_row: [(Icon, &str); 16] = [
            (Icon::New, "New timeline"),
            (Icon::NewFolder, "New animation folder"),
            (Icon::Cel, "New animation cel"),
            (Icon::Edit, "Specify cels"),
            (Icon::Delete, "Delete cel"),
            (Icon::Add, "Add keyframe"),
            (Icon::Minus, "Delete keyframe"),
            (Icon::Onion, "Onion skin"),
            (Icon::Layers, "Light table"),
            (Icon::First, "First frame"),
            (Icon::Prev, "Previous frame"),
            (Icon::Play, "Play"),
            (Icon::Stop, "Stop"),
            (Icon::Next, "Next frame"),
            (Icon::Last, "Last frame"),
            (Icon::Loop, "Loop"),
        ];
        let mut x = bar.left() + 130.0;
        for (index, (icon, tip)) in icons_row.iter().enumerate() {
            if index == 9 || index == 5 {
                ui.painter().vline(
                    x + 1.0,
                    bar.shrink2(vec2(0.0, 6.0)).y_range(),
                    theme.hairline(),
                );
                x += 6.0;
            }
            let at = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(22.0, 22.0));
            if at.right() > bar.right() - 70.0 {
                break;
            }
            icon_button(ui, at, *icon, false, false, tip, theme);
            x += 25.0;
        }
        ui.painter().text(
            pos2(bar.right() - 8.0, bar.center().y),
            Align2::RIGHT_CENTER,
            "24 fps",
            theme.font(theme.small_size),
            theme.muted.gamma_multiply(0.7),
        );
        ui.painter()
            .hline(body.x_range(), bar.bottom(), theme.hairline());
        let tracks = Rect::from_min_max(pos2(body.left(), bar.bottom()), body.max);
        let names = Rect::from_min_max(tracks.min, pos2(tracks.left() + 160.0, tracks.bottom()));
        ui.painter()
            .vline(names.right(), tracks.y_range(), theme.hairline());
        ui.painter().text(
            names.left_top() + vec2(10.0, 14.0),
            Align2::LEFT_CENTER,
            "No timeline",
            theme.font(theme.small_size + 0.5),
            theme.muted.gamma_multiply(0.7),
        );
        let frames = Rect::from_min_max(pos2(names.right(), tracks.top()), tracks.max);
        let step = 24.0;
        let mut x = frames.left();
        let mut frame = 1;
        while x < frames.right() {
            ui.painter().vline(
                x,
                frames.y_range(),
                Stroke::new(1.0, theme.border.gamma_multiply(0.5)),
            );
            if frame % 6 == 1 {
                ui.painter().text(
                    pos2(x + 3.0, frames.top() + 8.0),
                    Align2::LEFT_CENTER,
                    frame.to_string(),
                    theme.font(theme.small_size - 1.0),
                    theme.muted.gamma_multiply(0.6),
                );
            }
            x += step;
            frame += 1;
        }
        ui.painter().text(
            frames.center(),
            Align2::CENTER_CENTER,
            "Animation timelines arrive in a later Sue Paint",
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
    }
}

/// The icon of a tool.
pub(crate) fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Zoom => Icon::Zoom,
        Tool::Hand => Icon::Hand,
        Tool::MoveLayer => Icon::MoveLayer,
        Tool::Select => Icon::Select,
        Tool::AutoSelect => Icon::AutoSelect,
        Tool::Eyedropper => Icon::Eyedropper,
        Tool::Pen => Icon::Pen,
        Tool::Pencil => Icon::Pencil,
        Tool::Brush => Icon::Brush,
        Tool::Airbrush => Icon::Airbrush,
        Tool::Decoration => Icon::Decoration,
        Tool::Eraser => Icon::Eraser,
        Tool::Blend => Icon::Blend,
        Tool::Fill => Icon::Fill,
        Tool::Gradient => Icon::Gradient,
        Tool::Figure => Icon::Figure,
        Tool::Text => Icon::Text,
        Tool::Balloon => Icon::Balloon,
        Tool::Frame => Icon::Frame,
        Tool::Ruler => Icon::Ruler,
        Tool::CorrectLine => Icon::CorrectLine,
        Tool::Operation => Icon::Operation,
    }
}
