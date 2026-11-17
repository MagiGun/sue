//! The commands of the palettes that keep work at hand: Sub View,
//! Information, Item Bank, Auto Action and Material. Their palettes' menus
//! and buttons run them, so each can be given a key in Settings like any
//! menu item.
//!
//! Only Paste to Canvas is recorded by Auto Actions: the others change a
//! palette, not the picture. What changes the picture (Paste to Canvas,
//! an Item Bank picture dropped on the canvas) goes through the
//! document's history, one undo step each. The commands of the family
//! `auto.run.` play an Auto Action by its id; Settings lists one row per
//! action for them.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{flag, when};
use crate::panels::Unit;
use crate::studio::SuePaint;

/// A command of these palettes: in the Window group, never recorded.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "window", run).icon(icon)
}

// ----- Sub View ----------------------------------------------------------------

fn sub_view_import(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view_import(cx);
}

fn sub_view_previous(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.step(-1);
}

fn sub_view_next(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.step(1);
}

fn sub_view_list(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.list = !paint.sub_view.list;
}

fn sub_view_eyedropper(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.eyedropper = !paint.sub_view.eyedropper;
}

fn sub_view_flip_h(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.flip_h = !paint.sub_view.flip_h;
}

fn sub_view_flip_v(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.flip_v = !paint.sub_view.flip_v;
}

fn sub_view_fit(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.fit();
}

fn sub_view_remove(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.remove_current();
}

fn sub_view_show_bar(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.show_bar = !paint.sub_view.show_bar;
}

fn sub_view_show_turn(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.sub_view.show_turn = !paint.sub_view.show_turn;
}

fn sub_view_any(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(!paint.sub_view.images.is_empty())
}

fn sub_view_many(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.sub_view.images.len() > 1)
}

fn sub_view_list_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState {
        enabled: !paint.sub_view.images.is_empty(),
        checked: Some(paint.sub_view.list),
    }
}

fn sub_view_eyedropper_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.sub_view.eyedropper)
}

fn sub_view_flip_h_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.sub_view.flip_h)
}

fn sub_view_flip_v_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.sub_view.flip_v)
}

fn sub_view_show_bar_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.sub_view.show_bar)
}

fn sub_view_show_turn_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.sub_view.show_turn)
}

// ----- Information -------------------------------------------------------------

fn info_canvas(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.set_info(cx, |options| options.canvas = !options.canvas);
}

fn info_canvas_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.extras.info.canvas)
}

fn info_memory(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.set_info(cx, |options| options.memory = !options.memory);
}

fn info_coordinates(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.set_info(cx, |options| options.coordinates = !options.coordinates);
}

fn info_length(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.set_info(cx, |options| options.length = !options.length);
}

/// The unit a `info.unit_<word>` command picks.
fn unit_of(call: Call<'_>) -> Unit {
    call.name
        .strip_prefix("info.unit_")
        .and_then(Unit::from_word)
        .unwrap_or_default()
}

fn info_unit(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let unit = unit_of(call);
    paint.set_info(cx, |options| options.unit = unit);
}

fn info_memory_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.extras.info.memory)
}

fn info_coordinates_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.extras.info.coordinates)
}

fn info_length_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.extras.info.length)
}

fn info_unit_on(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.extras.info.unit == unit_of(call))
}

// ----- Item Bank ---------------------------------------------------------------

fn bank_keep_sub_tool(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.bank_keep_sub_tool(cx);
}

fn bank_add_picture(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.bank_pick_picture(cx);
}

fn bank_keep_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.bank_keep_layer(cx);
}

fn bank_layer_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.layer_has_pixels(facts.arena))
}

// ----- Auto Action -------------------------------------------------------------

fn auto_record(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_record(cx);
}

fn auto_play(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.play_auto(paint.auto.selected, cx);
}

fn auto_new(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_new(cx);
}

fn auto_delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_delete(cx);
}

fn auto_rename(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    let index = paint.auto.selected;
    if let Some(action) = paint.auto.sets.get(index) {
        paint.auto.renaming = Some((index, action.name.clone()));
    }
}

fn auto_duplicate(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_duplicate(cx);
}

fn auto_new_set(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_new_set(cx);
}

fn auto_rename_set(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto.renaming_set = Some(paint.auto.set_name.clone());
}

fn auto_delete_set(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_delete_set(cx);
}

fn auto_import_set(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_file(false, cx);
}

fn auto_export_set(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_file(true, cx);
}

fn auto_restore(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.auto_restore(cx);
}

fn auto_run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.play_auto_id(call.arg, cx);
}

fn auto_recording(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.auto.recording.is_some())
}

fn auto_chosen(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.auto.sets.get(paint.auto.selected).is_some())
}

fn auto_playable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint.auto.recording.is_none()
            && paint
                .auto
                .sets
                .get(paint.auto.selected)
                .is_some_and(|action| !action.off && !action.steps.is_empty()),
    )
}

fn auto_many_sets(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(!paint.auto.others.is_empty())
}

fn auto_run_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    when(paint.auto.steps_of(call.arg).is_some())
}

// ----- Material ----------------------------------------------------------------

fn material_paste(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.paste_picked_materials(cx);
}

fn material_favorite(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_material_favorite(cx);
}

fn material_delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.delete_picked_materials(cx);
}

fn material_size(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    if let Some(size) = crate::panels::ThumbSize::of_command(call.name) {
        paint.extras.material.size = size;
    }
}

fn material_picked(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(!paint.extras.material.picked.is_empty() && paint.doc().is_some())
}

fn material_favorite_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState {
        enabled: !paint.extras.material.picked.is_empty(),
        checked: Some(paint.material_picked_favorite()),
    }
}

fn material_removable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.material_picked_removable())
}

fn material_size_on(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(crate::panels::ThumbSize::of_command(call.name) == Some(paint.extras.material.size))
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    // Sub View.
    c(
        "subview.import",
        "sub-view-import",
        "import",
        sub_view_import,
    )
    .with(Flags::LEAVES_CANVAS),
    c(
        "subview.previous",
        "sub-view-previous",
        "chevron-left",
        sub_view_previous,
    )
    .state(sub_view_many),
    c(
        "subview.next",
        "sub-view-next",
        "chevron-right",
        sub_view_next,
    )
    .state(sub_view_many),
    c(
        "subview.list",
        "sub-view-list",
        "picture-list",
        sub_view_list,
    )
    .state(sub_view_list_on),
    c(
        "subview.eyedropper",
        "sub-view-eyedropper",
        "tool-eyedropper",
        sub_view_eyedropper,
    )
    .state(sub_view_eyedropper_on),
    c(
        "subview.flip_h",
        "sub-view-flip-h",
        "flip-horizontal",
        sub_view_flip_h,
    )
    .state(sub_view_flip_h_on),
    c(
        "subview.flip_v",
        "sub-view-flip-v",
        "flip-vertical",
        sub_view_flip_v,
    )
    .state(sub_view_flip_v_on),
    c("subview.fit", "sub-view-fit", "zoom-fit", sub_view_fit).state(sub_view_any),
    c(
        "subview.remove",
        "sub-view-remove",
        "delete",
        sub_view_remove,
    )
    .state(sub_view_any),
    c(
        "subview.show_bar",
        "sub-view-show-bar",
        "palette-command-bar",
        sub_view_show_bar,
    )
    .state(sub_view_show_bar_on),
    c(
        "subview.show_turn",
        "sub-view-show-turn",
        "rotate-right",
        sub_view_show_turn,
    )
    .state(sub_view_show_turn_on),
    // Information.
    c("info.canvas", "info-show-canvas", "info", info_canvas).state(info_canvas_on),
    c("info.memory", "info-show-memory", "info", info_memory).state(info_memory_on),
    c(
        "info.coordinates",
        "info-show-coordinates",
        "info",
        info_coordinates,
    )
    .state(info_coordinates_on),
    c("info.length", "info-show-length", "info", info_length).state(info_length_on),
    c("info.unit_px", "info-unit-px", "info", info_unit).state(info_unit_on),
    c("info.unit_mm", "info-unit-mm", "info", info_unit).state(info_unit_on),
    c("info.unit_cm", "info-unit-cm", "info", info_unit).state(info_unit_on),
    c("info.unit_in", "info-unit-in", "info", info_unit).state(info_unit_on),
    // Item Bank.
    c(
        "item_bank.keep_sub_tool",
        "keep-current-sub-tool",
        "plus",
        bank_keep_sub_tool,
    ),
    c(
        "item_bank.add_picture",
        "item-bank-add-picture",
        "import",
        bank_add_picture,
    )
    .with(Flags::LEAVES_CANVAS),
    c(
        "item_bank.keep_layer",
        "item-bank-keep-layer",
        "picture",
        bank_keep_layer,
    )
    .state(bank_layer_state),
    // Auto Action.
    c("auto.record", "record", "radio-on", auto_record).state(auto_recording),
    c("auto.play", "play", "play", auto_play).state(auto_playable),
    c("auto.new", "auto-new-action", "plus", auto_new),
    c("auto.delete", "auto-delete", "delete", auto_delete).state(auto_chosen),
    c("auto.rename", "auto-rename", "rename", auto_rename).state(auto_chosen),
    c(
        "auto.duplicate",
        "auto-duplicate",
        "duplicate",
        auto_duplicate,
    )
    .state(auto_chosen),
    c("auto.new_set", "auto-new-set", "folder-new", auto_new_set),
    c(
        "auto.rename_set",
        "auto-rename-set",
        "rename",
        auto_rename_set,
    ),
    c(
        "auto.delete_set",
        "auto-delete-set",
        "delete",
        auto_delete_set,
    )
    .state(auto_many_sets),
    c(
        "auto.import_set",
        "auto-import-set-ellipsis",
        "import",
        auto_import_set,
    )
    .with(Flags::LEAVES_CANVAS),
    c(
        "auto.export_set",
        "auto-export-set-ellipsis",
        "export",
        auto_export_set,
    )
    .with(Flags::LEAVES_CANVAS),
    c(
        "auto.restore_defaults",
        "auto-restore-defaults",
        "reset",
        auto_restore,
    ),
    Command::family("auto.run.", "auto-run", "window", auto_run)
        .icon("play")
        .state(auto_run_state)
        .with(Flags::HIDDEN),
    // Material.
    c("material.paste", "paste-to-canvas", "paste", material_paste)
        .state(material_picked)
        .with(Flags::RECORDABLE),
    c(
        "material.favorite",
        "material-favorite",
        "star",
        material_favorite,
    )
    .state(material_favorite_on),
    c(
        "material.delete",
        "delete-material",
        "delete",
        material_delete,
    )
    .state(material_removable),
    c(
        "material.size_small",
        "material-size-small",
        "view-grid-small",
        material_size,
    )
    .state(material_size_on),
    c(
        "material.size_large",
        "material-size-large",
        "view-grid",
        material_size,
    )
    .state(material_size_on),
    c(
        "material.size_list",
        "material-size-list",
        "view-list",
        material_size,
    )
    .state(material_size_on),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palettes_menus_name_registered_commands() {
        let registry = super::super::registry();
        for name in crate::panels::NAVIGATOR_COMMANDS
            .iter()
            .chain(crate::panels::SUB_VIEW_COMMANDS.iter())
            .chain(crate::panels::INFO_COMMANDS.iter())
            .chain(crate::panels::ITEM_BANK_COMMANDS.iter())
            .chain(crate::panels::AUTO_ACTION_COMMANDS.iter())
            .chain(crate::panels::MATERIAL_COMMANDS.iter())
        {
            assert!(registry.has(name), "{name}");
        }
        // Only Paste to Canvas changes the picture, so only it is
        // recorded.
        for command in COMMANDS {
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                command.name == "material.paste",
                "{}",
                command.name
            );
        }
    }
}
