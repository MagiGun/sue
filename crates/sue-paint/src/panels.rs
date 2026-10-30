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

use crate::brush::{AntiAlias, Dynamics, Jitter, Stroke as BrushStroke, TaperMode};
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
    icons::paint(painter, rect.shrink(rect.width() * 0.13), icon, color);
    let response = if enabled {
        response.on_hover_text(tip)
    } else {
        response.on_hover_text(tr!("not-available-yet", tip = tip))
    };
    enabled && response.clicked()
}

/// A palette's tab strip, as Clip Studio Paint draws it: each tab an icon
/// and its name, the chosen one lit. When the names do not all fit, only
/// the chosen tab keeps its name and the others show their icon (with the
/// name on hover). Returns the body below it and a clicked tab.
fn header(
    ui: &mut Ui,
    rect: Rect,
    tabs: &[(String, Icon, bool)],
    selected: u8,
    theme: &Theme,
) -> (Rect, Option<u8>) {
    let strip = Rect::from_min_size(rect.min, vec2(rect.width(), 28.0));
    let painter = ui.painter().clone();
    painter.rect_filled(strip, 0.0, theme.pill);
    painter.hline(strip.x_range(), strip.bottom(), theme.hairline());
    let font = theme.font(theme.small_size + 0.5);
    let icon_side = 18.0;
    let galleys: Vec<_> = tabs
        .iter()
        .map(|(title, _, _)| painter.layout_no_wrap(title.clone(), font.clone(), theme.text))
        .collect();
    let labelled = |galley: &std::sync::Arc<egui::Galley>| icon_side + 6.0 + galley.size().x + 14.0;
    let bare = icon_side + 14.0;
    let all: f32 = galleys.iter().map(labelled).sum::<f32>() + 2.0 * tabs.len() as f32;
    let names = all <= strip.width() - 8.0;
    let mut x = strip.left() + 4.0;
    let mut clicked = None;
    let painter = painter.with_clip_rect(strip);
    for ((index, (title, icon, enabled)), galley) in tabs.iter().enumerate().zip(galleys) {
        let chosen = index as u8 == selected;
        let named = names || chosen;
        let width = if named { labelled(&galley) } else { bare };
        let tab = Rect::from_min_size(pos2(x, strip.top() + 2.0), vec2(width, 26.0));
        x = tab.right() + 2.0;
        let response = ui.interact(
            tab,
            Id::unique(("sue-paint-tab", title, rect.min.x as i32, rect.min.y as i32)),
            Sense::click(),
        );
        if chosen {
            painter.rect_filled(tab, theme.control_radius, theme.panel);
            painter.hline(
                tab.shrink2(vec2(6.0, 0.0)).x_range(),
                tab.bottom() - 1.0,
                Stroke::new(2.0, theme.border_active),
            );
        } else if *enabled && response.hovered() {
            painter.rect_filled(tab, theme.control_radius, theme.hover);
        }
        let color = if !enabled {
            theme.muted.gamma_multiply(0.5)
        } else if chosen {
            theme.text
        } else {
            theme.muted
        };
        let icon_rect = Rect::from_center_size(
            pos2(tab.left() + 7.0 + icon_side / 2.0, tab.center().y),
            vec2(icon_side, icon_side),
        );
        icons::paint(&painter, icon_rect, *icon, color);
        if named {
            painter.galley(
                pos2(
                    icon_rect.right() + 6.0,
                    tab.center().y - galley.size().y / 2.0,
                ),
                galley,
                color,
            );
        }
        let response = if named {
            response
        } else {
            response.on_hover_text(title)
        };
        if *enabled && response.clicked() {
            clicked = Some(index as u8);
        }
    }
    (
        Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max),
        clicked,
    )
}

/// Clip Studio Paint's wedge slider: a dark triangle that grows to the
/// right, lit up to the value, with a thin bar at it. A widget, so its
/// response says when it changed and when a drag ended.
pub(crate) struct Wedge<'a> {
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    logarithmic: bool,
    width: f32,
    theme: &'a Theme,
}

impl<'a> Wedge<'a> {
    pub(crate) fn new(
        value: &'a mut f32,
        range: std::ops::RangeInclusive<f32>,
        width: f32,
        theme: &'a Theme,
    ) -> Self {
        Self {
            value,
            range,
            logarithmic: false,
            width,
            theme,
        }
    }

    pub(crate) fn logarithmic(mut self) -> Self {
        self.logarithmic = true;
        self
    }

    fn to_unit(&self, value: f32) -> f32 {
        let (low, high) = (*self.range.start(), *self.range.end());
        let t = if self.logarithmic {
            (value.max(low).ln() - low.max(1e-3).ln()) / (high.ln() - low.max(1e-3).ln())
        } else {
            (value - low) / (high - low)
        };
        t.clamp(0.0, 1.0)
    }

    fn value_at(&self, t: f32) -> f32 {
        let (low, high) = (*self.range.start(), *self.range.end());
        if self.logarithmic {
            (low.max(1e-3).ln() + t * (high.ln() - low.max(1e-3).ln())).exp()
        } else {
            low + t * (high - low)
        }
    }
}

impl egui::Widget for Wedge<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, mut response) =
            ui.allocate_exact_size(vec2(self.width.max(24.0), 18.0), Sense::click_and_drag());
        if let Some(pointer) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked())
        {
            let t = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let value = self.value_at(t);
            if (value - *self.value).abs() > f32::EPSILON {
                *self.value = value;
                response.mark_changed();
            }
        }
        let theme = self.theme;
        let wedge = rect.shrink2(vec2(1.0, 2.0));
        let t = self.to_unit(*self.value);
        let x = wedge.left() + t * wedge.width();
        // The wedge's top edge at `x`: a point at the left, full height at
        // the right.
        let top_at = |x: f32| {
            let u = (x - wedge.left()) / wedge.width();
            wedge.bottom() - wedge.height() * (0.08 + 0.92 * u)
        };
        let painter = ui.painter();
        let lit = if response.hovered() || response.dragged() {
            theme.text
        } else {
            theme.text.gamma_multiply(0.78)
        };
        let dark = theme.void;
        let polygon = |from: f32, to: f32, color: Color32| {
            if to - from < 0.5 {
                return;
            }
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(from, wedge.bottom()),
                    pos2(from, top_at(from)),
                    pos2(to, top_at(to)),
                    pos2(to, wedge.bottom()),
                ],
                color,
                Stroke::NONE,
            ));
        };
        polygon(wedge.left(), x, lit);
        polygon(x, wedge.right(), dark);
        painter.line_segment(
            [pos2(x, wedge.top()), pos2(x, wedge.bottom())],
            Stroke::new(1.5, theme.text),
        );
        response
    }
}

/// A tall slider down `lane` below its `label` space: a thin track with a
/// round knob, the top being 1. `fading` draws the track as a fade, for
/// opacity. The new place while it is dragged.
fn tall_slider(
    ui: &mut Ui,
    lane: Rect,
    label: f32,
    at: f32,
    fading: bool,
    salt: &str,
    theme: &Theme,
) -> Option<f32> {
    let track = Rect::from_center_size(
        pos2(
            lane.center().x,
            (lane.top() + label + 8.0 + lane.bottom() - 10.0) / 2.0,
        ),
        vec2(4.0, (lane.height() - label - 18.0).max(20.0)),
    );
    let response = ui.interact(
        track.expand2(vec2(10.0, 8.0)),
        Id::unique(salt),
        Sense::click_and_drag(),
    );
    let painter = ui.painter();
    if fading {
        // Light at the top, fading to nothing at the bottom.
        let mut mesh = Mesh::default();
        let (top, bottom) = (theme.text, theme.void);
        mesh.colored_vertex(track.left_top(), top);
        mesh.colored_vertex(track.right_top(), top);
        mesh.colored_vertex(track.right_bottom(), bottom);
        mesh.colored_vertex(track.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    } else {
        painter.rect_filled(track, 2.0, theme.void);
    }
    let y = track.bottom() - at.clamp(0.0, 1.0) * track.height();
    let knob = pos2(track.center().x, y);
    painter.circle_filled(knob, 7.0, theme.text);
    painter.circle_stroke(knob, 7.0, Stroke::new(1.0, theme.border));
    let pointer = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())?;
    Some(((track.bottom() - pointer.y) / track.height()).clamp(0.0, 1.0))
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

/// A slider row with a dynamics button at its end (Brush Size and Opacity,
/// as in Clip Studio Paint): the button shows whether the pen changes the
/// setting and opens its dynamics. `extra` adds the setting's own options
/// under them. Returns `true` when the value changed.
#[allow(clippy::too_many_arguments, reason = "a slider row and its dynamics")]
fn dynamic_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    log: bool,
    dynamics: &mut Dynamics,
    extra: impl FnOnce(&mut Ui),
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
        ui.spacing_mut().slider_width = (ui.available_width() - 86.0).max(36.0);
        let changed = ui
            .add(
                egui::Slider::new(value, range)
                    .logarithmic(log)
                    .max_decimals(if log { 1 } else { 0 })
                    .suffix(suffix),
            )
            .changed();
        let (rect, button) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
        let on = dynamics.any();
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            3.0,
            if on {
                theme.active
            } else if button.hovered() {
                theme.hover
            } else {
                theme.pill
            },
        );
        // A little pressure graph.
        let inner = rect.shrink2(vec2(5.0, 4.0));
        let curve: Vec<Pos2> = (0..=8)
            .map(|step| {
                let t = step as f32 / 8.0;
                pos2(
                    inner.left() + t * inner.width(),
                    inner.bottom() - t * t * inner.height(),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            curve,
            Stroke::new(1.2, if on { theme.text } else { theme.muted }),
        ));
        let button = button.on_hover_text(tr!("dynamics-what-the-pen-changes", label = label));
        egui::Popup::from_toggle_button_response(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(300.0);
                dynamics_ui(ui, label, dynamics, theme);
                extra(ui);
            });
        changed
    })
    .inner
}

/// Starting and ending (Clip Studio Paint's): how they are measured, how
/// long they are, what they change, and whether speed stretches them.
fn starting_and_ending_ui(ui: &mut Ui, tip: &mut crate::brush::Tip, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    egui::CollapsingHeader::new(small(&tr!("starting-and-ending")))
        .id_salt("starting-and-ending")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    vec2(78.0, 18.0),
                    egui::Label::new(small(&tr!("how-to-specify"))),
                );
                egui::ComboBox::from_id_salt("taper-mode")
                    .selected_text(tip.taper_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in TaperMode::ALL {
                            ui.selectable_value(&mut tip.taper_mode, mode, mode.label());
                        }
                    });
            });
            let (unit, top) = match tip.taper_mode {
                TaperMode::Percent => (" %".to_owned(), 100.0),
                TaperMode::Length | TaperMode::Fade => (tr!("px"), 400.0),
            };
            slider_row(
                ui,
                &tr!("starting-2"),
                &mut tip.taper_start,
                0.0..=top,
                &unit,
                false,
                theme,
            );
            let ending = if tip.taper_mode == TaperMode::Fade {
                tr!("fade-by")
            } else {
                tr!("ending")
            };
            slider_row(
                ui,
                &ending,
                &mut tip.taper_end,
                0.0..=top,
                &unit,
                false,
                theme,
            );
            ui.horizontal(|ui| {
                ui.checkbox(&mut tip.taper_size, small(&tr!("brush-size-2")));
                ui.checkbox(&mut tip.taper_opacity, small(&tr!("opacity")));
            });
            ui.checkbox(
                &mut tip.taper_by_speed,
                small(&tr!("starting-and-ending-by-speed")),
            )
            .on_hover_text(tr!("quicker-strokes-get-longer-starts-ends"));
        });
}

/// Color Jitter (Clip Studio Paint's): every dab, or every stroke, in a
/// slightly different color.
fn color_jitter_ui(ui: &mut Ui, jitter: &mut Jitter, stamps: bool, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    let amount = |ui: &mut Ui, label: &str, value: &mut f32, top: f32, unit: &str, on: bool| {
        let mut shown = *value * top;
        ui.add_enabled_ui(on, |ui| {
            if slider_row(ui, label, &mut shown, 0.0..=top, unit, false, theme) {
                *value = shown / top;
            }
        });
    };
    egui::CollapsingHeader::new(small(&tr!("color-jitter")))
        .id_salt("color-jitter")
        .show(ui, |ui| {
            ui.add_enabled_ui(stamps, |ui| {
                ui.checkbox(&mut jitter.tip, small(&tr!("change-brush-tip-color")))
                    .on_disabled_hover_text(tr!("brushes-stamp-build-up-color-mixing"));
            });
            let on = jitter.tip && stamps;
            amount(ui, &tr!("hue"), &mut jitter.hue, 180.0, &tr!("deg"), on);
            amount(
                ui,
                &tr!("saturation"),
                &mut jitter.saturation,
                100.0,
                " %",
                on,
            );
            amount(ui, &tr!("luminosity"), &mut jitter.value, 100.0, " %", on);
            amount(ui, &tr!("blend-sub"), &mut jitter.sub, 100.0, " %", on);
            ui.checkbox(&mut jitter.stroke, small(&tr!("randomize-per-stroke")));
            let on = jitter.stroke;
            amount(
                ui,
                &tr!("hue"),
                &mut jitter.stroke_hue,
                180.0,
                &tr!("deg"),
                on,
            );
            amount(
                ui,
                &tr!("saturation"),
                &mut jitter.stroke_saturation,
                100.0,
                " %",
                on,
            );
            amount(
                ui,
                &tr!("luminosity"),
                &mut jitter.stroke_value,
                100.0,
                " %",
                on,
            );
        });
}

/// The dynamics of one setting: which inputs change it, down to what, and
/// their curves (Clip Studio Paint's Brush Size Dynamics).
fn dynamics_ui(ui: &mut Ui, label: &str, dynamics: &mut Dynamics, theme: &Theme) {
    let small = |text: &str| {
        RichText::new(text)
            .size(theme.small_size + 0.5)
            .color(theme.muted)
    };
    ui.label(
        RichText::new(tr!("dynamics", label = label))
            .size(theme.ui_size)
            .color(theme.text),
    );
    ui.label(small(&tr!("input-affecting", label = label)));
    let input = |ui: &mut Ui, name: &str, on: &mut bool, min: &mut f32| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(96.0, 18.0), egui::Checkbox::new(on, small(name)));
            let mut percent = *min * 100.0;
            ui.spacing_mut().slider_width = (ui.available_width() - 56.0).max(36.0);
            let slider = egui::Slider::new(&mut percent, 0.0..=100.0)
                .max_decimals(0)
                .suffix(" %");
            if ui
                .add_enabled(*on, slider)
                .on_hover_text(tr!("minimum-value-how-much-left-least"))
                .changed()
            {
                *min = percent / 100.0;
            }
        });
    };
    input(
        ui,
        &tr!("pen-pressure"),
        &mut dynamics.pressure,
        &mut dynamics.pressure_min,
    );
    input(
        ui,
        &tr!("velocity"),
        &mut dynamics.velocity,
        &mut dynamics.velocity_min,
    );
    input(
        ui,
        &tr!("random"),
        &mut dynamics.random,
        &mut dynamics.random_min,
    );
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(small(&tr!("pen-pressure-settings")));
            ui.add_enabled_ui(dynamics.pressure, |ui| {
                sue_wm::widgets::response_curve(
                    ui,
                    (label, "pressure"),
                    &mut dynamics.pressure_curve,
                    92.0,
                    sue_wm::widgets::CurveLabels::pressure(),
                    theme,
                );
            });
        });
        ui.vertical(|ui| {
            ui.label(small(&tr!("velocity-settings")));
            ui.add_enabled_ui(dynamics.velocity, |ui| {
                sue_wm::widgets::response_curve(
                    ui,
                    (label, "velocity"),
                    &mut dynamics.velocity_curve,
                    92.0,
                    sue_wm::widgets::CurveLabels {
                        input: tr!("velocity"),
                        low: tr!("still"),
                        high: tr!("fast"),
                        output: tr!("output"),
                    },
                    theme,
                );
            });
        });
    });
}

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
        let groups = BAR_GROUPS;
        let mut x = strip.left() + 8.0;
        let mut clicked: Option<String> = None;
        let mut save_at = None;
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
                if self.bar_hidden.contains(*command) {
                    continue;
                }
                let rect = Rect::from_min_size(pos2(x, strip.top() + 3.0), vec2(32.0, 32.0));
                x += 34.0;
                if *command == "file.save" {
                    save_at = Some(rect);
                }
                let state = self.state(command, &cx.world.arena, cx.history);
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
            let rect = Rect::from_min_size(
                pos2(x + 4.0, strip.top() + 4.0),
                vec2(galley.size().x + 14.0, 26.0),
            );
            x = rect.right() + 2.0;
            let state = self.state(command, &cx.world.arena, cx.history);
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
            pos2(save_button.right() - 2.0, strip.top() + 9.0),
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
        // As roomy as the strip allows, every tool always in it.
        let room = strip.height() - 6.0 - 70.0;
        let step = (room / Tool::ALL.len() as f32).clamp(24.0, 36.0);
        for tool in Tool::ALL {
            let rect = Rect::from_min_size(pos2(strip.left() + 4.0, y), vec2(38.0, step - 2.0));
            y += step;
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
            &tr!("swap-main-sub-color-x"),
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
            &tr!("sub-color"),
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
            &tr!("main-color"),
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
            .on_hover_text(tr!("transparent-color-brushes-erase-c"))
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
            (tr!("sub-tool"), Icon::SubTool, self.panels.sub_tool),
            (
                tr!("tool-property"),
                Icon::Sliders,
                self.panels.tool_property,
            ),
            (tr!("brush-size"), Icon::BrushSize, true),
        ];
        let (body, clicked) = header(ui, rect, &tabs, self.tool_tab, theme);
        if let Some(tab) = clicked {
            self.tool_tab = tab;
        }
        if !self.panels.sub_tool && self.tool_tab == 0 {
            self.tool_tab = 1;
        } else if !self.panels.tool_property && self.tool_tab == 1 {
            self.tool_tab = 0;
        }
        if self.tool_tab == 2 {
            self.brush_size_grid(ui, body.shrink2(vec2(6.0, 6.0)), theme);
            return;
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
                .map(|sub| crate::localize(&sub.name))
                .collect();
            let tips: Vec<Option<crate::brush::Tip>> = self
                .toolbox
                .subs(tool)
                .iter()
                .map(|sub| tool.paints().then_some(sub.tip))
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("sue-paint-subs")
                .show(&mut inner, |ui| {
                    let mut action = None;
                    for (index, name) in names.iter().enumerate() {
                        let chosen = index == picked;
                        // A painting sub tool is a row with its own sample
                        // stroke and its name, as in Clip Studio Paint.
                        let tall = if tips[index].is_some() { 44.0 } else { 26.0 };
                        let (row, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), tall), Sense::click());
                        let painter = ui.painter();
                        let fill = if chosen {
                            theme.active
                        } else if response.hovered() {
                            theme.hover
                        } else {
                            theme.pill
                        };
                        painter.rect_filled(
                            row.shrink2(vec2(0.0, 1.0)),
                            theme.control_radius,
                            fill,
                        );
                        if let Some(tip) = tips[index] {
                            let size = [
                                (row.width().round() as u32).clamp(40, 600),
                                (tall as u32) - 8,
                            ];
                            if let Some(texture) =
                                Self::sub_preview(&mut self.sub_previews, cx, tip, size)
                            {
                                ui.painter().image(
                                    texture,
                                    Rect::from_min_size(
                                        row.min + vec2(0.0, 2.0),
                                        vec2(size[0] as f32, size[1] as f32),
                                    ),
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    if chosen {
                                        theme.text
                                    } else {
                                        theme.text.gamma_multiply(0.85)
                                    },
                                );
                            }
                            ui.painter().text(
                                row.right_bottom() + vec2(-6.0, -4.0),
                                Align2::RIGHT_BOTTOM,
                                name,
                                theme.font(theme.small_size + 0.5),
                                theme.text,
                            );
                        } else {
                            ui.painter().text(
                                row.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                name,
                                theme.font(theme.small_size + 0.5),
                                theme.text,
                            );
                        }
                        if response.clicked() {
                            self.toolbox.pick_sub(tool, index);
                            self.tool_tab = 1;
                        }
                        // Clip Studio Paint's sub tool menu.
                        response.context_menu(|ui| {
                            for (what, label) in [
                                (0, tr!("duplicate-sub-tool")),
                                (1, tr!("delete-sub-tool")),
                                (2, tr!("reset-to-initial-settings")),
                            ] {
                                if ui
                                    .add_enabled(
                                        what != 1 || names.len() > 1,
                                        egui::Button::new(label),
                                    )
                                    .clicked()
                                {
                                    action = Some((what, index));
                                    ui.close();
                                }
                            }
                        });
                    }
                    match action {
                        Some((0, index)) => self.toolbox.duplicate_sub(tool, index),
                        Some((1, index)) => {
                            self.toolbox.delete_sub(tool, index);
                        }
                        Some((_, index)) => {
                            self.toolbox.reset_sub(tool, index);
                        }
                        None => {}
                    }
                });
            return;
        }
        // Tool Property.
        let sub_name = crate::localize(&self.toolbox.current().name);
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
                            .map(|sub| crate::localize(&sub.name))
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
        let command = egui::ScrollArea::vertical()
            .id_salt("sue-paint-props")
            .show(&mut inner, |ui| {
                self.tool_properties(ui, tool, &cx.world.arena, theme)
            })
            .inner;
        if let Some(command) = command {
            self.run(command, cx);
        }
        // A 3D layer's view from Tool Property: shown now, kept when let go.
        if let Some((layer, view)) = self.pending_view3d.take() {
            self.preview_view3d(cx, layer, &view);
        }
        if std::mem::take(&mut self.view3d_release)
            && let Some((layer, base, before)) = self.view3d_before.take()
        {
            self.finish_view3d(cx, layer, base, &before);
        }
    }

    /// History, as Clip Studio Paint lists it: the start, then every step
    /// to the current one (lit) and the steps undone after it (dimmed); a
    /// click on any of them goes back or forward to it.
    fn history_tab(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        use sue_core::NodeId;
        let history = &*cx.history;
        let cursor = history.cursor();
        let mut done = Vec::new();
        let mut node = cursor;
        while node != NodeId::ROOT {
            done.push(node);
            node = history.parent(node).unwrap_or(NodeId::ROOT);
        }
        done.reverse();
        let mut undone = Vec::new();
        let mut next = history.preferred_child(cursor);
        while let Some(node) = next {
            undone.push(node);
            next = history.preferred_child(node);
        }
        let start = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("picture"), |entity| entity.name().to_owned());
        let rows: Vec<(NodeId, String, bool)> = std::iter::once((NodeId::ROOT, start, true))
            .chain(done.iter().map(|node| {
                (
                    *node,
                    history.label(*node).unwrap_or_default().to_owned(),
                    true,
                )
            }))
            .chain(undone.iter().map(|node| {
                (
                    *node,
                    history.label(*node).unwrap_or_default().to_owned(),
                    false,
                )
            }))
            .collect();
        let mut go = None;
        let mut inner = child(ui, body, "layer-history");
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-history")
            .stick_to_bottom(true)
            .show(&mut inner, |ui| {
                for (node, label, applied) in &rows {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                    let painter = ui.painter();
                    if *node == cursor {
                        painter.rect_filled(row, 0.0, theme.active);
                    } else if response.hovered() {
                        painter.rect_filled(row, 0.0, theme.hover);
                    }
                    painter.hline(row.x_range(), row.bottom(), theme.hairline());
                    painter.text(
                        row.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        label,
                        theme.font(theme.small_size + 0.5),
                        if *applied {
                            theme.text
                        } else {
                            theme.muted.gamma_multiply(0.6)
                        },
                    );
                    if response.clicked() && *node != cursor {
                        go = Some(*node);
                    }
                }
            });
        if let Some(node) = go {
            cx.typing.flush(cx.history);
            if let Err(error) = cx.history.checkout(cx.world, node) {
                cx.fail(tr!("undo-failed", error = error));
            }
        }
    }

    /// The texture of `tip`'s sample stroke for the Sub Tool list, `size`
    /// pixels: white ink on clear, tinted when drawn.
    fn sub_preview(
        cache: &mut std::collections::HashMap<u64, Vec<u8>>,
        cx: &mut AppletContext<'_>,
        mut tip: crate::brush::Tip,
        size: [u32; 2],
    ) -> Option<egui::TextureId> {
        use std::hash::{Hash, Hasher};
        // Thin pens stay thin and big brushes fit the row.
        tip.size = (tip.size * 0.7).clamp(1.0, size[1] as f32 * 0.4);
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{tip:?}{size:?}").hash(&mut hasher);
            hasher.finish()
        };
        if !cache.contains_key(&key) {
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(key, Self::render_stroke(tip, size)?);
        }
        let pixels = cache.get(&key)?;
        cx.textures
            .texture(0x5355_4253_5452_4b45, key, size, 4, pixels, false)
    }

    /// A sample stroke with `tip` in white over nothing, `size` pixels.
    fn render_stroke(tip: crate::brush::Tip, [width, height]: [u32; 2]) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        let mut tip = tip;
        // An eraser's sample is drawn as ink, so it shows.
        tip.eraser = false;
        let mut stroke = BrushStroke::new(sue_core::EntityId::NIL, tip, [1.0, 1.0, 1.0], false);
        for step in 0..=48 {
            let t = step as f32 / 48.0;
            let x = 10.0 + t * (width as f32 * 0.62);
            let y = height as f32 * (0.42 - 0.2 * (t * std::f32::consts::TAU * 0.8).sin());
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            stroke.to(&mut raster, [x, y], pressure, None);
        }
        stroke.end(&mut raster, crate::brush::Finish::default(), None);
        raster.bytes().map(<[u8]>::to_vec)
    }

    /// Brush Size: Clip Studio Paint's grid of sizes, each a dot of its
    /// size (up to the cell) over its number; a click takes it.
    fn brush_size_grid(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        const SIZES: [f32; 40] = [
            0.7, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 15.0, 17.0, 20.0,
            25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0, 120.0, 150.0, 170.0, 200.0, 250.0,
            300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 1000.0, 1200.0, 1500.0, 1700.0, 2000.0,
        ];
        let tool = self.toolbox.tool();
        if !tool.paints() && !matches!(tool, Tool::Fill | Tool::Gradient) {
            ui.painter().text(
                body.center_top() + vec2(0.0, 16.0),
                Align2::CENTER_CENTER,
                tr!("no-brush-size-for-this-tool"),
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            return;
        }
        let columns = ((body.width() / 34.0).floor() as usize).clamp(3, 8);
        let cell = body.width() / columns as f32;
        let now = self.toolbox.current().tip.size;
        let mut take = None;
        let painter = ui.painter().with_clip_rect(body);
        for (index, size) in SIZES.iter().enumerate() {
            let (row, column) = (index / columns, index % columns);
            let rect = Rect::from_min_size(
                pos2(
                    body.left() + column as f32 * cell,
                    body.top() + row as f32 * (cell + 4.0),
                ),
                vec2(cell, cell + 4.0),
            )
            .shrink(1.0);
            if rect.top() > body.bottom() {
                break;
            }
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-size-cell", index)),
                Sense::click(),
            );
            let chosen = (now - size).abs() < 0.01;
            if chosen {
                painter.rect_filled(rect, theme.control_radius, theme.active);
            } else if response.hovered() {
                painter.rect_filled(rect, theme.control_radius, theme.hover);
            }
            let dot = rect.shrink2(vec2(0.0, 7.0)).translate(vec2(0.0, -5.0));
            // Sizes up to 30 grow; larger ones fill the cell.
            let radius =
                (size / 30.0).clamp(0.0, 1.0).sqrt() * dot.height().min(dot.width()) * 0.48;
            painter.circle_filled(dot.center(), radius.max(0.8), theme.text);
            painter.text(
                rect.center_bottom() - vec2(0.0, 1.0),
                Align2::CENTER_BOTTOM,
                if size.fract() == 0.0 {
                    format!("{size:.0}")
                } else {
                    format!("{size}")
                },
                theme.font(theme.small_size - 1.0),
                theme.muted,
            );
            if response.clicked() {
                take = Some(*size);
            }
        }
        if let Some(size) = take {
            self.toolbox.current_mut().tip.size = size;
        }
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

    /// The settings of the current sub tool, and a command they ask for.
    #[allow(clippy::too_many_lines, reason = "one block per tool")]
    fn tool_properties(
        &mut self,
        ui: &mut Ui,
        tool: Tool,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        if tool == Tool::Operation
            && let Some(folder) = self.camera_folder(arena)
        {
            return self.camera_properties(ui, folder, arena, theme);
        }
        if tool == Tool::Operation
            && let Some(layer) = self
                .layer()
                .filter(|layer| crate::three::is_3d(arena, *layer))
        {
            self.view3d_properties(ui, layer, arena, theme);
            return None;
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        let mut command = None;
        let sub = self.toolbox.current_mut();
        match tool {
            tool if tool.paints() => {
                if tool == Tool::Eraser {
                    // How it erases lines on vector layers.
                    ui.horizontal(|ui| {
                        ui.label(small(&tr!("vector")));
                        egui::ComboBox::from_id_salt("sue-paint-vector-eraser")
                            .selected_text(crate::localize(
                                crate::vector::Erase::NAMES[usize::from(sub.kind.min(2))],
                            ))
                            .show_ui(ui, |ui| {
                                for (kind, name) in crate::vector::Erase::NAMES.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        kind as u8,
                                        crate::localize(name),
                                    );
                                }
                            });
                    });
                }
                let tip = &mut sub.tip;
                let (mut screen_size, mut at_least_one) = (tip.screen_size, tip.at_least_one);
                dynamic_row(
                    ui,
                    &tr!("brush-size"),
                    &mut tip.size,
                    0.5..=1000.0,
                    &tr!("px"),
                    true,
                    &mut tip.size_by,
                    |ui| {
                        ui.separator();
                        ui.checkbox(&mut screen_size, small(&tr!("specify-by-size-on-screen")))
                            .on_hover_text(tr!("brush-looks-same-size-any-zoom"));
                        ui.checkbox(&mut at_least_one, small(&tr!("at-least-1-pixel")))
                            .on_hover_text(tr!("light-pressure-never-thins-line-below"));
                    },
                    theme,
                );
                tip.screen_size = screen_size;
                tip.at_least_one = at_least_one;
                let mut opacity = tip.opacity * 100.0;
                if dynamic_row(
                    ui,
                    &tr!("opacity"),
                    &mut opacity,
                    0.0..=100.0,
                    " %",
                    false,
                    &mut tip.opacity_by,
                    |_| {},
                    theme,
                ) {
                    tip.opacity = opacity / 100.0;
                }
                let mut hardness = tip.hardness * 100.0;
                if slider_row(
                    ui,
                    &tr!("hardness"),
                    &mut hardness,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    tip.hardness = hardness / 100.0;
                }
                ui.horizontal(|ui| {
                    ui.add_sized(
                        vec2(78.0, 18.0),
                        egui::Label::new(small(&tr!("anti-aliasing"))),
                    );
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
                    &tr!("stabilization"),
                    &mut stabilization,
                    0.0..=100.0,
                    "",
                    false,
                    theme,
                ) {
                    sub.stabilization = stabilization.round() as u32;
                }
                ui.checkbox(&mut sub.adjust_by_speed, small(&tr!("adjust-by-speed")))
                    .on_hover_text(tr!("less-smoothing-as-pen-moves-faster"));
                let mut correction = sub.correction as f32;
                if slider_row(
                    ui,
                    &tr!("correction"),
                    &mut correction,
                    0.0..=10.0,
                    "",
                    false,
                    theme,
                ) {
                    sub.correction = correction.round() as u32;
                }
                ui.checkbox(&mut sub.flick, small(&tr!("flick-into-a-point")))
                    .on_hover_text(tr!("quick-stroke-carries-along-pens-momentum"));
                let tip = &mut sub.tip;
                let shapes = crate::brush::shapes();
                if !shapes.is_empty() || tip.shape != 0 {
                    ui.horizontal(|ui| {
                        ui.label(small(&tr!("brush-tip")));
                        let now = shapes
                            .iter()
                            .find(|(id, _)| *id == tip.shape)
                            .map_or_else(|| tr!("round"), |(_, name)| name.clone());
                        egui::ComboBox::from_id_salt("sue-paint-tip-shape")
                            .selected_text(now)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut tip.shape, 0, tr!("round"));
                                for (id, name) in &shapes {
                                    ui.selectable_value(&mut tip.shape, *id, name);
                                }
                            });
                    });
                }
                starting_and_ending_ui(ui, tip, theme);
                let stamps = tip.dabbed();
                color_jitter_ui(ui, &mut tip.jitter, stamps, theme);
                if tip.dabbed() {
                    let mut spacing = tip.spacing * 100.0;
                    if slider_row(
                        ui,
                        &tr!("spacing"),
                        &mut spacing,
                        1.0..=200.0,
                        " %",
                        true,
                        theme,
                    ) {
                        tip.spacing = spacing / 100.0;
                    }
                }
                ui.checkbox(&mut tip.build_up, small(&tr!("build-up-airbrush")));
                if tip.build_up {
                    let mut flow = tip.flow * 100.0;
                    if slider_row(ui, &tr!("flow"), &mut flow, 1.0..=100.0, " %", false, theme) {
                        tip.flow = flow / 100.0;
                    }
                }
                if !tip.eraser {
                    let mut mix = tip.mix * 100.0;
                    if slider_row(
                        ui,
                        &tr!("color-mixing"),
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
                    &tr!("paper-grain"),
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
                    egui::Checkbox::new(&mut false, small(&tr!("vector-magnet"))),
                );
                if tool == Tool::Figure {
                    ui.label(small(&tr!("drag-canvas-draw-shape")));
                }
            }
            Tool::Fill | Tool::AutoSelect => {
                let mut tolerance = sub.tolerance * 100.0;
                if slider_row(
                    ui,
                    &tr!("tolerance"),
                    &mut tolerance,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    sub.tolerance = tolerance / 100.0;
                }
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("refer")));
                    ui.selectable_value(&mut sub.mode, 0, tr!("editing-layer"));
                    ui.selectable_value(&mut sub.mode, 1, tr!("all-layers"));
                    ui.selectable_value(&mut sub.mode, 2, tr!("reference-layer"));
                });
                if tool == Tool::Fill {
                    ui.horizontal(|ui| {
                        ui.label(small(&tr!("close-gap")));
                        for (level, name) in [
                            tr!("none"),
                            tr!("weak"),
                            tr!("medium"),
                            tr!("strong"),
                            tr!("max"),
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.selectable_value(&mut sub.gap, level as u32, name.as_str());
                        }
                    })
                    .response
                    .on_hover_text(tr!("fills-hold-inside-lines-small-breaks"));
                    let mut spread = sub.spread as f32;
                    if slider_row(
                        ui,
                        &tr!("area-scaling"),
                        &mut spread,
                        0.0..=20.0,
                        &tr!("px"),
                        false,
                        theme,
                    ) {
                        sub.spread = spread.round() as u32;
                    }
                }
                if tool == Tool::Fill {
                    let mut opacity = sub.tip.opacity * 100.0;
                    if slider_row(
                        ui,
                        &tr!("opacity"),
                        &mut opacity,
                        0.0..=100.0,
                        " %",
                        false,
                        theme,
                    ) {
                        sub.tip.opacity = opacity / 100.0;
                    }
                    ui.add_enabled(
                        false,
                        egui::Checkbox::new(&mut false, small(&tr!("close-gap"))),
                    );
                } else {
                    self.select_ops(ui, theme);
                }
            }
            Tool::Gradient => {
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("shape-2")));
                    let mut radial = sub.mode == 2;
                    ui.selectable_value(&mut radial, false, tr!("linear"));
                    ui.selectable_value(&mut radial, true, tr!("radial"));
                    if radial {
                        sub.mode = 2;
                    } else if sub.mode == 2 {
                        sub.mode = 0;
                    }
                });
                if sub.mode != 2 {
                    ui.horizontal(|ui| {
                        ui.label(small(&tr!("colors-2")));
                        ui.selectable_value(&mut sub.mode, 0, tr!("to-transparent"));
                        ui.selectable_value(&mut sub.mode, 1, tr!("main-to-sub"));
                    });
                }
                let mut opacity = sub.tip.opacity * 100.0;
                if slider_row(
                    ui,
                    &tr!("opacity"),
                    &mut opacity,
                    0.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    sub.tip.opacity = opacity / 100.0;
                }
            }
            Tool::Select if matches!(sub.mode, 4 | 5) => {
                slider_row(
                    ui,
                    &tr!("brush-size"),
                    &mut sub.tip.size,
                    1.0..=500.0,
                    &tr!("px"),
                    true,
                    theme,
                );
                ui.label(small(&if sub.mode == 4 {
                    tr!("paint-what-to-select")
                } else {
                    tr!("paint-what-to-leave-out")
                }));
            }
            Tool::Select if sub.mode == 3 => {
                ui.label(small(&tr!("click-corners-click-first-again-press")));
            }
            Tool::Select => {
                ui.horizontal(|ui| {
                    ui.label(small(&tr!("shape-2")));
                    ui.selectable_value(&mut sub.mode, 0, tr!("rectangle"));
                    ui.selectable_value(&mut sub.mode, 1, tr!("ellipse"));
                    ui.selectable_value(&mut sub.mode, 2, tr!("lasso"));
                });
                self.select_ops(ui, theme);
            }
            Tool::Eyedropper => {
                ui.selectable_value(&mut sub.mode, 0, tr!("obtain-display-color"));
                ui.selectable_value(&mut sub.mode, 1, tr!("obtain-layer-color"));
            }
            Tool::Zoom => {
                ui.selectable_value(&mut sub.mode, 0, tr!("zoom-in-on-click"));
                ui.selectable_value(&mut sub.mode, 1, tr!("zoom-out-on-click"));
                ui.label(small(&tr!("alt-click-does-the-opposite")));
            }
            Tool::Hand => {
                ui.selectable_value(&mut sub.mode, 0, tr!("scroll"));
                ui.selectable_value(&mut sub.mode, 1, tr!("rotate"));
            }
            Tool::MoveLayer => {
                ui.label(small(&tr!("drag-move-current-layer")));
            }
            Tool::Ruler => {
                match sub.mode {
                    6 => {
                        ui.label(small(&tr!("click-place-points-click-last-one")));
                        ui.label(small(&tr!("ctrl-1-snap-ruler-turns-rulers")));
                    }
                    7 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("figure")));
                            ui.selectable_value(&mut sub.kind, 0, tr!("rectangle"));
                            ui.selectable_value(&mut sub.kind, 1, tr!("ellipse"));
                        });
                        ui.label(small(&tr!("drag-lay-ruler-strokes-start-follow")));
                    }
                    8 => {
                        ui.label(small(&tr!("draw-ruler-freehand-strokes-start-follow")));
                    }
                    5 => {
                        ui.label(small(&tr!("drag-lay-straight-ruler-strokes-start")));
                        ui.label(small(&tr!("ctrl-1-snap-ruler-turns-off")));
                    }
                    2 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("special-ruler")));
                            egui::ComboBox::from_id_salt("sue-paint-special-ruler")
                                .selected_text(crate::localize(
                                    crate::rulers::Special::NAMES[usize::from(sub.kind.min(2))],
                                ))
                                .show_ui(ui, |ui| {
                                    for (kind, name) in
                                        crate::rulers::Special::NAMES.iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut sub.kind,
                                            kind as u8,
                                            crate::localize(name),
                                        );
                                    }
                                });
                        });
                        ui.label(small(&match sub.kind {
                            0 => tr!("drag-set-lines-direction-every-stroke"),
                            1 => tr!("click-middle-every-stroke-runs-toward"),
                            _ => tr!("click-middle-every-stroke-goes-round"),
                        }));
                        ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
                    }
                    1 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("process")));
                            egui::ComboBox::from_id_salt("sue-paint-perspective-process")
                                .selected_text(if sub.kind == 1 {
                                    tr!("delete-vanishing-point-2")
                                } else {
                                    tr!("add-vanishing-point-2")
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        0,
                                        tr!("add-vanishing-point-2"),
                                    );
                                    ui.selectable_value(
                                        &mut sub.kind,
                                        1,
                                        tr!("delete-vanishing-point-2"),
                                    );
                                });
                        });
                        ui.checkbox(&mut sub.fix_eye_level, small(&tr!("fix-eye-level")))
                            .on_hover_text(tr!("vanishing-points-slide-along-eye-level"));
                        ui.label(small(&if sub.kind == 1 {
                            tr!("click-vanishing-point-take-away")
                        } else {
                            tr!("drag-along-two-edges-meet-distance")
                        }));
                        if ui
                            .button(tr!("create-perspective-ruler-ellipsis"))
                            .clicked()
                        {
                            command = Some("layer.ruler.perspective");
                        }
                    }
                    _ => {
                        ui.label(small(&tr!("drag-canvas-lay-ruler-strokes-draw")));
                        let mut lines = sub.lines as f32;
                        if slider_row(
                            ui,
                            &tr!("number-of-lines"),
                            &mut lines,
                            2.0..=32.0,
                            "",
                            false,
                            theme,
                        ) {
                            sub.lines = lines.round() as u32;
                        }
                        ui.checkbox(&mut sub.mirror, small(&tr!("line-symmetry")))
                            .on_hover_text(tr!("every-other-line-mirrored-as-clip"));
                        ui.label(small(&tr!("ctrl-2-snap-special-ruler-turns")));
                    }
                }
                if ui.button(tr!("delete-ruler")).clicked() {
                    command = Some("layer.ruler.delete");
                }
            }
            Tool::CorrectLine => {
                let size = |ui: &mut Ui, sub: &mut crate::tools::SubTool| {
                    slider_row(
                        ui,
                        &tr!("brush-size"),
                        &mut sub.tip.size,
                        1.0..=500.0,
                        &tr!("px"),
                        true,
                        theme,
                    );
                };
                let levels = |ui: &mut Ui, name: &str, level: &mut u32| {
                    ui.horizontal(|ui| {
                        ui.label(small(name));
                        for value in 1..=5 {
                            ui.selectable_value(level, value, value.to_string());
                        }
                    });
                };
                match sub.mode {
                    0 => {
                        for (kind, name) in [
                            tr!("move-control-points"),
                            tr!("add-control-point"),
                            tr!("delete-control-point"),
                            tr!("switch-corner"),
                            tr!("adjust-line-width"),
                            tr!("adjust-opacity"),
                            tr!("split-line"),
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.radio_value(&mut sub.kind, kind as u8, name.as_str());
                        }
                        ui.label(small(&match sub.kind {
                            1 => tr!("click-line-add-control-point"),
                            2 => tr!("click-control-point-delete"),
                            3 => tr!("click-control-point-make-corner-smooth"),
                            4 => tr!("drag-control-point-up-widen-line"),
                            5 => tr!("drag-control-point-up-more-opacity"),
                            6 => tr!("click-line-split-two-there"),
                            _ => tr!("drag-control-point-move"),
                        }));
                    }
                    1 => {
                        ui.horizontal(|ui| {
                            ui.label(small(&tr!("fix-end")));
                            ui.selectable_value(&mut sub.kind, 0, tr!("both-ends"));
                            ui.selectable_value(&mut sub.kind, 1, tr!("free"));
                        });
                        levels(ui, &tr!("pinch-level"), &mut sub.level);
                        slider_row(
                            ui,
                            &tr!("effect-range"),
                            &mut sub.amount,
                            1.0..=500.0,
                            &tr!("px"),
                            true,
                            theme,
                        );
                        ui.label(small(&tr!("drag-line-pull-into-shape")));
                    }
                    2 => {
                        levels(ui, &tr!("simplify"), &mut sub.level);
                        ui.checkbox(&mut sub.whole, small(&tr!("delete-short-lines")));
                        size(ui, sub);
                    }
                    3 => {
                        ui.label(small(&tr!("brush-over-ends-two-lines-join")));
                        size(ui, sub);
                    }
                    4 => {
                        for (kind, name) in crate::correct::Widen::NAMES.iter().enumerate() {
                            ui.radio_value(&mut sub.kind, kind as u8, crate::localize(name));
                        }
                        if matches!(sub.kind, 0 | 1 | 4) {
                            slider_row(
                                ui,
                                &tr!("width-2"),
                                &mut sub.amount,
                                0.1..=100.0,
                                &tr!("px"),
                                true,
                                theme,
                            );
                        }
                        ui.checkbox(&mut sub.whole, small(&tr!("process-whole-line")));
                        size(ui, sub);
                    }
                    5 => {
                        let mut level = sub.stabilization as f32;
                        if slider_row(
                            ui,
                            &tr!("stabilization"),
                            &mut level,
                            0.0..=100.0,
                            "",
                            false,
                            theme,
                        ) {
                            sub.stabilization = level.round() as u32;
                        }
                        ui.label(small(&tr!("trace-over-part-line-draw-again")));
                    }
                    6 => {
                        size(ui, sub);
                        ui.label(small(&tr!("trace-over-line-width-follows-your")));
                    }
                    _ => {
                        slider_row(
                            ui,
                            &tr!("dust-size"),
                            &mut sub.amount,
                            1.0..=100.0,
                            &tr!("px"),
                            false,
                            theme,
                        );
                        ui.label(small(&tr!("drag-box-remove-specks-click-whole")));
                    }
                }
                ui.label(small(&tr!("correct-line-works-vector-layers-remove")));
            }
            Tool::Blend => {
                slider_row(
                    ui,
                    &tr!("brush-size"),
                    &mut sub.tip.size,
                    2.0..=600.0,
                    &tr!("px"),
                    true,
                    theme,
                );
                let mut strength = sub.tip.opacity * 100.0;
                if slider_row(
                    ui,
                    &tr!("strength"),
                    &mut strength,
                    1.0..=100.0,
                    " %",
                    false,
                    theme,
                ) {
                    sub.tip.opacity = strength / 100.0;
                }
                ui.label(small(&match sub.mode {
                    0 => tr!("softens-what-under-brush"),
                    1 => tr!("drags-color-along-the-stroke"),
                    2 => tr!("moves-picture-along-stroke"),
                    3 => tr!("swells-picture-brushs-middle"),
                    4 => tr!("draws-picture-into-brushs-middle"),
                    _ => tr!("turns-picture-around-brushs-middle"),
                }));
            }
            Tool::Frame => {
                ui.label(small(&if sub.mode == 1 {
                    tr!("drag-line-across-panel-divide")
                } else {
                    tr!("drag-draw-rectangle-panel")
                }));
            }
            Tool::Balloon => {
                let mut line = sub.tip.size;
                if slider_row(
                    ui,
                    &tr!("line-width-2"),
                    &mut line,
                    1.0..=20.0,
                    &tr!("px"),
                    false,
                    theme,
                ) {
                    sub.tip.size = line;
                }
                ui.label(small(&if sub.mode == 1 {
                    tr!("drag-inside-balloon-where-tail-points")
                } else {
                    tr!("drag-to-draw-a-balloon")
                }));
            }
            _ => {
                ui.label(small(&tr!("tool-arrives-later-sue-paint")));
            }
        }
        command
    }

    /// The Material palette: Sue Paint's tones and patterns and the
    /// pictures registered, to double-click onto the canvas.
    pub(crate) fn material_panel(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let library = self.material_kinds.scan(folder.as_deref());
        let mut inner = child(ui, body, "materials");
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size)
                .color(theme.muted)
        };
        // All, then every kind registered.
        let kinds: Vec<(String, String)> = self
            .material_kinds
            .iter()
            .map(|kind| (kind.id.clone(), crate::localize(&kind.name)))
            .collect();
        inner.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.material_kind, 0, tr!("all"));
            for (index, (_, name)) in kinds.iter().enumerate() {
                ui.selectable_value(&mut self.material_kind, index as u8 + 1, name.as_str());
            }
        });
        let chosen = usize::from(self.material_kind)
            .checked_sub(1)
            .and_then(|index| kinds.get(index))
            .map(|(id, _)| id.as_str());
        let shown: Vec<&crate::materials::Material> = library
            .iter()
            .filter(|material| chosen.is_none_or(|id| material.kind == id))
            .collect();
        let mut paste = None;
        let mut delete = None;
        let side = 52.0;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-materials")
            .max_height((body.height() - 70.0).max(40.0))
            .show(&mut inner, |ui| {
                let columns = ((ui.available_width() / (side + 10.0)).floor() as usize).max(1);
                for row in shown.chunks(columns) {
                    ui.horizontal(|ui| {
                        for material in row {
                            ui.vertical(|ui| {
                                ui.set_width(side + 4.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(side, side), Sense::click());
                                let picked =
                                    self.material_pick.as_deref() == Some(material.name.as_str());
                                let key = {
                                    use std::hash::{Hash, Hasher};
                                    let mut hasher =
                                        std::collections::hash_map::DefaultHasher::new();
                                    material.name.hash(&mut hasher);
                                    if let crate::materials::Source::File(path) = &material.source {
                                        path.hash(&mut hasher);
                                    }
                                    hasher.finish()
                                };
                                let thumb = self
                                    .material_thumbs
                                    .entry(key)
                                    .or_insert_with(|| material_thumb(material, side as u32));
                                if let Some(texture) = cx.textures.texture(
                                    key ^ 0x4d41_5445,
                                    1,
                                    [side as u32; 2],
                                    4,
                                    thumb,
                                    false,
                                ) {
                                    ui.painter().image(
                                        texture,
                                        rect,
                                        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                ui.painter().rect_stroke(
                                    rect,
                                    0.0,
                                    if picked {
                                        Stroke::new(2.0, theme.border_active)
                                    } else {
                                        theme.hairline()
                                    },
                                    StrokeKind::Outside,
                                );
                                if response.clicked() {
                                    self.material_pick = Some(material.name.clone());
                                }
                                if response.double_clicked() {
                                    paste = Some((*material).clone());
                                }
                                response.context_menu(|ui| {
                                    if ui.button(tr!("paste-to-canvas")).clicked() {
                                        paste = Some((*material).clone());
                                        ui.close();
                                    }
                                    if material.removable()
                                        && ui.button(tr!("delete-material")).clicked()
                                    {
                                        delete = Some((*material).clone());
                                        ui.close();
                                    }
                                });
                                ui.label(small(&crate::localize(&material.name)));
                            });
                        }
                    });
                }
            });
        let picked = shown
            .iter()
            .find(|material| self.material_pick.as_deref() == Some(material.name.as_str()))
            .map(|material| (*material).clone());
        inner.horizontal(|ui| {
            if ui
                .add_enabled(picked.is_some(), egui::Button::new(tr!("paste-to-canvas")))
                .clicked()
            {
                paste = picked.clone();
            }
            if let Some(material) = picked.as_ref().filter(|material| material.removable())
                && ui.button(tr!("delete")).clicked()
            {
                delete = Some(material.clone());
            }
        });
        inner.label(small(&tr!("double-click-paste-edit-register-material")));
        if let Some(material) = paste {
            self.paste_material(cx, &material);
        }
        if let Some(material) = delete
            && let crate::materials::Source::File(path) = &material.source
        {
            match sue_platform::fs::remove_file(path) {
                Ok(()) => cx.notify(tr!("is-no-longer-a-material", name = material.name)),
                Err(error) => cx.fail(tr!("cannot-delete-the-material", error = error)),
            }
            self.material_pick = None;
        }
    }

    /// The Object tool's settings for a 3D layer: its camera, light and
    /// look. A change shows at once and keeps as one step when let go.
    fn view3d_properties(
        &mut self,
        ui: &mut Ui,
        layer: sue_core::EntityId,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) {
        let Ok(entity) = arena.get_by_id(layer) else {
            return;
        };
        let now = crate::three::View3D::of(entity);
        let mut view = now;
        let mut released = false;
        let slider = |ui: &mut Ui,
                      name: &str,
                      value: &mut f32,
                      range: std::ops::RangeInclusive<f32>,
                      suffix: &str| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme.muted));
                let response = ui.add(egui::Slider::new(value, range).suffix(suffix));
                response.drag_stopped() || (response.changed() && !response.dragged())
            })
            .inner
        };
        released |= slider(
            ui,
            &tr!("camera-angle"),
            &mut view.yaw,
            -180.0..=180.0,
            &tr!("deg"),
        );
        released |= slider(
            ui,
            &tr!("height-2"),
            &mut view.pitch,
            -89.0..=89.0,
            &tr!("deg"),
        );
        released |= slider(ui, &tr!("distance"), &mut view.distance, 0.6..=30.0, "");
        released |= slider(
            ui,
            &tr!("perspective"),
            &mut view.fov,
            5.0..=120.0,
            &tr!("deg"),
        );
        released |= slider(
            ui,
            &tr!("light"),
            &mut view.light,
            -180.0..=180.0,
            &tr!("deg"),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("draw-as")).color(theme.muted));
            egui::ComboBox::from_id_salt(("sue-paint-3d-style", layer))
                .selected_text(crate::localize(
                    crate::three::STYLES[usize::from(view.style.min(2))],
                ))
                .show_ui(ui, |ui| {
                    for (index, name) in crate::three::STYLES.iter().enumerate() {
                        if ui
                            .selectable_value(&mut view.style, index as u8, crate::localize(name))
                            .clicked()
                        {
                            released = true;
                        }
                    }
                });
        });
        if view.style > 0 {
            released |= slider(
                ui,
                &tr!("line-width-2"),
                &mut view.line_width,
                0.5..=20.0,
                &tr!("px"),
            );
            released |= slider(
                ui,
                &tr!("fold-lines-at"),
                &mut view.crease,
                5.0..=170.0,
                &tr!("deg"),
            );
        }
        ui.label(
            RichText::new(tr!("drag-canvas-turn-camera-shift-moves"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        if view != now {
            if self
                .view3d_before
                .as_ref()
                .is_none_or(|(id, _, _)| *id != layer)
            {
                let before = crate::doc::raster(arena, layer)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                self.view3d_before = Some((layer, now, before));
            }
            self.pending_view3d = Some((layer, view));
        }
        if released {
            self.view3d_release = true;
        }
    }

    /// The Object tool's settings for a camera folder: where its camera
    /// stands at this frame, and its keyframes.
    fn camera_properties(
        &mut self,
        ui: &mut Ui,
        folder: sue_core::EntityId,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        let (frame, time) = self.frame_time(arena);
        let mut camera = self.camera_now(arena, folder);
        let before = camera;
        ui.label(small(&tr!("camera-at-frame", value = frame + 1)));
        egui::Grid::new("sue-paint-camera")
            .num_columns(2)
            .spacing(vec2(10.0, 6.0))
            .show(ui, |ui| {
                ui.label(small(&tr!("position-x")));
                ui.add(egui::DragValue::new(&mut camera.center[0]).suffix(tr!("px")));
                ui.end_row();
                ui.label(small(&tr!("position-y")));
                ui.add(egui::DragValue::new(&mut camera.center[1]).suffix(tr!("px")));
                ui.end_row();
                ui.label(small(&tr!("scale")));
                let mut percent = camera.scale * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(1.0..=2000.0)
                            .suffix(" %"),
                    )
                    .changed()
                {
                    camera.scale = percent / 100.0;
                }
                ui.end_row();
                ui.label(small(&tr!("rotation")));
                let mut degrees = camera.angle.to_degrees();
                if ui
                    .add(egui::DragValue::new(&mut degrees).suffix(tr!("deg")))
                    .changed()
                {
                    camera.angle = degrees.to_radians();
                }
                ui.end_row();
            });
        if camera != before {
            self.camera_pending = Some((folder, frame, camera));
        }
        let keys = crate::camera::keys(arena, folder);
        let keyed = keys.contains(&time);
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(if keyed {
                    tr!("update-keyframe")
                } else {
                    tr!("add-keyframe")
                })
                .clicked()
            {
                command = Some("anim.camera.key");
            }
            if keyed && ui.button(tr!("delete-keyframe")).clicked() {
                command = Some("anim.camera.unkey");
            }
        });
        let rate = self
            .picture()
            .and_then(|picture| crate::anim::timeline(arena, picture))
            .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
        let frames: Vec<String> = keys
            .iter()
            .map(|time| (time.frame(rate) + 1).to_string())
            .collect();
        ui.label(small(&if frames.is_empty() {
            tr!("no-keyframes-yet-camera-shows-canvas")
        } else {
            tr!("keyframes-at-frames", value = frames.join(", "))
        }));
        ui.label(small(&tr!("drag-inside-frame-move-camera-corner")));
        command
    }

    fn select_ops(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr!("mode"))
                    .size(theme.small_size + 0.5)
                    .color(theme.muted),
            );
            for op in Op::ALL {
                ui.selectable_value(&mut self.select_op, op, op.label());
            }
        });
        ui.label(
            RichText::new(tr!("shift-adds-alt-subtracts-both-intersect"))
                .size(theme.small_size)
                .color(theme.muted),
        );
    }

    // ----- Color ---------------------------------------------------------------------

    pub(crate) fn color_panel(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        let tabs = [
            (tr!("color-wheel"), Icon::Wheel, true),
            (tr!("color-slider"), Icon::ColorSlider, true),
            (tr!("intermediate-color"), Icon::Intermediate, true),
            (tr!("approximate-color"), Icon::Approximate, true),
        ];
        let (body, clicked) = header(ui, rect, &tabs, self.color_tab, theme);
        if let Some(tab) = clicked {
            self.color_tab = tab;
        }
        let body = body.shrink(8.0);
        match self.color_tab {
            0 => self.color_wheel(ui, body, theme),
            2 => self.intermediate_colors(ui, body, theme),
            3 => self.approximate_colors(ui, body, theme),
            _ => {
                let mut inner = child(ui, body, "color-sliders");
                self.color_sliders(&mut inner, theme);
            }
        }
    }

    /// A grid of `colors(column, row)`, `columns` by `rows` in `area`; a
    /// click picks a color. The cell picked, if any.
    fn color_grid(
        ui: &mut Ui,
        area: Rect,
        columns: u32,
        rows: u32,
        id: &str,
        colors: impl Fn(u32, u32) -> [u8; 3],
    ) -> Option<[u8; 3]> {
        let cell = vec2(area.width() / columns as f32, area.height() / rows as f32);
        for row in 0..rows {
            for column in 0..columns {
                let [r, g, b] = colors(column, row);
                let rect = Rect::from_min_size(
                    area.min + vec2(column as f32 * cell.x, row as f32 * cell.y),
                    cell,
                );
                ui.painter()
                    .rect_filled(rect.shrink(0.5), 0.0, Color32::from_rgb(r, g, b));
            }
        }
        let response = ui.interact(
            area,
            Id::unique(("sue-paint-color-grid", id)),
            Sense::click(),
        );
        let at = response
            .interact_pointer_pos()
            .filter(|_| response.clicked())?;
        let column = (((at.x - area.left()) / cell.x) as u32).min(columns - 1);
        let row = (((at.y - area.top()) / cell.y) as u32).min(rows - 1);
        Some(colors(column, row))
    }

    /// Clip Studio Paint's Intermediate Color palette: the colors between
    /// four corners; a click on a corner sets it to the drawing color.
    fn intermediate_colors(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let (corners, steps) = self.intermediate;
        let steps = steps.clamp(2, 24);
        let corner = 16.0;
        let side = (body.width() - corner * 2.0)
            .min(body.height() - corner * 2.0 - 30.0)
            .max(40.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top() + corner),
            vec2(side, side),
        );
        let mix = |a: [u8; 3], b: [u8; 3], t: f32| {
            [0, 1, 2].map(|channel| {
                (f32::from(a[channel]) + (f32::from(b[channel]) - f32::from(a[channel])) * t)
                    .round() as u8
            })
        };
        let picked = Self::color_grid(ui, area, steps, steps, "intermediate", |column, row| {
            let (u, v) = (
                column as f32 / (steps - 1) as f32,
                row as f32 / (steps - 1) as f32,
            );
            mix(
                mix(corners[0], corners[1], u),
                mix(corners[2], corners[3], u),
                v,
            )
        });
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        // The corners: click one to give it the drawing color.
        let drawing = self.drawing_color();
        for (index, at) in [
            area.left_top() - vec2(corner, corner),
            area.right_top() + vec2(0.0, -corner),
            area.left_bottom() + vec2(-corner, 0.0),
            area.right_bottom(),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::from_min_size(at, vec2(corner, corner));
            let [r, g, b] = corners[index];
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            ui.painter()
                .rect_stroke(rect, 2.0, theme.hairline(), StrokeKind::Inside);
            let response = ui.interact(
                rect,
                Id::unique(("sue-paint-intermediate-corner", index)),
                Sense::click(),
            );
            if response
                .on_hover_text(tr!("set-to-the-drawing-color"))
                .clicked()
            {
                self.intermediate.0[index] = drawing;
            }
        }
        let mut steps = self.intermediate.1 as f32;
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), area.bottom() + corner + 4.0), body.max),
            "intermediate-steps",
        );
        if slider_row(
            &mut inner,
            &tr!("steps-2"),
            &mut steps,
            2.0..=24.0,
            "",
            false,
            theme,
        ) {
            self.intermediate.1 = steps.round() as u32;
        }
    }

    /// Clip Studio Paint's Approximate Color palette: colors near the
    /// drawing color, changing one way across and another down.
    fn approximate_colors(&mut self, ui: &mut Ui, body: Rect, theme: &Theme) {
        let (across, down, reach) = self.approximate;
        let base = Hsv::from_rgb(color::unit(self.drawing_color()));
        let steps = 9_u32;
        let side = body.width().min(body.height() - 64.0).max(40.0);
        let area = Rect::from_min_size(
            pos2(body.center().x - side / 2.0, body.top()),
            vec2(side, side),
        );
        let shift = |hsv: Hsv, axis: u8, t: f32| {
            let mut hsv = hsv;
            match axis {
                0 => hsv.h = (hsv.h + t * reach * 180.0).rem_euclid(360.0),
                1 => hsv.s = (hsv.s + t * reach).clamp(0.0, 1.0),
                _ => hsv.v = (hsv.v - t * reach).clamp(0.0, 1.0),
            }
            hsv
        };
        let picked = Self::color_grid(ui, area, steps, steps, "approximate", |column, row| {
            let (u, v) = (
                column as f32 / (steps - 1) as f32 * 2.0 - 1.0,
                row as f32 / (steps - 1) as f32 * 2.0 - 1.0,
            );
            color::bytes(shift(shift(base, across, u), down, v).to_rgb())
        });
        if let Some(rgb) = picked {
            self.set_drawing_color(rgb);
            self.transparent = false;
        }
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), area.bottom() + 6.0), body.max),
            "approximate-axes",
        );
        let names = [tr!("hue"), tr!("saturation"), tr!("brightness")];
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(tr!("across"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            egui::ComboBox::from_id_salt("sue-paint-approximate-across")
                .selected_text(names[usize::from(across.min(2))].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.approximate.0, index as u8, name.as_str());
                    }
                });
            ui.label(
                RichText::new(tr!("down"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            egui::ComboBox::from_id_salt("sue-paint-approximate-down")
                .selected_text(names[usize::from(down.min(2))].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut self.approximate.1, index as u8, name.as_str());
                    }
                });
        });
        let mut spread = reach * 100.0;
        if slider_row(
            &mut inner,
            &tr!("range"),
            &mut spread,
            5.0..=100.0,
            " %",
            false,
            theme,
        ) {
            self.approximate.2 = spread / 100.0;
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
        // Red, green and blue, each behind a chip of its own, as Clip
        // Studio Paint shows them under its wheel.
        let rgb = self.drawing_color();
        let chips = [
            Color32::from_rgb(200, 40, 40),
            Color32::from_rgb(40, 160, 60),
            Color32::from_rgb(40, 70, 200),
        ];
        let step = ((body.right() - row.left() - 48.0) / 3.0).clamp(28.0, 44.0);
        for (index, (value, chip)) in rgb.iter().zip(chips).enumerate() {
            let x = row.left() + 46.0 + index as f32 * step;
            let mark = Rect::from_center_size(pos2(x + 3.0, row.top() + 14.0), vec2(6.0, 6.0));
            ui.painter().rect_filled(mark, 1.0, chip);
            ui.painter().text(
                pos2(x + 10.0, row.top() + 14.0),
                Align2::LEFT_CENTER,
                value.to_string(),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
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
                RichText::new(tr!("hex"))
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
        let tabs = [
            (tr!("color-set"), Icon::Swatches, true),
            (tr!("color-history"), Icon::History, true),
            (tr!("material"), Icon::Material, true),
        ];
        let (body, clicked) = header(ui, rect, &tabs, self.set_tab, theme);
        if let Some(tab) = clicked {
            self.set_tab = tab;
        }
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

    // ----- Brush size sliders ---------------------------------------------------------

    /// Clip Studio Paint's Tool Sliders: the brush size and opacity as two
    /// tall sliders side by side, their values above them.
    pub(crate) fn size_column(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .vline(rect.right(), rect.y_range(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("tool-sliders"), Icon::Sliders, true)],
            0,
            theme,
        );
        let tool = self.toolbox.tool();
        if !tool.paints() && !matches!(tool, Tool::Fill | Tool::Gradient) {
            return;
        }
        let tip = &mut self.toolbox.current_mut().tip;
        let half = body.width() / 2.0;
        let label = 30.0;
        let lanes = [
            Rect::from_min_max(body.min, pos2(body.left() + half, body.bottom())),
            Rect::from_min_max(pos2(body.left() + half, body.top()), body.max),
        ];
        // Size, from small at the bottom to large at the top.
        let (low, high) = (0.5_f32.ln(), 1000.0_f32.ln());
        ui.painter().text(
            pos2(lanes[0].center().x, lanes[0].top() + label / 2.0),
            Align2::CENTER_CENTER,
            format!("{:.1}\npx", tip.size),
            theme.font(theme.small_size),
            theme.text,
        );
        let at = (tip.size.max(0.5).ln() - low) / (high - low);
        if let Some(t) = tall_slider(ui, lanes[0], label, at, false, "sue-paint-tool-size", theme) {
            tip.size = (low + t * (high - low)).exp();
        }
        // Opacity, its value in a dark disc as Clip Studio Paint shows it.
        let disc = pos2(lanes[1].center().x, lanes[1].top() + label / 2.0);
        ui.painter().circle_filled(disc, 12.0, theme.void);
        ui.painter().text(
            disc,
            Align2::CENTER_CENTER,
            format!("{:.0}\n%", tip.opacity * 100.0),
            theme.font(theme.small_size - 1.0),
            theme.text,
        );
        if let Some(t) = tall_slider(
            ui,
            lanes[1],
            label,
            tip.opacity,
            true,
            "sue-paint-tool-opacity",
            theme,
        ) {
            tip.opacity = t;
        }
    }

    // ----- Right column: navigator and layers ------------------------------------------

    /// A palette of its own for what is also a tab elsewhere (Clip Studio
    /// Paint's Material, Layer Property, History, Animation Cels, Sub
    /// View, Auto Action, Item Bank and Info palettes).
    pub(crate) fn single_palette(
        &mut self,
        ui: &mut Ui,
        palette: crate::dock::Palette,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let (body, _) = header(
            ui,
            rect,
            &[(palette.title(), palette.icon(), true)],
            0,
            theme,
        );
        match palette {
            Palette::Material => self.material_panel(ui, body, cx, theme),
            Palette::LayerProperty => {
                let current = self.layer_info(&cx.world.arena);
                let mut inner = child(ui, body.shrink(8.0), "layer-property-palette");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
            }
            Palette::History => self.history_tab(ui, body, cx, theme),
            Palette::Cels => self.cels_tab(ui, body, cx, theme),
            Palette::SubView => self.sub_view(ui, body.shrink(6.0), cx, theme),
            Palette::AutoAction => self.auto_tab(ui, body, cx, theme),
            Palette::ItemBank => self.item_bank_tab(ui, body, cx, theme),
            Palette::Info => self.info_tab(ui, body, cx, theme),
            _ => {}
        }
    }

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
            &[
                (tr!("navigator"), Icon::Navigator, true),
                (tr!("sub-view"), Icon::SubView, true),
            ],
            self.nav_tab,
            theme,
        );
        if let Some(tab) = clicked {
            self.nav_tab = tab;
        }
        match self.nav_tab {
            2 => return self.info_tab(ui, body, cx, theme),
            3 => return self.auto_tab(ui, body, cx, theme),
            4 => return self.item_bank_tab(ui, body, cx, theme),
            _ => {}
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
        // The canvas's own texture: only what changed goes up.
        let compositor = &doc.compositor;
        if let Some(texture) = cx.textures.texture_changed(
            texture_key(doc.picture, 0),
            version,
            size,
            4,
            pixels,
            doc.view.zoom >= 2.0,
            &|since| compositor.changed_since(since),
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
                // Red, as Clip Studio Paint draws the part the canvas shows.
                Stroke::new(1.5, Color32::from_rgb(220, 40, 40)),
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
        // Zoom and rotation, as Clip Studio Paint's Navigator: each a
        // value, a wedge and its buttons on one row; fit and the flips
        // below.
        let row = 22.0;
        let buttons_width = 5.0 * (row + 4.0);
        let wedge_width = (body.width() - 62.0 - 8.0 - buttons_width - 4.0).max(24.0);
        let mut zoom_button = None;
        let mut turn_button = None;
        let mut percent = doc.view.zoom * 100.0;
        let mut rotation = doc.view.rotation;
        let mut inner = child(
            ui,
            Rect::from_min_max(pos2(body.left(), thumb_area.bottom() + 4.0), body.max),
            "navigator-controls",
        );
        inner.spacing_mut().item_spacing = vec2(4.0, 4.0);
        let mut zoomed = false;
        let mut turned = false;
        inner.horizontal(|ui| {
            zoomed |= ui
                .add_sized(
                    vec2(62.0, row),
                    egui::DragValue::new(&mut percent)
                        .range(1.0..=6400.0)
                        .max_decimals(1),
                )
                .on_hover_text(tr!("zoom"))
                .changed();
            zoomed |= ui
                .add(Wedge::new(&mut percent, 1.0..=6400.0, wedge_width, theme).logarithmic())
                .on_hover_text(tr!("zoom"))
                .changed();
            for (index, (icon, tip)) in [
                (Icon::ZoomOut, tr!("zoom-out")),
                (Icon::ZoomIn, tr!("zoom-in")),
                (Icon::Actual, "100%".to_owned()),
                (Icon::Fit, tr!("fit-to-screen-2")),
            ]
            .into_iter()
            .enumerate()
            {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(row), Sense::hover());
                if icon_button(ui, rect, icon, true, false, &tip, theme) {
                    zoom_button = Some(index);
                }
            }
        });
        inner.horizontal(|ui| {
            turned |= ui
                .add_sized(
                    vec2(62.0, row),
                    egui::DragValue::new(&mut rotation)
                        .range(-180.0..=180.0)
                        .max_decimals(1),
                )
                .on_hover_text(tr!("rotation"))
                .changed();
            turned |= ui
                .add(Wedge::new(
                    &mut rotation,
                    -180.0..=180.0,
                    wedge_width,
                    theme,
                ))
                .on_hover_text(tr!("rotation"))
                .changed();
            for (index, (icon, tip)) in [
                (Icon::RotateLeft, tr!("rotate-left")),
                (Icon::RotateRight, tr!("rotate-right")),
                (Icon::Reset, tr!("reset-rotation")),
                (Icon::FlipH, tr!("flip-horizontally")),
                (Icon::FlipV, tr!("flip-vertically")),
            ]
            .into_iter()
            .enumerate()
            {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(row), Sense::hover());
                let active = match index {
                    3 => doc.view.flip_h,
                    4 => doc.view.flip_v,
                    _ => false,
                };
                if icon_button(ui, rect, icon, true, active, &tip, theme) {
                    turn_button = Some(index);
                }
            }
        });
        let center = viewport.center();
        if zoomed {
            doc.view
                .set_zoom_at(viewport, size, center, percent / 100.0);
        }
        if turned {
            doc.view.set_rotation(viewport, size, rotation);
        }
        match zoom_button {
            Some(0) => doc.view.zoom_at(viewport, size, center, 0.8),
            Some(1) => doc.view.zoom_at(viewport, size, center, 1.25),
            Some(2) => doc.view.set_zoom_at(viewport, size, center, 1.0),
            Some(_) => doc.view.fit(viewport, size),
            None => {}
        }
        match turn_button {
            Some(0) => doc.view.rotate_by(viewport, size, -15.0),
            Some(1) => doc.view.rotate_by(viewport, size, 15.0),
            Some(2) => doc.view.set_rotation(viewport, size, 0.0),
            Some(3) => doc.view.flip(viewport, size, true),
            Some(_) => doc.view.flip(viewport, size, false),
            None => {}
        }
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
            (tr!("layer-2"), Icon::Layers, true),
            (tr!("layer-property"), Icon::LayerProperty, true),
            (tr!("history"), Icon::History, true),
            (tr!("animation-cels"), Icon::Cel, true),
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
            self.cels_tab(ui, body, cx, theme);
            return;
        }
        match self.layer_tab {
            1 => {
                let mut inner = child(ui, body.shrink(8.0), "layer-property");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
                return;
            }
            2 => {
                self.history_tab(ui, body, cx, theme);
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
                    .selected_text(sue_lang::translate("model", blend.label()))
                    .width(108.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        let mut group = 0;
                        for mode in BlendMode::ALL {
                            if mode.group() != group {
                                ui.separator();
                                group = mode.group();
                            }
                            if ui
                                .selectable_label(
                                    mode == blend,
                                    sue_lang::translate("model", mode.label()),
                                )
                                .clicked()
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
            let width = (ui.available_width() - 52.0).max(40.0);
            ui.add_enabled_ui(current.is_some(), |ui| {
                let wedge = ui.add(Wedge::new(&mut opacity, 0.0..=100.0, width, theme));
                let typed = ui.add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.0)
                        .max_decimals(0),
                );
                let response = if typed.changed() || typed.lost_focus() {
                    typed
                } else {
                    wedge
                };
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
            (Icon::Clip, "layer.toggle.clip", &tr!("clip-to-layer-below")),
            (
                Icon::Reference,
                "layer.toggle.reference",
                &tr!("set-as-reference-layer"),
            ),
            (
                Icon::Draft,
                "layer.toggle.draft",
                &tr!("set-as-draft-layer"),
            ),
            (Icon::Lock, "layer.toggle.lock", &tr!("lock-layer")),
            (
                Icon::LockAlpha,
                "layer.toggle.lock_alpha",
                &tr!("lock-transparent-pixels"),
            ),
            (Icon::Mask, "layer.mask.toggle", &tr!("enable-mask")),
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
                &tr!("new-raster-layer-ctrl-shift-n"),
            ),
            (
                Icon::NewVector,
                "layer.new.vector",
                &tr!("new-vector-layer"),
            ),
            (
                Icon::NewFolder,
                "layer.new.folder",
                &tr!("new-layer-folder"),
            ),
            (
                Icon::TransferDown,
                "layer.transfer_down",
                &tr!("transfer-to-lower-layer"),
            ),
            (
                Icon::MergeDown,
                "layer.merge_down",
                &tr!("merge-layer-below-ctrl-e"),
            ),
            (
                Icon::NewMask,
                "layer.mask.outside",
                &tr!("create-layer-mask"),
            ),
            (
                Icon::ApplyMask,
                "layer.mask.apply",
                &tr!("apply-mask-to-layer"),
            ),
            (Icon::Delete, "layer.delete", &tr!("delete-layer")),
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
        let mut also_pick = None;
        // Right-click on a layer: Clip Studio Paint's layer menu.
        let layer_menu: Vec<sue_wm::MenuItem> = crate::menus::menus()
            .into_iter()
            .find(|menu| menu.title == tr!("layer-2"))
            .map(|menu| menu.items)
            .unwrap_or_default();
        let mut from_menu = None;
        let mut toggle = None;
        let mut rename_done = None;
        let mut fold = None;
        let mut mask_click = None;
        let mut reopen = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-layers")
            .show(&mut list_ui, |ui| {
                for info in stack.iter().rev() {
                    // Layers in a closed folder stay out of sight; layers in
                    // folders sit indented under them.
                    let around = doc::folders_around(&cx.world.arena, info.id);
                    if around
                        .iter()
                        .any(|folder| !doc::is_open(&cx.world.arena, *folder))
                    {
                        continue;
                    }
                    let is_folder = doc::is_folder(&cx.world.arena, info.id);
                    let (full, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                    let row = Rect::from_min_max(
                        full.min + vec2(around.len() as f32 * 14.0, 0.0),
                        full.max,
                    );
                    let chosen = Some(info.id) == self.layer();
                    let also = self.doc().is_some_and(|doc| doc.picked.contains(&info.id));
                    if also {
                        ui.painter()
                            .rect_filled(full, 0.0, theme.active.gamma_multiply(0.6));
                    }
                    if chosen {
                        ui.painter().rect_filled(full, 0.0, theme.active);
                    } else if response.hovered() {
                        ui.painter().rect_filled(full, 0.0, theme.hover);
                    }
                    ui.painter()
                        .hline(full.x_range(), full.bottom(), theme.hairline());
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
                    if eye_response.on_hover_text(tr!("show-or-hide")).clicked() {
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
                    // Thumbnail, or a folder and its open and close arrow.
                    let thumb_rect =
                        Rect::from_min_size(row.min + vec2(50.0, 5.0), vec2(48.0, 36.0));
                    if is_folder {
                        let open = doc::is_open(&cx.world.arena, info.id);
                        let arrow =
                            Rect::from_min_size(thumb_rect.min + vec2(0.0, 8.0), vec2(18.0, 20.0));
                        let arrow_response = ui.interact(
                            arrow,
                            Id::unique(("sue-paint-fold", info.id.to_u128())),
                            Sense::click(),
                        );
                        let c = arrow.center();
                        let points = if open {
                            vec![
                                c + vec2(-5.0, -3.0),
                                c + vec2(5.0, -3.0),
                                c + vec2(0.0, 4.0),
                            ]
                        } else {
                            vec![
                                c + vec2(-3.0, -5.0),
                                c + vec2(4.0, 0.0),
                                c + vec2(-3.0, 5.0),
                            ]
                        };
                        ui.painter().add(egui::Shape::convex_polygon(
                            points,
                            theme.text,
                            Stroke::NONE,
                        ));
                        if arrow_response
                            .on_hover_text(if open {
                                tr!("close-the-folder")
                            } else {
                                tr!("open-the-folder")
                            })
                            .clicked()
                        {
                            fold = Some((info.id, !open));
                        }
                        icons::paint(
                            ui.painter(),
                            Rect::from_min_size(thumb_rect.min + vec2(20.0, 6.0), vec2(24.0, 24.0)),
                            Icon::NewFolder,
                            theme.text,
                        );
                    } else {
                        self.layer_thumbnail(ui, thumb_rect, info, cx, theme);
                    }
                    // The mask beside the layer: click it to draw on it.
                    let mut text_left = thumb_rect.right() + 8.0;
                    if doc::mask_of(&cx.world.arena, info.id).is_some() {
                        let mask_rect = Rect::from_min_size(
                            pos2(thumb_rect.right() + 4.0, thumb_rect.top()),
                            vec2(28.0, 36.0),
                        );
                        let on = doc::mask_on(&cx.world.arena, info.id);
                        let editing = chosen && self.doc().is_some_and(|doc| doc.mask_edit);
                        ui.painter().rect_filled(
                            mask_rect,
                            2.0,
                            egui::Color32::from_gray(if on { 225 } else { 120 }),
                        );
                        icons::paint(
                            ui.painter(),
                            mask_rect.shrink(6.0),
                            Icon::Mask,
                            egui::Color32::from_gray(40),
                        );
                        if editing {
                            ui.painter().rect_stroke(
                                mask_rect,
                                2.0,
                                Stroke::new(2.0, theme.border_active),
                                egui::StrokeKind::Outside,
                            );
                        }
                        let mask_response = ui.interact(
                            mask_rect,
                            Id::unique(("sue-paint-mask", info.id.to_u128())),
                            Sense::click(),
                        );
                        if mask_response
                            .on_hover_text(tr!("draw-mask-pen-shows-eraser-hides"))
                            .clicked()
                        {
                            mask_click = Some(info.id);
                        }
                        text_left = mask_rect.right() + 8.0;
                    }
                    // Text.
                    let top_line = match info.kind {
                        LayerKind::Paper => tr!("paper-2"),
                        _ => format!(
                            "{} % {}",
                            sue_lang::number(f64::from(info.style.opacity) * 100.0, 0),
                            sue_lang::translate("model", info.style.blend.label())
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
                        let name = match (
                            info.style.draft,
                            doc::is_reference(&cx.world.arena, info.id),
                        ) {
                            (true, _) => tr!("draft", name = info.name),
                            (false, true) => tr!("reference", name = info.name),
                            _ if crate::three::is_3d(&cx.world.arena, info.id) => {
                                format!("{} (3D)", info.name)
                            }
                            _ => info.name.clone(),
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
                    let correction = cx
                        .world
                        .arena
                        .get_by_id(info.id)
                        .ok()
                        .filter(|entity| entity.class() == doc::CORRECTION)
                        .and_then(|entity| entity.property(doc::CORRECTION_FILTER))
                        .and_then(Value::as_str)
                        .and_then(|text| {
                            crate::filters::Filter::from_text(text)
                                .map(|filter| (filter, text.to_owned()))
                        });
                    if response.double_clicked()
                        && let Some((filter, text)) = correction
                    {
                        // A correction layer's settings, as Clip Studio Paint
                        // opens them on a double-click.
                        reopen = Some((info.id, filter, text));
                    } else if response.double_clicked() {
                        self.renaming = Some((info.id, info.name.clone()));
                    } else if response.clicked() && ui.input(|input| input.modifiers.command) {
                        // Ctrl+click picks more layers (Merge Selected Layers).
                        also_pick = Some(info.id);
                    } else if response.clicked() || response.secondary_clicked() {
                        pick = Some(info.id);
                    }
                    let state = |command: &str| self.state(command, &cx.world.arena, cx.history);
                    response.context_menu(|ui| {
                        if let Some(command) = sue_wm::context_items(ui, &layer_menu, &state) {
                            from_menu = Some(command);
                        }
                    });
                }
            });
        if let Some(id) = pick
            && let Some(doc) = self.doc_mut()
        {
            if doc.layer != Some(id) {
                doc.mask_edit = false;
                doc.picked.clear();
            }
            doc.layer = Some(id);
        }
        if let Some(id) = also_pick
            && let Some(doc) = self.doc_mut()
            && doc.layer != Some(id)
        {
            if let Some(at) = doc.picked.iter().position(|known| *known == id) {
                doc.picked.remove(at);
            } else {
                doc.picked.push(id);
            }
        }
        if let Some((id, visible)) = toggle {
            Self::apply(cx, doc::set(id, picture::VISIBLE, visible));
        }
        if let Some(command) = from_menu {
            self.run(&command, cx);
        }
        if let Some((id, open)) = fold {
            Self::apply(cx, doc::set(id, doc::OPEN, open));
        }
        if let Some((id, filter, text)) = reopen {
            self.dialog = Some(crate::dialogs::Dialog::Correction(id, filter, text, false));
        }
        if let Some(id) = mask_click
            && let Some(doc) = self.doc_mut()
        {
            // The mask of the current layer toggles; another layer's mask
            // picks that layer with its mask.
            doc.mask_edit = doc.layer != Some(id) || !doc.mask_edit;
            doc.layer = Some(id);
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
    pub(crate) fn layer_property(
        &mut self,
        ui: &mut Ui,
        current: Option<&doc::LayerInfo>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(info) = current else {
            ui.label(RichText::new(tr!("no-layer")).color(theme.muted));
            return;
        };
        ui.label(
            RichText::new(&info.name)
                .size(theme.ui_size + 1.0)
                .color(theme.text),
        );
        ui.label(
            RichText::new(match info.kind {
                LayerKind::Raster => tr!("raster-layer"),
                LayerKind::Paper => tr!("paper-2"),
                LayerKind::Other => tr!("layer-kind-sue-cannot-edit-yet"),
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
                    ui.label(RichText::new(tr!("color-2")).color(theme.muted));
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
                    ui.label(RichText::new(tr!("opacity")).color(theme.muted));
                    let width = (ui.available_width() - 60.0).max(40.0);
                    let wedge = ui.add(Wedge::new(&mut opacity, 0.0..=100.0, width, theme));
                    let typed = ui.add(
                        egui::DragValue::new(&mut opacity)
                            .range(0.0..=100.0)
                            .max_decimals(0)
                            .suffix(" %"),
                    );
                    let response = if typed.changed() || typed.lost_focus() {
                        typed
                    } else {
                        wedge
                    };
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
                    ui.label(RichText::new(tr!("offset")).color(theme.muted));
                    let x = ui.add(egui::DragValue::new(&mut offset[0]).suffix(tr!("px")));
                    self.live_property(cx, &x, info.id, picture::X, Value::Int(offset[0]));
                    let y = ui.add(egui::DragValue::new(&mut offset[1]).suffix(tr!("px")));
                    self.live_property(cx, &y, info.id, picture::Y, Value::Int(offset[1]));
                });
                if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
                    ui.label(
                        RichText::new(tr!(
                            "x-px",
                            width = raster.width(),
                            height = raster.height(),
                            format = format!("{:?}", raster.format())
                        ))
                        .size(theme.small_size)
                        .color(theme.muted),
                    );
                }
            }
            LayerKind::Other => {}
        }
        if info.kind == LayerKind::Raster {
            ui.add_space(6.0);
            self.layer_effects(ui, info, cx, theme);
        }
    }

    /// Clip Studio Paint's Layer Property effects: Border effect, Tone and
    /// Layer color, and the Expression color.
    fn layer_effects(
        &mut self,
        ui: &mut Ui,
        info: &doc::LayerInfo,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::effects::{self, Effects};
        let small = |text: &str| RichText::new(text).color(theme.muted);
        let Ok(entity) = cx.world.arena.get_by_id(info.id) else {
            return;
        };
        let effects = Effects::of(entity, 72.0);
        let number =
            |key: &str, or: f64| entity.property(key).and_then(Value::as_f64).unwrap_or(or);
        let (border_width, lines, angle, threshold) = (
            number(effects::BORDER_WIDTH, 4.0),
            number(effects::TONE_LINES, 60.0),
            number(effects::TONE_ANGLE, 45.0),
            number(effects::THRESHOLD, 0.5),
        );
        let color_of = |key: &str| match entity.property(key) {
            Some(Value::Color(color)) => *color,
            _ => effects::default_color(key),
        };
        let (border_color, layer_color) = (
            color_of(effects::BORDER_COLOR),
            color_of(effects::COLOR_VALUE),
        );
        // Effect: a row of buttons, as Clip Studio Paint's Layer Property
        // has them; those Sue Paint has not got yet stay grey.
        ui.label(small(&tr!("effect")));
        let mut toggle = None;
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        for (index, (key, icon, name, on)) in [
            (
                effects::BORDER,
                Icon::BorderEffect,
                tr!("border-effect-2"),
                Some(effects.border.is_some()),
            ),
            ("", Icon::WatercolorEdge, tr!("watercolor-edge"), None),
            ("", Icon::ExtractLine, tr!("extract-line"), None),
            (
                effects::TONE,
                Icon::Tone,
                tr!("tone"),
                Some(effects.tone.is_some()),
            ),
            (
                effects::COLOR,
                Icon::LayerColor,
                tr!("layer-color-2"),
                Some(effects.color.is_some()),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::from_min_size(
                row.min + vec2(index as f32 * 32.0, 0.0),
                egui::Vec2::splat(30.0),
            );
            if icon_button(ui, rect, icon, on.is_some(), on == Some(true), &name, theme)
                && let Some(on) = on
            {
                toggle = Some((key, !on));
            }
        }
        if let Some((key, on)) = toggle {
            Self::apply(cx, doc::set(info.id, key, on));
        }
        let color_row = |this: &mut Self,
                         ui: &mut Ui,
                         cx: &mut AppletContext<'_>,
                         key: &str,
                         color: sue_core::Color,
                         salt: &str| {
            let [r, g, b] = color::bytes(color.to_space(sue_core::ColorSpace::Srgb).rgb);
            let mut rgba = [r, g, b, 255];
            ui.horizontal(|ui| {
                ui.label(small(&if salt == "sue-paint-border" {
                    tr!("edge-color")
                } else {
                    tr!("layer-color-2")
                }));
                let pick = sue_wm::color_button(ui, (salt, info.id), &mut rgba, false, theme);
                let [r, g, b, _] = rgba;
                this.live_value(
                    cx,
                    (
                        pick.changed,
                        pick.released || (pick.changed && !pick.dragging),
                    ),
                    info.id,
                    key,
                    Value::from(sue_core::Color::srgb8(r, g, b)),
                );
            });
        };
        if effects.border.is_some() {
            let mut width = border_width as f32;
            ui.horizontal(|ui| {
                ui.label(small(&tr!("thickness-of-edge")));
                let response = ui.add(egui::Slider::new(&mut width, 1.0..=50.0).suffix(tr!("px")));
                self.live_property(
                    cx,
                    &response,
                    info.id,
                    effects::BORDER_WIDTH,
                    Value::Float(f64::from(width)),
                );
            });
            color_row(
                self,
                ui,
                cx,
                effects::BORDER_COLOR,
                border_color,
                "sue-paint-border",
            );
        }
        if effects.tone.is_some() {
            let (mut lines, mut angle) = (lines as f32, angle as f32);
            ui.horizontal(|ui| {
                ui.label(small(&tr!("frequency")));
                let response =
                    ui.add(egui::Slider::new(&mut lines, 5.0..=300.0).suffix(tr!("lpi")));
                self.live_property(
                    cx,
                    &response,
                    info.id,
                    effects::TONE_LINES,
                    Value::Float(f64::from(lines)),
                );
            });
            ui.horizontal(|ui| {
                ui.label(small(&tr!("angle")));
                let response =
                    ui.add(egui::Slider::new(&mut angle, 0.0..=180.0).suffix(tr!("deg")));
                self.live_property(
                    cx,
                    &response,
                    info.id,
                    effects::TONE_ANGLE,
                    Value::Float(f64::from(angle)),
                );
            });
        }
        if effects.color.is_some() {
            color_row(
                self,
                ui,
                cx,
                effects::COLOR_VALUE,
                layer_color,
                "sue-paint-layer-color",
            );
        }
        ui.add_space(4.0);
        let mut expression = effects.expression;
        ui.horizontal(|ui| {
            ui.label(small(&tr!("expression-color-2")));
            egui::ComboBox::from_id_salt(("sue-paint-expression", info.id))
                .selected_text(effects::EXPRESSIONS[usize::from(expression.min(2))])
                .show_ui(ui, |ui| {
                    for (index, name) in effects::EXPRESSIONS.iter().enumerate() {
                        ui.selectable_value(&mut expression, index as u8, *name);
                    }
                });
        });
        if expression != effects.expression {
            Self::apply(
                cx,
                doc::set(info.id, effects::EXPRESSION, i64::from(expression)),
            );
        }
        if effects.expression == 2 {
            let mut threshold = threshold as f32 * 100.0;
            ui.horizontal(|ui| {
                ui.label(small(&tr!("threshold")));
                let response = ui.add(egui::Slider::new(&mut threshold, 1.0..=99.0).suffix(" %"));
                self.live_property(
                    cx,
                    &response,
                    info.id,
                    effects::THRESHOLD,
                    Value::Float(f64::from(threshold) / 100.0),
                );
            });
        }
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

    /// The Animation Cels tab, as Clip Studio Paint's Animation cels
    /// palette: its three rows of light table and cel buttons with the
    /// opacity between them, then the layers on the cel's light table.
    fn cels_tab(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let cel = self.editing_cel(&cx.world.arena);
        let has = cel.is_some();
        let registered = cel.zip(self.layer()).is_some_and(|(cel, layer)| {
            crate::anim::light_table(&cx.world.arena, cel).contains(&layer)
        });
        let side = 24.0;
        let at = |row: usize, column: usize| {
            Rect::from_min_size(
                body.min
                    + vec2(
                        6.0 + column as f32 * (side + 2.0),
                        6.0 + row as f32 * (side + 4.0),
                    ),
                egui::Vec2::splat(side),
            )
        };
        // (row, column, icon, tip, command, enabled, active); a command
        // of "" is one Sue Paint has not got yet.
        let buttons = [
            (
                0,
                0,
                Icon::Reset,
                tr!("reset-position-layers-light-table"),
                "",
                false,
                false,
            ),
            (
                0,
                1,
                Icon::Reverse,
                tr!("reverse-layers-vertically-light-table"),
                "anim.light_table.reverse",
                has,
                false,
            ),
            (
                0,
                2,
                Icon::OpacityTarget,
                tr!("switch-opacity-target"),
                "",
                false,
                false,
            ),
            (
                1,
                0,
                Icon::LightTable,
                tr!("enable-light-table-2"),
                "anim.light_table",
                true,
                self.light_table,
            ),
            (
                1,
                1,
                Icon::Lock,
                tr!("lock-cel-editing-target"),
                "anim.cel.lock",
                has || self.cel_lock.is_some(),
                self.cel_lock.is_some(),
            ),
            (
                1,
                2,
                Icon::Prev,
                tr!("select-previous-cel"),
                "anim.cel.prev",
                true,
                false,
            ),
            (
                1,
                3,
                Icon::Next,
                tr!("select-next-cel"),
                "anim.cel.next",
                true,
                false,
            ),
            (
                1,
                4,
                Icon::Cel,
                tr!("new-animation-cel-2"),
                "anim.cel.new",
                true,
                false,
            ),
            (
                1,
                6,
                Icon::Import,
                tr!("select-and-register-file"),
                "",
                false,
                false,
            ),
            (
                1,
                7,
                Icon::Reference,
                tr!("register-selected-layer-light-table"),
                "anim.light_table.register",
                has,
                false,
            ),
            (
                1,
                8,
                Icon::Minus,
                tr!("deregister-selected-light-table"),
                "anim.light_table.deregister",
                registered,
                false,
            ),
            (
                2,
                0,
                Icon::Operation,
                tr!("light-table-tool"),
                "",
                false,
                false,
            ),
            (
                2,
                1,
                Icon::Onion,
                tr!("show-cel-light-table"),
                "anim.light_table.cel",
                true,
                self.light_cel,
            ),
            (
                2,
                2,
                Icon::Layers,
                tr!("show-general-light-table"),
                "",
                false,
                false,
            ),
        ];
        let mut run = None;
        for (row, column, icon, tip, command, enabled, active) in &buttons {
            if icon_button(ui, at(*row, *column), *icon, *enabled, *active, tip, theme) {
                run = Some(*command);
            }
        }
        // Opacity and Color mode, on the first row after its buttons.
        let opacity_rect = Rect::from_min_max(
            pos2(at(0, 3).left() + 4.0, at(0, 3).top()),
            pos2(body.right() - side - 12.0, at(0, 3).bottom()),
        );
        let mut inner = child(ui, opacity_rect, "cels-opacity");
        let mut opacity = self.light_opacity * 100.0;
        let width = (opacity_rect.width() - 50.0).max(30.0);
        let changed = inner
            .horizontal(|ui| {
                let wedge = ui
                    .add(Wedge::new(&mut opacity, 0.0..=100.0, width, theme))
                    .on_hover_text(tr!("opacity"));
                let typed = ui.add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.0)
                        .max_decimals(0),
                );
                wedge.changed() || typed.changed()
            })
            .inner;
        if changed {
            self.light_opacity = opacity / 100.0;
        }
        let color_rect = Rect::from_min_size(
            pos2(body.right() - side - 6.0, at(0, 0).top()),
            egui::Vec2::splat(side),
        );
        if icon_button(
            ui,
            color_rect,
            Icon::ColorMode,
            true,
            self.light_color,
            &tr!("color-mode"),
            theme,
        ) {
            run = Some("anim.light_table.color");
        }
        let mut inner = child(
            ui,
            Rect::from_min_max(
                pos2(body.left() + 8.0, at(2, 0).bottom() + 8.0),
                body.max - vec2(8.0, 8.0),
            ),
            "cels-light-table",
        );
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        match cel {
            None => {
                inner.label(small(&tr!("go-frame-cel-see-light-table")));
            }
            Some(cel) => {
                let name = |id: sue_core::EntityId| {
                    cx.world
                        .arena
                        .get_by_id(id)
                        .map_or_else(|_| tr!("layer-2"), |entity| entity.name().to_owned())
                };
                inner.label(small(&tr!("light-table-of-cel", value = name(cel))));
                let layers = crate::anim::light_table(&cx.world.arena, cel);
                if layers.is_empty() {
                    inner.label(small(&tr!("pick-layer-trace-then-register-last")));
                }
                let mut off = None;
                for layer in layers {
                    inner.horizontal(|ui| {
                        if ui
                            .small_button("x")
                            .on_hover_text(tr!("deregister-from-the-light-table"))
                            .clicked()
                        {
                            off = Some(layer);
                        }
                        ui.label(name(layer));
                    });
                }
                if let Some(layer) = off {
                    Self::apply(cx, crate::anim::deregister(cel, &[layer]));
                }
            }
        }
        if let Some(command) = run.filter(|command| !command.is_empty()) {
            self.run(command, cx);
        }
    }

    pub(crate) fn timeline_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("timeline"), Icon::Timeline, true)],
            0,
            theme,
        );
        let bar = Rect::from_min_size(body.min, vec2(body.width(), 28.0));
        ui.painter()
            .hline(body.x_range(), bar.bottom(), theme.hairline());
        let tracks = Rect::from_min_max(pos2(body.left(), bar.bottom()), body.max);
        // Clip Studio Paint's timeline: its tools and playback, then the
        // folders' cels frame by frame.
        self.timeline_ui(ui, bar, tracks, cx, theme);
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

/// A material's thumbnail, `side` square, over white.
pub(crate) fn material_thumb(material: &crate::materials::Material, side: u32) -> Vec<u8> {
    let tile = match material.placement {
        crate::materials::Placement::Fit => crate::materials::layer(material, [side, side], [0; 3]),
        crate::materials::Placement::Cover => {
            crate::materials::picture(material, [side, side], [0; 3])
        }
    };
    let mut out = vec![255_u8; (side * side * 4) as usize];
    if let Some(bytes) = tile.as_ref().and_then(|tile| tile.bytes()) {
        for (into, from) in out.chunks_mut(4).zip(bytes.chunks(4)) {
            let alpha = u32::from(from[3]);
            for channel in 0..3 {
                into[channel] =
                    ((u32::from(from[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
    }
    out
}
