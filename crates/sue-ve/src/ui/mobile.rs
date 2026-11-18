//! Sue VE on phones and tablets, laid out as CapCut is: a top bar with
//! the frame size and Export, the preview large, the play row, the
//! timeline in one column scrolled with a finger under a playhead that
//! stays in the middle (two fingers pinch to zoom it, a long press picks
//! a clip up to drag it), and a bottom bar of large tools, each opening a
//! sheet: Edit, Audio, Text, Effects, Filters, Overlay, Aspect, Canvas,
//! Speed, Split and Delete. The data, the edits and the engine are the
//! desktop's: only the layout differs.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_core::Time;
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font, monitor, pool::texture_key, timecode};
use crate::edit;
use crate::grade::{self, Corrector};
use crate::model::{self, SourceKind, key};
use crate::studio::SueVe;

/// The tools along the bottom: their command name, icon and label.
pub const TOOLS: [(&str, Icon, &str); 11] = [
    ("edit", Icon::ToolRazor, "mobile-edit"),
    ("audio", Icon::Audio, "mobile-audio"),
    ("text", Icon::ToolText, "mobile-text"),
    ("effects", Icon::Effects, "mobile-effects"),
    ("filters", Icon::PaletteColorWheel, "mobile-filters"),
    ("overlay", Icon::Layers, "mobile-overlay"),
    ("aspect", Icon::Aspect, "mobile-aspect"),
    ("canvas", Icon::FillSelection, "mobile-canvas"),
    ("speed", Icon::Speed, "mobile-speed"),
    ("split", Icon::ToolRazor, "mobile-split"),
    ("delete", Icon::Delete, "mobile-delete"),
];

/// The frame shapes the Aspect sheet offers: name and width to height.
pub const ASPECTS: [(&str, u32, u32); 4] = [
    ("9:16", 9, 16),
    ("16:9", 16, 9),
    ("1:1", 1, 1),
    ("4:5", 4, 5),
];

/// The looks the Filters sheet offers: a corrector each.
#[must_use]
pub fn filters() -> Vec<(String, Corrector)> {
    vec![
        (tr!("filter-none"), Corrector::default()),
        (
            tr!("filter-warm"),
            Corrector {
                temperature: 35.0,
                saturation: 1.1,
                ..Corrector::default()
            },
        ),
        (
            tr!("filter-cool"),
            Corrector {
                temperature: -35.0,
                tint: -5.0,
                ..Corrector::default()
            },
        ),
        (
            tr!("filter-vivid"),
            Corrector {
                saturation: 1.45,
                contrast: 1.15,
                ..Corrector::default()
            },
        ),
        (
            tr!("filter-mono"),
            Corrector {
                saturation: 0.0,
                contrast: 1.1,
                ..Corrector::default()
            },
        ),
        (
            tr!("filter-fade"),
            Corrector {
                lift: [0.0, 0.0, 0.0, 0.12],
                saturation: 0.8,
                contrast: 0.85,
                ..Corrector::default()
            },
        ),
    ]
}

/// The frame size of `aspect` for a sequence whose longer side is
/// `long`: even sides.
#[must_use]
pub fn aspect_size(width: u32, height: u32, long: u32) -> (u32, u32) {
    let even = |value: f64| ((value.round() as u32).max(2) + 1) & !1;
    if width >= height {
        (
            even(f64::from(long)),
            even(f64::from(long) * f64::from(height) / f64::from(width)),
        )
    } else {
        (
            even(f64::from(long) * f64::from(width) / f64::from(height)),
            even(f64::from(long)),
        )
    }
}

/// The export sizes the Export sheet offers (the shorter side).
pub const RESOLUTIONS: [u32; 4] = [720, 1080, 1440, 2160];
/// The frame rates it offers.
pub const FRAME_RATES: [u32; 5] = [24, 25, 30, 50, 60];

/// The phone and tablet layout of the studio, in `body` (under Sue's top
/// bar).
pub fn mobile_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    ui.painter().rect_filled(body, SQUARE, theme.void);
    let tablet = cx.view.form_factor == sue_platform::FormFactor::Tablet;
    let touch = Density::Touch.icon_button();
    let bar = touch + theme.space(Space::S2) * 2.0;
    let top = Rect::from_min_size(body.min, vec2(body.width(), bar));
    let tools_height = touch * 1.7;
    let tools = Rect::from_min_max(pos2(body.left(), body.bottom() - tools_height), body.max);
    let rest = Rect::from_min_max(
        pos2(body.left(), top.bottom()),
        pos2(body.right(), tools.top()),
    );
    let preview_share = if tablet { 0.55 } else { 0.48 };
    let preview = Rect::from_min_size(rest.min, vec2(rest.width(), rest.height() * preview_share));
    let play_row = Rect::from_min_size(
        pos2(rest.left(), preview.bottom()),
        vec2(rest.width(), touch),
    );
    let lanes = Rect::from_min_max(pos2(rest.left(), play_row.bottom()), rest.max);
    top_bar(studio, ui, top, cx, theme);
    if studio.project.is_none() {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        super::pages::start_ui(studio, &mut child, rest, cx, theme);
        return;
    }
    monitor::program_monitor(studio, ui, preview, cx, theme);
    play_ui(studio, ui, play_row, cx, theme);
    lanes_ui(studio, ui, lanes, cx, theme);
    tools_ui(studio, ui, tools, theme);
    if let Some(sheet) = studio.mobile.sheet {
        sheet_ui(studio, ui, body, sheet, cx, theme);
    }
    if studio.mobile.export {
        export_sheet(studio, ui, body, cx, theme);
    }
}

/// The top bar: the frame size, templates (later) and Export.
fn top_bar(studio: &mut SueVe, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
    ui.painter().rect_filled(rect, SQUARE, theme.panel);
    let size = studio.frame_size(cx);
    ui.painter().text(
        pos2(rect.left() + theme.space(Space::S5), rect.center().y),
        Align2::LEFT_CENTER,
        format!("{} x {}", size.0, size.1),
        font(theme, TextRole::Mono),
        theme.text_muted(),
    );
    let width = theme.space(Space::S9) * 2.2;
    let export = Rect::from_min_size(
        pos2(
            rect.right() - width - theme.space(Space::S4),
            rect.top() + theme.space(Space::S2),
        ),
        vec2(width, rect.height() - theme.space(Space::S2) * 2.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(export));
    if sue_wm::ui::button("sve-mobile-export", &tr!("mobile-export"))
        .primary()
        .icon(Icon::Export)
        .enabled(studio.sequence.is_some())
        .show(&mut child, theme)
        .clicked()
    {
        studio.mobile.export = true;
        studio.mobile.sheet = None;
    }
    let templates = Rect::from_min_size(
        pos2(
            export.left() - Density::Touch.icon_button() - theme.space(Space::S2),
            export.top(),
        ),
        vec2(Density::Touch.icon_button(), export.height()),
    );
    if sue_wm::ui::icon_button(
        "sve-mobile-templates",
        Icon::Template,
        &tr!("mobile-templates-later"),
    )
    .quiet()
    .show_at(ui, templates, theme)
    .clicked()
    {
        cx.notify(tr!("mobile-templates-later"));
    }
}

/// The play row: where the playhead is, play, undo and redo.
fn play_ui(studio: &mut SueVe, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rate = studio
        .current(cx)
        .map_or(sue_core::Rate::FPS_30, |sequence| sequence.rate);
    let length = studio
        .current(cx)
        .map_or(Time::ZERO, sue_model::Sequence::duration);
    ui.painter().text(
        pos2(rect.left() + theme.space(Space::S5), rect.center().y),
        Align2::LEFT_CENTER,
        format!(
            "{} / {}",
            timecode(studio.clock.position, rate),
            timecode(length, rate)
        ),
        font(theme, TextRole::Mono),
        theme.text,
    );
    let side = Density::Touch.icon_button();
    let play = Rect::from_center_size(rect.center(), vec2(side, side));
    let mut run = None;
    if sue_wm::ui::icon_button(
        "sve-mobile-play",
        if studio.clock.playing {
            Icon::Pause
        } else {
            Icon::Play
        },
        &tr!("cmd-play"),
    )
    .show_at(ui, play, theme)
    .clicked()
    {
        run = Some("playback.play");
    }
    for (index, (icon, command, tip)) in [
        (Icon::Redo, "edit.redo", tr!("cmd-redo")),
        (Icon::Undo, "edit.undo", tr!("cmd-undo")),
    ]
    .into_iter()
    .enumerate()
    {
        let place = Rect::from_min_size(
            pos2(
                rect.right() - (side + theme.space(Space::S2)) * (index as f32 + 1.0),
                rect.top(),
            ),
            vec2(side, side),
        );
        if sue_wm::ui::icon_button(("sve-mobile", command), icon, &tip)
            .quiet()
            .show_at(ui, place, theme)
            .clicked()
        {
            run = Some(command);
        }
    }
    if let Some(command) = run {
        studio.focus = crate::studio::Monitor::Program;
        studio.run(command, cx);
    }
}

/// The lanes: every track as a row, the playhead fixed in the middle and
/// the clips sliding under it as the finger drags; pinch zooms; a long
/// press picks a clip up and a drag then moves it.
#[allow(
    clippy::too_many_lines,
    reason = "the lanes' drawing and their touch handling share their geometry"
)]
fn lanes_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let Some(sequence) = studio.current(cx).cloned() else {
        return;
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SQUARE, theme.void);
    let middle = rect.center().x;
    let zoom = studio.zoom.clamp(10.0, 400.0);
    let now = studio.clock.position;
    let x_of = |time: Time| middle + ((time - now).seconds() as f32) * zoom;
    let time_of =
        |x: f32| (now + Time::from_seconds(f64::from((x - middle) / zoom))).max(Time::ZERO);
    // The rows: the video tracks that have clips (the main track always),
    // then the audio tracks that have clips.
    let main = sequence.tracks.iter().rposition(model::is_video);
    let rows: Vec<usize> = (0..sequence.tracks.len())
        .filter(|index| Some(*index) == main || !sequence.tracks[*index].clips().is_empty())
        .collect();
    let main_height = (rect.height() * 0.32).max(Density::Touch.icon_button());
    let other = ((rect.height() - main_height) / rows.len().saturating_sub(1).max(1) as f32)
        .clamp(Density::Touch.row() * 0.6, Density::Touch.row());
    let mut y = rect.top() + theme.space(Space::S5);
    let mut places: Vec<(u64, usize, Rect)> = Vec::new();
    // The ruler's seconds along the top.
    let step = if zoom >= 60.0 {
        1.0
    } else if zoom >= 20.0 {
        5.0
    } else {
        10.0
    };
    let first = (time_of(rect.left()).seconds() / step).floor() as i64;
    let last = (time_of(rect.right()).seconds() / step).ceil() as i64;
    for tick in first..=last {
        let at = Time::from_seconds(tick as f64 * step);
        let x = x_of(at);
        painter.text(
            pos2(x, rect.top() + theme.space(Space::S1)),
            Align2::CENTER_TOP,
            format!("{}s", tick as f64 * step),
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
    }
    for index in rows {
        let track = &sequence.tracks[index];
        let height = if Some(index) == main {
            main_height
        } else {
            other
        };
        let lane = Rect::from_min_size(
            pos2(rect.left(), y),
            vec2(rect.width(), height - theme.space(Space::S1)),
        );
        y += height;
        for clip in track.clips() {
            let left = x_of(clip.start);
            let right = x_of(clip.end());
            if right < rect.left() || left > rect.right() {
                continue;
            }
            let body = Rect::from_min_max(pos2(left, lane.top()), pos2(right, lane.bottom()));
            places.push((clip.id, index, body));
            let selected = studio.selection.contains(&clip.id);
            let video = model::is_video(track);
            let fill = if video { theme.info() } else { theme.success() }.gamma_multiply(0.45);
            painter.rect_filled(body.shrink(1.0), theme.corner(sue_wm::ui::Radius::Sm), fill);
            if video && Some(index) == main {
                // Thumbnails along the main track.
                if let Some(strip) = studio.hub.thumbs.strip(clip.source) {
                    let strip = strip
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(first) = strip.frames.first() {
                        let width = body.height() * first.width as f32 / first.height.max(1) as f32;
                        let mut x = body.left().max(rect.left() - width);
                        while x < body.right().min(rect.right()) {
                            let share = f64::from((x - body.left()) / body.width().max(1.0));
                            let at = clip.source_in.seconds()
                                + (clip.source_out() - clip.source_in).seconds() * share;
                            if let Some(frame) = strip.at(at) {
                                let key =
                                    texture_key(("sve-mobile-thumb", clip.source, frame.index));
                                if let Some(texture) = cx.textures.texture(
                                    key,
                                    frame.serial,
                                    [frame.width, frame.height],
                                    4,
                                    &frame.rgba,
                                    false,
                                ) {
                                    let place = Rect::from_min_size(
                                        pos2(x, body.top()),
                                        vec2(width, body.height()),
                                    )
                                    .intersect(body);
                                    painter.image(
                                        texture,
                                        place,
                                        Rect::from_min_max(
                                            pos2(0.0, 0.0),
                                            pos2(place.width() / width, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                            }
                            x += width;
                        }
                    }
                } else {
                    super::pool::picture(studio, ui, body.shrink(2.0), clip.source, cx, theme);
                }
            }
            let kind = cx
                .arena()
                .get_by_id(clip.source)
                .map_or(SourceKind::Unknown, SourceKind::of);
            if !video && let Some(sound) = studio.hub.sounds.get(clip.source) {
                super::timeline::waveform(
                    ui,
                    body.shrink(2.0).intersect(rect),
                    &sound,
                    clip.source_in,
                    clip.source_out(),
                    1.0,
                    theme,
                );
            } else if Some(index) != main {
                let name = cx
                    .arena()
                    .get_by_id(clip.source)
                    .map(|entity| entity.name().to_owned())
                    .unwrap_or_default();
                let side = Density::Compact.icon();
                icons::paint(
                    &painter,
                    Rect::from_min_size(
                        pos2(body.left() + 4.0, body.center().y - side / 2.0),
                        vec2(side, side),
                    ),
                    super::pool::kind_icon(&kind),
                    theme.text,
                );
                painter.text(
                    pos2(body.left() + side + 8.0, body.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    font(theme, TextRole::Caption),
                    theme.text,
                );
            }
            painter.rect_stroke(
                body.shrink(1.0),
                theme.corner(sue_wm::ui::Radius::Sm),
                if selected {
                    egui::Stroke::new(Theme::STRONG_STROKE * 1.5, theme.focus())
                } else {
                    theme.hairline()
                },
                egui::StrokeKind::Inside,
            );
        }
    }
    // The playhead, fixed in the middle.
    painter.vline(
        middle,
        rect.y_range(),
        egui::Stroke::new(Theme::STRONG_STROKE, theme.text),
    );
    // Touch: a tap picks a clip, a drag scrolls, a long press then a drag
    // moves the clip, two fingers zoom.
    let response = ui.interact(
        rect,
        egui::Id::unique("sve-mobile-lanes"),
        Sense::click_and_drag(),
    );
    if let Some(touch) = ui.input(egui::InputState::multi_touch)
        && (touch.zoom_delta - 1.0).abs() > 1e-4
    {
        studio.zoom = (studio.zoom * touch.zoom_delta).clamp(10.0, 400.0);
    }
    let pinch = ui.input(egui::InputState::zoom_delta);
    if (pinch - 1.0).abs() > 1e-4 {
        studio.zoom = (studio.zoom * pinch).clamp(10.0, 400.0);
    }
    let hit = |at: egui::Pos2| {
        places
            .iter()
            .find(|(_, _, body)| body.contains(at))
            .map(|(id, track, _)| (*id, *track))
    };
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        match hit(at) {
            Some((id, _)) => {
                studio.selection = vec![id];
                studio.picked = sequence.find(id).map(|(_, clip)| clip.source);
            }
            None => studio.selection.clear(),
        }
    }
    let held = egui::Id::unique("sve-mobile-held");
    if response.drag_started()
        && let Some(at) = response.interact_pointer_pos()
    {
        // A press held still for half a second before moving picks the
        // clip up.
        let long = ui.input(|input| {
            input
                .pointer
                .press_start_time()
                .is_some_and(|start| input.time - start >= 0.45)
        });
        let grabbed = if long { hit(at) } else { None };
        if let Some((id, _)) = grabbed {
            studio.selection = vec![id];
        }
        ui.data_mut(|data| data.insert_temp(held, grabbed.map(|(id, track)| (id, track, at.x))));
    }
    let grabbed: Option<(u64, usize, f32)> = ui.data(|data| data.get_temp(held)).flatten();
    if response.dragged() {
        let delta = response.drag_delta().x;
        if grabbed.is_none() && delta != 0.0 {
            // The content follows the finger: the playhead moves the other
            // way.
            let to = (studio.clock.position - Time::from_seconds(f64::from(delta / zoom)))
                .max(Time::ZERO);
            let now = cx.view.now;
            studio.clock.seek(to, now, cx.sink());
        }
    }
    if response.drag_stopped() {
        if let (Some((id, _, from)), Some(at)) = (grabbed, response.interact_pointer_pos()) {
            let moved = Time::from_seconds(f64::from((at.x - from) / zoom));
            let ids = studio.selected_with_links(cx);
            let ids = if ids.is_empty() { vec![id] } else { ids };
            studio.edit(cx, &tr!("edit-move"), |sequence, _| {
                edit::move_clips(sequence, &ids, moved, 0, false).map(|_| ())
            });
        }
        ui.data_mut(|data| data.insert_temp::<Option<(u64, usize, f32)>>(held, None));
    }
    if grabbed.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
}

/// The bottom bar of tools, scrolled sideways when they do not fit.
fn tools_ui(studio: &mut SueVe, ui: &mut Ui, rect: Rect, theme: &Theme) {
    ui.painter().rect_filled(rect, SQUARE, theme.panel);
    ui.painter()
        .hline(rect.x_range(), rect.top(), theme.hairline());
    let side = Density::Touch.icon_button();
    let width = side * 1.5;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .id_salt("sve-mobile-tools"),
    );
    egui::ScrollArea::horizontal()
        .id_salt("sve-mobile-tools-scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.horizontal(|ui| {
                for (name, icon, label) in TOOLS {
                    let (place, response) =
                        ui.allocate_exact_size(vec2(width, rect.height()), Sense::click());
                    let on = studio.mobile.sheet == Some(name);
                    if on {
                        ui.painter().rect_filled(
                            place.shrink(2.0),
                            theme.corner(sue_wm::ui::Radius::Md),
                            theme.fill_selected(),
                        );
                    }
                    let icon_side = Density::Touch.icon();
                    icons::paint(
                        ui.painter(),
                        Rect::from_center_size(
                            place.center() - vec2(0.0, icon_side * 0.4),
                            vec2(icon_side, icon_side),
                        ),
                        icon,
                        theme.text,
                    );
                    ui.painter().text(
                        place.center() + vec2(0.0, icon_side * 0.5),
                        Align2::CENTER_TOP,
                        sue_lang::tr!("ve", label),
                        font(theme, TextRole::Micro),
                        theme.text,
                    );
                    if response.clicked() {
                        studio.mobile.sheet = if on { None } else { Some(name) };
                        studio.mobile.export = false;
                    }
                }
            });
        });
}

/// A sheet rising from the bottom over the lanes: its title, a close
/// button, and `content`.
fn sheet(
    ui: &mut Ui,
    body: Rect,
    title: &str,
    theme: &Theme,
    content: impl FnOnce(&mut Ui),
) -> bool {
    let height = (body.height() * 0.36).max(Density::Touch.row() * 4.0);
    let tools = Density::Touch.icon_button() * 1.7;
    let rect = Rect::from_min_max(
        pos2(body.left(), body.bottom() - tools - height),
        pos2(body.right(), body.bottom() - tools),
    );
    ui.painter()
        .rect_filled(rect, theme.corner(sue_wm::ui::Radius::Lg), theme.surface(2));
    let side = Density::Touch.icon_button();
    let close = Rect::from_min_size(
        pos2(
            rect.right() - side - theme.space(Space::S2),
            rect.top() + theme.space(Space::S2),
        ),
        vec2(side, side),
    );
    let closed = sue_wm::ui::icon_button("sve-sheet-close", Icon::Close, &tr!("mobile-close"))
        .quiet()
        .show_at(ui, close, theme)
        .clicked();
    ui.painter().text(
        pos2(rect.left() + theme.space(Space::S5), close.center().y),
        Align2::LEFT_CENTER,
        title,
        font(theme, TextRole::Title),
        theme.text,
    );
    let inner = Rect::from_min_max(
        pos2(
            rect.left() + theme.space(Space::S5),
            close.bottom() + theme.space(Space::S2),
        ),
        rect.max - vec2(theme.space(Space::S5), theme.space(Space::S3)),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .id_salt(("sve-sheet", title)),
    );
    child.set_clip_rect(inner);
    egui::ScrollArea::vertical()
        .id_salt("sve-sheet-scroll")
        .auto_shrink([false, false])
        .show(&mut child, content);
    closed
}

/// A big tile button of a sheet.
fn tile(ui: &mut Ui, salt: &str, icon: Icon, label: &str, on: bool, theme: &Theme) -> bool {
    let side = Density::Touch.icon_button() * 1.6;
    let (place, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    ui.painter().rect_filled(
        place.shrink(2.0),
        theme.corner(sue_wm::ui::Radius::Md),
        if on {
            theme.fill_selected()
        } else {
            theme.surface(1)
        },
    );
    let icon_side = Density::Touch.icon();
    icons::paint(
        ui.painter(),
        Rect::from_center_size(
            place.center() - vec2(0.0, icon_side * 0.35),
            vec2(icon_side, icon_side),
        ),
        icon,
        theme.text,
    );
    ui.painter().text(
        place.center() + vec2(0.0, icon_side * 0.45),
        Align2::CENTER_TOP,
        label,
        font(theme, TextRole::Micro),
        theme.text,
    );
    let _ = salt;
    response.clicked()
}

/// The sheet of tool `name`.
#[allow(clippy::too_many_lines, reason = "one sheet per tool, each a few rows")]
fn sheet_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    name: &'static str,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    // Split and Delete act at once.
    match name {
        "split" => {
            studio.mobile.sheet = None;
            studio.run("sequence.razor", cx);
            return;
        }
        "delete" => {
            studio.mobile.sheet = None;
            studio.run("edit.delete", cx);
            return;
        }
        _ => {}
    }
    let title = TOOLS
        .iter()
        .find(|(tool, _, _)| *tool == name)
        .map(|(_, _, label)| sue_lang::tr!("ve", label))
        .unwrap_or_default();
    let mut run: Option<&'static str> = None;
    let mut effect: Option<&'static str> = None;
    let mut filter: Option<Corrector> = None;
    let mut aspect: Option<(u32, u32)> = None;
    let mut canvas: Option<[u8; 3]> = None;
    let mut speed: Option<f64> = None;
    let selected = studio.selected_clip(cx);
    let closed = sheet(ui, body, &title, theme, |ui| match name {
        "edit" => {
            ui.horizontal_wrapped(|ui| {
                for (command, icon, label) in [
                    ("sequence.razor", Icon::ToolRazor, tr!("mobile-split")),
                    ("clip.reverse", Icon::Reverse, tr!("cmd-reverse")),
                    ("clip.freeze", Icon::Stop, tr!("cmd-freeze-frame")),
                    ("edit.copy", Icon::Copy, tr!("cmd-copy")),
                    ("edit.paste", Icon::Paste, tr!("cmd-paste")),
                    ("edit.delete", Icon::Delete, tr!("cmd-delete")),
                ] {
                    if tile(ui, command, icon, &label, false, theme) {
                        run = Some(command);
                    }
                }
            });
        }
        "audio" => {
            ui.horizontal_wrapped(|ui| {
                for (command, icon, label) in [
                    ("file.import", Icon::Import, tr!("mobile-add-sound")),
                    (
                        "audio.record",
                        Icon::Microphone,
                        tr!("cmd-record-voice-over"),
                    ),
                    ("audio.normalize", Icon::Mixer, tr!("cmd-normalize")),
                    ("clip.fade", Icon::Audio, tr!("cmd-sound-fades")),
                ] {
                    if tile(ui, command, icon, &label, false, theme) {
                        run = Some(command);
                    }
                }
            });
        }
        "text" => {
            ui.horizontal_wrapped(|ui| {
                for (command, icon, label) in [
                    ("file.new_title", Icon::ToolText, tr!("mobile-add-text")),
                    (
                        "fx.text_typewriter",
                        Icon::Keyboard,
                        tr!("cmd-text-typewriter"),
                    ),
                    ("fx.text_drop", Icon::ArrowDown, tr!("cmd-text-drop")),
                    ("fx.text_grow", Icon::ZoomIn, tr!("cmd-text-grow")),
                    ("file.new_shape", Icon::ToolFigure, tr!("cmd-new-shape")),
                ] {
                    if tile(ui, command, icon, &label, false, theme) {
                        run = Some(command);
                    }
                }
                if tile(
                    ui,
                    "captions",
                    Icon::Captions,
                    &tr!("mobile-auto-captions"),
                    false,
                    theme,
                ) {
                    run = Some("captions");
                }
            });
        }
        "effects" => {
            ui.horizontal_wrapped(|ui| {
                for kind in crate::fx::kinds() {
                    if kind.id == "key.track_matte" {
                        continue;
                    }
                    if tile(ui, kind.id, Icon::Effects, &kind.label(), false, theme) {
                        effect = Some(kind.id);
                    }
                }
            });
        }
        "filters" => {
            ui.horizontal_wrapped(|ui| {
                for (label, look) in filters() {
                    if tile(ui, &label, Icon::PaletteColorWheel, &label, false, theme) {
                        filter = Some(look);
                    }
                }
            });
        }
        "overlay" => {
            ui.label(
                egui::RichText::new(tr!("mobile-overlay-note"))
                    .font(font(theme, TextRole::Caption))
                    .color(theme.text_muted()),
            );
            ui.horizontal_wrapped(|ui| {
                if tile(
                    ui,
                    "overlay-import",
                    Icon::Import,
                    &tr!("mobile-add-overlay"),
                    false,
                    theme,
                ) {
                    run = Some("overlay");
                }
                if tile(
                    ui,
                    "overlay-mask",
                    Icon::LayerMaskNew,
                    &tr!("cmd-mask-ellipse"),
                    false,
                    theme,
                ) {
                    run = Some("fx.mask_ellipse");
                }
            });
        }
        "aspect" => {
            ui.horizontal_wrapped(|ui| {
                for (label, w, h) in ASPECTS {
                    if tile(ui, label, Icon::Aspect, label, false, theme) {
                        aspect = Some((w, h));
                    }
                }
            });
        }
        "canvas" => {
            ui.horizontal_wrapped(|ui| {
                for (label, rgb) in [
                    (tr!("canvas-black"), [0, 0, 0]),
                    (tr!("canvas-white"), [255, 255, 255]),
                    (tr!("canvas-gray"), [64, 64, 64]),
                    (tr!("canvas-blue"), [24, 40, 72]),
                ] {
                    if tile(ui, &label, Icon::FillSelection, &label, false, theme) {
                        canvas = Some(rgb);
                    }
                }
            });
        }
        "speed" => {
            let current = selected.as_ref().map_or(1.0, |(_, clip)| clip.speed.abs());
            ui.horizontal_wrapped(|ui| {
                for value in [0.25, 0.5, 1.0, 1.5, 2.0, 4.0] {
                    let label = format!("{value}x");
                    if tile(
                        ui,
                        &label,
                        Icon::Speed,
                        &label,
                        (current - value).abs() < 1e-6,
                        theme,
                    ) {
                        speed = Some(value);
                    }
                }
            });
        }
        _ => {}
    });
    if closed {
        studio.mobile.sheet = None;
    }
    if let Some(command) = run {
        match command {
            "captions" => {
                cx.notify(tr!("mobile-captions-later"));
            }
            "overlay" => {
                // What is imported goes over the main track.
                if let Some(sequence) = studio.current(cx) {
                    let main = sequence.tracks.iter().rposition(model::is_video);
                    let over = main.and_then(|main| main.checked_sub(1));
                    if let Some(over) = over {
                        studio.edit(cx, &tr!("track-change"), |sequence, _| {
                            for (index, track) in sequence.tracks.iter_mut().enumerate() {
                                if model::is_video(track) {
                                    track.properties.set(key::TARGET, index == over);
                                }
                            }
                            Ok(())
                        });
                    }
                }
                studio.run("file.import", cx);
            }
            other => {
                studio.run(other, cx);
            }
        }
    }
    if let Some(kind) = effect {
        let ids = studio.selected_with_links(cx);
        super::browser::add_effect(studio, cx, kind, &ids);
    }
    if let (Some(look), Some((_, clip))) = (filter, &selected) {
        studio.change_clip(cx, &tr!("mobile-filters"), clip.id, |clip| {
            grade::set_grade(
                clip,
                &if look.is_neutral() {
                    Vec::new()
                } else {
                    vec![look]
                },
            );
            Ok(())
        });
    }
    if let (Some((w, h)), Some(id)) = (aspect, studio.sequence) {
        let size = studio.frame_size(cx);
        let long = size.0.max(size.1);
        let (width, height) = aspect_size(w, h, long);
        let plan = model::plan(cx.arena(), id, &tr!("mobile-aspect"), |sequence| {
            sequence.properties.set(key::SEQ_WIDTH, i64::from(width));
            sequence.properties.set(key::SEQ_HEIGHT, i64::from(height));
            Ok(())
        });
        if let Ok((Some(step), ())) = plan {
            studio.apply(cx, step);
        }
    }
    if let (Some(rgb), Some(id)) = (canvas, studio.sequence) {
        let plan = model::plan(cx.arena(), id, &tr!("mobile-canvas"), |sequence| {
            sequence.properties.set(
                crate::compose::CANVAS,
                sue_core::Color::srgb8(rgb[0], rgb[1], rgb[2]),
            );
            Ok(())
        });
        if let Ok((Some(step), ())) = plan {
            studio.apply(cx, step);
        }
    }
    if let (Some(value), Some((_, clip))) = (speed, &selected) {
        let id = clip.id;
        let signed = if clip.speed < 0.0 { -value } else { value };
        studio.edit(cx, &tr!("cmd-speed"), |sequence, _| {
            edit::set_speed(sequence, id, signed, true)
        });
    }
}

/// The Export sheet: size and frame rate, then Export.
fn export_sheet(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let size = studio.frame_size(cx);
    let short = size.0.min(size.1).max(1);
    let mut resolution = RESOLUTIONS
        .iter()
        .position(|value| *value == studio.deliver.height.min(studio.deliver.width) as u32)
        .unwrap_or(1);
    let rate_now = studio
        .current(cx)
        .map_or(30, |sequence| sequence.rate.as_f64().round() as u32);
    let mut rate = FRAME_RATES
        .iter()
        .position(|value| *value == rate_now)
        .unwrap_or(2);
    let mut go = false;
    let closed = sheet(ui, body, &tr!("mobile-export"), theme, |ui| {
        let names: Vec<String> = RESOLUTIONS
            .iter()
            .map(|value| format!("{value}p"))
            .collect();
        ui.label(
            egui::RichText::new(tr!("mobile-resolution")).font(font(theme, TextRole::Caption)),
        );
        if let Some(picked) =
            sue_wm::ui::segmented("sve-export-resolution", &names, Some(resolution))
                .show(ui, theme)
                .picked
        {
            resolution = picked;
        }
        ui.add_space(theme.space(Space::S3));
        let rates: Vec<String> = FRAME_RATES.iter().map(|value| format!("{value}")).collect();
        ui.label(
            egui::RichText::new(tr!("mobile-frame-rate")).font(font(theme, TextRole::Caption)),
        );
        if let Some(picked) = sue_wm::ui::segmented("sve-export-rate", &rates, Some(rate))
            .show(ui, theme)
            .picked
        {
            rate = picked;
        }
        ui.add_space(theme.space(Space::S4));
        if sue_wm::ui::button("sve-export-go", &tr!("deliver-export"))
            .primary()
            .icon(Icon::Export)
            .show(ui, theme)
            .clicked()
        {
            go = true;
        }
        for running in &studio.exports {
            let done = match cx
                .transfers
                .get(running.transfer)
                .map(|transfer| transfer.state.clone())
            {
                Some(sue_wm::TransferState::Running(done)) => done,
                _ => 1.0,
            };
            sue_wm::ui::progress_bar(done).show(ui, theme);
        }
    });
    let target = RESOLUTIONS[resolution];
    let scale = f64::from(target) / f64::from(short);
    let even = |value: f64| ((value.round() as u32).max(2) + 1) & !1;
    studio.deliver.width = f64::from(even(f64::from(size.0) * scale));
    studio.deliver.height = f64::from(even(f64::from(size.1) * scale));
    studio.deliver.rate = Some(FRAME_RATES[rate]);
    if closed {
        studio.mobile.export = false;
    }
    if go {
        studio.deliver.target = crate::export::Target::Mp4;
        studio.deliver.whole = true;
        let name = studio
            .sequence
            .and_then(|id| cx.arena().get_by_id(id).ok())
            .map_or_else(
                || tr!("untitled-project"),
                |entity| entity.name().to_owned(),
            );
        if cx.view.interactive {
            let pick = sue_wm::FilePick::save(
                Some(crate::studio::me()),
                "export",
                &tr!("deliver-export"),
                &format!("{name}.mp4"),
            )
            .extensions(&["mp4"]);
            cx.request(sue_wm::ShellRequest::PickFile(pick));
        } else {
            let path = crate::media::import::cache_dir()
                .join("exports")
                .join(format!("{name}.mp4"));
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            studio.export_to(cx, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_sizes_keep_the_long_side_and_stay_even() {
        assert_eq!(aspect_size(9, 16, 1920), (1080, 1920));
        assert_eq!(aspect_size(16, 9, 1920), (1920, 1080));
        assert_eq!(aspect_size(1, 1, 1920), (1920, 1920));
        assert_eq!(aspect_size(4, 5, 1920), (1536, 1920));
        assert_eq!(TOOLS.len(), 11);
        assert!(filters()[0].1.is_neutral());
        assert!(!filters()[4].1.is_neutral());
    }
}
