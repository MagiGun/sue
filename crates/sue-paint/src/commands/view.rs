//! The View menu's commands.
//!
//! Each command is an entry of its own: its name, label, default chord,
//! icon and flags, and the functions that tell how its item looks and
//! do what it does.
//!
//! Flags of the group: every View command may be recorded by Auto
//! Actions ([`Flags::RECORDABLE`], as `recordable()` of `extras.rs` says
//! for every name outside File, Window, Help and Auto Actions), and none
//! leaves the canvas ([`Flags::LEAVES_CANVAS`]): Color Profile, Preview
//! Settings opens a dialog of the studio, which Esc closes.

use std::collections::VecDeque;

use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui, pos2, vec2};
use sue_core::{Arena, EntityId, Value};
use sue_model::picture::{self, LayerKind};
use sue_platform::time::Instant;
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState, Theme};

use super::{flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::studio::SuePaint;

// ----- What the View menu shows on the canvas ------------------------------

/// The picture's switch for snapping to its special rulers (special,
/// perspective and symmetrical), kept on the picture so it undoes, saves
/// and opens again with it: `off` while they do not snap, absent while
/// they do. View, Snap to Special Ruler is the view's own switch; a
/// stroke follows these rulers only while both are on.
pub(crate) const SPECIAL_SNAP: &str = "ruler.special.snap";

/// How far inside the finished page the safety margin runs, in
/// millimeters: what is inside it is never cut off by a trim that is a
/// little off.
pub(crate) const SAFETY_MARGIN_MM: f32 = 5.0;

/// Which layers show the paths of their vector lines (View, Show Vector
/// Paths).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PathsOn {
    /// None.
    #[default]
    Off,
    /// The current layer and the layers picked with it.
    Selected,
    /// Every visible vector layer.
    All,
}

/// Which control points the shown paths mark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PointsOn {
    /// None.
    #[default]
    Off,
    /// Where each line begins and ends.
    Ends,
    /// Every control point.
    All,
}

/// What the View menu adds over the picture: the safety margin, the
/// paths of vector lines and the playback rate.
#[derive(Debug, Default)]
pub(crate) struct ViewMarks {
    /// View, Safety Margin.
    pub safety_margin: bool,
    /// View, Show Vector Paths: which layers.
    pub paths: PathsOn,
    /// View, Show Vector Paths: which control points.
    pub points: PointsOn,
    /// View, Show Playback FPS.
    pub playback_fps: bool,
    /// When the frames shown during playback changed, and to which, over
    /// about the last second.
    shown: VecDeque<(Instant, u32)>,
}

impl ViewMarks {
    /// How many frames a second playback showed, over about the last
    /// second; `None` until two frames were seen.
    pub(crate) fn rate(&self) -> Option<f64> {
        let (first, _) = *self.shown.front()?;
        let (last, _) = *self.shown.back()?;
        let seconds = last.saturating_duration_since(first).as_secs_f64();
        (self.shown.len() > 1 && seconds > 0.0).then(|| (self.shown.len() - 1) as f64 / seconds)
    }

    /// Notes the frame on show at `now` (`None` while nothing plays).
    pub(crate) fn note(&mut self, now: Instant, frame: Option<u32>) {
        let Some(frame) = frame else {
            self.shown.clear();
            return;
        };
        if self.shown.back().is_some_and(|(_, last)| *last == frame) {
            return;
        }
        self.shown.push_back((now, frame));
        while self.shown.len() > 2
            && self
                .shown
                .front()
                .is_some_and(|(at, _)| now.saturating_duration_since(*at).as_secs_f64() > 1.0)
        {
            self.shown.pop_front();
        }
    }
}

/// `true` while strokes may snap to the special rulers of `picture`, as
/// far as the picture's own switch goes.
pub(crate) fn special_snaps(arena: &Arena, picture: EntityId) -> bool {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(SPECIAL_SNAP))
        .and_then(Value::as_str)
        != Some("off")
}

/// `true` when `picture` has a special, perspective or symmetrical ruler.
fn has_special_ruler(arena: &Arena, picture: EntityId) -> bool {
    let rulers = crate::rulers::Rulers::of(arena, picture);
    rulers.special.is_some() || rulers.perspective.is_some() || rulers.symmetry.is_some()
}

/// The layer ids under `parent`, folders opened, bottom first.
fn all_layers(arena: &Arena, parent: EntityId, out: &mut Vec<EntityId>) {
    for child in arena.relations().children(parent) {
        out.push(child);
        all_layers(arena, child, out);
    }
}

impl SuePaint {
    /// What the View menu adds over the picture: the safety margin of a
    /// page, the paths of vector lines and their control points, and the
    /// rate playback shows.
    pub(crate) fn view_marks(
        &mut self,
        ui: &Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let arena = &cx.world.arena;
        let view = doc.view;
        let screen = |[x, y]: [f32; 2]| view.to_screen(viewport, size, pos2(x, y));
        let painter = ui.painter().with_clip_rect(viewport);
        if self.view_marks.safety_margin && crate::story::story_of(arena, doc.picture).is_some() {
            let number = |key: &str| {
                arena
                    .get_by_id(doc.picture)
                    .ok()
                    .and_then(|entity| entity.property(key))
                    .and_then(Value::as_f64)
            };
            let mm = number(picture::DPI).unwrap_or(72.0) as f32 / 25.4;
            let bleed =
                number(crate::story::PAGE_BLEED).map_or(3.0 * mm, |value| value as f32 * mm);
            let inset = bleed + SAFETY_MARGIN_MM * mm;
            let (w, h) = (size[0] as f32, size[1] as f32);
            if w > 2.0 * inset && h > 2.0 * inset {
                let corners = [
                    [inset, inset],
                    [w - inset, inset],
                    [w - inset, h - inset],
                    [inset, h - inset],
                ];
                // Dark under light dashes, so it reads on any picture.
                let mut around = corners.map(screen).to_vec();
                around.push(around[0]);
                painter.add(Shape::line(around.clone(), Stroke::new(1.0, theme.void)));
                painter.extend(Shape::dashed_line(
                    &around,
                    Stroke::new(1.0, theme.text),
                    4.0,
                    4.0,
                ));
            }
        }
        if self.view_marks.paths != PathsOn::Off {
            let layers: Vec<EntityId> = match self.view_marks.paths {
                PathsOn::Selected => doc.layer.into_iter().chain(doc.picked.clone()).collect(),
                _ => {
                    let mut out = Vec::new();
                    all_layers(arena, doc.picture, &mut out);
                    out.retain(|layer| {
                        arena
                            .get_by_id(*layer)
                            .is_ok_and(|entity| picture::LayerStyle::of(entity).visible)
                    });
                    out
                }
            };
            let line = Stroke::new(1.0, theme.border_active);
            for layer in layers {
                if !crate::vector::is_vector(arena, layer) {
                    continue;
                }
                let offset = arena
                    .get_by_id(layer)
                    .map_or([0, 0], |entity| picture::LayerStyle::of(entity).offset);
                let shift = |[x, y]: [f32; 2]| screen([x + offset[0] as f32, y + offset[1] as f32]);
                for vector in crate::vector::lines(&crate::vector::geometry(arena, layer)) {
                    let points: Vec<Pos2> = vector
                        .samples
                        .iter()
                        .map(|sample| shift(sample.at))
                        .collect();
                    if points.len() > 1 {
                        painter.add(Shape::line(points, line));
                    }
                    let marks: Vec<usize> = match self.view_marks.points {
                        PointsOn::Off => Vec::new(),
                        PointsOn::Ends => match vector.samples.len() {
                            0 => Vec::new(),
                            1 => vec![0],
                            n => vec![0, n - 1],
                        },
                        PointsOn::All => vector.controls(),
                    };
                    for index in marks {
                        let at = shift(vector.samples[index].at);
                        let rect = Rect::from_center_size(at, vec2(5.0, 5.0));
                        painter.rect_filled(rect, 0.0, theme.panel);
                        painter.rect_stroke(rect, 0.0, line, egui::StrokeKind::Inside);
                    }
                }
            }
        }
        let playing = doc.playing.map(|_| doc.frame);
        self.view_marks.note(cx.view.now, playing);
        if self.view_marks.playback_fps
            && playing.is_some()
            && let Some(rate) = self.view_marks.rate()
        {
            let text = tr!("playback-fps-shown", fps = format!("{rate:.1}"));
            let font = theme.font(theme.small_size);
            let at = viewport.left_top() + vec2(8.0, 8.0);
            let galley = painter.layout_no_wrap(text, font, theme.text);
            let back = Rect::from_min_size(at, galley.size()).expand(4.0);
            painter.rect_filled(back, 3.0, theme.panel.gamma_multiply(0.85));
            painter.galley(at, galley, Color32::PLACEHOLDER);
        }
    }
}

// ----- How the items look -----------------------------------------------------

/// Works while a canvas is open.
fn doc_open(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some())
}

/// The paper layer of the picture in focus.
fn paper(paint: &SuePaint, arena: &Arena) -> Option<doc::LayerInfo> {
    paint.picture().and_then(|picture| {
        doc::layers(arena, picture)
            .into_iter()
            .find(|info| info.kind == LayerKind::Paper)
    })
}

/// A command that turns a switch of the studio on and off: its run and
/// its state.
macro_rules! switch {
    ($run:ident, $state:ident, $($field:ident).+) => {
        fn $run(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
            paint.$($field).+ = !paint.$($field).+;
        }

        fn $state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
            flag(paint.$($field).+)
        }
    };
}

/// A command that turns a switch of the canvas in focus on and off.
macro_rules! doc_switch {
    ($run:ident, $state:ident, $($field:ident).+) => {
        fn $run(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
            if let Some(doc) = paint.doc_mut() {
                doc.$($field).+ = !doc.$($field).+;
            }
        }

        fn $state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
            paint.doc().map_or(ItemState::off(), |doc| flag(doc.$($field).+))
        }
    };
}

switch!(
    transform_launcher,
    transform_launcher_on,
    transform_launcher
);
switch!(text_launcher, text_launcher_on, text_launcher);
switch!(object_launcher, object_launcher_on, object_launcher);
switch!(grid, grid_on, grid);
switch!(pixel_grid, pixel_grid_on, pixel_grid);
switch!(selection, selection_on, show_selection);
switch!(ruler_bar, ruler_bar_on, ruler_bar);
switch!(scroll_bar, scroll_bar_on, scroll_bars);
switch!(snap_grid, snap_grid_on, snap_grid);
switch!(crop_frame, crop_frame_on, crop_frame);
switch!(bleed, bleed_on, bleed);
switch!(page_image, page_image_on, page_image);
switch!(tone_area, tone_area_on, tone_area);
switch!(profile_preview, profile_preview_on, proof_on);
doc_switch!(snap_ruler, snap_ruler_on, snap_ruler);
doc_switch!(snap_special, snap_special_on, ruler_on);

fn flip_h_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .doc()
        .map_or(ItemState::off(), |doc| flag(doc.view.flip_h))
}

fn flip_v_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .doc()
        .map_or(ItemState::off(), |doc| flag(doc.view.flip_v))
}

fn paper_on(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paper(paint, facts.arena).map_or(ItemState::off(), |paper| flag(paper.style.visible))
}

/// Safety Margin: on a page of a story, which has a trim to keep inside.
fn safety_margin_on(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let page = paint
        .picture()
        .is_some_and(|picture| crate::story::story_of(facts.arena, picture).is_some());
    if page {
        flag(paint.view_marks.safety_margin)
    } else {
        ItemState::off()
    }
}

fn paths_selected_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paths_item(paint, PathsOn::Selected)
}

fn paths_all_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paths_item(paint, PathsOn::All)
}

fn paths_item(paint: &SuePaint, which: PathsOn) -> ItemState {
    if paint.doc().is_some() {
        flag(paint.view_marks.paths == which)
    } else {
        ItemState::off()
    }
}

fn points_ends_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    points_item(paint, PointsOn::Ends)
}

fn points_all_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    points_item(paint, PointsOn::All)
}

/// Control points show on the paths shown, so they wait for those.
fn points_item(paint: &SuePaint, which: PointsOn) -> ItemState {
    if paint.doc().is_some() && paint.view_marks.paths != PathsOn::Off {
        flag(paint.view_marks.points == which)
    } else {
        ItemState::off()
    }
}

/// Show Playback FPS: on a picture with a timeline.
fn playback_fps_on(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    let animated = paint
        .picture()
        .is_some_and(|picture| crate::anim::timeline(facts.arena, picture).is_some());
    if animated {
        flag(paint.view_marks.playback_fps)
    } else {
        ItemState::off()
    }
}

/// Toggle Special Ruler Snapping: on a picture with a special ruler,
/// checked while it snaps.
fn special_snap_on(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    match paint.picture() {
        Some(picture) if has_special_ruler(facts.arena, picture) => {
            flag(special_snaps(facts.arena, picture))
        }
        _ => ItemState::off(),
    }
}

// ----- What the items do ------------------------------------------------------

fn rotate_to(paint: &mut SuePaint, cx: &mut AppletContext<'_>, angle: f32) {
    paint.with_view(cx, |view, rect, size| view.set_rotation(rect, size, angle));
}

fn rotate_90(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_to(paint, cx, 90.0);
}

fn rotate_180(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_to(paint, cx, 180.0);
}

fn rotate_270(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_to(paint, cx, 270.0);
}

/// Unturns and unflips the view, and fits it when `fit`.
fn reset_view(paint: &mut SuePaint, cx: &mut AppletContext<'_>, fit: bool) {
    paint.with_view(cx, |view, rect, size| {
        view.set_rotation(rect, size, 0.0);
        if view.flip_h {
            view.flip(rect, size, true);
        }
        if view.flip_v {
            view.flip(rect, size, false);
        }
        if fit {
            view.fit(rect, size);
        }
    });
}

fn reset_turn(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    reset_view(paint, cx, false);
}

fn reset_display(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    reset_view(paint, cx, true);
}

fn paper_toggle(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(paper) = paper(paint, &cx.world.arena) {
        SuePaint::apply(
            cx,
            doc::set(paper.id, picture::VISIBLE, !paper.style.visible),
        );
    }
}

/// Turns the view by the angle Preferences, Canvas gives each turn, to
/// the left when `left`.
fn rotate_step(paint: &mut SuePaint, cx: &mut AppletContext<'_>, left: bool) {
    let step = cx
        .rice()
        .get_i64("sue_paint.rotate_step")
        .unwrap_or(15)
        .clamp(1, 90) as f32;
    let step = if left { -step } else { step };
    paint.with_view(cx, |view, rect, size| view.rotate_by(rect, size, step));
}

fn rotate_left(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_step(paint, cx, true);
}

fn rotate_right(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_step(paint, cx, false);
}

fn rotate_reset(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    rotate_to(paint, cx, 0.0);
}

fn flip_h(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| view.flip(rect, size, true));
}

fn flip_v(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| view.flip(rect, size, false));
}

fn zoom_in(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| {
        view.zoom_at(rect, size, rect.center(), 1.25);
    });
}

fn zoom_out(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| {
        view.zoom_at(rect, size, rect.center(), 0.8);
    });
}

fn zoom_to(paint: &mut SuePaint, cx: &mut AppletContext<'_>, zoom: f32) {
    paint.with_view(cx, |view, rect, size| {
        view.set_zoom_at(rect, size, rect.center(), zoom);
    });
}

fn actual(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    zoom_to(paint, cx, 1.0);
}

fn double(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    zoom_to(paint, cx, 2.0);
}

fn fit(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| view.fit(rect, size));
}

fn profile_settings(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Proof(paint.proof_profile));
}

fn fit_navigator(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.with_view(cx, |view, rect, size| {
        // The canvas fills the window, as the navigator shows it.
        let zoom =
            (rect.width() / size[0].max(1) as f32).max(rect.height() / size[1].max(1) as f32);
        view.set_rotation(rect, size, 0.0);
        view.set_zoom_at(rect, size, rect.center(), zoom);
        let middle = pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
        view.center_on(rect, size, middle);
    });
}

fn print_size(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let dpi = paint
        .picture()
        .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
        .and_then(|entity| entity.property(picture::DPI))
        .and_then(Value::as_f64)
        .unwrap_or(72.0) as f32;
    // A screen shows about 96 points an inch.
    zoom_to(paint, cx, 96.0 / dpi.max(1.0));
}

fn safety_margin(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.view_marks.safety_margin = !paint.view_marks.safety_margin;
}

/// Shows the paths of `which` layers, or none when they show already.
fn show_paths(paint: &mut SuePaint, which: PathsOn) {
    let marks = &mut paint.view_marks;
    marks.paths = if marks.paths == which {
        PathsOn::Off
    } else {
        which
    };
}

fn paths_selected(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    show_paths(paint, PathsOn::Selected);
}

fn paths_all(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    show_paths(paint, PathsOn::All);
}

/// Marks `which` control points, or none when they show already.
fn show_points(paint: &mut SuePaint, which: PointsOn) {
    let marks = &mut paint.view_marks;
    marks.points = if marks.points == which {
        PointsOn::Off
    } else {
        which
    };
}

fn points_ends(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    show_points(paint, PointsOn::Ends);
}

fn points_all(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    show_points(paint, PointsOn::All);
}

fn playback_fps(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.view_marks.playback_fps = !paint.view_marks.playback_fps;
}

/// Turns the picture's special ruler snap off or on again, as an edit.
fn special_snap(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    // Grayed without a special ruler: nothing to switch, no empty step
    // (Auto Actions run a command without asking its item).
    let Some(picture) = paint
        .picture()
        .filter(|picture| has_special_ruler(&cx.world.arena, *picture))
    else {
        return;
    };
    let value = special_snaps(&cx.world.arena, picture).then(|| "off".to_owned());
    let label = tr!("switch-special-ruler-snap");
    SuePaint::apply(
        cx,
        crate::rulers::edit(picture, SPECIAL_SNAP, value, &label),
    );
}

// ----- The table --------------------------------------------------------------

/// A command of this group: recorded by Auto Actions, staying on the
/// canvas, working while a canvas is open unless its state says more.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "view", run)
        .icon(icon)
        .with(Flags::RECORDABLE)
        .state(doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "view.reset_turn",
        "reset-rotation-inversion",
        "reset",
        reset_turn,
    ),
    c(
        "view.rotate_reset",
        "reset-rotation-2",
        "reset",
        rotate_reset,
    )
    .keys("Ctrl+Shift+2"),
    c("view.rotate_90", "rotate-90", "rotate-right", rotate_90),
    c("view.rotate_180", "rotate-180", "rotate-right", rotate_180),
    c("view.rotate_270", "rotate-270", "rotate-left", rotate_270),
    c("view.flip_h", "flip-horizontal", "flip-horizontal", flip_h).state(flip_h_on),
    c("view.flip_v", "flip-vertical", "flip-vertical", flip_v).state(flip_v_on),
    c(
        "view.rotate_left",
        "rotate-left-2",
        "rotate-left",
        rotate_left,
    )
    .keys("-"),
    c(
        "view.rotate_right",
        "rotate-right-2",
        "rotate-right",
        rotate_right,
    )
    .keys("="),
    c("view.zoom_in", "zoom-in-2", "zoom-in", zoom_in).keys("Ctrl++ or Ctrl+= or Ctrl+;"),
    c("view.zoom_out", "zoom-out-2", "zoom-out", zoom_out).keys("Ctrl+-"),
    c("view.actual", "zoom-100-percent", "zoom-actual", actual).keys("Ctrl+Alt+0"),
    c("view.double", "zoom-200-percent", "zoom-in", double),
    c("view.fit", "fit-to-screen", "zoom-fit", fit).keys("Ctrl+0"),
    c(
        "view.reset_display",
        "reset-display",
        "reset",
        reset_display,
    ),
    c(
        "view.fit_navigator",
        "fit-to-navigator",
        "palette-navigator",
        fit_navigator,
    ),
    c("view.print_size", "print-size-2", "print", print_size),
    c("view.scroll_bar", "scroll-bar", "more-vertical", scroll_bar).state(scroll_bar_on),
    c(
        "view.selection",
        "selection-border",
        "tool-select",
        selection,
    )
    .state(selection_on),
    c(
        "view.transform_launcher",
        "transform-launcher",
        "transform",
        transform_launcher,
    )
    .state(transform_launcher_on),
    c(
        "view.text_launcher",
        "text-launcher",
        "tool-text",
        text_launcher,
    )
    .state(text_launcher_on),
    c(
        "view.object_launcher",
        "object-launcher",
        "tool-operation",
        object_launcher,
    )
    .state(object_launcher_on),
    c("view.grid", "grid", "view-grid", grid).state(grid_on),
    c("view.ruler_bar", "ruler-bar", "tool-ruler", ruler_bar)
        .keys("Ctrl+R")
        .state(ruler_bar_on),
    c(
        "view.pixel_grid",
        "pixel-grid",
        "view-grid-small",
        pixel_grid,
    )
    .state(pixel_grid_on),
    c(
        "view.crop_frame",
        "crop-marks-inner-border",
        "crop",
        crop_frame,
    )
    .state(crop_frame_on),
    c("view.bleed", "bleed-2", "page", bleed).state(bleed_on),
    c(
        "view.page_image",
        "show-page-image",
        "page-empty",
        page_image,
    )
    .state(page_image_on),
    c("view.paper", "paper-2", "layer-paper", paper_toggle).state(paper_on),
    c("view.tone_area", "show-tone-area", "tone", tone_area).state(tone_area_on),
    c("view.snap_ruler", "snap-to-ruler", "snap-ruler", snap_ruler)
        .keys("Ctrl+1")
        .state(snap_ruler_on),
    c(
        "view.snap_special",
        "snap-to-special-ruler",
        "snap-ruler",
        snap_special,
    )
    .keys("Ctrl+2")
    .state(snap_special_on),
    c("view.snap_grid", "snap-to-grid", "snap-grid", snap_grid)
        .keys("Ctrl+3")
        .state(snap_grid_on),
    c("view.profile_preview", "preview", "eye", profile_preview).state(profile_preview_on),
    c(
        "view.profile_settings",
        "preview-settings-ellipsis",
        "settings",
        profile_settings,
    ),
    // New in 0.0.8 (gap FGV-51 of the tutorial ledger).
    c(
        "view.safety_margin",
        "safety-margin",
        "doc-print-book",
        safety_margin,
    )
    .state(safety_margin_on),
    c(
        "view.vector_paths.selected",
        "show-vector-paths-on-selected-layers",
        "layer-new-vector",
        paths_selected,
    )
    .state(paths_selected_on),
    c(
        "view.vector_paths.all",
        "show-all-vector-paths-on-vector-layers",
        "layers",
        paths_all,
    )
    .state(paths_all_on),
    c(
        "view.vector_paths.ends",
        "show-control-points-at-beginning-and-end",
        "tool-correct-line",
        points_ends,
    )
    .state(points_ends_on),
    c(
        "view.vector_paths.points",
        "show-all-control-points",
        "tool-correct-line",
        points_all,
    )
    .state(points_all_on),
    c(
        "view.playback_fps",
        "show-playback-fps",
        "play",
        playback_fps,
    )
    .state(playback_fps_on),
    c(
        "view.special_ruler_snapping",
        "toggle-special-ruler-snapping",
        "snap-ruler",
        special_snap,
    )
    .keys("Ctrl+4")
    .state(special_snap_on),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_command_is_recordable_and_stays_on_the_canvas() {
        // RECORDABLE and LEAVES_CANVAS of the group: all recordable, none
        // leaving, as `recordable()` and the runtime test had it.
        for command in COMMANDS {
            assert_eq!(command.group, "view", "{}", command.name);
            assert!(command.flags.has(Flags::RECORDABLE), "{}", command.name);
            assert!(!command.flags.has(Flags::LEAVES_CANVAS), "{}", command.name);
            assert!(crate::extras::recordable(command.name), "{}", command.name);
        }
    }

    #[test]
    fn every_view_command_has_an_icon_of_the_set() {
        for command in COMMANDS {
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{}: no icon {:?}",
                command.name,
                command.icon
            );
        }
    }

    #[test]
    fn the_view_group_keeps_every_name_of_the_ledger() {
        // The names 0.0.75 saved (shortcut settings, Auto Actions, touch
        // keys) are each a command of this group still.
        let ledger = include_str!("../../tests/golden/ledger/paint-commands.txt");
        let names: Vec<&str> = ledger
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("view."))
            .collect();
        assert_eq!(names.len(), 35);
        for name in names {
            assert!(
                COMMANDS.iter().any(|command| command.name == name),
                "{name} left the View group"
            );
        }
    }

    /// Runs `work` with Sue Paint and a context holding one picture of
    /// 200 by 100 with a symmetrical ruler, as the studio sees it.
    fn with_picture(work: impl FnOnce(&mut SuePaint, &mut AppletContext<'_>, EntityId)) {
        let mut rice =
            sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped settings");
        sue_wm::register_rice(&mut rice).expect("the shell's settings register");
        let theme = sue_wm::Theme::from_rice(&rice);
        let mut world = sue_wm::World {
            arena: Arena::new(),
            rice,
        };
        let mut history = sue_core::UndoStack::new();
        let mut typing = sue_wm::Typing::default();
        let mut requests = Vec::new();
        let catalog = sue_wm::Catalog::default();
        let mut textures = sue_platform::NoTextures;
        let mut transport = sue_wm::Transport::default();
        let work_list = sue_wm::WorkList::default();
        let mut transfers = sue_wm::Transfers::default();
        let mut cx = AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: sue_wm::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        let (edit, picture) = doc::create(&crate::doc::NewPicture {
            name: "View".to_owned(),
            width: 200,
            height: 100,
            ..crate::doc::NewPicture::default()
        })
        .expect("a picture");
        assert!(SuePaint::apply(&mut cx, edit));
        let ruler = crate::rulers::edit(
            picture,
            crate::rulers::SYMMETRY,
            Some("100,50,1.5707964".to_owned()),
            "ruler",
        );
        assert!(SuePaint::apply(&mut cx, ruler));
        let mut paint = SuePaint::default();
        paint.sync_docs(&cx.world.arena);
        work(&mut paint, &mut cx, picture);
    }

    /// The item `name` shows now.
    fn item(paint: &SuePaint, cx: &AppletContext<'_>, name: &str) -> ItemState {
        paint.state(name, &Facts::of(cx))
    }

    #[test]
    fn every_view_command_runs_and_each_switch_turns_its_item_over() {
        with_picture(|paint, cx, _| {
            let registry = super::super::registry();
            for command in COMMANDS {
                let before = item(paint, cx, command.name);
                registry.run(paint, cx, command.name);
                let after = item(paint, cx, command.name);
                // A switch that works now shows the other side; the rest
                // keep how they look (the view commands, and the items a
                // plain picture grays: Safety Margin wants a page, Show
                // Playback FPS a timeline, the control points shown
                // paths).
                match before.checked {
                    Some(on) if before.enabled => {
                        assert_eq!(after.checked, Some(!on), "{}", command.name);
                    }
                    _ => assert_eq!(after, before, "{}", command.name),
                }
                // Esc closes what opened (Preview Settings).
                paint.dialog = None;
            }
            for name in ["view.safety_margin", "view.playback_fps"] {
                assert!(!item(paint, cx, name).enabled, "{name}");
            }
        });
    }

    #[test]
    fn the_control_points_follow_the_paths_shown_and_each_pair_is_one_choice() {
        with_picture(|paint, cx, _| {
            let registry = super::super::registry();
            assert!(!item(paint, cx, "view.vector_paths.ends").enabled);
            registry.run(paint, cx, "view.vector_paths.selected");
            assert_eq!(paint.view_marks.paths, PathsOn::Selected);
            registry.run(paint, cx, "view.vector_paths.all");
            assert_eq!(paint.view_marks.paths, PathsOn::All, "one or the other");
            assert!(item(paint, cx, "view.vector_paths.ends").enabled);
            registry.run(paint, cx, "view.vector_paths.ends");
            registry.run(paint, cx, "view.vector_paths.points");
            assert_eq!(paint.view_marks.points, PointsOn::All);
            registry.run(paint, cx, "view.vector_paths.points");
            assert_eq!(paint.view_marks.points, PointsOn::Off, "picked again, off");
            registry.run(paint, cx, "view.vector_paths.all");
            assert_eq!(paint.view_marks.paths, PathsOn::Off);
        });
    }

    #[test]
    fn toggle_special_ruler_snapping_is_an_edit_that_undoes_and_redoes() {
        with_picture(|paint, cx, picture| {
            let state = |cx: &AppletContext<'_>| {
                let mut entities: Vec<String> = cx
                    .world
                    .arena
                    .iter()
                    .map(|(_, entity)| format!("{:?} {:?}", entity.id(), entity.properties()))
                    .collect();
                entities.sort();
                entities
            };
            let before = state(cx);
            assert_eq!(
                item(paint, cx, "view.special_ruler_snapping"),
                ItemState::check(true)
            );
            assert!(special_snaps(&cx.world.arena, picture));
            super::super::registry().run(paint, cx, "view.special_ruler_snapping");
            assert!(!special_snaps(&cx.world.arena, picture));
            assert_eq!(
                item(paint, cx, "view.special_ruler_snapping"),
                ItemState::check(false)
            );
            let after = state(cx);
            assert_ne!(before, after);
            cx.history.undo(cx.world).expect("one step to undo");
            assert_eq!(state(cx), before, "undone, as before, ids and all");
            assert!(special_snaps(&cx.world.arena, picture));
            cx.history.redo(cx.world).expect("one step to redo");
            assert_eq!(state(cx), after, "redone, as after");
            // Without a special ruler the item is grayed.
            let none = crate::rulers::edit(picture, crate::rulers::SYMMETRY, None, "ruler");
            assert!(SuePaint::apply(cx, none));
            assert!(!item(paint, cx, "view.special_ruler_snapping").enabled);
            let depth = cx.history.depth();
            super::super::registry().run(paint, cx, "view.special_ruler_snapping");
            assert_eq!(cx.history.depth(), depth, "grayed, it does nothing");
        });
    }

    #[test]
    fn the_playback_rate_counts_the_frames_shown_over_a_second() {
        let mut marks = ViewMarks::default();
        let start = Instant::now();
        assert_eq!(marks.rate(), None);
        for frame in 0..=24_u32 {
            let at = start + std::time::Duration::from_millis(u64::from(frame) * 1000 / 24);
            marks.note(at, Some(frame));
            // A repaint that shows the same frame again is no new frame.
            marks.note(at, Some(frame));
        }
        let rate = marks.rate().expect("frames were shown");
        assert!((rate - 24.0).abs() < 0.5, "{rate}");
        marks.note(start, None);
        assert_eq!(marks.rate(), None, "stopping forgets the rate");
    }
}
