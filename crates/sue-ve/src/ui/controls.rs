//! Effect Controls, as Premiere Pro's panel and After Effects' layer
//! properties show them: the selected clip's fixed effects (Motion with
//! its anchor point and motion blur, Opacity with the blend mode, Time
//! Remapping with speed ramps), then its effects stack, each effect with
//! its switch, order and parameters, then its masks. Every number is
//! keyframable: its diamond adds or takes away a key at the playhead, and
//! the graph editor shows its curve. A value dragged changes the clip live
//! and becomes one undo step when the drag ends.

use egui::{Rect, Sense, Ui, pos2, vec2};
use sue_core::Value;
use sue_model::{BlendMode, Clip};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::font;
use crate::edit::{self, EditError};
use crate::fx::{self, Effect, ParamKind, key as fx_key, mask};
use crate::model::{self, key};
use crate::studio::SueVe;

/// Where a number is kept.
#[derive(Clone, Debug, PartialEq)]
pub enum Write {
    /// A clip property (and its channel of the same name).
    Property(&'static str),
    /// An effect's parameter.
    Effect(u64, String),
    /// A mask's number (feather, expansion, opacity).
    Mask(usize, String),
}

impl Write {
    /// The animation channel its keys go on.
    #[must_use]
    pub fn channel(&self) -> String {
        match self {
            Self::Property(name) => (*name).to_owned(),
            Self::Effect(id, param) => fx::channel(*id, param),
            Self::Mask(index, param) => mask::channel(*index, param),
        }
    }

    /// Sets the value (not a key) on `clip`.
    pub fn set(&self, clip: &mut Clip, value: f64) {
        match self {
            Self::Property(name) => {
                clip.properties.set(*name, value);
            }
            Self::Effect(id, param) => {
                fx::update(clip, *id, |effect| {
                    effect.params.set(param.clone(), value);
                });
            }
            Self::Mask(index, param) => {
                let mut masks = mask::masks(clip);
                if let Some(mask) = masks.get_mut(*index) {
                    match param.as_str() {
                        "feather" => mask.feather = value,
                        "expansion" => mask.expansion = value,
                        _ => mask.opacity = value,
                    }
                    mask::set_masks(clip, &masks);
                }
            }
        }
    }
}

/// One keyframable number row.
#[derive(Clone, Debug)]
pub struct NumberRow {
    /// Where it is kept.
    pub write: Write,
    /// Its name.
    pub label: String,
    /// Its stored value now.
    pub value: f64,
    /// The least and most, as shown.
    pub range: (f64, f64),
    /// Drag speed.
    pub speed: f64,
    /// Decimals shown.
    pub decimals: usize,
    /// Shown is stored times this.
    pub shown: f64,
    /// The unit after it.
    pub suffix: String,
}

/// The share of a mask's row its name takes.
const MASK_NAME_SHARE: f32 = 0.3;

/// The share of a row its label takes.
const LABEL_SHARE: f32 = 0.42;

fn row_width(ui: &Ui) -> f32 {
    ui.available_width()
        .min(ui.clip_rect().right() - ui.cursor().left())
        .max(1.0)
}

/// A row with `label` at the left, then what `add` draws.
pub(crate) fn labeled<R>(
    ui: &mut Ui,
    label: &str,
    theme: &Theme,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let room = row_width(ui);
    let height = Density::Compact.icon_button();
    ui.allocate_ui_with_layout(
        vec2(room, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (name, _) =
                ui.allocate_exact_size(vec2(room * LABEL_SHARE, height), Sense::hover());
            ui.painter().text(
                pos2(name.left(), name.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                font(theme, TextRole::Caption),
                theme.text_muted(),
            );
            add(ui)
        },
    )
    .inner
}

/// A keyframable number: its field and its key diamond. Picking its name
/// shows its curve in the graph editor.
pub fn number_row(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    row: &NumberRow,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let at = studio.clock.position;
    let inside = clip.start <= at && at < clip.end();
    let channel = row.write.channel();
    let curve = clip.animation.channel(&channel);
    let keyed = curve.is_some_and(|curve| !curve.keys().is_empty());
    let source_time = clip.source_time(at);
    let on_key = curve.is_some_and(|curve| curve.keys().iter().any(|key| key.time == source_time));
    let mut value = row.value * row.shown;
    let id = clip.id;
    let button = Density::Compact.icon_button();
    let room = row_width(ui);
    let picked = studio.graph.channel.as_deref() == Some(channel.as_str());
    ui.allocate_ui_with_layout(
        vec2(room, button),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let (spot, name) =
                ui.allocate_exact_size(vec2(room * LABEL_SHARE, button), Sense::click());
            if picked {
                ui.painter().rect_filled(
                    spot,
                    theme.corner(sue_wm::ui::Radius::Xs),
                    theme.fill_selected(),
                );
            }
            ui.painter().text(
                pos2(spot.left() + theme.space(Space::S1), spot.center().y),
                egui::Align2::LEFT_CENTER,
                &row.label,
                font(theme, TextRole::Caption),
                if keyed {
                    theme.text
                } else {
                    theme.text_muted()
                },
            );
            if name.clicked() {
                studio.graph.channel = Some(channel.clone());
                studio.graph.picked.clear();
            }
            let gap = ui.spacing().item_spacing.x;
            let width = (room * (1.0 - LABEL_SHARE) - button - gap * 2.0).max(button);
            let response = sue_wm::ui::number_field(("sve-control", &channel), &mut value)
                .label(&row.label)
                .range(row.range.0..=row.range.1)
                .speed(row.speed)
                .decimals(row.decimals)
                .suffix(&row.suffix)
                .width(width)
                .show(ui, theme);
            if response.changed() {
                let stored = value / row.shown;
                let write = row.write.clone();
                let channel = channel.clone();
                studio.live_clip(cx, id, |clip| {
                    if keyed && inside {
                        let _ = clip.animation.channel_mut(&channel).insert(sue_model::Key {
                            time: source_time,
                            value: stored,
                            interpolation: sue_model::Interpolation::Linear,
                        });
                    } else {
                        write.set(clip, stored);
                    }
                });
                if !response.dragged() {
                    studio.commit_live(cx, &row.label);
                }
            }
            if response.drag_stopped() || response.lost_focus() {
                studio.commit_live(cx, &row.label);
            }
            let (icon, tip) = if on_key {
                (Icon::Keyframe, tr!("inspector-remove-key"))
            } else {
                (Icon::KeyframeAdd, tr!("inspector-add-key"))
            };
            if sue_wm::ui::icon_button(("sve-control-key", &channel), icon, &tip)
                .selected(keyed)
                .enabled(inside)
                .show(ui, theme)
                .clicked()
            {
                let stored = row.value;
                studio.edit(cx, &tip, |sequence, _| {
                    if on_key {
                        edit::remove_key(sequence, id, &channel, at)
                    } else {
                        edit::set_key(sequence, id, &channel, at, stored)
                    }
                });
                studio.graph.channel = Some(channel.clone());
            }
        },
    );
}

/// The Effect Controls palette: the selected clip's settings and stack.
pub fn controls_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect();
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(theme.space(Space::S4))));
    let ui = &mut child;
    let Some((track, clip)) = studio.selected_clip(cx) else {
        sue_wm::ui::empty_state("sve-controls-empty", &tr!("controls-empty"))
            .icon(Icon::Effects)
            .show(ui, theme);
        return;
    };
    let video = studio
        .current(cx)
        .and_then(|sequence| sequence.tracks.get(track))
        .is_some_and(model::is_video);
    let name = cx
        .arena()
        .get_by_id(clip.source)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("sve-controls")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            sue_wm::ui::heading(ui, &name, theme);
            if video {
                motion_ui(studio, ui, &clip, cx, theme);
                opacity_ui(studio, ui, &clip, cx, theme);
            } else {
                sound_ui(studio, ui, &clip, cx, theme);
            }
            remap_ui(studio, ui, &clip, cx, theme);
            if video {
                effects_ui(studio, ui, &clip, cx, theme);
                masks_ui(studio, ui, &clip, cx, theme);
            }
        });
}

fn number(
    write: Write,
    label: String,
    value: f64,
    range: (f64, f64),
    shown: f64,
    suffix: &str,
) -> NumberRow {
    let span = (range.1 - range.0).abs() * shown;
    NumberRow {
        write,
        label,
        value,
        range: (range.0 * shown, range.1 * shown),
        speed: (span / 400.0).clamp(0.01, 5.0),
        decimals: if span <= 10.0 {
            2
        } else if span <= 400.0 {
            1
        } else {
            0
        },
        shown,
        suffix: suffix.to_owned(),
    }
}

/// Motion: position, scale, rotation, anchor point, motion blur.
fn motion_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    sue_wm::ui::list::heading(ui, &tr!("controls-motion"), theme);
    let at = studio.clock.position.max(clip.start).min(clip.end());
    let get = |name: &str| clip.value_at(name, at, model::default_of(name));
    let rows = [
        number(
            Write::Property(key::POS_X),
            tr!("inspector-x"),
            get(key::POS_X),
            (-16_384.0, 16_384.0),
            1.0,
            " px",
        ),
        number(
            Write::Property(key::POS_Y),
            tr!("inspector-y"),
            get(key::POS_Y),
            (-16_384.0, 16_384.0),
            1.0,
            " px",
        ),
        number(
            Write::Property(key::SCALE),
            tr!("inspector-scale"),
            get(key::SCALE),
            (0.0, 10.0),
            100.0,
            " %",
        ),
        number(
            Write::Property(key::ROTATION),
            tr!("inspector-rotation"),
            get(key::ROTATION),
            (-3600.0, 3600.0),
            1.0,
            " deg",
        ),
        number(
            Write::Property(fx_key::ANCHOR_X),
            tr!("controls-anchor-x"),
            clip.value_at(fx_key::ANCHOR_X, at, 0.0),
            (-16_384.0, 16_384.0),
            1.0,
            " px",
        ),
        number(
            Write::Property(fx_key::ANCHOR_Y),
            tr!("controls-anchor-y"),
            clip.value_at(fx_key::ANCHOR_Y, at, 0.0),
            (-16_384.0, 16_384.0),
            1.0,
            " px",
        ),
    ];
    for row in &rows {
        number_row(studio, ui, clip, row, cx, theme);
    }
    // Motion blur's shutter: not keyframed.
    let mut shutter = clip
        .properties
        .get(fx_key::MOTION_BLUR)
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let label = tr!("controls-motion-blur");
    let response = labeled(ui, &label, theme, |ui| {
        sue_wm::ui::number_field("sve-motion-blur", &mut shutter)
            .label(&label)
            .range(0.0..=720.0)
            .speed(2.0)
            .decimals(0)
            .suffix(" deg")
            .show(ui, theme)
    });
    if response.changed() {
        let id = clip.id;
        studio.live_clip(cx, id, |clip| {
            if shutter > 0.0 {
                clip.properties.set(fx_key::MOTION_BLUR, shutter);
            } else {
                clip.properties.remove(fx_key::MOTION_BLUR);
            }
        });
        if !response.dragged() {
            studio.commit_live(cx, &label);
        }
    }
    if response.drag_stopped() || response.lost_focus() {
        studio.commit_live(cx, &label);
    }
}

/// Opacity and the blend mode.
fn opacity_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    sue_wm::ui::list::heading(ui, &tr!("inspector-opacity"), theme);
    let at = studio.clock.position.max(clip.start).min(clip.end());
    let row = number(
        Write::Property(key::OPACITY),
        tr!("inspector-opacity"),
        clip.value_at(key::OPACITY, at, 1.0),
        (0.0, 1.0),
        100.0,
        " %",
    );
    number_row(studio, ui, clip, &row, cx, theme);
    let modes: Vec<String> = BlendMode::ALL
        .iter()
        .map(|mode| sue_lang::translate("model", mode.label()))
        .collect();
    let current = clip
        .properties
        .get(fx_key::BLEND)
        .and_then(Value::as_str)
        .map_or(BlendMode::Normal, BlendMode::from_name_or_normal);
    let mut chosen = BlendMode::ALL
        .iter()
        .position(|mode| *mode == current)
        .unwrap_or(0);
    let before = chosen;
    let label = tr!("controls-blend");
    labeled(ui, &label, theme, |ui| {
        sue_wm::ui::dropdown("sve-blend", &mut chosen, &modes)
            .label(&label)
            .show(ui, theme);
    });
    if chosen != before {
        let mode = BlendMode::ALL[chosen];
        studio.change_clip(cx, &label, clip.id, |clip| {
            if mode == BlendMode::Normal {
                clip.properties.remove(fx_key::BLEND);
            } else {
                clip.properties.set(fx_key::BLEND, mode.name());
            }
            Ok(())
        });
    }
}

/// A sound clip's volume and pan.
fn sound_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    sue_wm::ui::list::heading(ui, &tr!("inspector-sound"), theme);
    let at = studio.clock.position.max(clip.start).min(clip.end());
    let rows = [
        number(
            Write::Property(key::VOLUME),
            tr!("inspector-volume"),
            clip.value_at(key::VOLUME, at, 0.0),
            (-96.0, 24.0),
            1.0,
            " dB",
        ),
        number(
            Write::Property(fx_key::PAN),
            tr!("controls-pan"),
            clip.value_at(fx_key::PAN, at, 0.0),
            (-1.0, 1.0),
            100.0,
            "",
        ),
    ];
    for row in &rows {
        number_row(studio, ui, clip, row, cx, theme);
    }
}

/// Time remapping: on or off, and a speed ramp from the playhead.
fn remap_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    sue_wm::ui::list::heading(ui, &tr!("controls-time-remapping"), theme);
    let on = clip.animation.channel(fx_key::REMAP).is_some();
    let id = clip.id;
    let label = tr!("controls-remap-on");
    if sue_wm::ui::switch("sve-remap-on", &label, on)
        .show(ui, theme)
        .clicked()
    {
        studio.change_clip(cx, &label, id, |clip| {
            if on {
                clip.animation.remove_channel(fx_key::REMAP);
            } else {
                crate::keys::enable_remap(clip);
            }
            Ok(())
        });
        studio.graph.channel = Some(fx_key::REMAP.to_owned());
        studio.graph.speed = true;
    }
    if !on {
        return;
    }
    let at = studio.clock.position;
    let speed_now = clip
        .animation
        .channel(fx_key::REMAP)
        .map_or(1.0, |curve| crate::keys::speed(curve, clip.source_time(at)));
    let mut percent = speed_now * 100.0;
    let label = tr!("controls-ramp-speed");
    let response = labeled(ui, &label, theme, |ui| {
        sue_wm::ui::number_field("sve-ramp-speed", &mut percent)
            .label(&label)
            .range(-1000.0..=1000.0)
            .speed(1.0)
            .decimals(0)
            .suffix(" %")
            .show(ui, theme)
    });
    if (response.lost_focus()
        || response.drag_stopped()
        || (response.changed() && !response.dragged()))
        && (percent / 100.0 - speed_now).abs() > 1e-3
    {
        studio.change_clip(cx, &label, id, |clip| {
            crate::keys::ramp(clip, at, percent / 100.0)
        });
    }
    ui.label(
        egui::RichText::new(tr!("controls-ramp-note"))
            .font(font(theme, TextRole::Micro))
            .color(theme.text_muted()),
    );
}

/// The effects stack.
fn effects_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let list = fx::effects(clip);
    if list.is_empty() {
        sue_wm::ui::list::heading(ui, &tr!("controls-effects"), theme);
        ui.label(
            egui::RichText::new(tr!("controls-no-effects"))
                .font(font(theme, TextRole::Caption))
                .color(theme.text_muted()),
        );
        return;
    }
    let count = list.len();
    for (index, effect) in list.iter().enumerate() {
        effect_ui(studio, ui, clip, effect, index, count, cx, theme);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "an effect's rows need the clip, the effect, its place in the stack and the studio"
)]
fn effect_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    effect: &Effect,
    index: usize,
    count: usize,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let Some(kind) = fx::kind(&effect.kind) else {
        sue_wm::ui::list::heading(ui, &effect.kind, theme);
        return;
    };
    let id = clip.id;
    let effect_id = effect.id;
    // The header: on, name, up, down, remove.
    let button = Density::Compact.icon_button();
    let room = row_width(ui);
    let mut action: Option<&str> = None;
    ui.allocate_ui_with_layout(
        vec2(room, button),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if sue_wm::ui::icon_button(
                ("sve-fx-on", effect_id),
                if effect.enabled {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                },
                &tr!("controls-effect-on"),
            )
            .selected(effect.enabled)
            .quiet()
            .show(ui, theme)
            .clicked()
            {
                action = Some("toggle");
            }
            let name_width = (room - button * 4.0 - ui.spacing().item_spacing.x * 5.0).max(button);
            let (spot, picked) = ui.allocate_exact_size(vec2(name_width, button), Sense::click());
            if studio.fx_picked == Some(effect_id) {
                ui.painter().rect_filled(
                    spot,
                    theme.corner(sue_wm::ui::Radius::Xs),
                    theme.fill_selected(),
                );
            }
            ui.painter().text(
                pos2(spot.left() + theme.space(Space::S1), spot.center().y),
                egui::Align2::LEFT_CENTER,
                kind.label(),
                font(theme, TextRole::Body),
                theme.text,
            );
            if picked.clicked() {
                studio.fx_picked = Some(effect_id);
            }
            for (icon, what, tip, enabled) in [
                (Icon::ArrowUp, "up", tr!("controls-move-up"), index > 0),
                (
                    Icon::ArrowDown,
                    "down",
                    tr!("controls-move-down"),
                    index + 1 < count,
                ),
                (Icon::Delete, "remove", tr!("controls-remove-effect"), true),
            ] {
                if sue_wm::ui::icon_button(("sve-fx", what, effect_id), icon, &tip)
                    .enabled(enabled)
                    .quiet()
                    .show(ui, theme)
                    .clicked()
                {
                    action = Some(what);
                }
            }
        },
    );
    if let Some(what) = action {
        let label = kind.label();
        studio.change_clip(cx, &label, id, |clip| {
            let done = match what {
                "toggle" => fx::update(clip, effect_id, |effect| effect.enabled = !effect.enabled),
                "up" => fx::reorder(clip, effect_id, -1),
                "down" => fx::reorder(clip, effect_id, 1),
                _ => fx::remove(clip, effect_id),
            };
            if done {
                Ok(())
            } else {
                Err(EditError::Nothing)
            }
        });
        return;
    }
    let at = studio.clock.position.max(clip.start).min(clip.end());
    for param in &kind.params {
        let label = param.label();
        match param.kind {
            ParamKind::Number {
                min, max, suffix, ..
            } => {
                let row = NumberRow {
                    write: Write::Effect(effect_id, param.key.to_owned()),
                    label,
                    value: effect.number(clip, param.key, at),
                    range: (min, max),
                    speed: ((max - min).abs() / 400.0).clamp(0.01, 5.0),
                    decimals: if (max - min).abs() <= 10.0 { 2 } else { 1 },
                    shown: 1.0,
                    suffix: if suffix.is_empty() {
                        String::new()
                    } else {
                        format!(" {suffix}")
                    },
                };
                number_row(studio, ui, clip, &row, cx, theme);
            }
            ParamKind::Toggle(default) => {
                let on = effect
                    .params
                    .get(param.key)
                    .and_then(Value::as_bool)
                    .unwrap_or(default);
                if sue_wm::ui::switch(("sve-fx-toggle", effect_id, param.key), &label, on)
                    .show(ui, theme)
                    .clicked()
                {
                    let key = param.key;
                    studio.change_clip(cx, &label, id, |clip| {
                        fx::update(clip, effect_id, |effect| {
                            effect.params.set(key, !on);
                        });
                        Ok(())
                    });
                }
            }
            ParamKind::Choice(options, default) => {
                let names: Vec<String> = options
                    .iter()
                    .map(|option| tr!(&format!("fx-choice-{option}")))
                    .collect();
                let mut chosen = effect
                    .params
                    .get(param.key)
                    .and_then(Value::as_i64)
                    .map_or(default, |index| index.max(0) as usize);
                let before = chosen;
                labeled(ui, &label, theme, |ui| {
                    sue_wm::ui::dropdown(
                        ("sve-fx-choice", effect_id, param.key),
                        &mut chosen,
                        &names,
                    )
                    .label(&label)
                    .show(ui, theme);
                });
                if chosen != before {
                    let key = param.key;
                    studio.change_clip(cx, &label, id, |clip| {
                        fx::update(clip, effect_id, |effect| {
                            effect.params.set(key, Value::Int(chosen as i64));
                        });
                        Ok(())
                    });
                }
            }
            ParamKind::Color(default) => {
                let rgb = effect
                    .params
                    .get(param.key)
                    .and_then(fx::rgb_of)
                    .unwrap_or(default);
                let mut rgba = [rgb[0], rgb[1], rgb[2], 255];
                labeled(ui, &label, theme, |ui| {
                    let _ = sue_wm::color_button(
                        ui,
                        ("sve-fx-color", effect_id, param.key),
                        &mut rgba,
                        false,
                        theme,
                    );
                });
                if rgba[..3] != rgb[..] {
                    let key = param.key;
                    studio.change_clip(cx, &label, id, |clip| {
                        fx::update(clip, effect_id, |effect| {
                            effect
                                .params
                                .set(key, fx::color_value([rgba[0], rgba[1], rgba[2]]));
                        });
                        Ok(())
                    });
                }
            }
            ParamKind::Curve => {
                let mut points = effect
                    .params
                    .get(param.key)
                    .map(fx::points_of)
                    .filter(|points| points.len() >= 2)
                    .unwrap_or_else(|| vec![[0.0, 0.0], [1.0, 1.0]]);
                ui.label(egui::RichText::new(&label).font(font(theme, TextRole::Caption)));
                let side = row_width(ui).min(theme.space(Space::S9) * 4.0);
                let (changed, done) = curve_box(
                    ui,
                    ("sve-fx-curve", effect_id, param.key),
                    &mut points,
                    side,
                    theme,
                );
                if changed {
                    let key = param.key;
                    let value = fx::curve_value(&points);
                    studio.live_clip(cx, id, |clip| {
                        fx::update(clip, effect_id, |effect| {
                            effect.params.set(key, value);
                        });
                    });
                }
                if done {
                    studio.commit_live(cx, &label);
                }
            }
        }
    }
}

/// A tone curve editor: its points from (0, 0) to (1, 1) dragged, a
/// double click adds one, a right click takes one away (never the ends).
/// Returns whether it changed and whether a change ended.
pub fn curve_box(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    points: &mut Vec<[f32; 2]>,
    side: f32,
    theme: &Theme,
) -> (bool, bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.corner(sue_wm::ui::Radius::Xs), theme.void);
    for step in 1..4 {
        let share = step as f32 / 4.0;
        painter.vline(
            rect.left() + rect.width() * share,
            rect.y_range(),
            theme.hairline(),
        );
        painter.hline(
            rect.x_range(),
            rect.top() + rect.height() * share,
            theme.hairline(),
        );
    }
    let to_screen = |[x, y]: [f32; 2]| {
        pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let to_point = |at: egui::Pos2| {
        [
            ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - at.y) / rect.height()).clamp(0.0, 1.0),
        ]
    };
    let response_curve = sue_model::Response::new(points);
    let line: Vec<egui::Pos2> = (0..=64)
        .map(|step| {
            let x = step as f32 / 64.0;
            to_screen([x, response_curve.eval(x)])
        })
        .collect();
    painter.add(egui::Shape::line(
        line,
        egui::Stroke::new(Theme::STRONG_STROKE, theme.text),
    ));
    let id = ui.scope_id().with(salt);
    let mut dragging: Option<usize> = ui.data(|data| data.get_temp(id));
    let mut changed = false;
    let reach = Density::Compact.icon() * 0.6;
    let near = |at: egui::Pos2, points: &[[f32; 2]]| {
        points
            .iter()
            .enumerate()
            .map(|(index, point)| (index, to_screen(*point).distance(at)))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    };
    if response.drag_started()
        && let Some(at) = response.interact_pointer_pos()
    {
        dragging = near(at, points);
    }
    if response.dragged()
        && let (Some(index), Some(at)) = (dragging, response.interact_pointer_pos())
    {
        let mut point = to_point(at);
        let last = points.len() - 1;
        if index == 0 {
            point[0] = 0.0;
        } else if index == last {
            point[0] = 1.0;
        } else {
            let low = points[index - 1][0] + 0.01;
            let high = points[index + 1][0] - 0.01;
            point[0] = point[0].clamp(low, high.max(low));
        }
        points[index] = point;
        changed = true;
    }
    let done = response.drag_stopped() && dragging.is_some();
    if response.drag_stopped() {
        dragging = None;
    }
    if response.double_clicked()
        && let Some(at) = response.interact_pointer_pos()
        && near(at, points).is_none()
    {
        let point = to_point(at);
        let index = points.partition_point(|p| p[0] < point[0]);
        points.insert(index, point);
        changed = true;
    }
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
        && let Some(index) = near(at, points)
        && index != 0
        && index + 1 != points.len()
    {
        points.remove(index);
        changed = true;
    }
    ui.data_mut(|data| data.insert_temp(id, dragging));
    for point in points.iter() {
        let at = to_screen(*point);
        painter.rect_filled(
            Rect::from_center_size(at, vec2(reach, reach)),
            super::SQUARE,
            theme.focus(),
        );
    }
    let clicked_change = changed && !response.dragged();
    (changed, done || clicked_change)
}

/// The masks: each one's mode, inversion, feather, expansion and
/// opacity, and buttons that add an ellipse, a rectangle or a path.
fn masks_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    sue_wm::ui::list::heading(ui, &tr!("controls-masks"), theme);
    let id = clip.id;
    let list = mask::masks(clip);
    let at = studio.clock.position.max(clip.start).min(clip.end());
    let keyed = mask::masks_at(clip, at);
    let modes: Vec<String> = mask::Mode::ALL
        .iter()
        .map(|mode| tr!(&format!("mask-mode-{}", mode.id())))
        .collect();
    for (index, mask) in list.iter().enumerate() {
        let title = tr!("controls-mask-number", number = index as i64 + 1);
        let button = Density::Compact.icon_button();
        let room = row_width(ui);
        let mut action = None;
        let mut chosen = mask::Mode::ALL
            .iter()
            .position(|mode| *mode == mask.mode)
            .unwrap_or(0);
        let before = chosen;
        ui.allocate_ui_with_layout(
            vec2(room, button),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                let (spot, picked) =
                    ui.allocate_exact_size(vec2(room * MASK_NAME_SHARE, button), Sense::click());
                if studio.mask_picked == Some(index) {
                    ui.painter().rect_filled(
                        spot,
                        theme.corner(sue_wm::ui::Radius::Xs),
                        theme.fill_selected(),
                    );
                }
                ui.painter().text(
                    pos2(spot.left() + theme.space(Space::S1), spot.center().y),
                    egui::Align2::LEFT_CENTER,
                    &title,
                    font(theme, TextRole::Body),
                    theme.text,
                );
                if picked.clicked() {
                    studio.mask_picked = Some(index);
                }
                sue_wm::ui::dropdown(("sve-mask-mode", index), &mut chosen, &modes)
                    .width((room * 0.35).max(button))
                    .show(ui, theme);
                if sue_wm::ui::icon_button(
                    ("sve-mask-invert", index),
                    Icon::SelectInvert,
                    &tr!("controls-mask-invert"),
                )
                .selected(mask.inverted)
                .quiet()
                .show(ui, theme)
                .clicked()
                {
                    action = Some("invert");
                }
                // Tracking comes in a later stage: shown, not yet working.
                let _ = sue_wm::ui::icon_button(
                    ("sve-mask-track", index),
                    Icon::ToolMoveLayer,
                    &tr!("controls-mask-track-later"),
                )
                .enabled(false)
                .quiet()
                .show(ui, theme);
                if sue_wm::ui::icon_button(
                    ("sve-mask-remove", index),
                    Icon::Delete,
                    &tr!("controls-mask-remove"),
                )
                .quiet()
                .show(ui, theme)
                .clicked()
                {
                    action = Some("remove");
                }
            },
        );
        if chosen != before || action.is_some() {
            let mode = mask::Mode::ALL[chosen];
            studio.change_clip(cx, &title, id, |clip| {
                let mut masks = mask::masks(clip);
                match action {
                    Some("remove") => {
                        if index < masks.len() {
                            masks.remove(index);
                        }
                        // Its keys go with it; the later masks' keys move up.
                        let names: Vec<String> = clip
                            .animation
                            .channels()
                            .map(|(name, _)| name.to_owned())
                            .filter(|name| name.starts_with("mask."))
                            .collect();
                        let mut moved = Vec::new();
                        for name in names {
                            let mut parts = name.splitn(3, '.');
                            let (_, Some(number), Some(param)) =
                                (parts.next(), parts.next(), parts.next())
                            else {
                                continue;
                            };
                            let Ok(number) = number.parse::<usize>() else {
                                continue;
                            };
                            let curve = clip.animation.remove_channel(&name);
                            if number > index
                                && let Some(curve) = curve
                            {
                                moved.push((mask::channel(number - 1, param), curve));
                            }
                        }
                        for (name, curve) in moved {
                            *clip.animation.channel_mut(&name) = curve;
                        }
                    }
                    Some(_) => {
                        if let Some(mask) = masks.get_mut(index) {
                            mask.inverted = !mask.inverted;
                        }
                    }
                    None => {
                        if let Some(mask) = masks.get_mut(index) {
                            mask.mode = mode;
                        }
                    }
                }
                mask::set_masks(clip, &masks);
                Ok(())
            });
            return;
        }
        let shown = keyed.get(index).unwrap_or(mask);
        for (param, label, value, range, shown_by, suffix) in [
            (
                "feather",
                tr!("controls-mask-feather"),
                shown.feather,
                (0.0, 500.0),
                1.0,
                " px",
            ),
            (
                "expansion",
                tr!("controls-mask-expansion"),
                shown.expansion,
                (-500.0, 500.0),
                1.0,
                " px",
            ),
            (
                "opacity",
                tr!("controls-mask-opacity"),
                shown.opacity,
                (0.0, 1.0),
                100.0,
                " %",
            ),
        ] {
            let row = number(
                Write::Mask(index, param.to_owned()),
                label,
                value,
                range,
                shown_by,
                suffix,
            );
            number_row(studio, ui, clip, &row, cx, theme);
        }
    }
    let mut add = None;
    ui.horizontal_wrapped(|ui| {
        for (what, label) in [
            ("ellipse", tr!("controls-mask-ellipse")),
            ("rect", tr!("controls-mask-rect")),
            ("path", tr!("controls-mask-path")),
        ] {
            if sue_wm::ui::button(("sve-mask-add", what), &label)
                .icon(Icon::LayerMaskNew)
                .show(ui, theme)
                .clicked()
            {
                add = Some(what);
            }
        }
    });
    if let Some(what) = add {
        let label = tr!("controls-add-mask");
        let count = list.len();
        if studio.change_clip(cx, &label, id, |clip| {
            let mut masks = mask::masks(clip);
            masks.push(match what {
                "ellipse" => mask::Mask::ellipse(),
                "rect" => mask::Mask::rect(),
                _ => mask::Mask::of(mask::Shape::Path(
                    [[0.5, 0.2], [0.8, 0.5], [0.5, 0.8], [0.2, 0.5]]
                        .into_iter()
                        .map(mask::PathPoint::corner)
                        .collect(),
                )),
            });
            mask::set_masks(clip, &masks);
            Ok(())
        }) {
            studio.mask_picked = Some(count);
        }
    }
}
