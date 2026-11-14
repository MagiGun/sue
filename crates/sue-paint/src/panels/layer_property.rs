//! The Layer Property palette, its effects and its live properties.

use egui::{Rect, RichText, Sense, Ui, vec2};
use sue_core::Value;
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Theme};

use crate::color;
use crate::doc;
use crate::icons::Icon;
use crate::panels::{Wedge, icon_button};
use crate::studio::{LiveProperty, SuePaint};

impl SuePaint {
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
        // has them.
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
            (
                effects::WATERCOLOR,
                Icon::WatercolorEdge,
                tr!("watercolor-edge"),
                Some(effects.watercolor.is_some()),
            ),
            (
                effects::EXTRACT,
                Icon::ExtractLine,
                tr!("extract-line"),
                Some(effects.extract.is_some()),
            ),
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
        // Watercolor edge: how wide and how dark.
        if let Some((width, darkness)) = effects.watercolor {
            for (label, key, mut value, range, suffix) in [
                (
                    tr!("watercolor-edge-width"),
                    effects::WATERCOLOR_WIDTH,
                    width,
                    1.0..=100.0,
                    tr!("px"),
                ),
                (
                    tr!("watercolor-edge-darkness"),
                    effects::WATERCOLOR_DARKNESS,
                    darkness * 100.0,
                    0.0..=100.0,
                    "%".to_owned(),
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.label(small(&label));
                    let response = ui.add(egui::Slider::new(&mut value, range).suffix(suffix));
                    let stored = if key == effects::WATERCOLOR_DARKNESS {
                        value / 100.0
                    } else {
                        value
                    };
                    self.live_property(
                        cx,
                        &response,
                        info.id,
                        key,
                        Value::Float(f64::from(stored)),
                    );
                });
            }
        }
        // Extract line: how faint an edge still makes a line, and how wide.
        if let Some((threshold, width)) = effects.extract {
            for (label, key, mut value, range, suffix) in [
                (
                    tr!("extract-line-threshold"),
                    effects::EXTRACT_THRESHOLD,
                    threshold * 100.0,
                    1.0..=100.0,
                    "%".to_owned(),
                ),
                (
                    tr!("extract-line-width"),
                    effects::EXTRACT_WIDTH,
                    width,
                    0.5..=10.0,
                    tr!("px"),
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.label(small(&label));
                    let response = ui.add(egui::Slider::new(&mut value, range).suffix(suffix));
                    let stored = if key == effects::EXTRACT_THRESHOLD {
                        value / 100.0
                    } else {
                        value
                    };
                    self.live_property(
                        cx,
                        &response,
                        info.id,
                        key,
                        Value::Float(f64::from(stored)),
                    );
                });
            }
        }
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
    pub(super) fn live_property(
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
}
