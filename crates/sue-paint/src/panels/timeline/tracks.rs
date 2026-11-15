//! The Timeline palette's buttons and tracks.

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_core::EntityId;
use sue_wm::{AppletContext, Theme};

use super::view::timeline_scrollbars;
use crate::anim;
use crate::composite;
use crate::doc;
use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::SuePaint;

/// A timeline button: its icon, hint, command, whether it can be used and
/// whether it is on.
type Button<'a> = (Icon, String, &'a str, bool, bool);

impl SuePaint {
    /// The Timeline palette: its buttons, frame rate and length, and the
    /// folders' keys frame by frame.
    #[allow(clippy::too_many_lines, reason = "the whole palette")]
    /// The picture of `id` in `rect` (kept to its aspect), from the same
    /// thumbnails the layer palette keeps.
    fn cel_thumb(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Ok(entity) = cx.world.arena.get_by_id(id) else {
            return;
        };
        let revision = entity.revision();
        if self
            .thumbs
            .get(&id)
            .is_none_or(|(had, _, _)| *had != revision)
            && let Some((pixels, size)) = composite::thumbnail(&cx.world.arena, id, [96, 72])
        {
            self.thumbs.insert(id, (revision, pixels, size));
        }
        let Some((revision, pixels, size)) = self.thumbs.get(&id) else {
            return;
        };
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let fit = if aspect > rect.width() / rect.height() {
            vec2(rect.width(), rect.width() / aspect)
        } else {
            vec2(rect.height() * aspect, rect.height())
        };
        let image = Rect::from_min_size(rect.min, fit);
        painter.rect_filled(image, 0.0, Color32::WHITE);
        if let Some(texture) = cx.textures.texture(
            crate::studio::texture_key(id, 0x7448_554d_4253),
            *revision,
            *size,
            4,
            pixels,
            false,
        ) {
            painter.image(
                texture,
                image,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        painter.rect_stroke(image, 0.0, theme.hairline(), egui::StrokeKind::Outside);
    }

    /// Clip Studio Paint's Timeline palette: its buttons, then a column
    /// of tracks (each with its eye, name and thumbnail) beside the frames,
    /// where every second is a band, every key shows its cel's picture and
    /// the current frame is a red bar.
    #[allow(clippy::too_many_lines, reason = "the whole Timeline palette")]
    pub(crate) fn timeline_ui(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        tracks: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let here = self.timeline_here(&cx.world.arena);
        let has = here.is_some();
        let playing = self.doc().is_some_and(|doc| doc.playing.is_some());
        // Clip Studio Paint's order: the timeline, zoom, playback, cels,
        // onion skin. A group starts after each `None`.
        let buttons: Vec<Option<Button<'_>>> = vec![
            Some((
                Icon::New,
                tr!("new-timeline-2"),
                "anim.timeline.new",
                !has,
                false,
            )),
            Some((
                Icon::Timeline,
                tr!("edit-timeline"),
                "anim.timeline.edit",
                has,
                false,
            )),
            Some((
                Icon::NewFolder,
                tr!("new-animation-folder"),
                "anim.layer.folder",
                has,
                false,
            )),
            None,
            Some((
                Icon::ZoomOut,
                tr!("zoom-out"),
                "timeline.zoom_out",
                has,
                false,
            )),
            Some((Icon::ZoomIn, tr!("zoom-in"), "timeline.zoom_in", has, false)),
            None,
            Some((Icon::First, tr!("first-frame"), "anim.first", has, false)),
            Some((Icon::Prev, tr!("previous-frame"), "anim.prev", has, false)),
            // One button, as in Clip Studio Paint: Play, and Stop while it plays.
            Some(if playing {
                (Icon::Stop, tr!("stop"), "anim.play", has, true)
            } else {
                (Icon::Play, tr!("play"), "anim.play", has, false)
            }),
            Some((Icon::Next, tr!("next-frame"), "anim.next", has, false)),
            Some((Icon::Last, tr!("last-frame"), "anim.last", has, false)),
            Some((Icon::Loop, tr!("loop"), "anim.loop", has, self.looping)),
            None,
            Some((
                Icon::Cel,
                tr!("new-animation-cel-2"),
                "anim.cel.new",
                true,
                false,
            )),
            Some((
                Icon::Edit,
                tr!("specify-cel-show-current-layer-frame"),
                "anim.cel.assign",
                has,
                false,
            )),
            Some((
                Icon::Minus,
                tr!("delete-key-frame"),
                "anim.key.delete",
                has,
                false,
            )),
            Some((
                Icon::Add,
                tr!("insert-a-frame-here"),
                "anim.frame.insert",
                has,
                false,
            )),
            Some((
                Icon::Delete,
                tr!("delete-this-frame"),
                "anim.frame.delete",
                has,
                false,
            )),
            None,
            Some((
                Icon::Onion,
                tr!("onion-skin"),
                "anim.onion",
                has,
                self.onion,
            )),
        ];
        let mut run = None;
        let mut x = bar.left() + 8.0;
        for button in &buttons {
            let Some((icon, tip, command, enabled, active)) = button else {
                ui.painter().vline(
                    x + 1.0,
                    bar.shrink2(vec2(0.0, 6.0)).y_range(),
                    theme.hairline(),
                );
                x += 8.0;
                continue;
            };
            let at = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(22.0, 22.0));
            if at.right() > bar.right() - 150.0 {
                break;
            }
            if icon_button(ui, at, *icon, *enabled, *active, tip, theme) {
                run = Some(*command);
            }
            x += 25.0;
        }
        // At the right, as Clip Studio Paint: the current frame, then the
        // first and last frames played.
        if let Some((_, _, length)) = &here {
            let frame = self.doc().map_or(0, |doc| doc.frame);
            ui.painter().text(
                pos2(bar.right() - 12.0, bar.center().y),
                Align2::RIGHT_CENTER,
                format!("{}   /   1   /   {length}", frame + 1),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
        match run {
            Some(zoom @ ("timeline.zoom_out" | "timeline.zoom_in")) => {
                let view = &mut self.timeline_view;
                let step = view.step.unwrap_or(15.0);
                let factor = if zoom == "timeline.zoom_in" {
                    1.25
                } else {
                    1.0 / 1.25
                };
                view.zoom_at(step, 0.0, factor);
            }
            Some(command) => {
                self.anim_command(command, cx);
            }
            None => {}
        }

        // The track column, then the frames.
        let names = Rect::from_min_max(tracks.min, pos2(tracks.left() + 190.0, tracks.bottom()));
        ui.painter()
            .vline(names.right(), tracks.y_range(), theme.hairline());
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            // As Clip Studio Paint: an empty timeline list and nothing else.
            let title =
                Rect::from_min_size(names.min + vec2(6.0, 6.0), vec2(names.width() - 12.0, 22.0));
            ui.painter().rect_filled(title, 2.0, theme.pill);
            ui.painter()
                .rect_stroke(title, 2.0, theme.hairline(), egui::StrokeKind::Inside);
            crate::icons::paint(
                ui.painter(),
                Rect::from_center_size(title.right_center() - vec2(10.0, 0.0), vec2(9.0, 9.0)),
                Icon::Down,
                theme.muted,
            );
            ui.interact(title, ui.scope_id().with("timeline-list"), Sense::hover())
                .on_hover_text(tr!("new-timeline-2"));
            return;
        };
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let fps = anim::fps(&sequence).round().max(1.0) as u32;
        let row_height = 74.0;
        let ruler = 34.0;
        let frames_area = Rect::from_min_max(pos2(names.right(), tracks.top()), tracks.max);
        let mut step = self.timeline_view.step(frames_area.width(), length);
        // As Clip Studio Paint, over the frames: the wheel zooms around the
        // pointer, Shift and the wheel pan, Alt and the wheel scroll the
        // tracks, Space and a drag or the middle button pan.
        let content = ruler
            + anim::folders(&sequence).len() as f32 * row_height
            + anim::audio_tracks(&sequence).len() as f32 * 26.0;
        let (hover, wheel, shift, alt, space, middle, primary, motion) = ui.input(|input| {
            (
                input.pointer.hover_pos(),
                input.smooth_scroll_delta,
                input.modifiers.shift,
                input.modifiers.alt,
                input.key_down(egui::Key::Space),
                input.pointer.middle_down(),
                input.pointer.primary_down(),
                input.pointer.delta(),
            )
        });
        let over = hover.filter(|_| ui.rect_contains_pointer(frames_area));
        let panning = over.is_some() && (middle || (space && primary));
        if let Some(pos) = over {
            if panning {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                self.timeline_view.pan(motion.x, step);
                self.timeline_view.scroll(motion.y);
            } else if space {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if wheel != egui::Vec2::ZERO {
                step =
                    self.timeline_view
                        .wheel(step, pos.x - frames_area.left(), wheel, shift, alt);
                // The timeline took the wheel: nothing under it scrolls.
                ui.input_mut(|input| input.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }
        let shown = self.timeline_view.settle(
            frame,
            length,
            frames_area.width(),
            step,
            content,
            tracks.height(),
        );
        let origin = self.timeline_view.first;
        let down = self.timeline_view.down;
        let first = origin.floor() as u32;
        let column = |at: u32| frames_area.left() + (at as f32 - origin) * step;
        let painter = ui.painter().with_clip_rect(frames_area);
        let below = Rect::from_min_max(
            pos2(frames_area.left(), frames_area.top() + ruler),
            frames_area.max,
        );
        // The timeline's name above the tracks, as Clip Studio Paint's
        // timeline list.
        let title = Rect::from_min_size(
            names.min + vec2(6.0, 6.0),
            vec2(names.width() - 12.0, ruler - 12.0),
        );
        ui.painter().rect_filled(title, 2.0, theme.pill);
        ui.painter()
            .rect_stroke(title, 2.0, theme.hairline(), egui::StrokeKind::Inside);
        let timeline_name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("timeline"), |entity| entity.name().to_owned());
        ui.painter().text(
            title.center(),
            Align2::CENTER_CENTER,
            timeline_name,
            theme.font(theme.small_size + 0.5),
            theme.text,
        );
        // Every other second a lighter band, its number above it.
        let last = (first + shown + 1).min(length);
        for second in (first / fps)..=(last / fps) {
            let (from, to) = (second * fps, ((second + 1) * fps).min(length));
            let band = Rect::from_min_max(
                pos2(column(from), frames_area.top() + 14.0),
                pos2(column(to), frames_area.bottom()),
            );
            if second % 2 == 1 {
                painter.rect_filled(band, 0.0, theme.hover);
            }
            if second > 0 {
                painter.vline(
                    column(from),
                    frames_area.top()..=frames_area.top() + 14.0,
                    Stroke::new(1.0, theme.muted),
                );
                painter.text(
                    pos2(column(from) + 3.0, frames_area.top() + 7.0),
                    Align2::LEFT_CENTER,
                    second.to_string(),
                    theme.font(theme.small_size - 1.0),
                    theme.text,
                );
            }
        }
        painter.hline(
            frames_area.x_range(),
            frames_area.top() + 14.0,
            theme.hairline(),
        );
        // The frames: a line each, numbered every third (or sparser when
        // small), as Clip Studio Paint numbers them.
        let every = ((24.0 / step).ceil() as u32).max(1);
        let every = if every <= 3 { 3 } else { every.div_ceil(3) * 3 };
        for at in first..last {
            let x = column(at);
            painter.vline(
                x,
                below.y_range(),
                Stroke::new(1.0, theme.border.gamma_multiply(0.35)),
            );
            if at % every == 0 {
                painter.vline(
                    x,
                    frames_area.top() + 26.0..=frames_area.top() + ruler,
                    Stroke::new(1.0, theme.muted),
                );
                painter.text(
                    pos2(x + 2.0, frames_area.top() + 22.0),
                    Align2::LEFT_CENTER,
                    (at + 1).to_string(),
                    theme.font(theme.small_size - 1.0),
                    theme.muted,
                );
            }
        }
        painter.hline(frames_area.x_range(), below.top(), theme.hairline());
        // The end of the timeline: what lies past it is darker.
        if (length as f32) < origin + shown as f32 {
            painter.rect_filled(
                Rect::from_min_max(pos2(column(length), frames_area.top()), frames_area.max),
                0.0,
                theme.void.gamma_multiply(0.6),
            );
        }
        let ruler_rect = Rect::from_min_size(frames_area.min, vec2(frames_area.width(), ruler));
        let ruler_response = ui.interact(
            ruler_rect,
            ui.scope_id().with("timeline-ruler"),
            Sense::click_and_drag(),
        );
        if let Some(pos) = ruler_response.interact_pointer_pos()
            && (ruler_response.clicked() || ruler_response.dragged())
            && !panning
        {
            let at = (origin + (pos.x - frames_area.left()) / step).max(0.0) as u32;
            self.go_to_frame(&cx.world.arena, i64::from(at.min(length - 1)));
        }
        // One tall row per folder: its name and the picture it shows now in
        // the column, and each key's cel as a picture in the frames.
        let mut action: Option<(usize, u32, Option<Option<EntityId>>)> = None;
        let mut eye: Option<(usize, bool)> = None;
        // The rows scroll under the ruler, not over it.
        let rows_area = Rect::from_min_max(pos2(names.left(), tracks.top() + ruler), names.max);
        let names_painter = ui.painter().with_clip_rect(rows_area);
        let painter = painter.with_clip_rect(below);
        for (row, index) in anim::folders(&sequence).into_iter().enumerate() {
            let top = tracks.top() + ruler + row as f32 * row_height - down;
            let name_rect =
                Rect::from_min_size(pos2(names.left(), top), vec2(names.width(), row_height));
            let chosen = index == track;
            if chosen {
                names_painter.rect_filled(name_rect, 0.0, theme.active);
                painter.rect_filled(
                    Rect::from_min_size(
                        pos2(frames_area.left(), top),
                        vec2(frames_area.width(), row_height),
                    ),
                    0.0,
                    theme.active.gamma_multiply(0.35),
                );
            }
            names_painter.hline(names.x_range(), name_rect.bottom(), theme.hairline());
            painter.hline(frames_area.x_range(), name_rect.bottom(), theme.hairline());
            let line = pos2(name_rect.left() + 8.0, top + 11.0);
            // The eye shows and hides the animation folder, as the Layer
            // palette's does: it is the same folder.
            let folder = anim::track_folder(&sequence.tracks[index]);
            let visible = folder
                .and_then(|folder| cx.world.arena.get_by_id(folder).ok())
                .map_or(!sequence.tracks[index].muted, |entity| {
                    sue_model::picture::LayerStyle::of(entity).visible
                });
            let eye_rect = Rect::from_center_size(line + vec2(6.0, 0.0), vec2(18.0, 18.0));
            crate::icons::paint(
                &names_painter,
                Rect::from_center_size(line + vec2(6.0, 0.0), vec2(14.0, 14.0)),
                Icon::Eye,
                if visible {
                    theme.text
                } else {
                    theme.muted.gamma_multiply(0.35)
                },
            );
            let eye_response = ui
                .interact(
                    eye_rect.intersect(rows_area),
                    ui.scope_id().with(("timeline-eye", index)),
                    Sense::click(),
                )
                .on_hover_text(tr!("show-or-hide-the-animation-folder"));
            if eye_response.clicked() {
                eye = Some((index, !visible));
            }
            crate::icons::paint(
                &names_painter,
                Rect::from_center_size(line + vec2(26.0, 0.0), vec2(14.0, 14.0)),
                Icon::NewFolder,
                theme.muted,
            );
            names_painter.text(
                line + vec2(40.0, 0.0),
                Align2::LEFT_CENTER,
                format!(
                    "{} :",
                    anim::track_name(&cx.world.arena, &sequence.tracks[index])
                ),
                theme.font(theme.small_size + 0.5),
                if chosen { theme.text } else { theme.muted },
            );
            let keys = anim::keys(&sequence.tracks[index], sequence.rate);
            let thumb =
                |x: f32| Rect::from_min_size(pos2(x, top + 24.0), vec2(64.0, row_height - 30.0));
            if let Some(cel) = anim::cel_at(&sequence.tracks[index], sequence.rate, frame) {
                self.cel_thumb(
                    &names_painter,
                    thumb(name_rect.left() + 22.0),
                    cel,
                    cx,
                    theme,
                );
            }
            for (key, (at, cel)) in keys.iter().enumerate() {
                if *at >= last || *at >= length {
                    continue;
                }
                let end = keys
                    .get(key + 1)
                    .map_or(length, |(next, _)| *next)
                    .min(last);
                let x = column(*at);
                // The cel's name over its picture, and a line for as long
                // as it shows.
                let name = cel.map_or_else(
                    || "x".to_owned(),
                    |cel| {
                        cx.world
                            .arena
                            .get_by_id(cel)
                            .map_or_else(|_| "?".to_owned(), |entity| entity.name().to_owned())
                    },
                );
                painter.text(
                    pos2(x + 3.0, top + 11.0),
                    Align2::LEFT_CENTER,
                    name,
                    theme.font(theme.small_size),
                    theme.text,
                );
                if let Some(cel) = cel {
                    let width = ((end - at) as f32 * step - 2.0).clamp(8.0, 64.0);
                    let rect = Rect::from_min_size(
                        pos2(x + 1.0, top + 24.0),
                        vec2(width, row_height - 30.0),
                    );
                    self.cel_thumb(&painter, rect, *cel, cx, theme);
                    painter.hline(
                        (x + 1.0)..=column(end) - 1.0,
                        top + 20.0,
                        Stroke::new(1.0, theme.muted),
                    );
                }
            }
            // Cells: click to go there, double-click to show the current
            // layer from there, right-click for more.
            let cells = Rect::from_min_size(
                pos2(frames_area.left(), top),
                vec2(frames_area.width(), row_height),
            )
            .intersect(below);
            let response = ui.interact(
                cells,
                ui.scope_id().with(("timeline-cells", index)),
                Sense::click(),
            );
            let name_click = ui.interact(
                Rect::from_min_max(
                    pos2(name_rect.left() + 18.0, name_rect.top()),
                    name_rect.max,
                )
                .intersect(rows_area),
                ui.scope_id().with(("timeline-folder", index)),
                Sense::click(),
            );
            if name_click.clicked() {
                let shown = anim::cel_at(&sequence.tracks[index], sequence.rate, frame);
                let folder = anim::track_folder(&sequence.tracks[index]);
                if let Some(doc) = self.doc_mut() {
                    doc.track = index;
                    if let Some(layer) = shown.or(folder) {
                        doc.layer = Some(layer);
                    }
                }
            }
            let at_pointer = |pos: egui::Pos2| {
                ((origin + (pos.x - frames_area.left()) / step).max(0.0) as u32).min(length - 1)
            };
            if let Some(pos) = response.interact_pointer_pos() {
                if response.double_clicked() {
                    let layer = self.doc().and_then(|doc| doc.layer);
                    action = Some((index, at_pointer(pos), Some(layer)));
                } else if response.clicked() {
                    action = Some((index, at_pointer(pos), None));
                }
            }
            let pointer = response.hover_pos();
            response.context_menu(|ui| {
                let at = pointer.map_or(frame, at_pointer);
                if ui.button(tr!("show-the-current-layer-here")).clicked() {
                    action = Some((index, at, Some(self.doc().and_then(|doc| doc.layer))));
                    ui.close();
                }
                if ui.button(tr!("blank-from-here")).clicked() {
                    action = Some((index, at, Some(None)));
                    ui.close();
                }
                if let Some(folder) = anim::track_folder(&sequence.tracks[index]) {
                    let cels = anim::cels(&cx.world.arena, picture, folder);
                    ui.add_enabled_ui(!cels.is_empty(), |ui| {
                        ui.menu_button(tr!("specify-cels"), |ui| {
                            for cel in cels.iter().rev() {
                                let name = cx
                                    .world
                                    .arena
                                    .get_by_id(*cel)
                                    .map_or_else(|_| "?".to_owned(), |e| e.name().to_owned());
                                if ui.button(name).clicked() {
                                    action = Some((index, at, Some(Some(*cel))));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            });
        }
        // The current frame: a red bar down the frames, as Clip Studio
        // Paint marks it.
        let painter = painter.with_clip_rect(frames_area);
        let red = Color32::from_rgb(196, 64, 64);
        let current = Rect::from_min_size(
            pos2(column(frame), frames_area.top() + 14.0),
            vec2(step.min(14.0), frames_area.height() - 14.0),
        );
        painter.rect_filled(current, 0.0, red.gamma_multiply(0.75));
        painter.text(
            current.center_top() + vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            (frame + 1).to_string(),
            theme.font(theme.small_size - 1.0),
            Color32::WHITE,
        );
        let painter = painter.with_clip_rect(below);
        let row_height = 26.0;
        let ruler = ruler + anim::folders(&sequence).len() as f32 * 74.0
            - anim::folders(&sequence).len() as f32 * row_height;
        // One row per audio track: its clip as a bar to drag, and a menu.
        let rows = anim::folders(&sequence).len();
        let mut audio_edit: Option<(sue_model::Sequence, String)> = None;
        for (row, index) in anim::audio_tracks(&sequence).into_iter().enumerate() {
            let top = tracks.top() + ruler + (rows + row) as f32 * row_height - down;
            let track_ref = &sequence.tracks[index];
            let name_rect =
                Rect::from_min_size(pos2(names.left(), top), vec2(names.width(), row_height));
            names_painter.text(
                name_rect.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                format!(
                    "{}{}",
                    track_ref.name,
                    if track_ref.muted {
                        tr!("muted")
                    } else {
                        String::new()
                    }
                ),
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            let Some(clip) = track_ref.clips().first() else {
                continue;
            };
            let begin = clip.start.frame(sequence.rate);
            let span = ((clip.duration.seconds() * anim::fps(&sequence)).ceil() as i64).max(1);
            let dragged_by = match self.audio_drag {
                Some((dragging, from, _)) if dragging == index => {
                    ui.ctx().pointer_latest_pos().map_or(0, |pos| {
                        (origin + (pos.x - frames_area.left()) / step).floor() as i64 - from
                    })
                }
                _ => 0,
            };
            let shown_at = (begin + dragged_by).max(0);
            let x0 = frames_area.left() + (shown_at as f32 - origin) * step;
            let bar = Rect::from_min_max(
                pos2(x0, top + 4.0),
                pos2(x0 + span as f32 * step, top + row_height - 4.0),
            );
            painter.rect_filled(
                bar,
                3.0,
                theme
                    .active
                    .gamma_multiply(if track_ref.muted { 0.4 } else { 1.0 }),
            );
            painter.text(
                bar.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                "audio",
                theme.font(theme.small_size - 1.0),
                theme.text,
            );
            let row_rect = Rect::from_min_size(
                pos2(frames_area.left(), top),
                vec2(frames_area.width(), row_height),
            )
            .intersect(below);
            let response = ui.interact(
                row_rect,
                ui.scope_id().with(("timeline-audio", index)),
                Sense::click_and_drag(),
            );
            let at_pointer =
                |pos: egui::Pos2| (origin + (pos.x - frames_area.left()) / step).floor() as i64;
            if response.drag_started()
                && !panning
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.audio_drag = Some((index, at_pointer(pos), begin));
            }
            if response.drag_stopped()
                && let Some((dragging, from, was)) = self.audio_drag.take()
                && dragging == index
            {
                let to = response
                    .interact_pointer_pos()
                    .map_or(was, |pos| was + at_pointer(pos) - from);
                if to != was {
                    audio_edit = Some((anim::audio_moved(&sequence, index, to), tr!("move-audio")));
                }
            }
            response.context_menu(|ui| {
                let label = if track_ref.muted {
                    tr!("unmute")
                } else {
                    tr!("mute")
                };
                if ui.button(label).clicked() {
                    let mut toggled = sequence.clone();
                    toggled.tracks[index].muted = !track_ref.muted;
                    audio_edit = Some((
                        toggled,
                        if track_ref.muted {
                            tr!("unmute-audio")
                        } else {
                            tr!("mute-audio")
                        },
                    ));
                    ui.close();
                }
                if ui.button(tr!("delete-audio-track")).clicked() {
                    audio_edit = Some((
                        anim::without_track(&sequence, index),
                        tr!("delete-audio-track"),
                    ));
                    ui.close();
                }
            });
        }
        timeline_scrollbars(
            ui,
            &mut self.timeline_view,
            frames_area.with_min_y(frames_area.top() + 34.0),
            (length, shown),
            (content - 34.0, tracks.height() - 34.0),
            theme,
        );
        if let Some((changed, label)) = audio_edit {
            Self::apply(cx, anim::set(picture, changed, &label));
        }
        if let Some((index, visible)) = eye {
            match anim::track_folder(&sequence.tracks[index]) {
                Some(folder) => {
                    Self::apply(cx, doc::set(folder, sue_model::picture::VISIBLE, visible));
                }
                // An older project's track, with no folder: it mutes.
                None => {
                    let mut toggled = sequence.clone();
                    toggled.tracks[index].muted = !visible;
                    Self::apply(
                        cx,
                        anim::set(picture, toggled, &tr!("show-or-hide-the-animation-folder")),
                    );
                }
            }
        }
        if let Some((index, at, assign)) = action {
            if let Some(doc) = self.doc_mut() {
                doc.track = index;
            }
            self.go_to_frame(&cx.world.arena, i64::from(at));
            // A blank there: the folder itself is picked, as in the Layer
            // palette, so the two stay in step.
            if assign.is_none()
                && anim::cel_at(&sequence.tracks[index], sequence.rate, at).is_none()
                && let Some(folder) = anim::track_folder(&sequence.tracks[index])
                && let Some(doc) = self.doc_mut()
            {
                doc.layer = Some(folder);
            }
            if let Some(cel) = assign {
                if cel.is_some_and(|cel| !anim::may_show(&cx.world.arena, &sequence, index, cel)) {
                    cx.notify(tr!("only-cels-of-this-animation-folder"));
                } else {
                    let sequence = anim::assign(&sequence, index, at, cel, picture, length);
                    Self::apply(cx, anim::set(picture, sequence, "Specify Cel"));
                    self.go_to_frame(&cx.world.arena, i64::from(at));
                }
            }
        }
    }
}
