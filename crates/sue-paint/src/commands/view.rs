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

use egui::pos2;
use sue_core::{Arena, Value};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::studio::SuePaint;

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
];

#[cfg(test)]
mod tests {
    use sue_core::EntityId;
    use sue_platform::time::Instant;

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
                // (the view commands) keep how they look.
                match before.checked {
                    Some(on) if before.enabled => {
                        assert_eq!(after.checked, Some(!on), "{}", command.name);
                    }
                    _ => assert_eq!(after, before, "{}", command.name),
                }
                // Esc closes what opened (Preview Settings).
                paint.dialog = None;
            }
        });
    }
}
