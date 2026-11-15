//! The Timeline palette's tracks: a column of names beside the frames,
//! under a ruler. Every track of the timeline has a row: the 2D camera's
//! keys, each animation folder's cels, each layer's clip and keys, and
//! each sound's clip with its waveform. Frames are picked by a click, a
//! drag or Shift and a click; what is picked moves by a drag; a right
//! click opens the commands for that frame.

use egui::{
    Align2, CornerRadius, CursorIcon, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2,
};
use sue_core::EntityId;
use sue_model::{Sequence, Track};
use sue_wm::MenuItem;
use sue_wm::ui::{self as wm_ui, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::bar::icon;
use super::view::timeline_scrollbars;
use super::{Action, Press, Thumbs};
use crate::anim;
use crate::anim::editing::{Selection, TrackLabel};
use crate::anim::motion::Ease;
use crate::composite;
use crate::studio::SuePaint;

/// What a row of the timeline shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Camera,
    Cels,
    Layer,
    Audio,
}

/// A row: the track it shows, its kind, where it starts (from the top of
/// the rows, before scrolling) and how tall it is.
#[derive(Clone, Copy, Debug)]
struct Row {
    track: usize,
    kind: Kind,
    top: f32,
    height: f32,
}

/// The kind of `track`.
fn kind_of(track: &Track) -> Kind {
    if track.kind == anim::AUDIO {
        Kind::Audio
    } else if anim::setup::is_camera_track(track) {
        Kind::Camera
    } else if track.kind == anim::CELS {
        Kind::Cels
    } else {
        Kind::Layer
    }
}

/// The rows of `sequence` laid out with rows `row` tall and cel pictures
/// `thumbs` rows tall: the camera first, then the layers and folders in
/// the Layer palette's order (`stack` gives a track's place there, the
/// top layer highest; tracks with none keep their order after them), the
/// sounds last.
fn rows(
    sequence: &Sequence,
    row: f32,
    thumbs: Thumbs,
    stack: impl Fn(&Track) -> Option<usize>,
) -> Vec<Row> {
    let mut out = Vec::new();
    let mut top = 0.0;
    let mut pictured: Vec<usize> = (0..sequence.tracks.len())
        .filter(|index| kind_of(&sequence.tracks[*index]) != Kind::Audio)
        .collect();
    pictured.sort_by_key(|index| {
        let track = &sequence.tracks[*index];
        (
            kind_of(track) != Kind::Camera,
            stack(track).map_or(usize::MAX, |place| usize::MAX - 1 - place),
        )
    });
    let order = pictured.into_iter().chain(anim::audio_tracks(sequence));
    for track in order {
        let kind = kind_of(&sequence.tracks[track]);
        let height = match kind {
            Kind::Cels => row * (1.0 + thumbs.rows()),
            Kind::Audio => row * 1.5,
            Kind::Camera | Kind::Layer => row,
        };
        out.push(Row {
            track,
            kind,
            top,
            height,
        });
        top += height;
    }
    out
}

/// A layer's palette color as the timeline draws it (the person's own
/// color, opaque).
fn palette_ink(color: sue_core::Color) -> egui::Color32 {
    let [r, g, b] = color
        .to_space(sue_core::ColorSpace::Srgb)
        .rgb
        .map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8);
    egui::Color32::from_rgb(r, g, b)
}

/// The entity whose keyframes a row shows: the camera folder, a layer's
/// own, or the animation folder's.
fn keyed(track: &Track) -> Option<EntityId> {
    anim::track_folder(track).or_else(|| anim::setup::track_layer(track))
}

/// A key's mark at `at`: Smooth a diamond, Linear a diamond outline, Hold
/// a square, so the three read apart without color.
fn key_mark(painter: &egui::Painter, at: Pos2, size: f32, ease: Ease, ink: egui::Color32) {
    let half = size / 2.0;
    let (across, down) = (Vec2::X * half, Vec2::Y * half);
    let diamond = vec![at - down, at + across, at + down, at - across];
    let width = size / 6.0;
    let pen = Stroke::new(width, ink);
    match ease {
        Ease::Smooth => {
            painter.add(Shape::convex_polygon(diamond, ink, pen));
        }
        Ease::Linear => {
            painter.add(Shape::closed_line(diamond, pen));
        }
        Ease::Hold => {
            let side = size * 0.75;
            let square = Rect::from_center_size(at, egui::Vec2::splat(side));
            painter.rect_filled(square, CornerRadius::ZERO, ink);
        }
    }
}

impl SuePaint {
    /// The picture of `id` in `rect` (kept to its aspect, over the
    /// checkerboard of clear), from the same thumbnails the layer palette
    /// keeps.
    pub(crate) fn cel_thumb(
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
        let pixels_wanted = [
            (rect.width() * 2.0).ceil().max(1.0) as u32,
            (rect.height() * 2.0).ceil().max(1.0) as u32,
        ];
        if self
            .thumbs
            .get(&id)
            .is_none_or(|(had, _, _)| *had != revision)
            && let Some((pixels, size)) =
                composite::thumbnail(&cx.world.arena, id, pixels_wanted.map(|side| side.max(96)))
        {
            self.thumbs.insert(id, (revision, pixels, size));
        }
        let Some((revision, pixels, size)) = self.thumbs.get(&id) else {
            return;
        };
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let fit = if aspect > rect.width() / rect.height() {
            egui::Vec2::new(rect.width(), rect.width() / aspect)
        } else {
            egui::Vec2::new(rect.height() * aspect, rect.height())
        };
        let image = Rect::from_min_size(rect.min, fit);
        let cell = theme.space(Space::S1);
        sue_wm::checker_fill(painter, image, 0.0, cell);
        if let Some(texture) = cx.textures.texture(
            crate::studio::texture_key(id, 0x7448_554d_4253),
            *revision,
            *size,
            4,
            pixels,
            false,
        ) {
            // A picture drawn as it is: no tint.
            let as_it_is: egui::Color32 = egui::Rgba::WHITE.into();
            painter.image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                as_it_is,
            );
        }
        painter.rect_stroke(
            image,
            CornerRadius::ZERO,
            theme.hairline(),
            egui::StrokeKind::Outside,
        );
    }

    /// The waveform of the sound `source` across `rect`, from its peaks
    /// (kept until the sound changes).
    fn waveform(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        source: EntityId,
        cx: &AppletContext<'_>,
        ink: egui::Color32,
    ) {
        let Ok(entity) = cx.world.arena.get_by_id(source) else {
            return;
        };
        let revision = entity.revision();
        if self
            .anim_ui
            .peaks
            .get(&source)
            .is_none_or(|(had, _)| *had != revision)
            && let Some(pcm) = entity
                .component(sue_core::ComponentKind::PCMFloat1D)
                .and_then(|component| component.as_pcm())
        {
            // Enough columns for a wide clip; drawn thinned to the room.
            let peaks = super::audio::peaks(pcm, 2048);
            self.anim_ui.peaks.insert(source, (revision, peaks));
        }
        let Some((_, peaks)) = self.anim_ui.peaks.get(&source) else {
            return;
        };
        let columns = rect.width().max(1.0) as usize;
        let middle = rect.center().y;
        let half = rect.height() / 2.0;
        let width = painter.pixels_per_point().recip().max(1.0);
        let pen = Stroke::new(width, ink);
        for column in 0..columns {
            let peak = peaks[column * peaks.len() / columns.max(1)].min(1.0);
            let x = rect.left() + column as f32 + 0.5;
            let reach = (peak * half).max(half / 16.0);
            painter.line_segment([pos2(x, middle - reach), pos2(x, middle + reach)], pen);
        }
    }

    /// The commands a right click on a frame of `row` offers, Clip Studio
    /// Paint's list: the folder's cels by name, then New Animation Cel,
    /// Delete, Cut, Copy, Paste, the clips, labels, keys and frames.
    fn cell_menu(
        &self,
        cx: &AppletContext<'_>,
        picture: EntityId,
        sequence: &Sequence,
        row: Row,
    ) -> Vec<MenuItem> {
        let item = |command: &str| {
            let label = crate::commands::registry()
                .label(command)
                .unwrap_or_else(|| command.to_owned());
            MenuItem::action(label, command).key(super::command_keys(&cx.world.rice, command))
        };
        let arena = &cx.world.arena;
        let mut items = Vec::new();
        match row.kind {
            Kind::Audio => {
                return vec![
                    item("anim.audio.settings"),
                    item("anim.audio.mute"),
                    MenuItem::Separator,
                    item("anim.audio.delete"),
                ];
            }
            Kind::Cels => {
                if let Some(folder) = anim::track_folder(&sequence.tracks[row.track]) {
                    for cel in anim::cels(arena, picture, folder).into_iter().rev() {
                        if let Ok(entity) = arena.get_by_id(cel) {
                            items.push(MenuItem::action(
                                entity.name().to_owned(),
                                format!("anim.cel.show.{cel}"),
                            ));
                        }
                    }
                }
                items.push(MenuItem::action(tr!("blank-cel"), "anim.cel.show.blank"));
                items.push(MenuItem::Separator);
                items.push(item("anim.cel.new"));
                items.push(item("anim.cel.assign"));
                items.push(MenuItem::Separator);
            }
            Kind::Camera | Kind::Layer => {
                items.push(item("anim.key.layer"));
                items.push(MenuItem::Sub {
                    label: tr!("keyframe-interpolation"),
                    items: Ease::ALL
                        .iter()
                        .map(|ease| item(&format!("anim.key.ease.{}", ease.name())))
                        .collect(),
                });
                items.push(item("anim.key.delete"));
                items.push(MenuItem::Separator);
            }
        }
        items.extend([
            item("anim.edit.delete"),
            item("anim.edit.cut"),
            item("anim.edit.copy"),
            item("anim.edit.paste"),
            MenuItem::Separator,
            item("anim.clip.split"),
            item("anim.clip.merge"),
            item("anim.clip.first"),
            item("anim.clip.last"),
            MenuItem::Separator,
        ]);
        if row.kind == Kind::Cels {
            items.push(MenuItem::Sub {
                label: tr!("track-label"),
                items: ["inbetween", "inbetween-done", "reverse-sheet", "none"]
                    .iter()
                    .map(|kind| item(&format!("anim.track_label.{kind}")))
                    .collect(),
            });
        }
        items.extend([
            item("anim.label"),
            MenuItem::Separator,
            item("anim.frame.insert"),
            item("anim.frame.delete"),
            MenuItem::Separator,
            item("anim.play_range.start"),
            item("anim.play_range.end"),
        ]);
        items
    }

    /// The tracks of the Timeline palette, in `area` under its bar.
    #[allow(
        clippy::too_many_lines,
        reason = "the whole frames area, drawn in order"
    )]
    pub(crate) fn timeline_ui(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let density = Density::of(ui);
        let row_size = density.row();
        let pad = theme.space(Space::S3);
        let half_pad = theme.space(Space::S2) * 0.75;
        let quarter_row = density.row() / 4.0;
        let line_width = theme.hairline().width;
        let thick = line_width * 2.0;
        let names_width = row_size * 7.0;
        let names = Rect::from_min_max(area.min, pos2(area.left() + names_width, area.bottom()));
        ui.painter()
            .vline(names.right(), area.y_range(), theme.hairline());
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            // No timeline yet: say how to make one, with the button.
            let inner = names.shrink(pad);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
            child.label(
                egui::RichText::new(tr!("this-canvas-has-no-timeline"))
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
            let name = self.button_name(cx, "anim.timeline.new");
            if wm_ui::button("timeline-make", &name)
                .icon(icon("timeline-new"))
                .show(&mut child, theme)
                .clicked()
            {
                self.anim_command("anim.timeline.new", cx);
            }
            return;
        };
        let arena = &cx.world.arena;
        let (frame, current_track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let fps = anim::fps(&sequence).round().max(1.0) as u32;
        let settings = anim::setup::Settings::of(arena, picture);
        let (play_start, play_end) = settings.range(length);
        let thumbs = self.anim_ui.thumbs;
        let stack = sue_model::picture::layers(arena, picture);
        let layout = rows(&sequence, row_size, thumbs, |track| {
            keyed(track).and_then(|layer| stack.iter().position(|id| *id == layer))
        });
        let ruler = row_size * 1.5;
        let ruler_half = ruler / 2.0;
        // The shape of the cels' pictures: the canvas's, kept in reason.
        let aspect = arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)
            .map_or(1.0, |[w, h]| (w as f32 / h.max(1) as f32).clamp(0.5, 2.5));
        let frames_area = Rect::from_min_max(pos2(names.right(), area.top()), area.max);
        let content = ruler + layout.last().map_or(0.0, |row| row.top + row.height);
        let mut step = self.timeline_view.step(frames_area.width(), length);
        // Over the frames, as Clip Studio Paint: the wheel zooms around the
        // pointer, Shift and the wheel pan, Alt and the wheel scroll the
        // tracks, Space and a drag or the middle button pan.
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
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                self.timeline_view.pan(motion.x, step);
                self.timeline_view.scroll(motion.y);
            } else if space {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
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
            area.height(),
        );
        let origin = self.timeline_view.first;
        let down = self.timeline_view.down;
        let first = origin.floor() as u32;
        // The ruler numbers frames past the end too, so the last frame
        // shown is the frames' right edge, not the timeline's.
        let last_seen = first + shown + 1;
        let column = |at: u32| frames_area.left() + (at as f32 - origin) * step;
        let at_pointer = |pos: Pos2| (origin + (pos.x - frames_area.left()) / step).max(0.0) as u32;
        let font = theme.text_font(TextRole::Caption);
        let micro = theme.text_font(TextRole::Micro);
        let painter = ui.painter().with_clip_rect(frames_area);
        let ruler_rect = Rect::from_min_max(
            frames_area.min,
            pos2(frames_area.right(), frames_area.top() + ruler),
        );
        let below = Rect::from_min_max(
            pos2(frames_area.left(), ruler_rect.bottom()),
            frames_area.max,
        );
        let half_ruler = ruler_rect.top() + ruler / 2.0;

        // ----- The timeline picker, at the left of the ruler.
        let picker = Rect::from_min_max(
            names.min + egui::Vec2::splat(half_pad),
            pos2(names.right() - half_pad, names.top() + ruler - half_pad),
        );
        let timelines = anim::setup::timelines(arena, picture);
        let titles: Vec<String> = timelines.iter().map(|entry| entry.name.clone()).collect();
        let mut chosen = timelines
            .iter()
            .position(|entry| entry.aside.is_none())
            .unwrap_or(0);
        let mut actions: Vec<Action> = Vec::new();
        {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(picker)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            Density::Compact.scope(&mut child, |ui| {
                if wm_ui::dropdown("timeline-picker", &mut chosen, &titles)
                    .label(&tr!("timeline-list"))
                    .width(picker.width())
                    .show(ui, theme)
                    .changed()
                    && let Some(aside) = timelines.get(chosen).and_then(|entry| entry.aside)
                {
                    actions.push(Action::Switch(aside));
                }
            });
        }

        // ----- The ruler: seconds, division lines, numbered frames, labels
        // and the playback range.
        let division = settings.division.max(1);
        let every = {
            let wanted = ((row_size / step).ceil() as u32).max(1);
            if wanted <= 1 {
                1
            } else {
                wanted.div_ceil(division) * division
            }
        };
        for second in (first / fps)..=(last_seen / fps) {
            let (from, to) = (second * fps, (second + 1) * fps);
            if second % 2 == 1 {
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(column(from), half_ruler),
                        pos2(column(to), frames_area.bottom()),
                    ),
                    CornerRadius::ZERO,
                    theme.fill_hover(),
                );
            }
            painter.vline(
                column(from),
                ruler_rect.top()..=half_ruler,
                theme.hairline(),
            );
            painter.text(
                pos2(
                    column(from) + pad / 2.0,
                    (ruler_rect.top() + half_ruler) / 2.0,
                ),
                Align2::LEFT_CENTER,
                format!("{second}s"),
                micro.clone(),
                theme.text_muted(),
            );
        }
        for at in first..last_seen {
            let x = column(at);
            let division_line = at % division == 0;
            painter.vline(
                x,
                below.y_range(),
                if division_line {
                    theme.hairline()
                } else {
                    Stroke::new(theme.hairline().width, theme.fill_hover())
                },
            );
            if at % every == 0 || every == 1 {
                // Clear of the playback range's first handle (as wide as
                // `pad`), which sits on the same frame.
                let clear = if at == play_start { pad } else { 0.0 };
                painter.text(
                    pos2(x + pad / 2.0 + clear, half_ruler + ruler / 4.0),
                    Align2::LEFT_CENTER,
                    self.anim_ui.unit.show(at, fps),
                    micro.clone(),
                    if at < length {
                        theme.text
                    } else {
                        theme.text_disabled(theme.panel)
                    },
                );
            }
        }
        painter.hline(frames_area.x_range(), half_ruler, theme.hairline());
        painter.hline(frames_area.x_range(), ruler_rect.bottom(), theme.hairline());
        // What lies past the end is shaded.
        if (length as f32) < origin + shown as f32 {
            painter.rect_filled(
                Rect::from_min_max(pos2(column(length), half_ruler), frames_area.max),
                CornerRadius::ZERO,
                theme.scrim(),
            );
        }
        // The playback range: the frames outside it are shaded on the
        // ruler, and its two ends are handles.
        let marker_width = pad;
        let start_x = column(play_start);
        let end_x = column(play_end + 1);
        for (from, to) in [
            (frames_area.left(), start_x),
            (end_x, column(length.max(play_end + 1))),
        ] {
            if to > from {
                painter.rect_filled(
                    Rect::from_min_max(pos2(from, half_ruler), pos2(to, ruler_rect.bottom())),
                    CornerRadius::ZERO,
                    theme.scrim(),
                );
            }
        }
        let markers = [
            (
                true,
                Rect::from_min_max(
                    pos2(start_x, half_ruler),
                    pos2(start_x + marker_width, ruler_rect.bottom()),
                ),
            ),
            (
                false,
                Rect::from_min_max(
                    pos2(end_x - marker_width, half_ruler),
                    pos2(end_x, ruler_rect.bottom()),
                ),
            ),
        ];
        let mut on_marker = false;
        for (start, rect) in markers {
            let response = ui
                .interact(
                    rect.intersect(ruler_rect),
                    ui.scope_id().with(("timeline-play-marker", start)),
                    Sense::drag(),
                )
                .on_hover_text(if start {
                    tr!("drag-to-set-the-first-frame-played")
                } else {
                    tr!("drag-to-set-the-last-frame-played")
                });
            let lit = response.hovered() || response.dragged();
            on_marker |= lit;
            painter.rect_filled(
                rect,
                CornerRadius::same(theme.radius(wm_ui::Radius::Xs) as u8),
                if lit { theme.text } else { theme.text_muted() },
            );
            if response.drag_started() {
                self.anim_ui.marker = Some(start);
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
            {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                let to = at_pointer(pos);
                let to = if start { to } else { to.saturating_sub(1) };
                self.drag_play_marker(cx, start, to);
            }
            if response.drag_stopped() {
                self.anim_ui.marker = None;
            }
        }
        // Timeline labels: a tag and the name over their frame.
        for label in anim::setup::labels(&cx.world.arena, picture) {
            let x = column(label.frame);
            sue_wm::icons::paint(
                &painter,
                Rect::from_min_size(pos2(x, ruler_rect.top()), egui::Vec2::splat(ruler / 2.0)),
                icon("tag"),
                theme.text,
            );
            painter.text(
                pos2(x + ruler / 2.0, (ruler_rect.top() + half_ruler) / 2.0),
                Align2::LEFT_CENTER,
                label.name,
                micro.clone(),
                theme.text,
            );
        }
        // A click or drag on the ruler goes to that frame; hovering it
        // names the frame under the pointer.
        let ruler_response = ui.interact(
            ruler_rect,
            ui.scope_id().with("timeline-ruler"),
            Sense::click_and_drag(),
        );
        if !on_marker && let Some(pos) = ruler_response.hover_pos() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            let at = at_pointer(pos);
            ruler_response.clone().on_hover_text_at_pointer(tr!(
                "frame-under-the-pointer",
                value = self.anim_ui.unit.show(at, fps)
            ));
        }
        if self.anim_ui.marker.is_none()
            && !panning
            && (ruler_response.clicked() || ruler_response.dragged())
            && let Some(pos) = ruler_response.interact_pointer_pos()
        {
            self.go_to_frame(&cx.world.arena, i64::from(at_pointer(pos).min(length - 1)));
        }

        // ----- The rows.
        let rows_top = area.top() + ruler;
        let rows_area = Rect::from_min_max(pos2(names.left(), rows_top), names.max);
        let names_painter = ui.painter().with_clip_rect(rows_area);
        let cells_painter = painter.with_clip_rect(below);
        let selection = self.anim_ui.selection.clone();
        let press = self.anim_ui.press;
        let hover_frame = hover.filter(|pos| below.contains(*pos)).map(at_pointer);
        let row_at = |y: f32| {
            layout.iter().position(|row| {
                y >= rows_top + row.top - down && y < rows_top + row.top - down + row.height
            })
        };
        let hovered_row = hover.and_then(|pos| row_at(pos.y));
        let mut menu_picked: Option<String> = None;
        let mut audio_drop: Option<(usize, i64)> = None;
        for (index, row) in layout.iter().copied().enumerate() {
            let top = rows_top + row.top - down;
            if top > area.bottom() || top + row.height < rows_top {
                continue;
            }
            let track = &sequence.tracks[row.track];
            let name_rect =
                Rect::from_min_size(pos2(names.left(), top), vec2(names.width(), row.height));
            let cells = Rect::from_min_size(
                pos2(frames_area.left(), top),
                vec2(frames_area.width(), row.height),
            );
            let chosen = row.track == current_track;
            if chosen {
                names_painter.rect_filled(name_rect, CornerRadius::ZERO, theme.fill_selected());
                cells_painter.rect_filled(cells, CornerRadius::ZERO, theme.fill_hover());
            }
            names_painter.hline(names.x_range(), name_rect.bottom(), theme.hairline());
            cells_painter.hline(frames_area.x_range(), name_rect.bottom(), theme.hairline());
            // The frames picked on this row.
            if let Some(selection) = &selection
                && selection.tracks.contains(&row.track)
            {
                cells_painter.rect_filled(
                    Rect::from_min_max(
                        pos2(column(selection.start), top),
                        pos2(column(selection.end), top + row.height),
                    ),
                    CornerRadius::ZERO,
                    theme.fill_selected(),
                );
            }
            // The name column: the eye, the kind, the name and what shows.
            let line = top + row_size / 2.0;
            let target = keyed(track);
            let locked = target
                .and_then(|layer| cx.world.arena.get_by_id(layer).ok())
                .is_some_and(|entity| {
                    entity
                        .property(sue_model::picture::LOCKED)
                        .and_then(sue_core::Value::as_bool)
                        .unwrap_or(false)
                });
            let visible = target
                .and_then(|layer| cx.world.arena.get_by_id(layer).ok())
                .map_or(!track.muted, |entity| {
                    sue_model::picture::LayerStyle::of(entity).visible
                });
            // The layer's palette color, as a strip at the left of its
            // name and the line of what each cel holds.
            let tint = target
                .and_then(|layer| crate::doc::palette_color(&cx.world.arena, layer))
                .map(palette_ink);
            if let Some(tint) = tint {
                names_painter.rect_filled(
                    Rect::from_min_max(
                        pos2(names.left(), top),
                        pos2(names.left() + theme.space(Space::S2), top + row.height),
                    ),
                    CornerRadius::ZERO,
                    tint,
                );
            }
            let icon_side = density.icon();
            let eye_rect = Rect::from_center_size(
                pos2(names.left() + pad + icon_side / 2.0, line),
                egui::Vec2::splat(row_size),
            );
            if row.kind != Kind::Audio {
                let eye = wm_ui::icon_button(
                    ("timeline-eye", row.track),
                    icon(if visible { "eye" } else { "eye-off" }),
                    &if row.kind == Kind::Cels {
                        tr!("show-or-hide-the-animation-folder")
                    } else {
                        tr!("show-or-hide")
                    },
                )
                .quiet()
                .show_at(ui, eye_rect.intersect(rows_area), theme);
                if eye.clicked() {
                    actions.push(Action::Eye(row.track, !visible));
                }
            }
            let kind_icon = match row.kind {
                Kind::Camera => "film",
                Kind::Cels => "animation-folder",
                Kind::Layer => "layers",
                Kind::Audio => "audio",
            };
            let kind_rect = Rect::from_center_size(
                pos2(eye_rect.right() + icon_side / 2.0, line),
                egui::Vec2::splat(icon_side),
            );
            sue_wm::icons::paint(
                &names_painter,
                kind_rect,
                icon(kind_icon),
                theme.text_muted(),
            );
            let cel_now = (row.kind == Kind::Cels)
                .then(|| anim::cel_at(track, sequence.rate, frame))
                .flatten();
            let title = match (row.kind, cel_now) {
                (Kind::Cels, Some(cel)) => format!(
                    "{} : {}",
                    anim::track_name(&cx.world.arena, track),
                    cx.world
                        .arena
                        .get_by_id(cel)
                        .map_or_else(|_| "?".to_owned(), |entity| entity.name().to_owned())
                ),
                (Kind::Audio, _) if track.muted => format!("{}{}", track.name, tr!("muted")),
                (Kind::Audio, _) => track.name.clone(),
                _ => anim::track_name(&cx.world.arena, track),
            };
            names_painter.text(
                pos2(kind_rect.right() + pad, line),
                Align2::LEFT_CENTER,
                title,
                font.clone(),
                // A locked layer reads as such: its name in the danger
                // color, with the lock beside it.
                if locked {
                    theme.danger()
                } else if chosen {
                    theme.text
                } else {
                    theme.text_muted()
                },
            );
            if locked {
                sue_wm::icons::paint(
                    &names_painter,
                    Rect::from_center_size(
                        pos2(names.right() - pad - icon_side / 2.0, line),
                        egui::Vec2::splat(icon_side),
                    ),
                    icon("lock"),
                    theme.text_muted(),
                );
            }
            let thumb_height = row_size * thumbs.rows();
            if let Some(cel) = cel_now
                && thumb_height > 0.0
            {
                // The cel's picture keeps the canvas's shape.
                let height = thumb_height - pad;
                let rect = Rect::from_min_size(
                    pos2(kind_rect.left(), top + row_size),
                    vec2(height * aspect, height),
                )
                .intersect(rows_area);
                self.cel_thumb(&names_painter, rect, cel, cx, theme);
            }
            let name_click = ui.interact(
                Rect::from_min_max(pos2(kind_rect.left(), name_rect.top()), name_rect.max)
                    .intersect(rows_area),
                ui.scope_id().with(("timeline-name", row.track)),
                Sense::click(),
            );
            if name_click.clicked() {
                actions.push(Action::Go {
                    track: row.track,
                    frame,
                });
            }

            // The frames.
            match row.kind {
                Kind::Cels => {
                    let keys = anim::keys(track, sequence.rate);
                    for (key, (at, cel)) in keys.iter().enumerate() {
                        if *at >= last_seen || *at >= length {
                            continue;
                        }
                        let end = keys
                            .get(key + 1)
                            .map_or(length, |(next, _)| *next)
                            .min(last_seen);
                        let x = column(*at);
                        let name = cel.map_or_else(
                            || "x".to_owned(),
                            |cel| {
                                cx.world.arena.get_by_id(cel).map_or_else(
                                    |_| "?".to_owned(),
                                    |entity| entity.name().to_owned(),
                                )
                            },
                        );
                        cells_painter.text(
                            pos2(x + pad / 2.0, line),
                            Align2::LEFT_CENTER,
                            name,
                            font.clone(),
                            theme.text,
                        );
                        if let Some(cel) = cel {
                            // A line for as long as it shows.
                            cells_painter.hline(
                                (x + pad / 2.0)..=column(end) - pad / 2.0,
                                top + row_size - pad / 2.0,
                                tint.map_or_else(
                                    || Stroke::new(theme.hairline().width, theme.text_muted()),
                                    |tint| Stroke::new(thick, tint),
                                ),
                            );
                            if thumb_height > 0.0 {
                                let width = ((end - at) as f32 * step - pad / 2.0)
                                    .clamp(pad, thumb_height * aspect);
                                let rect = Rect::from_min_size(
                                    pos2(x + pad / 4.0, top + row_size),
                                    vec2(width, thumb_height - pad),
                                );
                                self.cel_thumb(&cells_painter, rect, *cel, cx, theme);
                            }
                        }
                    }
                    // Track labels: an in-between to draw an open circle,
                    // one drawn a filled circle, a reverse sheet a bar.
                    for (at, label) in anim::editing::track_labels(track) {
                        let center = pos2(column(at) + step / 2.0, top + row.height - pad);
                        let radius = (step / 4.0).min(pad / 2.0 + 1.0);
                        match label {
                            TrackLabel::InBetween => {
                                cells_painter.circle_stroke(
                                    center,
                                    radius,
                                    Stroke::new(thick, theme.text),
                                );
                            }
                            TrackLabel::InBetweenDone => {
                                cells_painter.circle_filled(center, radius, theme.text);
                            }
                            TrackLabel::ReverseSheet => {
                                cells_painter.hline(
                                    (center.x - radius)..=(center.x + radius),
                                    center.y,
                                    Stroke::new(thick, theme.text),
                                );
                            }
                        }
                    }
                }
                Kind::Layer => {
                    // Its clips: the frames it shows on.
                    for clip in track.clips() {
                        let from = clip.start.frame(sequence.rate).max(0) as u32;
                        let to = clip.end().frame(sequence.rate).max(0) as u32;
                        let bar = Rect::from_min_max(
                            pos2(column(from) + 1.0, top + pad / 2.0),
                            pos2(column(to) - 1.0, top + row.height - pad / 2.0),
                        );
                        cells_painter.rect_filled(
                            bar,
                            CornerRadius::same(theme.radius(wm_ui::Radius::Xs) as u8),
                            theme.surface(2),
                        );
                        cells_painter.rect_stroke(
                            bar,
                            CornerRadius::same(theme.radius(wm_ui::Radius::Xs) as u8),
                            Stroke::new(theme.hairline().width, theme.border_strong()),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                Kind::Camera | Kind::Audio => {}
            }
            // Keys of the camera, a layer or a folder: a mark each, shaped
            // by how it moves on.
            if row.kind != Kind::Audio
                && let Some(layer) = target
                && (row.kind == Kind::Camera
                    || cx
                        .world
                        .arena
                        .get_by_id(layer)
                        .is_ok_and(anim::motion::keyframes_on))
            {
                let size = (row_size / 2.0).min(step.max(pad));
                for time in anim::motion::key_times(&cx.world.arena, layer) {
                    let at = time.frame(sequence.rate).max(0) as u32;
                    let ease =
                        anim::motion::ease_at(&cx.world.arena, layer, time).unwrap_or(Ease::Linear);
                    let y = if row.kind == Kind::Cels {
                        top + row.height - row_size / 2.0
                    } else {
                        line
                    };
                    key_mark(
                        &cells_painter,
                        pos2(column(at) + step / 2.0, y),
                        size,
                        ease,
                        theme.text,
                    );
                }
            }
            if row.kind == Kind::Audio {
                // The sound's clip: its name, its waveform, dragged to move.
                let Some(clip) = track.clips().first() else {
                    continue;
                };
                let begin = clip.start.frame(sequence.rate);
                let span = ((clip.duration.seconds() * anim::fps(&sequence)).ceil() as i64).max(1);
                let moved = match self.audio_drag {
                    Some((dragging, from, _)) if dragging == row.track => {
                        ui.ctx().pointer_latest_pos().map_or(0, |pos| {
                            (origin + (pos.x - frames_area.left()) / step).floor() as i64 - from
                        })
                    }
                    _ => 0,
                };
                let shown_at = (begin + moved).max(0);
                let x0 = frames_area.left() + (shown_at as f32 - origin) * step;
                let bar = Rect::from_min_max(
                    pos2(x0, top + pad / 2.0),
                    pos2(x0 + span as f32 * step, top + row.height - pad / 2.0),
                );
                let ink = if track.muted {
                    theme.text_disabled(theme.panel)
                } else {
                    theme.text
                };
                cells_painter.rect_filled(
                    bar,
                    CornerRadius::same(theme.radius(wm_ui::Radius::Sm) as u8),
                    theme.surface(2),
                );
                let wave =
                    Rect::from_min_max(pos2(bar.left(), bar.top() + row_size / 2.0), bar.max)
                        .shrink2(egui::Vec2::Y * half_pad);
                self.waveform(&cells_painter, wave.intersect(below), clip.source, cx, ink);
                cells_painter.text(
                    bar.left_top() + vec2(pad, quarter_row),
                    Align2::LEFT_CENTER,
                    format!(
                        "{}  {}",
                        track.name,
                        tr!(
                            "volume-db",
                            value = format!(
                                "{:+.1}",
                                super::forms::AudioClipForm::decibels(anim::volume_of(clip))
                            )
                        )
                    ),
                    micro.clone(),
                    ink,
                );
                let response = ui.interact(
                    cells.intersect(below),
                    ui.scope_id().with(("timeline-audio", row.track)),
                    Sense::click_and_drag(),
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(if response.dragged() {
                        CursorIcon::Grabbing
                    } else {
                        CursorIcon::Grab
                    });
                }
                if response.drag_started()
                    && !panning
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    self.audio_drag = Some((
                        row.track,
                        (origin + (pos.x - frames_area.left()) / step).floor() as i64,
                        begin,
                    ));
                    self.anim_ui.audio_track = Some(row.track);
                }
                if response.dragged() {
                    // The frame it would start on.
                    cells_painter.text(
                        bar.right_center() + Vec2::X * pad,
                        Align2::LEFT_CENTER,
                        self.anim_ui.unit.show(shown_at.max(0) as u32, fps),
                        font.clone(),
                        theme.text,
                    );
                }
                if response.drag_stopped()
                    && let Some((dragging, from, was)) = self.audio_drag.take()
                    && dragging == row.track
                {
                    let to = response.interact_pointer_pos().map_or(was, |pos| {
                        was + (origin + (pos.x - frames_area.left()) / step).floor() as i64 - from
                    });
                    if to != was {
                        audio_drop = Some((row.track, to.max(0)));
                    }
                }
                if response.secondary_clicked() || response.clicked() {
                    self.anim_ui.audio_track = Some(row.track);
                }
                if response.double_clicked() {
                    actions.push(Action::AudioSettings(row.track));
                }
                let items = self.cell_menu(cx, picture, &sequence, row);
                let registry = crate::commands::registry();
                let facts = sue_wm::Facts::of(cx);
                if let Some(picked) = wm_ui::context_menu(
                    ui,
                    &response,
                    &items,
                    &|name| registry.state(self, &facts, name),
                    theme,
                ) {
                    menu_picked = Some(picked);
                }
                continue;
            }

            // Pointer on the frames: click, drag, Shift and click, double
            // click, right click.
            let response = ui.interact(
                cells.intersect(below),
                ui.scope_id().with(("timeline-cells", row.track)),
                Sense::click_and_drag(),
            );
            if panning {
                continue;
            }
            let shift_held = ui.input(|input| input.modifiers.shift);
            // A drag starts where the button went down, not where the
            // pointer is once it has moved far enough to be a drag.
            if response.drag_started()
                && let Some(pos) = ui
                    .input(|input| input.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos())
            {
                let at = at_pointer(pos);
                // What is picked moves (a click picks a cel); a drag from
                // anywhere else picks a box of frames.
                let moving = selection
                    .as_ref()
                    .is_some_and(|selection| selection.contains(row.track, at));
                self.anim_ui.press = Some(Press {
                    track: index,
                    frame: at,
                    moving,
                });
            }
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let at = at_pointer(pos).min(length - 1);
                if shift_held {
                    // Shift and a click picks a run of frames from the
                    // frame shown, across the tracks between.
                    let from_row = layout
                        .iter()
                        .position(|other| other.track == current_track)
                        .unwrap_or(index);
                    let tracks = layout[from_row.min(index)..=from_row.max(index)]
                        .iter()
                        .filter(|other| other.kind != Kind::Audio)
                        .map(|other| other.track)
                        .collect();
                    actions.push(Action::Select(Selection::dragged(tracks, frame, at)));
                } else {
                    // A click picks that frame alone, so a drag from it
                    // moves it.
                    self.anim_ui.selection = Some(Selection::dragged(vec![row.track], at, at));
                    actions.push(Action::Go {
                        track: row.track,
                        frame: at,
                    });
                }
            }
            if response.double_clicked()
                && row.kind == Kind::Cels
                && let Some(pos) = response.interact_pointer_pos()
            {
                // A double click shows the layer picked from there.
                let layer = self
                    .layer()
                    .filter(|layer| anim::may_show(&cx.world.arena, &sequence, row.track, *layer));
                if let Some(layer) = layer {
                    actions.push(Action::Assign {
                        track: row.track,
                        frame: at_pointer(pos).min(length - 1),
                        cel: Some(layer),
                    });
                }
            }
            if response.secondary_clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let at = at_pointer(pos).min(length - 1);
                let inside = selection
                    .as_ref()
                    .is_some_and(|selection| selection.contains(row.track, at));
                if !inside {
                    self.anim_ui.selection = None;
                }
                if let Some(doc) = self.doc_mut() {
                    doc.track = row.track;
                }
                self.go_to_frame(&cx.world.arena, i64::from(at));
            }
            let items = self.cell_menu(cx, picture, &sequence, row);
            let registry = crate::commands::registry();
            let facts = sue_wm::Facts::of(cx);
            if let Some(picked) = wm_ui::context_menu(
                ui,
                &response,
                &items,
                &|name| registry.state(self, &facts, name),
                theme,
            ) {
                menu_picked = Some(picked);
            }
        }

        // ----- A drag across the frames: a box of frames picked, or what
        // is picked moved, a line and the frame it lands on shown.
        if let Some(press) = press {
            let held = ui.input(|input| input.pointer.primary_down());
            let at = hover_frame.unwrap_or(press.frame);
            let row_now = hovered_row.unwrap_or(press.track);
            if press.moving {
                if let Some(selection) = &selection {
                    let to = (i64::from(selection.start) + i64::from(at) - i64::from(press.frame))
                        .max(0) as u32;
                    let x = column(to);
                    cells_painter.vline(x, below.y_range(), Stroke::new(thick, theme.focus()));
                    cells_painter.text(
                        pos2(x + pad / 2.0, below.top() + pad),
                        Align2::LEFT_TOP,
                        self.anim_ui.unit.show(to, fps),
                        font.clone(),
                        theme.text,
                    );
                    if !held {
                        actions.push(Action::MoveTo(to));
                        self.anim_ui.press = None;
                    }
                } else {
                    self.anim_ui.press = None;
                }
            } else {
                let (low, high) = (press.track.min(row_now), press.track.max(row_now));
                let tracks: Vec<usize> = layout[low..=high.min(layout.len() - 1)]
                    .iter()
                    .filter(|row| row.kind != Kind::Audio)
                    .map(|row| row.track)
                    .collect();
                if (at != press.frame || row_now != press.track || !held) && !tracks.is_empty() {
                    self.anim_ui.selection = Some(Selection::dragged(
                        tracks.clone(),
                        press.frame.min(length - 1),
                        at.min(length - 1),
                    ));
                }
                if !held {
                    self.anim_ui.press = None;
                    if let Some(selection) = self.anim_ui.selection.clone()
                        && selection.span() > 1
                    {
                        actions.push(Action::Select(selection));
                    }
                }
            }
        }

        // ----- The frame shown: a bar down the frames in the accent, its
        // number in the head.
        let current = Rect::from_min_size(
            pos2(column(frame), half_ruler),
            vec2(step.max(half_pad), frames_area.bottom() - half_ruler),
        );
        let head = Rect::from_min_max(
            current.min,
            pos2(
                current.left() + current.width().max(row_size),
                half_ruler + ruler_half,
            ),
        );
        painter.rect_filled(
            Rect::from_min_size(current.min, vec2(thick, current.height())),
            CornerRadius::ZERO,
            theme.border_active,
        );
        painter.rect_filled(
            head,
            CornerRadius::same(theme.radius(wm_ui::Radius::Xs) as u8),
            theme.border_active,
        );
        // Its number in whichever of the text and the panel colors reads
        // best on the head's fill.
        let ink = if wm_ui::contrast(theme.text, theme.border_active)
            >= wm_ui::contrast(theme.panel, theme.border_active)
        {
            theme.text
        } else {
            theme.panel
        };
        painter.text(
            head.center(),
            Align2::CENTER_CENTER,
            self.anim_ui.unit.show(frame, fps),
            micro,
            ink,
        );

        timeline_scrollbars(
            ui,
            &mut self.timeline_view,
            below,
            (length, shown),
            (content - ruler, area.height() - ruler),
            theme,
        );
        if let Some((track, to)) = audio_drop {
            actions.push(Action::AudioTo(track, to));
        }
        for action in actions {
            self.timeline_action(cx, action);
        }
        if let Some(command) = menu_picked {
            self.anim_command(&command, cx);
        }
    }
}
