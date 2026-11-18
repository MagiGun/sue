//! The timeline: the ruler with the in and out range and the markers,
//! the track headers (target, lock, hide or mute, solo, sync lock), and
//! the tracks with their clips (thumbnails on pictures, waveforms on
//! sound), transitions and the playhead.
//!
//! The tools work by dragging: the selection tool moves clips (holding
//! Ctrl inserts instead of overwriting) and trims their edges, the ripple
//! tool trims and moves what follows, the roll tool moves a cut, the
//! slip tool shows other frames of a clip, the slide tool moves a clip
//! between its neighbours, and the razor cuts. While a drag goes on the
//! tracks show the sequence as the edit would leave it (the same edit
//! algebra runs on a copy each frame); letting go makes it one undo step.

use std::sync::Arc;

use egui::{Align2, Pos2, Rect, Sense, Ui, pos2, vec2};
use sue_core::{EntityId, Time};
use sue_model::{Clip, Sequence};
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::pool::{PoolItem, texture_key};
use super::{SQUARE, font, timecode};
use crate::edit::{self, Edge, EditCtx, EditError};
use crate::media::sound::{PEAK_STEP, Sound};
use crate::model::{self, SourceKind, key};
use crate::studio::{SueVe, Tool};

/// The narrowest the timeline zooms: pixels a second.
pub const LEAST_ZOOM: f32 = 0.5;
/// The widest: pixels a second (about 20 pixels a frame at 30).
pub const MOST_ZOOM: f32 = 600.0;
/// The track headers' width.
pub const HEADER_WIDTH: f32 = 168.0;
/// A video track's height.
const VIDEO_HEIGHT: f32 = 58.0;
/// An audio track's height.
const AUDIO_HEIGHT: f32 = 50.0;
/// How near an edge a press grabs it, in pixels.
const EDGE_GRAB: f32 = 6.0;
/// How strongly clips and waveforms are colored over the lanes.
const SOFT: f32 = 0.55;
/// How near a point snaps, in pixels.
const SNAP_PIXELS: f32 = 8.0;

/// A drag on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    /// The playhead, on the ruler or a monitor's jog bar.
    Scrub,
    /// Clips moving.
    Move {
        /// The clips.
        ids: Vec<u64>,
        /// Where the press was.
        from: Pos2,
        /// The track pressed on.
        track: usize,
        /// How far in time.
        delta: Time,
        /// How many tracks down.
        tracks: isize,
    },
    /// Clip edges trimming.
    Trim {
        /// The clips.
        ids: Vec<u64>,
        /// Which edge.
        edge: Edge,
        /// Moving what follows.
        ripple: bool,
        /// Where the press was.
        from: Pos2,
        /// How far.
        delta: Time,
    },
    /// Cuts rolling: pairs of the clip before and the clip after.
    Roll {
        /// The pairs.
        pairs: Vec<(u64, u64)>,
        /// Where the press was.
        from: Pos2,
        /// How far.
        delta: Time,
    },
    /// Clips slipping.
    Slip {
        /// The clips.
        ids: Vec<u64>,
        /// Where the press was.
        from: Pos2,
        /// How far.
        delta: Time,
    },
    /// Clips sliding.
    Slide {
        /// The clips.
        ids: Vec<u64>,
        /// Where the press was.
        from: Pos2,
        /// How far.
        delta: Time,
    },
}

/// Does what `drag` does to `sequence` (a copy while dragging, the real
/// one on release through the undo history).
///
/// # Errors
/// The edit's own refusal.
pub fn apply_drag(
    sequence: &mut Sequence,
    drag: &Drag,
    insert: bool,
    ctx: &EditCtx<'_>,
) -> Result<(), EditError> {
    match drag {
        Drag::Scrub => Ok(()),
        Drag::Move {
            ids, delta, tracks, ..
        } => edit::move_clips(sequence, ids, *delta, *tracks, insert).map(|_| ()),
        Drag::Trim {
            ids,
            edge,
            ripple,
            delta,
            ..
        } => edit::trim(sequence, ids, *edge, *ripple, *delta, ctx).map(|_| ()),
        Drag::Roll { pairs, delta, .. } => {
            for (left, right) in pairs {
                edit::roll(sequence, *left, *right, *delta, ctx)?;
            }
            Ok(())
        }
        Drag::Slip { ids, delta, .. } => {
            for id in ids {
                edit::slip(sequence, *id, *delta, ctx)?;
            }
            Ok(())
        }
        Drag::Slide { ids, delta, .. } => {
            for id in ids {
                edit::slide(sequence, *id, *delta, ctx)?;
            }
            Ok(())
        }
    }
}

/// Where each track is down the lanes: its top and height.
fn rows(sequence: &Sequence, top: f32) -> Vec<(f32, f32)> {
    let mut y = top;
    sequence
        .tracks
        .iter()
        .map(|track| {
            let height = if model::is_video(track) {
                VIDEO_HEIGHT
            } else {
                AUDIO_HEIGHT
            };
            let row = (y, height);
            y += height;
            row
        })
        .collect()
}

/// Draws a waveform of `sound` from source time `from` to `to` across
/// `rect`, `gain` times as loud.
pub(crate) fn waveform(
    ui: &Ui,
    rect: Rect,
    sound: &Arc<Sound>,
    from: Time,
    to: Time,
    gain: f32,
    theme: &Theme,
) {
    let painter = ui.painter_at(rect);
    let middle = rect.center().y;
    let half = rect.height() / 2.0;
    let points_per_second = 48_000.0 / PEAK_STEP as f64;
    let span = (to - from).seconds();
    let columns = rect.width().max(1.0) as usize;
    let color = theme.text_muted();
    for column in 0..columns {
        let a = from.seconds() + span * column as f64 / columns as f64;
        let b = from.seconds() + span * (column + 1) as f64 / columns as f64;
        let first = (a * points_per_second).floor().max(0.0) as usize;
        let last = ((b * points_per_second).ceil() as usize).max(first + 1);
        let mut peak = [0.0_f32, 0.0_f32];
        for point in sound
            .peaks
            .get(first..last.min(sound.peaks.len()))
            .unwrap_or(&[])
        {
            peak[0] = peak[0].min(point[0]);
            peak[1] = peak[1].max(point[1]);
        }
        if peak == [0.0, 0.0] {
            continue;
        }
        let x = rect.left() + column as f32 + 0.5;
        painter.vline(
            x,
            (middle - (peak[1] * gain).min(1.0) * half)
                ..=(middle - (peak[0] * gain).max(-1.0) * half),
            egui::Stroke::new(theme.border_width, color),
        );
    }
}

/// The timeline palette.
#[allow(
    clippy::too_many_lines,
    reason = "the timeline's drawing and its tools share their geometry"
)]
pub fn timeline_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect();
    studio.timeline_rect = Some(rect);
    let Some(id) = studio.sequence else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        sue_wm::ui::empty_state("sve-timeline-empty", &tr!("timeline-empty"))
            .icon(Icon::PaletteTimeline)
            .show(&mut child, theme);
        return;
    };
    let Some(real) = model::sequence(cx.arena(), id).cloned() else {
        return;
    };
    let ruler_height = Density::Compact.row();
    let ruler = Rect::from_min_max(
        pos2(rect.left() + HEADER_WIDTH, rect.top()),
        pos2(rect.right(), rect.top() + ruler_height),
    );
    let lanes = Rect::from_min_max(pos2(rect.left() + HEADER_WIDTH, ruler.bottom()), rect.max);
    let headers = Rect::from_min_max(
        pos2(rect.left(), ruler.bottom()),
        pos2(lanes.left(), rect.bottom()),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SQUARE, theme.void);
    painter.rect_filled(
        Rect::from_min_max(rect.min, pos2(lanes.left(), ruler.bottom())),
        SQUARE,
        theme.panel,
    );
    let insert = ui.input(|input| input.modifiers.command);
    // While dragging, the tracks show what the edit would make.
    let shown = match &studio.drag {
        Some(drag) if !matches!(drag, Drag::Scrub) => {
            let arena = cx.arena();
            let length = |source: EntityId| model::length_of(arena, source);
            let ctx = EditCtx { media_len: &length };
            let mut preview = real.clone();
            preview.ensure_ids();
            match apply_drag(&mut preview, drag, insert, &ctx) {
                Ok(()) => preview,
                Err(_) => real.clone(),
            }
        }
        _ => real.clone(),
    };
    let zoom = studio.zoom;
    let scroll = studio.scroll;
    let x_of = |time: Time| lanes.left() + (time.seconds() as f32 - scroll) * zoom;
    let time_of =
        |x: f32| Time::from_seconds(f64::from(((x - lanes.left()) / zoom + scroll).max(0.0)));
    let top = lanes.top() - studio.scroll_y;
    let rows = rows(&shown, top);
    // Wheel: scroll across, with Ctrl zoom about the pointer, with Shift
    // scroll down the tracks.
    let hovered = ui.rect_contains_pointer(rect);
    if hovered {
        let (delta, zooming, down, pointer) = ui.input(|input| {
            (
                input.smooth_scroll_delta,
                input.modifiers.command,
                input.modifiers.shift,
                input.pointer.hover_pos(),
            )
        });
        if zooming && delta.y != 0.0 {
            let anchor = pointer.map_or(lanes.left(), |at| at.x);
            let at = time_of(anchor);
            studio.zoom = (zoom * (1.0 + delta.y / 200.0)).clamp(LEAST_ZOOM, MOST_ZOOM);
            studio.scroll = (at.seconds() as f32 - (anchor - lanes.left()) / studio.zoom).max(0.0);
        } else if down {
            let content: f32 = rows.iter().map(|row| row.1).sum();
            studio.scroll_y =
                (studio.scroll_y - delta.y).clamp(0.0, (content - lanes.height()).max(0.0));
        } else if delta != egui::Vec2::ZERO {
            studio.scroll = (scroll - (delta.y + delta.x) / zoom).max(0.0);
        }
    }
    // The ruler: ticks, seconds, the in to out range, markers.
    let ruler_painter = ui.painter_at(ruler);
    ruler_painter.rect_filled(ruler, SQUARE, theme.panel);
    let (mark_in, mark_out) = model::marks(&shown);
    if mark_in.is_some() || mark_out.is_some() {
        let a = x_of(mark_in.unwrap_or(Time::ZERO));
        let b = x_of(mark_out.unwrap_or(shown.duration()));
        let span = Rect::from_min_max(pos2(a, ruler.top()), pos2(b, ruler.bottom()));
        ruler_painter.rect_filled(span, SQUARE, theme.fill_selected());
    }
    let step = [
        0.04_f32, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0,
    ]
    .into_iter()
    .find(|step| step * zoom >= theme.space(Space::S9) * 2.0)
    .unwrap_or(600.0);
    let first = (scroll / step).floor() as i64;
    let last = ((scroll + ruler.width() / zoom) / step).ceil() as i64;
    for tick in first..=last {
        let seconds = tick as f32 * step;
        let x = x_of(Time::from_seconds(f64::from(seconds)));
        ruler_painter.vline(
            x,
            (ruler.bottom() - theme.space(Space::S3))..=ruler.bottom(),
            theme.hairline(),
        );
        ruler_painter.text(
            pos2(
                x + theme.space(Space::S1),
                ruler.top() + theme.space(Space::S1),
            ),
            Align2::LEFT_TOP,
            timecode(Time::from_seconds(f64::from(seconds)), shown.rate),
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
    }
    let mark = Density::Compact.icon();
    for marker in &shown.markers {
        let x = x_of(marker.at);
        icons::paint(
            &ruler_painter,
            Rect::from_center_size(pos2(x, ruler.bottom() - mark / 2.0), vec2(mark, mark)),
            Icon::Marker,
            theme.warning(),
        );
    }
    // The ruler scrubs.
    let scrub = ui.interact(
        ruler,
        egui::Id::unique("sve-ruler"),
        Sense::click_and_drag(),
    );
    if (scrub.dragged() || scrub.clicked() || scrub.drag_started())
        && let Some(at) = scrub.interact_pointer_pos()
    {
        let mut time = time_of(at.x);
        if studio.snapping {
            let points = edit::snap_points(&shown, &[], &[]);
            time = edit::snap(
                time,
                &points,
                Time::from_seconds(f64::from(SNAP_PIXELS / zoom)),
            )
            .unwrap_or(time);
        }
        let now = cx.view.now;
        studio.clock.seek(time, now, cx.sink());
        studio.focus = crate::studio::Monitor::Program;
        studio.drag = scrub.dragged().then_some(Drag::Scrub);
    }
    if scrub.drag_stopped() {
        studio.drag = None;
    }
    // The tracks.
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let mut hit: Option<(usize, u64, Option<Edge>)> = None;
    let mut track_under: Option<usize> = None;
    let mut header_actions: Vec<(usize, &'static str)> = Vec::new();
    for (index, track) in shown.tracks.iter().enumerate() {
        let (y, height) = rows[index];
        let lane = Rect::from_min_size(pos2(lanes.left(), y), vec2(lanes.width(), height));
        let header = Rect::from_min_size(pos2(headers.left(), y), vec2(headers.width(), height));
        if lane.bottom() < lanes.top() || lane.top() > lanes.bottom() {
            continue;
        }
        if pointer
            .is_some_and(|at| at.y >= lane.top() && at.y < lane.bottom() && at.x >= rect.left())
        {
            track_under = Some(index);
        }
        let lane_painter = ui.painter_at(lane.intersect(lanes));
        let video = model::is_video(track);
        lane_painter.rect_filled(
            lane,
            SQUARE,
            if index % 2 == 0 {
                theme.void
            } else {
                theme.surface(1)
            },
        );
        lane_painter.hline(lane.x_range(), lane.bottom(), theme.hairline());
        // The header.
        let header_painter = ui.painter_at(header.intersect(headers));
        header_painter.rect_filled(header, SQUARE, theme.panel);
        header_painter.hline(header.x_range(), header.bottom(), theme.hairline());
        let targeted = model::flag(track, key::TARGET);
        header_painter.text(
            pos2(
                header.left() + theme.space(Space::S3),
                header.top() + theme.space(Space::S2),
            ),
            Align2::LEFT_TOP,
            &track.name,
            font(theme, TextRole::Caption),
            if targeted {
                theme.text
            } else {
                theme.text_muted()
            },
        );
        let button = Density::Compact.icon_button();
        let mut x = header.left() + theme.space(Space::S2);
        let y_buttons = header.bottom() - button - theme.space(Space::S1);
        let mut toggles: Vec<(Icon, &'static str, bool, String)> = vec![
            (
                if targeted {
                    Icon::RadioOn
                } else {
                    Icon::RadioOff
                },
                "target",
                targeted,
                tr!("track-target"),
            ),
            (
                if track.locked {
                    Icon::Lock
                } else {
                    Icon::Unlock
                },
                "lock",
                track.locked,
                tr!("track-lock"),
            ),
        ];
        if video {
            toggles.push((
                if track.muted { Icon::EyeOff } else { Icon::Eye },
                "mute",
                track.muted,
                tr!("track-hide"),
            ));
        } else {
            toggles.push((Icon::Audio, "mute", track.muted, tr!("track-mute")));
            toggles.push((
                Icon::Star,
                "solo",
                model::flag(track, key::SOLO),
                tr!("track-solo"),
            ));
        }
        toggles.push((
            Icon::Link,
            "sync",
            model::flag(track, key::SYNC_LOCK),
            tr!("track-sync-lock"),
        ));
        for (icon, what, on, name) in toggles {
            let place = Rect::from_min_size(pos2(x, y_buttons), vec2(button, button));
            x += button;
            if header.intersect(headers).contains_rect(place)
                && sue_wm::ui::icon_button(("sve-track", index, what), icon, &name)
                    .selected(on)
                    .quiet()
                    .show_at(ui, place, theme)
                    .clicked()
            {
                header_actions.push((index, what));
            }
        }
        // The clips.
        for clip in track.clips() {
            let left = x_of(clip.start);
            let right = x_of(clip.end());
            if right < lanes.left() || left > lanes.right() {
                continue;
            }
            let body = Rect::from_min_max(
                pos2(left, lane.top() + theme.space(Space::S1)),
                pos2(
                    right.max(left + 1.0),
                    lane.bottom() - theme.space(Space::S1),
                ),
            );
            clip_ui(studio, ui, &lane_painter, body, clip, video, cx, theme);
            if let Some(at) = pointer
                && body.contains(at)
                && ui.rect_contains_pointer(lanes)
            {
                let edge = if at.x - body.left() <= EDGE_GRAB.min(body.width() / 3.0) {
                    Some(Edge::In)
                } else if body.right() - at.x <= EDGE_GRAB.min(body.width() / 3.0) {
                    Some(Edge::Out)
                } else {
                    None
                };
                hit = Some((index, clip.id, edge));
            }
        }
        // Transitions over their cuts.
        for transition in &track.transitions {
            let span = Rect::from_min_max(
                pos2(x_of(transition.start), lane.top() + theme.space(Space::S2)),
                pos2(
                    x_of(transition.end()),
                    lane.bottom() - theme.space(Space::S2),
                ),
            );
            lane_painter.rect_filled(span, theme.corner(sue_wm::ui::Radius::Xs), theme.scrim());
            lane_painter.line_segment(
                [span.left_bottom(), span.right_top()],
                theme.hairline_active(),
            );
            lane_painter.text(
                span.center(),
                Align2::CENTER_CENTER,
                transition_name(&transition.kind),
                font(theme, TextRole::Micro),
                theme.text,
            );
        }
    }
    // The line between the pictures and the sound.
    if let Some(boundary) = shown
        .tracks
        .iter()
        .position(|track| !model::is_video(track))
    {
        let y = rows[boundary].0;
        ui.painter_at(rect).hline(
            rect.x_range(),
            y,
            egui::Stroke::new(Theme::STRONG_STROKE, theme.border),
        );
    }
    // The playhead.
    let x = x_of(studio.clock.position);
    if x >= lanes.left() {
        let playhead = ui.painter_at(Rect::from_min_max(pos2(lanes.left(), rect.top()), rect.max));
        playhead.vline(
            x,
            rect.y_range(),
            egui::Stroke::new(Theme::STRONG_STROKE, theme.focus()),
        );
    }
    // The tools.
    interact(
        studio,
        ui,
        lanes,
        &real,
        hit,
        track_under,
        &time_of,
        insert,
        cx,
    );
    // Drops from the pool and the effects palette.
    let released = ui.input(|input| input.pointer.any_released());
    if released && ui.rect_contains_pointer(lanes) {
        let at = pointer.map_or(Time::ZERO, |at| time_of(at.x));
        if let Some(item) = egui::DragAndDrop::take_payload::<PoolItem>(ui.ctx()) {
            let track = track_under.unwrap_or_else(|| studio.targets(cx).0.unwrap_or(0));
            let mut at = at;
            if studio.snapping {
                let points = edit::snap_points(&real, &[], &[studio.clock.position]);
                at = edit::snap(
                    at,
                    &points,
                    Time::from_seconds(f64::from(SNAP_PIXELS / zoom)),
                )
                .unwrap_or(at);
            }
            studio.place(cx, item.0, at, track, None, insert);
        } else if let Some(kind) =
            egui::DragAndDrop::take_payload::<super::panels::TransitionItem>(ui.ctx())
            && let Some(track) = track_under
        {
            let cut = real.tracks[track]
                .clips()
                .iter()
                .flat_map(|clip| [clip.start, clip.end()])
                .min_by_key(|cut| (*cut - at).flicks().abs());
            if let Some(cut) = cut {
                let length = Time::from_frames(i64::from(real.rate.timecode_base()), real.rate);
                studio.edit(cx, &tr!("cmd-add-dissolve"), |sequence, _| {
                    edit::add_transition(sequence, track, kind.0, cut, length).map(|_| ())
                });
            }
        }
    }
    // Track header buttons.
    for (index, what) in header_actions {
        let label = tr!("track-change");
        studio.edit(cx, &label, |sequence, _| {
            let track = sequence
                .tracks
                .get_mut(index)
                .ok_or(EditError::NoTrack(index))?;
            match what {
                "lock" => track.locked = !track.locked,
                "mute" => track.muted = !track.muted,
                "target" => {
                    let on = !model::flag(track, key::TARGET);
                    track.properties.set(key::TARGET, on);
                }
                "solo" => {
                    let on = !model::flag(track, key::SOLO);
                    track.properties.set(key::SOLO, on);
                }
                _ => {
                    let on = !model::flag(track, key::SYNC_LOCK);
                    track.properties.set(key::SYNC_LOCK, on);
                }
            }
            Ok(())
        });
    }
}

/// A transition's short name.
fn transition_name(kind: &str) -> String {
    match kind {
        "dip-black" => tr!("transition-dip-black"),
        "dip-white" => tr!("transition-dip-white"),
        _ => tr!("transition-dissolve"),
    }
}

/// One clip: its body, its name, and thumbnails or a waveform.
#[allow(
    clippy::too_many_arguments,
    reason = "the clip's place, its track and the studio's caches"
)]
fn clip_ui(
    studio: &mut SueVe,
    ui: &Ui,
    painter: &egui::Painter,
    body: Rect,
    clip: &Clip,
    video: bool,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let selected = studio.selection.contains(&clip.id);
    let base = if video { theme.info() } else { theme.success() };
    let fill = if clip.enabled {
        base.gamma_multiply(SOFT)
    } else {
        theme.surface(2)
    };
    let radius = theme.corner(sue_wm::ui::Radius::Xs);
    painter.rect_filled(body, radius, fill);
    let kind = cx
        .arena()
        .get_by_id(clip.source)
        .map_or(SourceKind::Unknown, SourceKind::of);
    let inner = body.shrink(theme.space(Space::S1));
    let label_height = theme.text_size(TextRole::Micro) + theme.space(Space::S1);
    let content = Rect::from_min_max(pos2(inner.left(), inner.top() + label_height), inner.max);
    if video {
        clip_picture(studio, ui, painter, content, clip, &kind, cx);
    } else if let Some(sound) = studio.hub.sounds.get(clip.source) {
        let gain = crate::mix::gain(clip.value_at(key::VOLUME, clip.start, 0.0));
        let from = clip.source_in;
        let to = clip.source_out();
        waveform(
            ui,
            content.intersect(painter.clip_rect()),
            &sound,
            from,
            to,
            gain,
            theme,
        );
    }
    let name = cx
        .arena()
        .get_by_id(clip.source)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    let speed = if (clip.speed - 1.0).abs() > 1e-6 {
        format!("  {:.0}%", clip.speed * 100.0)
    } else {
        String::new()
    };
    painter.text(
        pos2(
            inner.left().max(painter.clip_rect().left()) + theme.space(Space::S1),
            inner.top(),
        ),
        Align2::LEFT_TOP,
        format!("{name}{speed}"),
        font(theme, TextRole::Micro),
        theme.text,
    );
    // Keyframes, as small diamonds along the foot of the clip.
    let side = Density::Compact.icon() * 0.75;
    let span = (clip.end() - clip.start).seconds().max(1e-6);
    for (_, curve) in clip.animation.channels() {
        for key in curve.keys() {
            let offset = (key.time - clip.source_in).seconds() / clip.speed.abs().max(1e-6);
            if !(0.0..=span).contains(&offset) {
                continue;
            }
            let x = body.left() + body.width() * (offset / span) as f32;
            icons::paint(
                painter,
                Rect::from_center_size(pos2(x, body.bottom() - side / 2.0), vec2(side, side)),
                Icon::Keyframe,
                theme.text,
            );
        }
    }
    let stroke = if selected {
        theme.selected_stroke()
    } else {
        theme.hairline()
    };
    painter.rect_stroke(body, radius, stroke, egui::StrokeKind::Inside);
    if clip.link != 0 {
        let side = Density::Compact.icon();
        icons::paint(
            painter,
            Rect::from_min_size(
                pos2(body.right() - side - theme.space(Space::S1), body.top()),
                vec2(side, side),
            ),
            Icon::Link,
            theme.text_muted(),
        );
    }
}

/// The thumbnails along a picture clip (or one picture for stills and
/// generated sources).
fn clip_picture(
    studio: &mut SueVe,
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    clip: &Clip,
    kind: &SourceKind,
    cx: &mut AppletContext<'_>,
) {
    if rect.height() < 4.0 || rect.width() < 4.0 {
        return;
    }
    let tint = egui::Color32::WHITE.gamma_multiply(if clip.enabled { 1.0 } else { 0.4 });
    if *kind == SourceKind::Movie {
        let Some(strip) = studio.hub.thumbs.strip(clip.source) else {
            return;
        };
        let strip = strip
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(first) = strip.frames.first() else {
            return;
        };
        let width = rect.height() * first.width as f32 / first.height.max(1) as f32;
        let count = (rect.width() / width).ceil().max(1.0) as usize;
        for index in 0..count {
            let left = rect.left() + index as f32 * width;
            let place = Rect::from_min_size(pos2(left, rect.top()), vec2(width, rect.height()))
                .intersect(rect);
            let share = f64::from((left - rect.left()) / rect.width().max(1.0));
            let at =
                clip.source_in.seconds() + (clip.source_out() - clip.source_in).seconds() * share;
            let Some(frame) = strip.at(at) else { continue };
            let key = texture_key(("sve-thumb", clip.source, frame.index));
            if let Some(texture) = cx.textures.texture(
                key,
                frame.serial,
                [frame.width, frame.height],
                4,
                &frame.rgba,
                false,
            ) {
                let uv_right = place.width() / width;
                painter.image(
                    texture,
                    place,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(uv_right, 1.0)),
                    tint,
                );
            }
        }
        let _ = ui;
        return;
    }
    if matches!(kind, SourceKind::Title | SourceKind::Adjustment) {
        let side = rect.height().min(Density::Compact.icon());
        icons::paint(
            painter,
            Rect::from_min_size(rect.left_center() - vec2(0.0, side / 2.0), vec2(side, side)),
            super::pool::kind_icon(kind),
            tint,
        );
        return;
    }
    if let Some(found) = studio.hub.frame(
        cx.arena(),
        clip.source,
        kind,
        clip.source_in,
        (256, 144),
        crate::media::Quality::Quarter,
        crate::media::Need::Exact,
    ) {
        let frame = found.frame;
        let key = texture_key(("sve-clip-picture", clip.source));
        if let Some(texture) = cx.textures.texture(
            key,
            frame.serial,
            [frame.width, frame.height],
            4,
            &frame.rgba,
            false,
        ) {
            let width = rect.height() * frame.width as f32 / frame.height.max(1) as f32;
            let place = Rect::from_min_size(rect.min, vec2(width.min(rect.width()), rect.height()));
            painter.image(
                texture,
                place,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                tint,
            );
        }
    }
}

/// Presses and drags on the lanes, by tool.
#[allow(
    clippy::too_many_arguments,
    reason = "the tools read the lanes' geometry and the sequence"
)]
fn interact(
    studio: &mut SueVe,
    ui: &Ui,
    lanes: Rect,
    real: &Sequence,
    hit: Option<(usize, u64, Option<Edge>)>,
    track_under: Option<usize>,
    time_of: &dyn Fn(f32) -> Time,
    insert: bool,
    cx: &mut AppletContext<'_>,
) {
    let response = ui.interact(
        lanes,
        egui::Id::unique("sve-lanes"),
        Sense::click_and_drag(),
    );
    let shift = ui.input(|input| input.modifiers.shift);
    let pointer = response.interact_pointer_pos();
    let zoom = studio.zoom;
    let snap_within = Time::from_seconds(f64::from(SNAP_PIXELS / zoom));
    // The cursor tells what a press would do.
    if let Some((_, _, edge)) = hit
        && response.hovered()
    {
        let cursor = match (studio.tool, edge) {
            (Tool::Razor, _) => egui::CursorIcon::Crosshair,
            (Tool::Slip | Tool::Slide, _) => egui::CursorIcon::ResizeHorizontal,
            (_, Some(_)) => egui::CursorIcon::ResizeColumn,
            _ => egui::CursorIcon::Grab,
        };
        ui.ctx().set_cursor_icon(cursor);
    }
    if response.clicked() {
        match (hit, studio.tool) {
            (Some((_, id, _)), Tool::Razor) => {
                let at = pointer.map_or(Time::ZERO, |at| time_of(at.x));
                let at = Time::from_frames(at.frame(real.rate), real.rate);
                let ids = if studio.linked {
                    real.linked(id)
                } else {
                    vec![id]
                };
                studio.edit(cx, &tr!("cmd-add-edit"), |sequence, _| {
                    if edit::razor_clips(sequence, &ids, at).is_empty() {
                        Err(EditError::Nothing)
                    } else {
                        Ok(())
                    }
                });
            }
            (Some((_, id, _)), _) => {
                if shift {
                    if let Some(at) = studio.selection.iter().position(|selected| *selected == id) {
                        studio.selection.remove(at);
                    } else {
                        studio.selection.push(id);
                    }
                } else {
                    studio.selection = vec![id];
                }
                // The clip's source is picked in the pool too.
                studio.picked = real.find(id).map(|(_, clip)| clip.source);
            }
            (None, _) => {
                studio.selection.clear();
                if let Some(at) = pointer {
                    let now = cx.view.now;
                    studio.clock.seek(time_of(at.x), now, cx.sink());
                }
            }
        }
    }
    if response.drag_started()
        && let Some(from) = pointer
        && let Some((track, id, edge)) = hit
    {
        if !studio.selection.contains(&id) {
            studio.selection = vec![id];
        }
        let ids = studio.selected_with_links(cx);
        let own = if studio.linked {
            real.linked(id)
        } else {
            vec![id]
        };
        studio.drag = match (studio.tool, edge) {
            (Tool::Razor, _) => None,
            (Tool::Slip, _) => Some(Drag::Slip {
                ids: own,
                from,
                delta: Time::ZERO,
            }),
            (Tool::Slide, _) => Some(Drag::Slide {
                ids: own,
                from,
                delta: Time::ZERO,
            }),
            (Tool::Roll, Some(edge)) => {
                let pairs: Vec<(u64, u64)> = own
                    .iter()
                    .filter_map(|clip| {
                        let (index, this) = real.find(*clip)?;
                        let track = &real.tracks[index];
                        match edge {
                            Edge::Out => track
                                .clips()
                                .iter()
                                .find(|next| next.start == this.end())
                                .map(|next| (this.id, next.id)),
                            Edge::In => track
                                .clips()
                                .iter()
                                .find(|prev| prev.end() == this.start)
                                .map(|prev| (prev.id, this.id)),
                        }
                    })
                    .collect();
                if pairs.is_empty() {
                    Some(Drag::Trim {
                        ids: own,
                        edge,
                        ripple: false,
                        from,
                        delta: Time::ZERO,
                    })
                } else {
                    Some(Drag::Roll {
                        pairs,
                        from,
                        delta: Time::ZERO,
                    })
                }
            }
            (tool, Some(edge)) => Some(Drag::Trim {
                ids: own,
                edge,
                ripple: tool == Tool::Ripple,
                from,
                delta: Time::ZERO,
            }),
            (_, None) => Some(Drag::Move {
                ids,
                from,
                track,
                delta: Time::ZERO,
                tracks: 0,
            }),
        };
    }
    // Following the pointer.
    if response.dragged()
        && let Some(at) = pointer
        && let Some(drag) = studio.drag.as_mut()
    {
        let seconds = |from: Pos2| Time::from_seconds(f64::from((at.x - from.x) / zoom));
        match drag {
            Drag::Scrub => {}
            Drag::Move {
                ids,
                from,
                track,
                delta,
                tracks,
            } => {
                let mut moved = seconds(*from);
                if studio.snapping {
                    let points = edit::snap_points(real, ids, &[studio.clock.position]);
                    let edges: Vec<Time> = ids
                        .iter()
                        .filter_map(|id| real.find(*id))
                        .flat_map(|(_, clip)| [clip.start, clip.end()])
                        .collect();
                    if let Some(adjust) = edges
                        .iter()
                        .filter_map(|edge| {
                            edit::snap(*edge + moved, &points, snap_within)
                                .map(|to| to - (*edge + moved))
                        })
                        .min_by_key(|adjust| adjust.flicks().abs())
                    {
                        moved = moved + adjust;
                    }
                }
                *delta = moved;
                if let Some(under) = track_under {
                    *tracks = under as isize - *track as isize;
                }
            }
            Drag::Trim {
                ids,
                edge,
                from,
                delta,
                ..
            } => {
                let mut moved = seconds(*from);
                if studio.snapping
                    && let Some((_, clip)) = ids.first().and_then(|id| real.find(*id))
                {
                    let point = if *edge == Edge::In {
                        clip.start
                    } else {
                        clip.end()
                    };
                    let points = edit::snap_points(real, ids, &[studio.clock.position]);
                    if let Some(to) = edit::snap(point + moved, &points, snap_within) {
                        moved = to - point;
                    }
                }
                *delta = moved;
            }
            Drag::Roll { from, delta, .. } | Drag::Slide { from, delta, .. } => {
                *delta = seconds(*from)
            }
            Drag::Slip { from, delta, .. } => *delta = -seconds(*from),
        }
    }
    if response.drag_stopped()
        && let Some(drag) = studio.drag.take()
        && !matches!(drag, Drag::Scrub)
    {
        let label = match &drag {
            Drag::Move { .. } => tr!("edit-move"),
            Drag::Trim { ripple: true, .. } => tr!("cmd-tool-ripple"),
            Drag::Trim { .. } => tr!("edit-trim"),
            Drag::Roll { .. } => tr!("cmd-tool-roll"),
            Drag::Slip { .. } => tr!("cmd-tool-slip"),
            Drag::Slide { .. } => tr!("cmd-tool-slide"),
            Drag::Scrub => String::new(),
        };
        studio.edit(cx, &label, |sequence, ctx| {
            apply_drag(sequence, &drag, insert, ctx)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::Rate;

    #[test]
    fn a_drag_previews_and_commits_the_same_edit() {
        let film = EntityId::new();
        let mut sequence = model::empty_sequence(64, 36, Rate::FPS_25, 2, 1);
        let mut clip = Clip::new(film, Time::ZERO, Time::SECOND);
        clip.id = 1;
        sequence.tracks[1].insert(clip);
        sequence.next_id = 5;
        let ctx = EditCtx::ENDLESS;
        let drag = Drag::Move {
            ids: vec![1],
            from: Pos2::ZERO,
            track: 1,
            delta: Time::SECOND,
            tracks: -1,
        };
        let mut moved = sequence.clone();
        apply_drag(&mut moved, &drag, false, &ctx).unwrap();
        assert!(moved.tracks[1].clips().is_empty());
        assert_eq!(moved.tracks[0].clips()[0].start, Time::SECOND);
        let rows = rows(&moved, 0.0);
        assert_eq!(rows[1], (VIDEO_HEIGHT, VIDEO_HEIGHT));
        assert_eq!(rows[2].1, AUDIO_HEIGHT);
        // A move onto the sound track is refused, as the preview shows.
        let wrong = Drag::Move {
            ids: vec![1],
            from: Pos2::ZERO,
            track: 1,
            delta: Time::SECOND,
            tracks: 1,
        };
        assert_eq!(
            apply_drag(&mut sequence.clone(), &wrong, false, &ctx),
            Err(EditError::WrongKind)
        );
    }
}
