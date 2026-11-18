//! The graph editor, as After Effects' is: the selected clip's curve of
//! one keyframed value over the clip's time, as a value graph (the value
//! itself) or a speed graph (how fast it changes, which is where eases
//! show). Keys drag in time and value, a Bezier key's handles drag its
//! shape, a click picks keys (Shift adds), and the buttons ease the picked
//! keys (Easy Ease, Ease In, Ease Out), make them linear or hold. Each
//! drag is one undo step.

use egui::{Align2, Pos2, Rect, Sense, Ui, pos2, vec2};
use sue_core::Time;
use sue_model::{Clip, Curve, Interpolation};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font};
use crate::edit::EditError;
use crate::keys::{self, Ease};
use crate::studio::SueVe;

/// A Bezier handle's dot, a share of a key's square.
const HANDLE_DOT: f32 = 0.4;

/// The channels of `clip` that have keys, by name, with a name to show.
#[must_use]
pub fn keyed_channels(clip: &Clip) -> Vec<(String, String)> {
    clip.animation
        .channels()
        .filter(|(_, curve)| !curve.keys().is_empty())
        .map(|(name, _)| (name.to_owned(), channel_label(clip, name)))
        .collect()
}

/// A channel's name as people read it.
#[must_use]
pub fn channel_label(clip: &Clip, name: &str) -> String {
    use crate::fx::key as fx_key;
    use crate::model::key;
    if let Some((id, param)) = crate::fx::parse_channel(name) {
        let effect = crate::fx::effects(clip)
            .into_iter()
            .find(|effect| effect.id == id)
            .and_then(|effect| crate::fx::kind(&effect.kind));
        return match effect {
            Some(kind) => {
                let param = kind
                    .param(param)
                    .map_or_else(|| param.to_owned(), crate::fx::ParamDef::label);
                format!("{}: {param}", kind.label())
            }
            None => name.to_owned(),
        };
    }
    match name {
        key::OPACITY => tr!("inspector-opacity"),
        key::SCALE => tr!("inspector-scale"),
        key::POS_X => tr!("inspector-x"),
        key::POS_Y => tr!("inspector-y"),
        key::ROTATION => tr!("inspector-rotation"),
        key::VOLUME => tr!("inspector-volume"),
        fx_key::ANCHOR_X => tr!("controls-anchor-x"),
        fx_key::ANCHOR_Y => tr!("controls-anchor-y"),
        fx_key::PAN => tr!("controls-pan"),
        fx_key::REMAP => tr!("controls-time-remapping"),
        other => other.to_owned(),
    }
}

/// The graph editor palette.
#[allow(
    clippy::too_many_lines,
    reason = "the graph's drawing and its dragging share their geometry"
)]
pub fn graph_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, SQUARE, theme.void);
    let Some((_, clip)) = studio.selected_clip(cx) else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        sue_wm::ui::empty_state("sve-graph-empty", &tr!("graph-empty"))
            .icon(Icon::Graph)
            .show(&mut child, theme);
        return;
    };
    let mut channels = keyed_channels(&clip);
    // A value of another clip (an effect or a mask this one lacks) goes.
    if let Some(name) = studio.graph.channel.clone() {
        let belongs = match crate::fx::parse_channel(&name) {
            Some((id, _)) => crate::fx::effects(&clip)
                .iter()
                .any(|effect| effect.id == id),
            None => name.strip_prefix("mask.").is_none_or(|rest| {
                rest.split('.')
                    .next()
                    .and_then(|index| index.parse::<usize>().ok())
                    .is_some_and(|index| index < crate::fx::mask::masks(&clip).len())
            }),
        };
        if !belongs {
            studio.graph.channel = None;
        }
    }
    // The value picked in Effect Controls stays picked before it has a
    // key (Add Keyframe keys it); with none picked, the first keyed one.
    match studio.graph.channel.clone() {
        Some(name) if !channels.iter().any(|(found, _)| *found == name) => {
            let label = channel_label(&clip, &name);
            channels.insert(0, (name, label));
        }
        Some(_) => {}
        None => studio.graph.channel = channels.first().map(|(name, _)| name.clone()),
    }
    // The bar: the channel, value or speed, the eases.
    let bar_height = Density::Compact.icon_button() + theme.space(Space::S2);
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), bar_height));
    ui.painter().rect_filled(bar, SQUARE, theme.panel);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(theme.space(Space::S2), theme.space(Space::S1))))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let names: Vec<String> = channels.iter().map(|(_, label)| label.clone()).collect();
    let mut chosen = studio
        .graph
        .channel
        .as_ref()
        .and_then(|name| channels.iter().position(|(found, _)| found == name))
        .unwrap_or(0);
    let mut ease = None;
    if names.is_empty() {
        child.label(
            egui::RichText::new(tr!("graph-no-keys"))
                .font(font(theme, TextRole::Caption))
                .color(theme.text_muted()),
        );
    } else {
        let width = (rect.width() * 0.3).max(theme.space(Space::S9) * 2.0);
        sue_wm::ui::dropdown("sve-graph-channel", &mut chosen, &names)
            .width(width)
            .show(&mut child, theme);
        studio.graph.channel = channels.get(chosen).map(|(name, _)| name.clone());
        let modes = [tr!("graph-value"), tr!("graph-speed")];
        let answer = sue_wm::ui::segmented(
            "sve-graph-mode",
            &modes,
            Some(usize::from(studio.graph.speed)),
        )
        .show(&mut child, theme);
        if let Some(picked) = answer.picked {
            studio.graph.speed = picked == 1;
        }
        for (icon, what, tip) in [
            (Icon::Keyframe, Ease::Both, tr!("graph-easy-ease")),
            (Icon::ArrowRight, Ease::In, tr!("graph-ease-in")),
            (Icon::ArrowLeft, Ease::Out, tr!("graph-ease-out")),
            (Icon::Minus, Ease::Linear, tr!("graph-linear")),
            (Icon::Stop, Ease::Hold, tr!("graph-hold")),
        ] {
            if sue_wm::ui::icon_button(("sve-graph-ease", tip.as_str()), icon, &tip)
                .quiet()
                .show(&mut child, theme)
                .clicked()
            {
                ease = Some(what);
            }
        }
    }
    let Some(channel) = studio.graph.channel.clone() else {
        return;
    };
    let Some(curve) = clip.animation.channel(&channel).cloned() else {
        return;
    };
    if let Some(ease) = ease {
        let picked: Vec<Time> = if studio.graph.picked.is_empty() {
            curve.keys().iter().map(|key| key.time).collect()
        } else {
            studio.graph.picked.clone()
        };
        let channel_name = channel.clone();
        studio.change_clip(cx, &tr!("graph-ease"), clip.id, |clip| {
            let curve = clip.animation.channel_mut(&channel_name);
            let mut any = false;
            for time in &picked {
                any |= keys::ease_key(curve, *time, ease);
            }
            if any { Ok(()) } else { Err(EditError::Nothing) }
        });
        return;
    }
    // The graph.
    let margin = theme.space(Space::S9) * 1.2;
    let area = Rect::from_min_max(
        pos2(rect.left() + margin, bar.bottom() + theme.space(Space::S4)),
        rect.max - vec2(theme.space(Space::S6), theme.space(Space::S4)),
    );
    if area.width() < 8.0 || area.height() < 8.0 {
        return;
    }
    let painter = ui.painter_at(Rect::from_min_max(
        pos2(rect.left(), area.top() - theme.space(Space::S3)),
        area.max + vec2(theme.space(Space::S3), theme.space(Space::S3)),
    ));
    let (from, to) = (
        clip.source_in,
        clip.source_out().max(clip.source_in + Time(1)),
    );
    let span = (to - from).seconds().max(1e-6);
    let x_of = |time: Time| area.left() + area.width() * ((time - from).seconds() / span) as f32;
    let time_of =
        |x: f32| from + Time::from_seconds(f64::from((x - area.left()) / area.width()) * span);
    // What is drawn: the value, or its speed.
    let sample = |time: Time| {
        if studio.graph.speed {
            keys::speed(&curve, time)
        } else {
            curve.sample(time).unwrap_or(0.0)
        }
    };
    let columns = area.width().max(2.0) as usize;
    let values: Vec<f64> = (0..=columns)
        .map(|column| sample(from + Time::from_seconds(span * column as f64 / columns as f64)))
        .collect();
    let mut low = values.iter().copied().fold(f64::MAX, f64::min);
    let mut high = values.iter().copied().fold(f64::MIN, f64::max);
    if !studio.graph.speed {
        for key in curve.keys() {
            low = low.min(key.value);
            high = high.max(key.value);
        }
    }
    if (high - low).abs() < 1e-9 {
        low -= 1.0;
        high += 1.0;
    }
    let pad = (high - low) * 0.1;
    let (low, high) = (low - pad, high + pad);
    let y_of = |value: f64| area.bottom() - area.height() * ((value - low) / (high - low)) as f32;
    let value_of = |y: f32| low + f64::from((area.bottom() - y) / area.height()) * (high - low);
    // Grid lines with their values.
    for step in 0..=4 {
        let value = low + (high - low) * f64::from(step) / 4.0;
        let y = y_of(value);
        painter.hline(area.x_range(), y, theme.hairline());
        painter.text(
            pos2(area.left() - theme.space(Space::S1), y),
            Align2::RIGHT_CENTER,
            format!("{value:.1}"),
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
    }
    // The playhead.
    let playhead = clip.source_time(studio.clock.position);
    if (from..=to).contains(&playhead) {
        painter.vline(
            x_of(playhead),
            area.y_range(),
            egui::Stroke::new(Theme::STRONG_STROKE, theme.focus()),
        );
    }
    let points: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(column, value)| pos2(area.left() + column as f32, y_of(*value)))
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(Theme::STRONG_STROKE, theme.text),
    ));
    // Keys and handles.
    let side = Density::Compact.icon() * 0.7;
    let keys_now = curve.keys().to_vec();
    let mut handles: Vec<(Time, u8, Pos2)> = Vec::new();
    for (index, key) in keys_now.iter().enumerate() {
        let at = pos2(x_of(key.time), y_of(sample(key.time)));
        let picked = studio.graph.picked.contains(&key.time);
        painter.rect_filled(
            Rect::from_center_size(at, vec2(side, side)),
            SQUARE,
            if picked { theme.focus() } else { theme.text },
        );
        handles.push((key.time, 0, at));
        if studio.graph.speed {
            continue;
        }
        if let Interpolation::Bezier { out, incoming } = key.interpolation
            && let Some(next) = keys_now.get(index + 1)
        {
            let seg = (next.time - key.time).seconds();
            let out_at = pos2(
                x_of(key.time + Time::from_seconds(out[0].clamp(0.0, seg))),
                y_of(key.value + out[1]),
            );
            let in_at = pos2(
                x_of(next.time + Time::from_seconds(incoming[0].clamp(-seg, 0.0))),
                y_of(next.value + incoming[1]),
            );
            for (start, end, which, owner) in [
                (at, out_at, 1, key.time),
                (pos2(x_of(next.time), y_of(next.value)), in_at, 2, next.time),
            ] {
                painter.line_segment([start, end], theme.hairline_active());
                painter.circle_filled(end, side * HANDLE_DOT, theme.focus());
                handles.push((owner, which, end));
            }
        }
    }
    // Dragging and picking.
    let response = ui.interact(
        area.expand(side),
        egui::Id::unique("sve-graph-area"),
        Sense::click_and_drag(),
    );
    let reach = side * 1.2;
    let nearest = |at: Pos2| {
        handles
            .iter()
            .map(|(time, which, spot)| (*time, *which, spot.distance(at)))
            .filter(|(_, _, d)| *d <= reach)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(time, which, _)| (time, which))
    };
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let shift = ui.input(|input| input.modifiers.shift);
        match nearest(at) {
            Some((time, 0)) => {
                if shift {
                    if let Some(found) = studio.graph.picked.iter().position(|t| *t == time) {
                        studio.graph.picked.remove(found);
                    } else {
                        studio.graph.picked.push(time);
                    }
                } else {
                    studio.graph.picked = vec![time];
                }
            }
            _ => studio.graph.picked.clear(),
        }
    }
    if response.drag_started()
        && let Some(at) = response.interact_pointer_pos()
    {
        studio.graph.dragging = nearest(at);
    }
    if response.dragged()
        && let (Some((time, which)), Some(at)) =
            (studio.graph.dragging, response.interact_pointer_pos())
        && !studio.graph.speed
    {
        let channel_name = channel.clone();
        let id = clip.id;
        let new_time = time_of(at.x);
        let new_value = value_of(at.y);
        let mut landed = time;
        studio.live_clip(cx, id, |clip| {
            let curve = clip.animation.channel_mut(&channel_name);
            match which {
                0 => {
                    if let Some(moved) = keys::move_key(curve, time, new_time, new_value) {
                        landed = moved;
                    }
                }
                _ => set_handle(curve, time, which, new_time, new_value),
            }
        });
        if which == 0 {
            studio.graph.dragging = Some((landed, 0));
            studio.graph.picked = vec![landed];
        }
    }
    if response.drag_stopped() && studio.graph.dragging.take().is_some() {
        studio.commit_live(cx, &tr!("graph-move-key"));
    }
}

/// Moves a Bezier handle of the key at `time` (1 its outgoing handle, 2
/// the incoming handle of the segment arriving at it) to `at`, `value`.
fn set_handle(curve: &mut Curve, time: Time, which: u8, at: Time, value: f64) {
    let keys = curve.keys().to_vec();
    let Some(index) = keys.iter().position(|key| key.time == time) else {
        return;
    };
    let owner = if which == 1 {
        Some(index)
    } else {
        index.checked_sub(1)
    };
    let Some(owner) = owner else { return };
    let (Some(key), Some(next)) = (keys.get(owner), keys.get(owner + 1)) else {
        return;
    };
    let Interpolation::Bezier {
        mut out,
        mut incoming,
    } = key.interpolation
    else {
        return;
    };
    let seg = (next.time - key.time).seconds();
    if which == 1 {
        out = [(at - key.time).seconds().clamp(0.0, seg), value - key.value];
    } else {
        incoming = [
            (at - next.time).seconds().clamp(-seg, 0.0),
            value - next.value,
        ];
    }
    curve.insert(sue_model::Key {
        interpolation: Interpolation::Bezier { out, incoming },
        ..*key
    });
}
