//! The source and program monitors, side by side as in DaVinci Resolve's
//! Edit page. The program monitor draws plain layers on the graphics
//! card through Sue's painter: each source frame is a texture (uploaded
//! once per new frame, straight from the decoded bytes) and each layer a
//! textured quad placed, turned and faded as its clip says, so cuts,
//! opacity, transforms and dissolves preview in real time without
//! compositing on the processor. A frame with effects, grades, masks,
//! wipes, adjustment clips or nested sequences is drawn by the export's
//! own renderer on the preview worker (`crate::preview`) and shown from
//! the render cache; until it is there, the last one drawn stands in.

use std::sync::Arc;

use egui::epaint::{Mesh, Vertex};
use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use sue_core::{EntityId, Time};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::pool::texture_key;
use super::{SQUARE, font, timecode};
use crate::compose::{self, Options};
use crate::media::Need;
use crate::model::{self, SourceKind};
use crate::preview::Want;
use crate::render::{self, Layer};
use crate::studio::{Monitor, Page, SueVe};

/// Draws both monitors in `rect`.
pub fn monitors(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    ui.painter().rect_filled(rect, SQUARE, theme.void);
    let gap = theme.space(Space::S2);
    let half = (rect.width() - gap) / 2.0;
    let source = Rect::from_min_size(rect.min, vec2(half, rect.height()));
    let program = Rect::from_min_max(pos2(rect.right() - half, rect.top()), rect.max);
    source_monitor(studio, ui, source, cx, theme);
    program_monitor(studio, ui, program, cx, theme);
}

/// The parts of a monitor: its title row, its picture, its jog bar and
/// its transport row.
struct Parts {
    title: Rect,
    picture: Rect,
    jog: Rect,
    transport: Rect,
}

fn parts(rect: Rect, theme: &Theme) -> Parts {
    let row = Density::Compact.icon_button() + theme.space(Space::S2);
    let jog = theme.space(Space::S5);
    let title = Rect::from_min_size(rect.min, vec2(rect.width(), row));
    let transport = Rect::from_min_max(pos2(rect.left(), rect.bottom() - row), rect.max);
    let jog = Rect::from_min_max(
        pos2(rect.left(), transport.top() - jog),
        pos2(rect.right(), transport.top()),
    );
    let picture = Rect::from_min_max(
        pos2(rect.left(), title.bottom()),
        pos2(rect.right(), jog.top()),
    );
    Parts {
        title,
        picture,
        jog,
        transport,
    }
}

/// The frame of `size` fitted in `area`.
fn fitted(area: Rect, size: (u32, u32)) -> Rect {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let fit = (area.width() / w).min(area.height() / h);
    Rect::from_center_size(area.center(), vec2(w * fit, h * fit))
}

/// Draws the layers onto `frame_rect`, a frame of `size`: each layer's
/// texture as a quad, then its tint over it. Returns whether every layer
/// showed the frame due.
fn draw_layers(
    studio: &mut SueVe,
    ui: &Ui,
    frame_rect: Rect,
    size: (u32, u32),
    layers: &[Layer],
    need: Need,
    cx: &mut AppletContext<'_>,
) -> bool {
    let painter = ui.painter_at(frame_rect);
    painter.rect_filled(frame_rect, SQUARE, Color32::BLACK);
    let scale = frame_rect.width() / size.0.max(1) as f32;
    let mut due = true;
    for (slot, layer) in layers.iter().enumerate() {
        let Some(found) = studio.hub.frame(
            cx.arena(),
            layer.source,
            &layer.kind,
            layer.source_time,
            size,
            studio.quality,
            need,
        ) else {
            due = false;
            studio.waiting_for_frame |= studio.hub.error(layer.source).is_none();
            continue;
        };
        due &= found.exact;
        if !found.exact && need != Need::Fast {
            studio.waiting_for_frame |= studio.hub.error(layer.source).is_none();
        }
        let frame = found.frame;
        let key = texture_key(("sve-layer", layer.source, slot));
        let Some(texture) = cx.textures.texture(
            key,
            frame.serial,
            [frame.width, frame.height],
            4,
            &frame.rgba,
            false,
        ) else {
            continue;
        };
        let natural = render::natural_size(&layer.kind, (frame.width, frame.height), size);
        let corners = render::corners(layer, natural, size)
            .map(|[x, y]| frame_rect.min + vec2(x * scale, y * scale));
        let alpha = (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let tint = Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha);
        let uvs = [
            pos2(0.0, 0.0),
            pos2(1.0, 0.0),
            pos2(1.0, 1.0),
            pos2(0.0, 1.0),
        ];
        let mut mesh = Mesh::with_texture(texture);
        for (corner, uv) in corners.iter().zip(uvs) {
            mesh.vertices.push(Vertex {
                pos: *corner,
                uv,
                color: tint,
            });
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        painter.add(mesh);
        if let Some((rgb, amount)) = layer.tint {
            let a = (amount.clamp(0.0, 1.0) * layer.opacity * 255.0).round() as u8;
            let premultiply = |channel: u8| ((u16::from(channel) * u16::from(a)) / 255) as u8;
            let color = Color32::from_rgba_premultiplied(
                premultiply(rgb[0]),
                premultiply(rgb[1]),
                premultiply(rgb[2]),
                a,
            );
            let mut wash = Mesh::default();
            for corner in &corners {
                wash.colored_vertex(*corner, color);
            }
            wash.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(wash);
        }
    }
    due
}

/// The frame at `time` drawn on the processor (by the preview worker,
/// shown from the render cache), filling `frame_rect`; with `split`, the
/// left half as shot and the right half graded. Returns whether the
/// frame due showed.
#[expect(
    clippy::too_many_arguments,
    reason = "what the program monitor draws: where, the sequence, the moment and the split"
)]
fn processed_frame(
    studio: &mut SueVe,
    ui: &Ui,
    frame_rect: Rect,
    size: (u32, u32),
    sequence: &sue_model::Sequence,
    time: Time,
    split: bool,
    cx: &mut AppletContext<'_>,
) -> bool {
    let painter = ui.painter_at(frame_rect);
    painter.rect_filled(frame_rect, SQUARE, Color32::BLACK);
    let Some(id) = studio.sequence else {
        return false;
    };
    let hardware = studio.hub.hardware;
    let playing = studio.clock.playing;
    // While playing, a frame larger than 720p that needs the processor is
    // drawn at half size unless already rendered (Premiere Pro's playback
    // resolution, which defaults to a half): paused, it is full.
    let drawn = {
        let full = studio.quality.of(size);
        let half = crate::media::Quality::Half.of(size);
        let held = |at: (u32, u32)| {
            let arena = cx.arena();
            studio.preview.has(compose::frame_key(
                arena,
                sequence,
                time,
                at,
                Options::default(),
            ))
        };
        if playing && full.0 * full.1 > 1280 * 720 && !held(full) {
            half
        } else {
            full
        }
    };
    let mut due = true;
    let sides: Vec<(Options, Rect, Rect)> = if split {
        let middle = frame_rect.center().x;
        vec![
            (
                Options { grades: false },
                Rect::from_min_max(frame_rect.min, pos2(middle, frame_rect.bottom())),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(0.5, 1.0)),
            ),
            (
                Options::default(),
                Rect::from_min_max(pos2(middle, frame_rect.top()), frame_rect.max),
                Rect::from_min_max(pos2(0.5, 0.0), pos2(1.0, 1.0)),
            ),
        ]
    } else {
        vec![(
            Options::default(),
            frame_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        )]
    };
    let mut wants = Vec::new();
    for (index, (options, place, uv)) in sides.iter().enumerate() {
        let arena = cx.arena();
        let key = compose::frame_key(arena, sequence, time, drawn, *options);
        let found = studio.preview.get(key);
        if found.is_none() {
            due = false;
            let ahead = if playing { 10 } else { 1 };
            for step in 0..ahead {
                let at = time + Time::from_frames(step, sequence.rate);
                wants.push(Want {
                    time: at,
                    key: compose::frame_key(arena, sequence, at, drawn, *options),
                    options: *options,
                });
            }
        }
        let frame = found.or_else(|| studio.preview.last.clone());
        let Some(frame) = frame else {
            continue;
        };
        if index + 1 == sides.len() {
            studio.preview.last = Some(Arc::clone(&frame));
            studio.shown = Some(Arc::clone(&frame));
        }
        let key = texture_key(("sve-processed", index));
        if let Some(texture) = cx.textures.texture(
            key,
            frame.serial,
            [frame.width, frame.height],
            4,
            &frame.rgba,
            false,
        ) {
            painter.image(texture, *place, *uv, Color32::WHITE);
        }
    }
    if split {
        painter.vline(
            frame_rect.center().x,
            frame_rect.y_range(),
            egui::Stroke::new(Theme::STRONG_STROKE, Color32::WHITE),
        );
    }
    if !wants.is_empty() {
        studio.preview.sync(cx.arena(), id, drawn, hardware);
        studio.preview.want(wants);
        ui.ctx().request_repaint();
    } else if !playing && !studio.preview.busy() {
        render_ahead(studio, sequence, time, drawn, cx);
    }
    due
}

/// While paused, the next few seconds that need the processor are drawn
/// into the render cache in the background (DaVinci Resolve's smart
/// cache), so pressing play after an edit plays them at once.
fn render_ahead(
    studio: &mut SueVe,
    sequence: &sue_model::Sequence,
    time: Time,
    drawn: (u32, u32),
    cx: &AppletContext<'_>,
) {
    if !crate::studio::rice_bool(cx.rice(), crate::studio::rice::BACKGROUND_RENDER, true) {
        return;
    }
    let arena = cx.arena();
    let mark = (arena.revision(), time.frame(sequence.rate));
    if studio.preview.ahead_for == Some(mark) {
        return;
    }
    studio.preview.ahead_for = Some(mark);
    let kind = |id: EntityId| {
        arena
            .get_by_id(id)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let seconds = 5;
    let frames = i64::from(sequence.rate.timecode_base()) * seconds;
    let wants: Vec<Want> = (1..=frames)
        .map(|step| time + Time::from_frames(step, sequence.rate))
        .filter(|at| *at < sequence.duration())
        .filter(|at| compose::needs_processing(sequence, *at, &kind))
        .map(|at| Want {
            time: at,
            key: compose::frame_key(arena, sequence, at, drawn, Options::default()),
            options: Options::default(),
        })
        .filter(|want| !studio.preview.has(want.key))
        .collect();
    if !wants.is_empty() {
        studio.preview.render_ahead(wants);
    }
}

/// A jog bar: the length of the source or sequence, its marks, and the
/// playhead, which a press or a drag moves. Returns where it was moved.
fn jog(
    ui: &Ui,
    rect: Rect,
    salt: &str,
    position: Time,
    length: Time,
    marks: (Option<Time>, Option<Time>),
    theme: &Theme,
) -> Option<(Time, bool)> {
    let painter = ui.painter_at(rect);
    let bar = rect.shrink2(vec2(theme.space(Space::S4), theme.space(Space::S2)));
    painter.rect_filled(bar, theme.corner(sue_wm::ui::Radius::Xs), theme.pill);
    let seconds = length.seconds().max(1e-3) as f32;
    let x_of =
        |time: Time| bar.left() + bar.width() * (time.seconds() as f32 / seconds).clamp(0.0, 1.0);
    if let (Some(a), Some(b)) = marks {
        let span = Rect::from_min_max(pos2(x_of(a), bar.top()), pos2(x_of(b), bar.bottom()));
        painter.rect_filled(span, SQUARE, theme.fill_selected());
    }
    for mark in [marks.0, marks.1].into_iter().flatten() {
        painter.vline(x_of(mark), rect.y_range(), theme.hairline_active());
    }
    let x = x_of(position);
    painter.vline(
        x,
        rect.y_range(),
        egui::Stroke::new(Theme::STRONG_STROKE, theme.focus()),
    );
    let response = ui.interact(
        rect,
        egui::Id::unique(("sve-jog", salt)),
        Sense::click_and_drag(),
    );
    if (response.dragged() || response.clicked())
        && let Some(at) = response.interact_pointer_pos()
    {
        let share = ((at.x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);
        return Some((
            Time::from_seconds(f64::from(share * seconds)),
            response.dragged(),
        ));
    }
    None
}

/// The transport row's buttons; returns the commands pressed.
fn transport(
    studio: &SueVe,
    ui: &Ui,
    rect: Rect,
    salt: &str,
    buttons: &[(Icon, &'static str, bool)],
    theme: &Theme,
) -> Vec<&'static str> {
    let size = Density::Compact.icon_button();
    let gap = theme.space(Space::S1);
    let width = buttons.len() as f32 * (size + gap);
    let mut x = rect.center().x - width / 2.0;
    let y = rect.center().y - size / 2.0;
    let mut pressed = Vec::new();
    for (icon, command, on) in buttons {
        let place = Rect::from_min_size(pos2(x, y), vec2(size, size));
        x += size + gap;
        let (name, keys) = studio.tip(command);
        if sue_wm::ui::icon_button((salt, *command), *icon, &name)
            .selected(*on)
            .shortcut(&keys)
            .show_at(ui, place, theme)
            .clicked()
        {
            pressed.push(*command);
        }
    }
    pressed
}

fn title_row(ui: &Ui, rect: Rect, name: &str, code: &str, focused: bool, theme: &Theme) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SQUARE, theme.panel);
    let color = if focused {
        theme.text
    } else {
        theme.text_muted()
    };
    painter.text(
        pos2(rect.left() + theme.space(Space::S4), rect.center().y),
        Align2::LEFT_CENTER,
        name,
        font(theme, TextRole::Caption),
        color,
    );
    painter.text(
        pos2(rect.right() - theme.space(Space::S4), rect.center().y),
        Align2::RIGHT_CENTER,
        code,
        font(theme, TextRole::Mono),
        color,
    );
    if focused {
        painter.hline(
            rect.x_range(),
            rect.bottom(),
            egui::Stroke::new(Theme::STRONG_STROKE, theme.focus()),
        );
    }
}

/// The source monitor: the pool item picked, its marks, insert and
/// overwrite.
pub(crate) fn source_monitor(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let parts = parts(rect, theme);
    let rate = studio
        .current(cx)
        .map_or(sue_core::Rate::FPS_30, |sequence| sequence.rate);
    let item = studio.source.item;
    let name = item
        .and_then(|id| cx.arena().get_by_id(id).ok())
        .map_or_else(|| tr!("monitor-source"), |entity| entity.name().to_owned());
    title_row(
        ui,
        parts.title,
        &name,
        &timecode(studio.source.clock.position, rate),
        studio.focus == Monitor::Source,
        theme,
    );
    let pressed = ui.interact(
        parts.picture,
        egui::Id::unique("sve-source-picture"),
        Sense::click(),
    );
    if pressed.clicked() {
        studio.focus = Monitor::Source;
    }
    let Some(item) = item else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(parts.picture));
        sue_wm::ui::empty_state("sve-source-empty", &tr!("monitor-source-empty"))
            .icon(Icon::Film)
            .show(&mut child, theme);
        return;
    };
    let Ok(entity) = cx.arena().get_by_id(item) else {
        return;
    };
    let kind = SourceKind::of(entity);
    let length = model::length_of(cx.arena(), item).unwrap_or(Time::from_seconds(5.0));
    let size = model::size_of(cx.arena(), item).unwrap_or_else(|| studio.frame_size(cx));
    let frame_rect = fitted(parts.picture.shrink(theme.space(Space::S2)), size);
    if kind == SourceKind::Sound {
        ui.painter().rect_filled(parts.picture, SQUARE, theme.void);
        if let Some(sound) = studio.hub.sounds.get(item) {
            super::timeline::waveform(
                ui,
                parts.picture.shrink(theme.space(Space::S6)),
                &sound,
                Time::ZERO,
                length,
                1.0,
                theme,
            );
        }
    } else {
        let layer = Layer::new(item, kind, studio.source.clock.position);
        let need = if studio.source.clock.playing {
            Need::Ahead(8)
        } else {
            Need::Exact
        };
        let size_for_layer = render::natural_size(&layer.kind, size, size);
        draw_layers(studio, ui, frame_rect, size_for_layer, &[layer], need, cx);
    }
    let marks = (studio.source.mark_in, studio.source.mark_out);
    if let Some((at, dragging)) = jog(
        ui,
        parts.jog,
        "source",
        studio.source.clock.position,
        length,
        marks,
        theme,
    ) {
        let now = cx.view.now;
        studio.source.clock.seek(at, now, None);
        studio.focus = Monitor::Source;
        let _ = dragging;
    }
    let buttons = [
        (Icon::MarkIn, "marker.in", false),
        (Icon::MarkOut, "marker.out", false),
        (Icon::FramePrev, "playback.prev_frame", false),
        (
            if studio.source.clock.playing {
                Icon::Pause
            } else {
                Icon::Play
            },
            "playback.play",
            false,
        ),
        (Icon::FrameNext, "playback.next_frame", false),
        (Icon::Insert, "clip.insert", false),
        (Icon::Overwrite, "clip.overwrite", false),
    ];
    let pressed = transport(studio, ui, parts.transport, "sve-source", &buttons, theme);
    for command in pressed {
        if command.starts_with("marker") || command.starts_with("playback") {
            studio.focus = Monitor::Source;
        }
        studio.run(command, cx);
    }
}

/// The program monitor: the sequence at the playhead.
pub(crate) fn program_monitor(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let parts = parts(rect, theme);
    let Some(sequence) = studio.current(cx).cloned() else {
        studio.program_rect = None;
        return;
    };
    let name = studio
        .sequence
        .and_then(|id| cx.arena().get_by_id(id).ok())
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    title_row(
        ui,
        parts.title,
        &name,
        &timecode(studio.clock.position, sequence.rate),
        studio.focus == Monitor::Program,
        theme,
    );
    let pressed = ui.interact(
        parts.picture,
        egui::Id::unique("sve-program-picture"),
        Sense::click(),
    );
    if pressed.clicked() {
        studio.focus = Monitor::Program;
    }
    program_picture(studio, ui, parts.picture, &sequence, cx, theme);
    let length = sequence.duration().max(Time::SECOND);
    let marks = model::marks(&sequence);
    program_chrome(studio, ui, &parts, length, marks, cx, theme);
}

/// The program monitor's picture alone, fitted in `area`: the frame at
/// the playhead, drawn on the graphics card or by the processor, and the
/// dropped frames note while playing (the phone layout shows only this).
pub(crate) fn program_picture(
    studio: &mut SueVe,
    ui: &mut Ui,
    area: Rect,
    sequence: &sue_model::Sequence,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let sequence = sequence.clone();
    let size = model::frame_size(&sequence);
    let frame_rect = fitted(area.shrink(theme.space(Space::S2)), size);
    studio.program_rect = Some(frame_rect);
    let time = studio.clock.position;
    let arena = cx.arena();
    let kind = |id: EntityId| {
        arena
            .get_by_id(id)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let layers = render::layers_at(&sequence, time, &kind);
    let scrubbing = matches!(studio.drag, Some(super::timeline::Drag::Scrub));
    let need = if studio.clock.playing {
        Need::Ahead(8)
    } else if scrubbing {
        Need::Fast
    } else {
        Need::Exact
    };
    let processed = studio.page == Page::Color || compose::needs_processing(&sequence, time, &kind);
    let due = if processed {
        let split = studio.page == Page::Color && studio.color.split;
        processed_frame(studio, ui, frame_rect, size, &sequence, time, split, cx)
    } else {
        studio.shown = None;
        draw_layers(studio, ui, frame_rect, size, &layers, need, cx)
    };
    if studio.clock.playing {
        studio.clock.count(due);
    }
    // Over the picture while playing: frames shown and dropped.
    if studio.clock.playing && studio.clock.dropped > 0 {
        let note = tr!(
            "monitor-dropped",
            dropped = studio.clock.dropped as i64,
            shown = studio.clock.shown as i64
        );
        ui.painter().text(
            frame_rect.left_bottom() + vec2(theme.space(Space::S2), -theme.space(Space::S2)),
            Align2::LEFT_BOTTOM,
            note,
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
    }
}

/// The program monitor's jog bar and transport row.
fn program_chrome(
    studio: &mut SueVe,
    ui: &mut Ui,
    parts: &Parts,
    length: Time,
    marks: (Option<Time>, Option<Time>),
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    if let Some((at, dragging)) = jog(
        ui,
        parts.jog,
        "program",
        studio.clock.position,
        length,
        marks,
        theme,
    ) {
        let now = cx.view.now;
        studio.clock.seek(at, now, cx.sink());
        studio.focus = Monitor::Program;
        studio.drag = dragging.then_some(super::timeline::Drag::Scrub);
    } else if matches!(studio.drag, Some(super::timeline::Drag::Scrub))
        && !ui.input(|input| input.pointer.any_down())
    {
        studio.drag = None;
    }
    let buttons = [
        (Icon::MarkIn, "marker.in", false),
        (Icon::MarkOut, "marker.out", false),
        (Icon::FrameFirst, "playback.start", false),
        (Icon::FramePrev, "playback.prev_frame", false),
        (
            if studio.clock.playing {
                Icon::Pause
            } else {
                Icon::Play
            },
            "playback.play",
            false,
        ),
        (Icon::FrameNext, "playback.next_frame", false),
        (Icon::FrameLast, "playback.end", false),
        (Icon::Loop, "playback.loop", studio.clock.looping),
    ];
    let pressed = transport(studio, ui, parts.transport, "sve-program", &buttons, theme);
    for command in pressed {
        studio.focus = Monitor::Program;
        studio.run(command, cx);
    }
}
