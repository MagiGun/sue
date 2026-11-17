//! The commands of templates, File Object layers, palette colors shown
//! round the canvas, Webtoon Display Settings, the Material palette's
//! folders, and the steps Auto Actions record from the canvas and the
//! Layer palette (a stroke, a layer dragged to another place).

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{flag, when};
use crate::dialogs::Dialog;
use crate::studio::SuePaint;

/// Show each layer's palette color in the row of the folder it is in
/// (`Bool`; Layer, Layer Settings).
pub(crate) const PALETTE_IN_FOLDER: &str = "sue_paint.palette_in_folder";
/// Show each layer's palette color on the canvas: its drawing shows in
/// that color, the picture itself unchanged (`Bool`; Layer, Layer
/// Settings).
pub(crate) const PALETTE_ON_CANVAS: &str = "sue_paint.palette_on_canvas";

/// The family that plays a recorded stroke: what follows is the sub tool
/// command, then each point as `x,y,pressure` in picture pixels.
pub(crate) const STROKE_PREFIX: &str = "auto.stroke.";
/// The family that moves the current layer up (more than 0) or down by
/// so many places: a layer dragged in the Layer palette, recorded.
pub(crate) const MOVE_BY_PREFIX: &str = "layer.move_by.";
/// The family that shows the Material palette at one folder, by the
/// kind's id: one per folder, as Window, Material lists them.
pub(crate) const FOLDER_PREFIX: &str = "material.folder.";

fn has_canvas(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.picture().is_some())
}

/// Register Material, Template: asks for its name.
fn register_template(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        cx.fail(tr!("open-make-canvas-first"));
        return;
    };
    let name = cx
        .world
        .arena
        .get_by_id(picture)
        .map_or_else(|_| tr!("template"), |entity| entity.name().to_owned());
    paint.dialog = Some(Dialog::Template(name));
}

/// New Layer, File Object: asks which picture file the layer shows.
fn new_file_object(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pick_file_object(cx);
}

/// File Object, Update: the layer shows its file as it is now.
fn update_file_object(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.update_file_object(cx);
}

fn on_file_object(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .layer()
            .is_some_and(|layer| SuePaint::is_file_object(facts.arena, layer)),
    )
}

/// The two palette color switches of Layer Settings: each a setting,
/// undone like any setting.
fn palette_switch(_: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let key = if call.name == "layer.palette_on_canvas" {
        PALETTE_ON_CANVAS
    } else {
        PALETTE_IN_FOLDER
    };
    let on = cx.rice().get_bool(key).unwrap_or(false);
    if let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
        key,
        sue_rice::RiceValue::Bool(!on),
    )) {
        cx.fail(error.to_string());
    }
    // The canvas and the Layer palette follow on the next frame.
}

fn palette_switch_state(_: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let key = if call.name == "layer.palette_on_canvas" {
        PALETTE_ON_CANVAS
    } else {
        PALETTE_IN_FOLDER
    };
    flag(facts.rice.get_bool(key).unwrap_or(false))
}

/// View, Webtoon Display Settings.
fn webtoon_display(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.open_webtoon_display(cx);
}

/// View, Fit Width: the canvas as wide as the window, its top showing.
fn fit_width(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.fit_width(cx);
}

/// Window, Material, one folder: the Material palette at that folder.
fn material_folder(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    paint.show_material_folder(call.arg);
}

fn material_folder_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    let known = paint.material_kinds.get(call.arg).is_some();
    let mut state = when(known);
    state.checked = Some(
        paint.palette_showing(crate::dock::Palette::Material)
            && matches!(&paint.extras.material.folder, crate::hub::Node::Kind(kind) if kind == call.arg),
    );
    state
}

/// A recorded stroke, drawn again.
fn stroke(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.play_stroke(call.arg, cx);
}

/// A recorded layer drag: the current layer moves by as many places.
fn move_by(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Ok(by) = call.arg.parse::<i64>() else {
        return;
    };
    paint.move_layer_by(cx, by);
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    Command::new(
        "edit.material.template",
        "template-ellipsis",
        "edit",
        register_template,
    )
    .icon("template")
    .state(has_canvas),
    Command::new(
        "layer.new.file_object",
        "file-object-ellipsis",
        "layer",
        new_file_object,
    )
    .icon("link")
    .state(has_canvas)
    .with(Flags::LEAVES_CANVAS),
    Command::new(
        "layer.file_object.update",
        "update-file-object",
        "layer",
        update_file_object,
    )
    .icon("reload")
    .state(on_file_object)
    .with(Flags::RECORDABLE),
    Command::new(
        "layer.palette_in_folder",
        "display-palette-colors-in-parent-folder",
        "layer",
        palette_switch,
    )
    .state(palette_switch_state),
    Command::new(
        "layer.palette_on_canvas",
        "display-palette-colors-on-canvas",
        "layer",
        palette_switch,
    )
    .state(palette_switch_state),
    Command::new(
        "view.webtoon_display",
        "webtoon-display-settings-ellipsis",
        "view",
        webtoon_display,
    )
    .state(has_canvas),
    Command::new("view.fit_width", "fit-width", "view", fit_width)
        .icon("zoom-fit")
        .state(has_canvas)
        .with(Flags::RECORDABLE),
    Command::family(FOLDER_PREFIX, "material-folder", "window", material_folder)
        .icon("materials")
        .state(material_folder_state),
    Command::family(STROKE_PREFIX, "auto-stroke", "window", stroke)
        .state(has_canvas)
        .with(Flags::RECORDABLE.with(Flags::HIDDEN)),
    Command::family(MOVE_BY_PREFIX, "auto-move-layer", "layer", move_by)
        .state(has_canvas)
        .with(Flags::RECORDABLE.with(Flags::HIDDEN)),
];
