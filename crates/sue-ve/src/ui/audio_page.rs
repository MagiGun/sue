//! The Audio page, laid out as DaVinci Resolve's Fairlight page is: the
//! timeline across the top (each sound clip with its volume line, keys
//! added and dragged there), and under it the mixer: a strip per audio
//! track with its equalizer, gate and compressor switches, pan, fader,
//! meter with loudness, mute and solo; the master strip with the mix's
//! loudness (momentary, short term, integrated LUFS); and at the right
//! the picked track's equalizer bands and dynamics, Normalize and Record.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_core::Value;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font, timeline};
use crate::audio::{self, BandKind, TrackFx};
use crate::edit::EditError;
use crate::fx::key as fx_key;
use crate::model::{self, key};
use crate::studio::SueVe;

/// The timeline's share of the page's height.
const TIMELINE_SHARE: f32 = 0.42;
/// The track settings' share of the width.
const SIDE_SHARE: f32 = 0.3;

/// Draws the Audio page in `body`.
pub fn audio_page(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let split = body.top() + body.height() * TIMELINE_SHARE;
    let top = Rect::from_min_max(body.min, pos2(body.right(), split));
    let side = Rect::from_min_max(
        pos2(body.right() - body.width() * SIDE_SHARE, split),
        body.max,
    );
    let mixer = Rect::from_min_max(pos2(body.left(), split), pos2(side.left(), body.bottom()));
    for rect in [top, mixer, side] {
        ui.painter().rect_filled(rect, SQUARE, theme.panel);
        ui.painter()
            .rect_stroke(rect, SQUARE, theme.hairline(), egui::StrokeKind::Inside);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(top)
            .id_salt("sve-audio-timeline"),
    );
    child.set_clip_rect(top);
    timeline::timeline_ui(studio, &mut child, cx, theme);
    mixer_ui(studio, ui, mixer, cx, theme);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(side.shrink(theme.space(Space::S3)))
            .id_salt("sve-audio-side"),
    );
    child.set_clip_rect(side);
    egui::ScrollArea::vertical()
        .id_salt("sve-audio-side-scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| side_ui(studio, ui, cx, theme));
}

/// The track the side panel edits: the picked one, else the first audio
/// track.
fn picked_track(studio: &SueVe, cx: &AppletContext<'_>) -> Option<usize> {
    let sequence = studio.current(cx)?;
    studio
        .audio_track
        .filter(|index| {
            sequence
                .tracks
                .get(*index)
                .is_some_and(|track| !model::is_video(track))
        })
        .or_else(|| {
            sequence
                .tracks
                .iter()
                .position(|track| !model::is_video(track))
        })
}

/// Changes track `index` with `change`, as one undo step.
fn change_track(
    studio: &mut SueVe,
    cx: &mut AppletContext<'_>,
    label: &str,
    index: usize,
    change: impl FnOnce(&mut sue_model::Track),
) {
    studio.edit(cx, label, |sequence, _| {
        let track = sequence
            .tracks
            .get_mut(index)
            .ok_or(EditError::NoTrack(index))?;
        change(track);
        Ok(())
    });
}

/// Changes track `index` in place while a value is dragged.
fn live_track(
    studio: &mut SueVe,
    cx: &mut AppletContext<'_>,
    index: usize,
    change: impl FnOnce(&mut sue_model::Track),
) {
    studio.edit_live(cx, |sequence| {
        if let Some(track) = sequence.tracks.get_mut(index) {
            change(track);
        }
    });
}

/// Decibels from a meter level.
fn decibels(level: f32) -> f32 {
    if level <= 1e-5 {
        -60.0
    } else {
        20.0 * level.log10()
    }
}

/// A vertical meter of `level` (and its held peak) in `rect`.
fn meter(ui: &Ui, rect: Rect, level: [f32; 2], hold: [f32; 2], theme: &Theme) {
    let painter = ui.painter_at(rect);
    let gap = 2.0_f32.min(rect.width() / 4.0);
    let width = (rect.width() - gap) / 2.0;
    let height_of = |level: f32| ((decibels(level) + 60.0) / 60.0).clamp(0.0, 1.0);
    for channel in 0..2 {
        let left = rect.left() + channel as f32 * (width + gap);
        let column = Rect::from_min_max(pos2(left, rect.top()), pos2(left + width, rect.bottom()));
        painter.rect_filled(column, SQUARE, theme.surface(1));
        let top = column.bottom() - column.height() * height_of(level[channel]);
        let color = if level[channel] >= 1.0 {
            theme.danger()
        } else if level[channel] >= 0.5 {
            theme.warning()
        } else {
            theme.success()
        };
        painter.rect_filled(
            Rect::from_min_max(pos2(column.left(), top), column.max),
            SQUARE,
            color,
        );
        let held = column.bottom() - column.height() * height_of(hold[channel]);
        painter.hline(column.x_range(), held, theme.hairline_active());
    }
}

/// The mixer strips and the master strip.
fn mixer_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let Some(sequence) = studio.current(cx).cloned() else {
        return;
    };
    let strip_width = (theme.space(Space::S9) * 3.2).max(Density::Compact.icon_button() * 4.0);
    let master_width = strip_width * 1.4;
    let inner = rect.shrink(theme.space(Space::S2));
    let picked = picked_track(studio, cx);
    let tracks: Vec<usize> = sequence
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| !model::is_video(track))
        .map(|(index, _)| index)
        .collect();
    let mut x = inner.left();
    let mut picks = None;
    for index in tracks {
        let strip = Rect::from_min_size(pos2(x, inner.top()), vec2(strip_width, inner.height()));
        x += strip_width + theme.space(Space::S2);
        if strip.right() > inner.right() - master_width {
            break;
        }
        let track = &sequence.tracks[index];
        if strip_ui(
            studio,
            ui,
            strip,
            index,
            track,
            picked == Some(index),
            cx,
            theme,
        ) {
            picks = Some(index);
        }
    }
    if let Some(index) = picks {
        studio.audio_track = Some(index);
    }
    // The master: the mix's peaks and loudness.
    let master = Rect::from_min_max(pos2(inner.right() - master_width, inner.top()), inner.max);
    ui.painter().rect_filled(
        master,
        theme.corner(sue_wm::ui::Radius::Sm),
        theme.surface(1),
    );
    let painter = ui.painter_at(master);
    let mut y = master.top() + theme.space(Space::S2);
    painter.text(
        pos2(master.center().x, y),
        Align2::CENTER_TOP,
        tr!("mixer-master"),
        font(theme, TextRole::Caption),
        theme.text,
    );
    y += theme.text_size(TextRole::Caption) + theme.space(Space::S2);
    let loudness = studio.clock.mixer.master.as_ref();
    for (label, value) in [
        (
            tr!("mixer-momentary"),
            loudness.map_or(-120.0, audio::Loudness::momentary),
        ),
        (
            tr!("mixer-short-term"),
            loudness.map_or(-120.0, audio::Loudness::short_term),
        ),
        (
            tr!("mixer-integrated"),
            loudness.map_or(-120.0, audio::Loudness::integrated),
        ),
    ] {
        let shown = if value <= -100.0 {
            "-".to_owned()
        } else {
            format!("{value:.1}")
        };
        painter.text(
            pos2(master.left() + theme.space(Space::S2), y),
            Align2::LEFT_TOP,
            label,
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
        painter.text(
            pos2(master.right() - theme.space(Space::S2), y),
            Align2::RIGHT_TOP,
            format!("{shown} LUFS"),
            font(theme, TextRole::Mono),
            theme.text,
        );
        y += theme.text_size(TextRole::Mono) + theme.space(Space::S1);
    }
    let bars = Rect::from_min_max(
        pos2(
            master.center().x - strip_width * 0.2,
            y + theme.space(Space::S2),
        ),
        pos2(
            master.center().x + strip_width * 0.2,
            master.bottom() - theme.space(Space::S3),
        ),
    );
    meter(
        ui,
        bars,
        studio.clock.meter.level,
        studio.clock.meter.hold,
        theme,
    );
}

/// One track's strip. Returns whether it was clicked (picked).
#[expect(
    clippy::too_many_arguments,
    reason = "a strip needs its place, its track and the studio"
)]
fn strip_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    strip: Rect,
    index: usize,
    track: &sue_model::Track,
    picked: bool,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> bool {
    let back = ui.interact(
        strip,
        egui::Id::unique(("sve-strip", index)),
        Sense::click(),
    );
    ui.painter().rect_filled(
        strip,
        theme.corner(sue_wm::ui::Radius::Sm),
        if picked {
            theme.fill_selected()
        } else {
            theme.surface(1)
        },
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip.shrink(theme.space(Space::S2)))
            .id_salt(("sve-strip-ui", index)),
    );
    let ui = &mut child;
    ui.label(
        egui::RichText::new(&track.name)
            .font(font(theme, TextRole::Caption))
            .color(theme.text),
    );
    let fx = audio::track_fx(track);
    let mut flip = None;
    ui.horizontal_wrapped(|ui| {
        for (what, label, on) in [
            ("eq", tr!("mixer-eq"), fx.eq_on),
            ("gate", tr!("mixer-gate"), fx.gate_on),
            ("comp", tr!("mixer-comp"), fx.comp_on),
        ] {
            if sue_wm::ui::chip(("sve-strip-fx", index, what), &label, on)
                .show(ui, theme)
                .response
                .clicked()
            {
                flip = Some(what);
            }
        }
    });
    if let Some(what) = flip {
        let mut next = fx.clone();
        match what {
            "eq" => next.eq_on = !next.eq_on,
            "gate" => next.gate_on = !next.gate_on,
            _ => next.comp_on = !next.comp_on,
        }
        change_track(studio, cx, &tr!("mixer-change"), index, |track| {
            track.properties.set(fx_key::AUDIO_FX, next.to_value());
        });
    }
    // Pan.
    let mut pan = track
        .properties
        .get(fx_key::PAN)
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        * 100.0;
    let response = sue_wm::ui::slider(("sve-strip-pan", index), &mut pan, -100.0..=100.0)
        .name(&tr!("controls-pan"))
        .field(false)
        .show(ui, theme);
    if response.changed() {
        let value = pan / 100.0;
        live_track(studio, cx, index, |track| {
            track.properties.set(fx_key::PAN, value);
        });
        if !response.dragged() {
            studio.commit_live(cx, &tr!("controls-pan"));
        }
    }
    if response.drag_stopped() || response.double_clicked() {
        if response.double_clicked() {
            live_track(studio, cx, index, |track| {
                track.properties.remove(fx_key::PAN);
            });
        }
        studio.commit_live(cx, &tr!("controls-pan"));
    }
    // Fader and meter side by side.
    let meters = studio.clock.mixer.tracks.get(&index).cloned();
    let loudness = meters.as_ref().map_or(-120.0, |m| m.loudness.short_term());
    let reduction = meters.as_ref().map_or(0.0, |m| m.reduction);
    let mut volume = track
        .properties
        .get(key::VOLUME)
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let left = ui.available_rect_before_wrap();
    let footer = Density::Compact.icon_button() * 2.0 + theme.space(Space::S3);
    let fader_height = (left.height() - footer).max(Density::Compact.icon_button() * 2.0);
    ui.horizontal(|ui| {
        let response = sue_wm::ui::tall_slider(("sve-fader", index), &mut volume, -60.0..=12.0)
            .label(&tr!("mixer-fader"))
            .decimals(1)
            .height(fader_height)
            .show(ui, theme);
        if response.changed() {
            let value = volume;
            live_track(studio, cx, index, |track| {
                track.properties.set(key::VOLUME, value);
            });
            if !response.dragged() {
                studio.commit_live(cx, &tr!("mixer-fader"));
            }
        }
        if response.drag_stopped() {
            studio.commit_live(cx, &tr!("mixer-fader"));
        }
        let (bars, _) = ui.allocate_exact_size(
            vec2(Density::Compact.icon_button() * 0.6, fader_height),
            Sense::hover(),
        );
        if let Some(found) = &meters {
            meter(ui, bars, found.meter.level, found.meter.hold, theme);
        } else {
            meter(ui, bars, [0.0; 2], [0.0; 2], theme);
        }
    });
    ui.label(
        egui::RichText::new(format!(
            "{volume:+.1} dB  {}",
            if loudness <= -100.0 {
                "-".to_owned()
            } else {
                format!("{loudness:.0} LUFS")
            }
        ))
        .font(font(theme, TextRole::Micro))
        .color(theme.text_muted()),
    );
    if reduction > 0.1 {
        ui.label(
            egui::RichText::new(tr!("mixer-reduction", decibels = format!("{reduction:.1}")))
                .font(font(theme, TextRole::Micro))
                .color(theme.warning()),
        );
    }
    let mut toggled = None;
    ui.horizontal(|ui| {
        if sue_wm::ui::icon_button(("sve-strip-mute", index), Icon::Audio, &tr!("track-mute"))
            .selected(track.muted)
            .show(ui, theme)
            .clicked()
        {
            toggled = Some("mute");
        }
        if sue_wm::ui::icon_button(("sve-strip-solo", index), Icon::Star, &tr!("track-solo"))
            .selected(model::flag(track, key::SOLO))
            .show(ui, theme)
            .clicked()
        {
            toggled = Some("solo");
        }
    });
    if let Some(what) = toggled {
        change_track(studio, cx, &tr!("track-change"), index, |track| {
            if what == "mute" {
                track.muted = !track.muted;
            } else {
                let on = !model::flag(track, key::SOLO);
                track.properties.set(key::SOLO, on);
            }
        });
    }
    back.clicked()
}

/// The picked track's equalizer and dynamics, and the page's actions.
fn side_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let recording = studio.recording.is_some();
        if sue_wm::ui::button(
            "sve-record",
            &if recording {
                tr!("record-stop")
            } else {
                tr!("cmd-record-voice-over")
            },
        )
        .icon(Icon::Microphone)
        .primary()
        .show(ui, theme)
        .clicked()
        {
            studio.run("audio.record", cx);
        }
        if sue_wm::ui::button("sve-normalize", &tr!("cmd-normalize"))
            .enabled(!studio.selection.is_empty())
            .show(ui, theme)
            .clicked()
        {
            studio.run("audio.normalize", cx);
        }
    });
    if let Some(recording) = &studio.recording {
        let seconds = recording.samples.len() as f64
            / f64::from(recording.input.rate.max(1))
            / f64::from(recording.input.channels.max(1));
        ui.label(
            egui::RichText::new(tr!(
                "record-running",
                seconds = format!("{seconds:.1}"),
                device = recording.input.device.clone()
            ))
            .font(font(theme, TextRole::Caption))
            .color(theme.danger()),
        );
    }
    let Some(index) = picked_track(studio, cx) else {
        return;
    };
    let Some(track) = studio
        .current(cx)
        .and_then(|sequence| sequence.tracks.get(index))
        .cloned()
    else {
        return;
    };
    sue_wm::ui::heading(
        ui,
        &tr!("mixer-track-settings", track = track.name.clone()),
        theme,
    );
    let before = audio::track_fx(&track);
    let mut fx = before.clone();
    let mut done = false;
    eq_graph(ui, &fx, theme);
    sue_wm::ui::list::heading(ui, &tr!("mixer-eq"), theme);
    let kinds: Vec<String> = BandKind::ALL
        .iter()
        .map(|kind| tr!(&format!("band-{}", kind.id())))
        .collect();
    for (band_index, band) in fx.bands.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut chosen = BandKind::ALL
                .iter()
                .position(|kind| *kind == band.kind)
                .unwrap_or(1);
            let was = chosen;
            sue_wm::ui::dropdown(("sve-band-kind", band_index), &mut chosen, &kinds)
                .width(theme.space(Space::S9) * 2.2)
                .show(ui, theme);
            if chosen != was {
                band.kind = BandKind::ALL[chosen];
                done = true;
            }
        });
        for (what, label, value, range, suffix, decimals) in [
            (
                "frequency",
                tr!("band-frequency"),
                &mut band.frequency,
                20.0..=20_000.0,
                " Hz",
                0,
            ),
            (
                "gain",
                tr!("band-gain"),
                &mut band.gain,
                -24.0..=24.0,
                " dB",
                1,
            ),
            ("q", tr!("band-q"), &mut band.q, 0.1..=10.0, "", 2),
        ] {
            let response = sue_wm::ui::slider(("sve-band", band_index, what), value, range)
                .label(&label)
                .decimals(decimals)
                .suffix(suffix)
                .show(ui, theme);
            if response.drag_stopped()
                || response.lost_focus()
                || (response.changed() && !response.dragged())
            {
                done = true;
            }
        }
    }
    sue_wm::ui::list::heading(ui, &tr!("mixer-gate"), theme);
    for (what, label, value, range, suffix) in [
        (
            "gate-threshold",
            tr!("mixer-threshold"),
            &mut fx.gate_threshold,
            -90.0..=0.0,
            " dB",
        ),
        (
            "gate-range",
            tr!("mixer-range"),
            &mut fx.gate_range,
            -90.0..=0.0,
            " dB",
        ),
        (
            "gate-release",
            tr!("mixer-release"),
            &mut fx.gate_release,
            5.0..=1000.0,
            " ms",
        ),
    ] {
        let response = sue_wm::ui::slider(("sve-dyn", what), value, range)
            .label(&label)
            .decimals(0)
            .suffix(suffix)
            .show(ui, theme);
        if response.drag_stopped()
            || response.lost_focus()
            || (response.changed() && !response.dragged())
        {
            done = true;
        }
    }
    sue_wm::ui::list::heading(ui, &tr!("mixer-comp"), theme);
    for (what, label, value, range, suffix) in [
        (
            "threshold",
            tr!("mixer-threshold"),
            &mut fx.threshold,
            -60.0..=0.0,
            " dB",
        ),
        ("ratio", tr!("mixer-ratio"), &mut fx.ratio, 1.0..=20.0, ":1"),
        (
            "attack",
            tr!("mixer-attack"),
            &mut fx.attack,
            0.1..=200.0,
            " ms",
        ),
        (
            "release",
            tr!("mixer-release"),
            &mut fx.release,
            5.0..=2000.0,
            " ms",
        ),
        (
            "makeup",
            tr!("mixer-makeup"),
            &mut fx.makeup,
            -12.0..=24.0,
            " dB",
        ),
    ] {
        let response = sue_wm::ui::slider(("sve-dyn", what), value, range)
            .label(&label)
            .decimals(1)
            .suffix(suffix)
            .show(ui, theme);
        if response.drag_stopped()
            || response.lost_focus()
            || (response.changed() && !response.dragged())
        {
            done = true;
        }
    }
    if fx != before {
        let value = fx.to_value();
        live_track(studio, cx, index, |track| {
            track.properties.set(fx_key::AUDIO_FX, value);
        });
    }
    if done {
        studio.commit_live(cx, &tr!("mixer-change"));
    }
}

/// The equalizer's response across the hearing range.
fn eq_graph(ui: &mut Ui, fx: &TrackFx, theme: &Theme) {
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(vec2(width, Density::Regular.row() * 3.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SQUARE, theme.void);
    painter.hline(rect.x_range(), rect.center().y, theme.hairline());
    let rate = 48_000.0;
    let filters = fx.filters(rate);
    let points: Vec<egui::Pos2> = (0..=96)
        .map(|step| {
            let share = f64::from(step) / 96.0;
            let frequency = 20.0 * (1000.0_f64).powf(share);
            let gain: f64 = if fx.eq_on {
                filters
                    .iter()
                    .map(|filter| filter.response(frequency, rate))
                    .sum()
            } else {
                0.0
            };
            pos2(
                rect.left() + rect.width() * share as f32,
                rect.center().y - (gain / 24.0).clamp(-1.0, 1.0) as f32 * rect.height() / 2.0,
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(Theme::STRONG_STROKE, theme.text),
    ));
}
