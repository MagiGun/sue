//! The Layer Property palette, its effects and its live properties, and
//! the Simple Tone Settings dialog of New Layer, Tone.

use egui::{RichText, Ui};
use sue_core::{EntityId, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::ui::tokens::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::color;
use crate::doc;
use crate::effects;
use crate::studio::{LiveProperty, SuePaint};

/// The frequencies Simple Tone Settings offers, in lines an inch: Clip
/// Studio Paint's list.
pub(crate) const TONE_FREQUENCIES: [f64; 12] = [
    27.5, 30.0, 32.5, 35.0, 40.0, 42.5, 45.0, 50.0, 55.0, 60.0, 65.0, 85.0,
];

/// The size of the Simple Tone Settings dialog (until the dialog host
/// sizes dialogs by their content).
pub(crate) const SIMPLE_TONE_SIZE: egui::Vec2 = egui::Vec2::new(380.0, 260.0);

/// What Simple Tone Settings makes: a new layer of tone, in the selection
/// if there is one.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SimpleTone {
    /// Lines an inch.
    pub lines: f64,
    /// How much of the area the dots cover, 0 to 100.
    pub density: f64,
    /// The dots' shape, an index of `effects::TONE_SHAPES`.
    pub shape: usize,
    /// The dots' angle in degrees.
    pub angle: f64,
}

impl Default for SimpleTone {
    fn default() -> Self {
        Self {
            lines: 60.0,
            density: 40.0,
            shape: 0,
            angle: 45.0,
        }
    }
}

/// A row's label in the palette's muted caption.
fn caption(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// The names of `effects::TONE_SHAPES`, as people read them.
fn shape_names() -> Vec<String> {
    effects::TONE_SHAPES
        .iter()
        .map(|shape| sue_lang::text("paint", &format!("tone-shape-{}", shape.to_lowercase())))
        .collect()
}

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
            caption(ui, &tr!("no-layer"), theme);
            return;
        };
        let folder = doc::is_folder(&cx.world.arena, info.id);
        sue_wm::ui::ScrollView::vertical("sue-paint-layer-property").show(ui, theme, |ui| {
            Density::Compact.scope(ui, |ui| {
                ui.label(
                    RichText::new(&info.name)
                        .font(theme.text_font(TextRole::Title))
                        .color(theme.text),
                );
                caption(
                    ui,
                    &match info.kind {
                        _ if folder => tr!("layer-folder"),
                        LayerKind::Raster => tr!("raster-layer"),
                        LayerKind::Paper => tr!("paper-2"),
                        LayerKind::Other => tr!("layer-kind-sue-cannot-edit-yet"),
                    },
                    theme,
                );
                ui.add_space(theme.space(Space::S3));
                match info.kind {
                    LayerKind::Paper => self.paper_color(ui, info, cx, theme),
                    LayerKind::Raster => self.raster_settings(ui, info, cx, theme),
                    LayerKind::Other => {}
                }
                if info.kind == LayerKind::Raster || folder {
                    ui.add_space(theme.space(Space::S3));
                    self.layer_effects(ui, info, folder, cx, theme);
                }
            });
        });
    }

    /// The paper's color.
    fn paper_color(
        &mut self,
        ui: &mut Ui,
        info: &doc::LayerInfo,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let paper = cx
            .world
            .arena
            .get_by_id(info.id)
            .map(picture::paper_color)
            .unwrap_or_else(|_| sue_core::Color::srgb8(255, 255, 255));
        let [r, g, b] = color::bytes(paper.rgb);
        let mut rgba = [r, g, b, 255];
        ui.horizontal(|ui| {
            caption(ui, &tr!("color-2"), theme);
            let pick =
                sue_wm::color_button(ui, ("sue-paint-paper", info.id), &mut rgba, false, theme);
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

    /// A raster layer's opacity, offset and size.
    fn raster_settings(
        &mut self,
        ui: &mut Ui,
        info: &doc::LayerInfo,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut opacity = f64::from(info.style.opacity) * 100.0;
        let name = tr!("opacity");
        let response = sue_wm::ui::slider("sue-paint-property-opacity", &mut opacity, 0.0..=100.0)
            .label(&name)
            .decimals(0)
            .suffix("%")
            .show(ui, theme);
        self.live_property(
            cx,
            &response,
            info.id,
            picture::OPACITY,
            Value::Float(opacity / 100.0),
        );
        let mut offset = info.style.offset.map(|at| at as f64);
        let px = tr!("px");
        ui.horizontal(|ui| {
            caption(ui, &tr!("offset"), theme);
            for (index, key) in [picture::X, picture::Y].into_iter().enumerate() {
                let axis = if index == 0 { tr!("x-2") } else { tr!("y-2") };
                let response =
                    sue_wm::ui::number_field(("sue-paint-offset", index), &mut offset[index])
                        .decimals(0)
                        .suffix(&px)
                        .label(&axis)
                        .show(ui, theme);
                self.live_property(
                    cx,
                    &response,
                    info.id,
                    key,
                    Value::Int(offset[index].round() as i64),
                );
            }
        });
        if let Some(raster) = doc::raster(&cx.world.arena, info.id) {
            caption(
                ui,
                &tr!(
                    "x-px",
                    width = raster.width(),
                    height = raster.height(),
                    format = format!("{:?}", raster.format())
                ),
                theme,
            );
        }
    }

    /// A slider for the effect setting `key` of `id`, live while dragged,
    /// one undo step when let go. `scale` turns what is stored into what
    /// it shows (100 for a share shown in percent).
    #[allow(clippy::too_many_arguments, reason = "one setting and how it shows")]
    fn effect_slider(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        (key, or): (&str, f64),
        label: &str,
        range: std::ops::RangeInclusive<f64>,
        (scale, suffix): (f64, &str),
        theme: &Theme,
    ) {
        let stored = cx
            .world
            .arena
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_f64)
            .unwrap_or(or);
        let mut shown = stored * scale;
        let response = sue_wm::ui::slider(("sue-paint-effect", key), &mut shown, range)
            .label(label)
            .decimals(if scale > 1.0 { 0 } else { 1 })
            .suffix(suffix)
            .show(ui, theme);
        self.live_property(cx, &response, id, key, Value::Float(shown / scale));
    }

    /// A switch for the setting `key` of `id` (`or` when missing), one
    /// undo step a flip.
    #[allow(clippy::too_many_arguments, reason = "one switch and its key")]
    fn effect_switch(
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        key: &str,
        or: bool,
        label: &str,
        theme: &Theme,
    ) {
        let on = cx
            .world
            .arena
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_bool)
            .unwrap_or(or);
        if sue_wm::ui::switch(("sue-paint-switch", key), label, on)
            .show(ui, theme)
            .clicked()
        {
            Self::apply(cx, doc::set(id, key, !on));
        }
    }

    /// A dropdown for the index setting `key` of `id`.
    fn effect_choice(
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        key: &str,
        (label, names): (&str, &[String]),
        theme: &Theme,
    ) {
        let now = cx
            .world
            .arena
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_i64)
            .map_or(0, |value| value.max(0) as usize)
            .min(names.len().saturating_sub(1));
        let mut chosen = now;
        ui.horizontal(|ui| {
            caption(ui, label, theme);
            sue_wm::ui::dropdown(("sue-paint-choice", key), &mut chosen, names)
                .label(label)
                .show(ui, theme);
        });
        if chosen != now {
            Self::apply(cx, doc::set(id, key, chosen as i64));
        }
    }

    /// A color row for the setting `key` of `id`.
    fn effect_color(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        key: &str,
        label: &str,
        theme: &Theme,
    ) {
        let color = match cx
            .world
            .arena
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
        {
            Some(Value::Color(color)) => *color,
            _ => effects::default_color(key),
        };
        let [r, g, b] = color::bytes(color.to_space(sue_core::ColorSpace::Srgb).rgb);
        let mut rgba = [r, g, b, 255];
        ui.horizontal(|ui| {
            caption(ui, label, theme);
            // A click on the bar opens Sue's color settings.
            let pick = sue_wm::color_button(ui, (key, id), &mut rgba, false, theme);
            let [r, g, b, _] = rgba;
            self.live_value(
                cx,
                (
                    pick.changed,
                    pick.released || (pick.changed && !pick.dragging),
                ),
                id,
                key,
                Value::from(sue_core::Color::srgb8(r, g, b)),
            );
        });
    }

    /// Clip Studio Paint's Layer Property effects: Border effect,
    /// Watercolor edge, Extract line, Tone and Layer color, and the
    /// Expression color. A folder takes the layer color only.
    #[allow(clippy::too_many_lines, reason = "every effect and its settings")]
    fn layer_effects(
        &mut self,
        ui: &mut Ui,
        info: &doc::LayerInfo,
        folder: bool,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use sue_wm::icons::Icon;
        let Ok(entity) = cx.world.arena.get_by_id(info.id) else {
            return;
        };
        let now = effects::Effects::of(entity, 72.0);
        let id = info.id;
        sue_wm::ui::heading(ui, &tr!("effect"), theme);
        let mut toggle = None;
        let buttons: Vec<(&str, Icon, String, bool)> = [
            (
                effects::BORDER,
                Icon::BorderEffect,
                tr!("border-effect-2"),
                now.border.is_some(),
            ),
            (
                effects::WATERCOLOR,
                Icon::WatercolorEdge,
                tr!("watercolor-edge"),
                now.watercolor.is_some(),
            ),
            (
                effects::EXTRACT,
                Icon::ExtractLine,
                tr!("extract-line"),
                now.extract.is_some(),
            ),
            (effects::TONE, Icon::Tone, tr!("tone"), now.tone.is_some()),
            (
                effects::COLOR,
                Icon::LayerColor,
                tr!("layer-color-2"),
                now.color.is_some(),
            ),
        ]
        .into_iter()
        .filter(|(key, ..)| !folder || *key == effects::COLOR)
        .collect();
        ui.horizontal(|ui| {
            for (key, icon, name, on) in &buttons {
                let keys = if *key == effects::COLOR {
                    super::layer::keys_of(cx, "layer.toggle.layer_color")
                } else {
                    String::new()
                };
                let response =
                    sue_wm::ui::icon_button(("sue-paint-effect-toggle", *key), *icon, "")
                        .selected(*on)
                        .show(ui, theme);
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, *on, name)
                });
                if sue_wm::ui::tooltip::tooltip(
                    response,
                    &sue_wm::ui::tooltip::Tip::new(name).shortcut(&keys),
                    theme,
                )
                .clicked()
                {
                    toggle = Some((*key, !*on));
                }
            }
        });
        if let Some((key, on)) = toggle {
            Self::apply(cx, doc::set(id, key, on));
        }
        let px = tr!("px");
        if now.border.is_some() {
            sue_wm::ui::heading(ui, &tr!("border-effect-2"), theme);
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::BORDER_WIDTH, 4.0),
                &tr!("thickness-of-edge"),
                1.0..=50.0,
                (1.0, &px),
                theme,
            );
            self.effect_color(ui, cx, id, effects::BORDER_COLOR, &tr!("edge-color"), theme);
            Self::effect_switch(
                ui,
                cx,
                id,
                effects::BORDER_AA,
                true,
                &tr!("anti-aliasing-on-border-effect"),
                theme,
            );
        }
        if now.watercolor.is_some() {
            sue_wm::ui::heading(ui, &tr!("watercolor-edge"), theme);
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::WATERCOLOR_WIDTH, 6.0),
                &tr!("watercolor-edge-width"),
                1.0..=100.0,
                (1.0, &px),
                theme,
            );
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::WATERCOLOR_DARKNESS, 0.5),
                &tr!("watercolor-edge-darkness"),
                0.0..=100.0,
                (100.0, "%"),
                theme,
            );
        }
        if now.extract.is_some() {
            sue_wm::ui::heading(ui, &tr!("extract-line"), theme);
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::EXTRACT_THRESHOLD, 0.25),
                &tr!("extract-line-threshold"),
                1.0..=100.0,
                (100.0, "%"),
                theme,
            );
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::EXTRACT_WIDTH, 1.0),
                &tr!("extract-line-width"),
                0.5..=10.0,
                (1.0, &px),
                theme,
            );
        }
        if now.tone.is_some() {
            sue_wm::ui::heading(ui, &tr!("tone"), theme);
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::TONE_LINES, 60.0),
                &tr!("frequency"),
                5.0..=300.0,
                (1.0, &tr!("lpi")),
                theme,
            );
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::TONE_ANGLE, 45.0),
                &tr!("angle"),
                0.0..=180.0,
                (1.0, &tr!("deg")),
                theme,
            );
            let sources: Vec<String> = effects::TONE_SOURCES
                .iter()
                .map(|source| {
                    sue_lang::text("paint", &format!("tone-source-{}", source.to_lowercase()))
                })
                .collect();
            Self::effect_choice(
                ui,
                cx,
                id,
                effects::TONE_SOURCE,
                (&tr!("tone-density-source"), &sources),
                theme,
            );
            Self::effect_choice(
                ui,
                cx,
                id,
                effects::TONE_SHAPE,
                (&tr!("tone-dot-shape"), &shape_names()),
                theme,
            );
            let mut levels = f64::from(now.tone_levels);
            ui.horizontal(|ui| {
                caption(ui, &tr!("tone-posterization"), theme);
                let response = sue_wm::ui::number_field("sue-paint-tone-levels", &mut levels)
                    .range(0.0..=255.0)
                    .decimals(0)
                    .label(&tr!("tone-posterization"))
                    .show(ui, theme);
                self.live_property(
                    cx,
                    &response,
                    id,
                    effects::TONE_LEVELS,
                    Value::Int(levels.round() as i64),
                );
            });
            Self::effect_switch(
                ui,
                cx,
                id,
                effects::TONE_REFLECT_OPACITY,
                false,
                &tr!("reflect-layer-opacity"),
                theme,
            );
            let mut dot = now.tone_dot.map(f64::from);
            ui.horizontal(|ui| {
                caption(ui, &tr!("dot-position"), theme);
                for (index, key) in [effects::TONE_DOT_X, effects::TONE_DOT_Y]
                    .into_iter()
                    .enumerate()
                {
                    let axis = if index == 0 { tr!("x-2") } else { tr!("y-2") };
                    let response =
                        sue_wm::ui::number_field(("sue-paint-dot", index), &mut dot[index])
                            .decimals(1)
                            .suffix(&px)
                            .label(&axis)
                            .show(ui, theme);
                    self.live_property(cx, &response, id, key, Value::Float(dot[index]));
                }
            });
        }
        if now.color.is_some() {
            sue_wm::ui::heading(ui, &tr!("layer-color-2"), theme);
            self.effect_color(
                ui,
                cx,
                id,
                effects::COLOR_VALUE,
                &tr!("layer-color-2"),
                theme,
            );
            let sub = now.sub_color.is_some();
            if sue_wm::ui::switch("sue-paint-sub-color", &tr!("use-sub-color"), sub)
                .show(ui, theme)
                .clicked()
            {
                Self::apply(
                    cx,
                    if sub {
                        doc::Edit::new(tr!("use-sub-color"))
                            .then(sue_core::txn::SetProperty::remove(id, effects::COLOR_SUB))
                    } else {
                        doc::set(
                            id,
                            effects::COLOR_SUB,
                            sue_core::Color::srgb8(255, 255, 255),
                        )
                    },
                );
            }
            if sub {
                self.effect_color(ui, cx, id, effects::COLOR_SUB, &tr!("sub-color"), theme);
            }
        }
        if folder {
            return;
        }
        ui.add_space(theme.space(Space::S2));
        let expressions: Vec<String> = effects::EXPRESSIONS
            .iter()
            .map(|name| {
                sue_lang::text("paint", &format!("expression-kind-{}", name.to_lowercase()))
            })
            .collect();
        Self::effect_choice(
            ui,
            cx,
            id,
            effects::EXPRESSION,
            (&tr!("expression-color-2"), &expressions),
            theme,
        );
        if now.expression == 2 {
            self.effect_slider(
                ui,
                cx,
                id,
                (effects::THRESHOLD, 0.5),
                &tr!("threshold"),
                1.0..=99.0,
                (100.0, "%"),
                theme,
            );
            // Which of black and white the layer may draw.
            ui.horizontal(|ui| {
                for (key, label) in [
                    (effects::MONO_BLACK, tr!("mono-use-black")),
                    (effects::MONO_WHITE, tr!("mono-use-white")),
                ] {
                    Self::effect_switch(ui, cx, id, key, true, &label, theme);
                }
            });
        }
        if now.expression != 0
            && sue_wm::ui::button("sue-paint-apply-expression", &tr!("apply-expression-color"))
                .show(ui, theme)
                .clicked()
        {
            self.apply_expression(cx, id);
        }
    }

    /// Apply expression color of preview: the layer's pixels become what
    /// its expression color shows (gray, or black and white), as one step;
    /// the expression color is set back to Color.
    fn apply_expression(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        let arena = &cx.world.arena;
        let Ok(entity) = arena.get_by_id(id) else {
            return;
        };
        let mut only = effects::Effects::of(entity, 72.0);
        // The expression alone: the other effects stay effects.
        only.border = None;
        only.watercolor = None;
        only.extract = None;
        only.tone = None;
        only.color = None;
        only.sub_color = None;
        let Some(raster) = doc::raster(arena, id) else {
            return;
        };
        let (Some(before), width, height) = (
            raster.bytes().map(<[u8]>::to_vec),
            raster.width(),
            raster.height(),
        ) else {
            return;
        };
        let [ox, oy] = info_offset(entity);
        let mut after = before.clone();
        for (index, pixel) in after.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if pixel[3] == 0 {
                continue;
            }
            let x = (index as i64 % i64::from(width) + ox).max(0) as u32;
            let y = (index as i64 / i64::from(width) + oy).max(0) as u32;
            *pixel = doc::bytes(only.apply(doc::unit(pixel), x, y));
        }
        let label = tr!("apply-expression-color");
        let Some(patch) = doc::diff_patch(&before, &after, width, height) else {
            Self::apply(cx, doc::set(id, effects::EXPRESSION, 0_i64));
            return;
        };
        Self::apply(
            cx,
            doc::Edit::new(&label)
                .then(sue_core::txn::RasterEdit::new(id, &label, vec![patch]))
                .then(sue_core::txn::SetProperty::new(
                    id,
                    effects::EXPRESSION,
                    0_i64,
                )),
        );
    }

    /// The body of Simple Tone Settings.
    pub(crate) fn simple_tone_ui(
        ui: &mut Ui,
        tone: &mut SimpleTone,
        theme: &Theme,
    ) -> crate::dialogs::Outcome {
        let frequencies: Vec<String> = TONE_FREQUENCIES
            .iter()
            .map(|lines| sue_lang::number(*lines, 1))
            .collect();
        let mut preset = TONE_FREQUENCIES
            .iter()
            .position(|lines| (*lines - tone.lines).abs() < 0.05)
            .unwrap_or(TONE_FREQUENCIES.len());
        let lpi = tr!("lpi");
        ui.horizontal(|ui| {
            caption(ui, &tr!("frequency"), theme);
            sue_wm::ui::number_field("sue-paint-simple-tone-lines", &mut tone.lines)
                .range(5.0..=300.0)
                .decimals(1)
                .suffix(&lpi)
                .label(&tr!("frequency"))
                .show(ui, theme);
            let before = preset;
            sue_wm::ui::dropdown("sue-paint-simple-tone-presets", &mut preset, &frequencies)
                .label(&tr!("tone-frequency-presets"))
                .show(ui, theme);
            if preset != before
                && let Some(lines) = TONE_FREQUENCIES.get(preset)
            {
                tone.lines = *lines;
            }
        });
        sue_wm::ui::slider(
            "sue-paint-simple-tone-density",
            &mut tone.density,
            0.0..=100.0,
        )
        .label(&tr!("tone-density"))
        .decimals(0)
        .suffix("%")
        .show(ui, theme);
        let shapes = shape_names();
        ui.horizontal(|ui| {
            caption(ui, &tr!("tone-dot-shape"), theme);
            sue_wm::ui::dropdown("sue-paint-simple-tone-shape", &mut tone.shape, &shapes)
                .label(&tr!("tone-dot-shape"))
                .show(ui, theme);
        });
        sue_wm::ui::slider("sue-paint-simple-tone-angle", &mut tone.angle, 0.0..=180.0)
            .label(&tr!("angle"))
            .decimals(0)
            .suffix(&tr!("deg"))
            .show(ui, theme);
        ui.add_space(theme.space(Space::S4));
        let mut outcome = crate::dialogs::Outcome::Open;
        ui.horizontal(|ui| {
            if sue_wm::ui::button("sue-paint-simple-tone-ok", &tr!("ok"))
                .primary()
                .show(ui, theme)
                .clicked()
            {
                outcome = crate::dialogs::Outcome::Ok;
            }
            if sue_wm::ui::button("sue-paint-simple-tone-cancel", &tr!("cancel"))
                .show(ui, theme)
                .clicked()
            {
                outcome = crate::dialogs::Outcome::Cancel;
            }
        });
        outcome
    }

    /// OK in Simple Tone Settings: a new layer of tone right above the
    /// current one, as dense as asked where the selection is (everywhere
    /// without one), with the tone effect on; one step.
    pub(crate) fn simple_tone_ok(&mut self, cx: &mut AppletContext<'_>, tone: &SimpleTone) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let density = (tone.density / 100.0).clamp(0.0, 1.0) as f32;
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let cover = selection
                    .as_ref()
                    .map_or(1.0, |selection| selection.at(x, y));
                let at = (y as usize * width as usize + x as usize) * 4;
                pixels[at + 3] = (density * cover * 255.0).round() as u8;
            }
        }
        let Ok(raster) =
            sue_core::RasterTile2D::from_bytes(width, height, sue_core::PixelFormat::Rgba8, pixels)
        else {
            return;
        };
        let name = tr!("tone");
        let Ok((spawn, id)) = doc::add_layer_pixels(arena, picture, self.layer(), name, raster)
        else {
            return;
        };
        let set = |key: &str, value: Value| sue_core::txn::SetProperty::new(id, key, value);
        let edit = doc::Edit::new(tr!("simple-tone-settings"))
            .then(spawn)
            .then(set(effects::TONE, Value::from(true)))
            .then(set(effects::TONE_SOURCE, Value::from(1_i64)))
            .then(set(effects::TONE_LINES, Value::from(tone.lines)))
            .then(set(effects::TONE_ANGLE, Value::from(tone.angle)))
            .then(set(
                effects::TONE_SHAPE,
                Value::from(tone.shape.min(effects::TONE_SHAPES.len() - 1) as i64),
            ));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
            doc.picked.clear();
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

/// Where `entity`'s pixels start in the picture.
fn info_offset(entity: &sue_core::Entity) -> [i64; 2] {
    picture::LayerStyle::of(entity).offset
}
