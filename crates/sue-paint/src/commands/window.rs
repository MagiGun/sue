//! The Window menu's commands: canvases, workspaces, the interface and
//! the palettes.
//!
//! Each command is one entry of [`COMMANDS`] with its own `run` and
//! `state`, its label, its default keys, its icon and its flags. No
//! Window command changes a document: they show and hide palettes, move
//! between canvases and keep workspaces, so none has an undo step.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{doc_open, flag, when};
use crate::dialogs::Dialog;
use crate::dock::{Dock, Palette};
use crate::studio::{Doc, Panels, SuePaint};

/// The commands of this group that Auto Actions may record: none. They
/// change how the studio looks, not the picture, so a recorded action
/// that replays them would only move palettes about (`extras::recordable`
/// has always left every `window.` command out).
pub(super) const RECORDABLE: [&str; 0] = [];

/// The commands of this group that go outside the canvas, so a test that
/// runs every command on a canvas skips them: New Window opens a second
/// view and tiles the canvases, and each interface choice changes the
/// setting `display.interface`, which lays the whole studio out anew.
pub(super) const LEAVES_CANVAS: [&str; 5] = [
    "window.new",
    "window.interface.auto",
    "window.interface.desktop",
    "window.interface.tablet",
    "window.interface.phone",
];

/// `true` when `a` and `b` are the same text; usable in a `const`.
const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// `true` when `list` holds `name`; usable in a `const`.
const fn listed(list: &[&str], name: &str) -> bool {
    let mut at = 0;
    while at < list.len() {
        if same(list[at], name) {
            return true;
        }
        at += 1;
    }
    false
}

/// The flags `name` takes from [`RECORDABLE`] and [`LEAVES_CANVAS`], so
/// the lists and the table can never say different things.
const fn flags_of(name: &str) -> Flags {
    let mut flags = Flags::NONE;
    if listed(&RECORDABLE, name) {
        flags = flags.with(Flags::RECORDABLE);
    }
    if listed(&LEAVES_CANVAS, name) {
        flags = flags.with(Flags::LEAVES_CANVAS);
    }
    flags
}

/// A command of this group: its name, label key, icon, `run` and `state`.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
    state: fn(&SuePaint, &Facts<'_>, Call<'_>) -> ItemState,
) -> Command<SuePaint> {
    Command::new(name, label, "window", run)
        .icon(icon)
        .state(state)
        .with(flags_of(name))
}

/// A family of this group: every name that goes on from `prefix`.
const fn family(
    prefix: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
    state: fn(&SuePaint, &Facts<'_>, Call<'_>) -> ItemState,
) -> Command<SuePaint> {
    Command::family(prefix, label, "window", run)
        .icon(icon)
        .state(state)
        .with(flags_of(prefix))
}

/// Shows the tab `wanted` of a palette with tabs, or hides the palette
/// when that tab is already the one shown.
fn toggle_tab(shown: &mut bool, tab: &mut u8, wanted: u8) {
    if *shown && *tab == wanted {
        *shown = false;
    } else {
        *shown = true;
        *tab = wanted;
    }
}

// Canvases.

/// Next Canvas: the canvas after the current one, round to the first.
fn next_tab(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    let count = paint.docs.len().max(1);
    paint.active = (paint.active + 1) % count;
}

/// Previous Canvas: the canvas before the current one, round to the last.
fn prev_tab(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    let count = paint.docs.len().max(1);
    paint.active = (paint.active + count - 1) % count;
}

/// Next and Previous Canvas work while there is another canvas to go to.
fn more_than_one(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.docs.len() > 1)
}

/// The open canvas a member of `window.canvas.` names, counted from 0.
fn canvas_index(paint: &SuePaint, call: Call<'_>) -> Option<usize> {
    call.arg
        .parse::<usize>()
        .ok()
        .filter(|&index| index < paint.docs.len())
}

/// One canvas of the Window, Canvas list: it becomes the current one.
fn canvas(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    if let Some(index) = canvas_index(paint, call) {
        paint.active = index;
    }
}

/// A canvas of the list is checked while it is the current one, and
/// grayed when no canvas has that number.
fn canvas_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    match canvas_index(paint, call) {
        Some(index) => flag(index == paint.active),
        None => ItemState::off(),
    }
}

/// New Window: a second view of the current canvas, at its own place
/// in the tabs, with the canvases tiled.
fn new_window(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc() {
        let mut view = Doc::new(doc.picture);
        view.view = doc.view;
        view.layer = doc.layer;
        view.frame = doc.frame;
        view.track = doc.track;
        paint.docs.insert(paint.active + 1, view);
        paint.active += 1;
        paint.tiled = true;
    }
}

/// Cascade: one canvas at a time, in tabs.
fn cascade(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.tiled = false;
}

/// Cascade works while the canvases are tiled.
fn cascade_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.tiled)
}

/// Tile: every canvas side by side.
fn tile(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.tiled = true;
}

/// Tile is checked while the canvases are tiled.
fn tile_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.tiled)
}

// Workspaces.

/// Register Workspace...: asks for a name for the palettes as they are.
fn workspace_save(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Workspace(tr!(
        "workspace-2",
        value = paint.workspaces.len() + 1
    )));
}

/// Manage Workspaces...: the list of registered workspaces.
fn workspace_manage(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Workspaces);
}

/// Use Current Workspace as Default.
fn workspace_default(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.set_default_workspace(cx);
}

/// Reset to Default: the workspace people chose as the default (Use
/// Current Workspace as Default), or Sue Paint's own when there is none
/// or the palettes already are as it says.
fn workspace_reset(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    let saved = paint
        .default_workspace
        .clone()
        .filter(|space| space.lines != paint.workspace_now("").lines);
    if let Some(space) = saved {
        paint.apply_layout(&space.lines);
    } else {
        paint.dock = Dock::default();
        paint.panels = Panels::default();
        paint.menus_hidden = false;
    }
}

/// One registered workspace of Window, Workspace: its palettes come back.
fn workspace_use(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    if let Ok(index) = call.arg.parse::<usize>() {
        paint.use_workspace(index);
    }
}

// The interface.

/// The interface a `window.interface.` command chooses: `auto`,
/// `desktop`, `tablet` or `phone`.
fn chosen_interface(call: Call<'_>) -> &'static str {
    match call.name {
        "window.interface.desktop" => "desktop",
        "window.interface.tablet" => "tablet",
        "window.interface.phone" => "phone",
        _ => "auto",
    }
}

/// Automatic, Desktop, Tablet or Phone: the setting `display.interface`.
fn interface(_: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let chosen = chosen_interface(call).to_owned();
    match cx.edit_rice(sue_rice::SetRiceValue::new(
        "display.interface",
        sue_rice::RiceValue::Text(chosen),
    )) {
        Ok(_) => cx.notify(tr!("interface-changes-now-window-interface-changes")),
        Err(error) => cx.fail(tr!("cannot-change-the-interface", error = error)),
    }
}

/// The interface in use is checked.
fn interface_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.interface == chosen_interface(call))
}

// Showing and hiding.

/// Hide All Palettes: every palette but the command bar goes, or comes
/// back when none is shown.
fn hide_all(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    let show = !paint.any_panel();
    let command_bar = paint.panels.command_bar;
    paint.panels = Panels {
        command_bar,
        tool: show,
        sub_tool: show,
        tool_property: show,
        brush_size: show,
        color: show,
        color_set: show,
        navigator: show,
        layer: show,
        timeline: show,
    };
}

/// Hide All Palettes is checked while no palette is shown.
fn hide_all_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(!paint.any_panel())
}

/// Hide Menu Bar.
fn hide_menus(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.menus_hidden = !paint.menus_hidden;
}

// The palettes. Each palette's item shows it or hides it; a palette that
// is a tab of another shows that palette at its tab, and hides it when
// the tab is already the one shown.

/// Command Bar.
fn command_bar(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.command_bar = !paint.panels.command_bar;
}

/// Command Bar is checked while shown.
fn command_bar_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.command_bar)
}

/// Tool.
fn tool(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.tool = !paint.panels.tool;
}

/// Tool is checked while shown.
fn tool_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.tool)
}

/// Sub Tool.
fn sub_tool(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.sub_tool = !paint.panels.sub_tool;
}

/// Sub Tool is checked while shown.
fn sub_tool_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.sub_tool)
}

/// Tool Property.
fn tool_property(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.tool_property = !paint.panels.tool_property;
}

/// Tool Property is checked while shown.
fn tool_property_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.tool_property)
}

/// The brush size sliders.
fn brush_size(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.brush_size = !paint.panels.brush_size;
}

/// The brush size sliders are checked while shown.
fn brush_size_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.brush_size)
}

/// Brush Size: the grid of sizes, a tab of the Sub Tool palette.
fn brush_grid(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.sub_tool = true;
    paint.tool_tab = 2;
}

/// Brush Size is checked while its tab is shown.
fn brush_grid_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.sub_tool && paint.tool_tab == 2)
}

/// The tab of the color palette a command shows: the wheel, the
/// sliders, the intermediate colors or the approximate colors.
fn color_tab_of(call: Call<'_>) -> u8 {
    match call.name {
        "window.color_slider" => 1,
        "window.intermediate" => 2,
        "window.approximate" => 3,
        _ => 0,
    }
}

/// Color Wheel, Color Slider, Intermediate Color, Approximate Color.
fn color(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    toggle_tab(
        &mut paint.panels.color,
        &mut paint.color_tab,
        color_tab_of(call),
    );
}

/// A color palette's item is checked while its tab is shown.
fn color_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.panels.color && paint.color_tab == color_tab_of(call))
}

/// The tab of the color set palette a command shows: the set, the
/// history or the materials.
fn set_tab_of(call: Call<'_>) -> u8 {
    match call.name {
        "window.color_history" => 1,
        "window.material" => 2,
        _ => 0,
    }
}

/// Color Set, Color History, Material.
fn color_set(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    toggle_tab(
        &mut paint.panels.color_set,
        &mut paint.set_tab,
        set_tab_of(call),
    );
}

/// A color set palette's item is checked while its tab is shown.
fn color_set_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.panels.color_set && paint.set_tab == set_tab_of(call))
}

/// Navigator.
fn navigator(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.navigator = !paint.panels.navigator;
}

/// Navigator is checked while shown.
fn navigator_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.navigator)
}

/// Sub View: a tab of the Navigator palette.
fn sub_view(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.navigator = true;
    paint.nav_tab = 1;
}

/// Sub View is checked while its tab is shown.
fn sub_view_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.navigator && paint.nav_tab == 1)
}

/// Layer.
fn layer(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    toggle_tab(&mut paint.panels.layer, &mut paint.layer_tab, 0);
}

/// The tab of the Layer palette a command shows.
fn layer_tab_of(call: Call<'_>) -> u8 {
    match call.name {
        "window.layer_property" => 1,
        "window.history" => 2,
        "window.cels" => 3,
        _ => 0,
    }
}

/// Layer Property, History, Animation Cels: tabs of the Layer palette,
/// shown (a second run leaves them shown).
fn layer_palette_tab(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    paint.panels.layer = true;
    paint.layer_tab = layer_tab_of(call);
}

/// A tab of the Layer palette is checked while it is the one shown.
fn layer_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.panels.layer && paint.layer_tab == layer_tab_of(call))
}

/// Timeline.
fn timeline(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.timeline = !paint.panels.timeline;
}

/// Timeline is checked while shown.
fn timeline_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.panels.timeline)
}

/// The palette of its own a command opens.
fn own_palette(call: Call<'_>) -> Palette {
    match call.name {
        "window.info" => Palette::Info,
        "window.auto_actions" => Palette::AutoAction,
        _ => Palette::ItemBank,
    }
}

/// Auto Action, Item Bank, Information: palettes of their own, opened
/// from the strip of icons.
fn open_own(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    let palette = own_palette(call);
    if !paint.dock.has(palette) {
        paint.dock.iconize(palette);
    }
    if paint.dock.iconized.contains(&palette) {
        paint.dock.popped = Some(palette);
    }
    paint.dock.choose(palette);
}

/// A palette of its own is checked while the dock has it.
fn own_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.dock.has(own_palette(call)))
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "window.next_tab",
        "next-canvas",
        "chevron-right",
        next_tab,
        more_than_one,
    )
    .keys("Ctrl+Tab"),
    c(
        "window.prev_tab",
        "previous-canvas",
        "chevron-left",
        prev_tab,
        more_than_one,
    )
    .keys("Ctrl+Shift+Tab"),
    c(
        "window.new",
        "new-window",
        "duplicate",
        new_window,
        doc_open,
    ),
    c(
        "window.cascade",
        "cascade",
        "window-restore",
        cascade,
        cascade_state,
    ),
    c("window.tile", "tile", "view-grid", tile, tile_state),
    c(
        "window.workspace.save",
        "register-workspace-ellipsis",
        "save",
        workspace_save,
        doc_open,
    ),
    c(
        "window.workspace.manage",
        "manage-workspaces-ellipsis",
        "settings",
        workspace_manage,
        doc_open,
    ),
    c(
        "window.workspace.default",
        "use-current-workspace-as-default",
        "check",
        workspace_default,
        doc_open,
    ),
    c(
        "window.workspace.reset",
        "reset-workspace-to-default",
        "reset",
        workspace_reset,
        doc_open,
    ),
    c(
        "window.interface.auto",
        "automatic-follows-the-device",
        "layout-style",
        interface,
        interface_state,
    ),
    c(
        "window.interface.desktop",
        "desktop",
        "layout-style",
        interface,
        interface_state,
    ),
    c(
        "window.interface.tablet",
        "tablet",
        "layout-style",
        interface,
        interface_state,
    ),
    c(
        "window.interface.phone",
        "phone",
        "layout-style",
        interface,
        interface_state,
    ),
    c(
        "window.hide_all",
        "hide-all-palettes",
        "eye-off",
        hide_all,
        hide_all_state,
    )
    .keys("Tab"),
    c(
        "window.hide_menus",
        "hide-menu-bar",
        "menu",
        hide_menus,
        doc_open,
    )
    .keys("Shift+Tab"),
    c(
        "window.command_bar",
        "command-bar",
        "palette-command-bar",
        command_bar,
        command_bar_state,
    ),
    c("window.tool", "tool", "palette-tools", tool, tool_state),
    c(
        "window.sub_tool",
        "tool-group",
        "palette-sub-tools",
        sub_tool,
        sub_tool_state,
    ),
    c(
        "window.tool_property",
        "tool-settings",
        "palette-tool-property",
        tool_property,
        tool_property_state,
    ),
    c(
        "window.brush_size",
        "tool-sliders",
        "palette-brush-size",
        brush_size,
        brush_size_state,
    ),
    c(
        "window.brush_grid",
        "brush-size",
        "palette-brush-grid",
        brush_grid,
        brush_grid_state,
    ),
    c(
        "window.color_wheel",
        "color-wheel",
        "palette-color-wheel",
        color,
        color_state,
    ),
    c(
        "window.color_slider",
        "color-slider",
        "palette-color-sliders",
        color,
        color_state,
    ),
    c(
        "window.color_set",
        "color-set",
        "palette-color-set",
        color_set,
        color_set_state,
    ),
    c(
        "window.color_history",
        "color-history",
        "palette-color-history",
        color_set,
        color_set_state,
    ),
    c(
        "window.intermediate",
        "intermediate-color",
        "palette-intermediate",
        color,
        color_state,
    ),
    c(
        "window.approximate",
        "approximate-color",
        "palette-approximate",
        color,
        color_state,
    ),
    c(
        "window.navigator",
        "navigator",
        "palette-navigator",
        navigator,
        navigator_state,
    ),
    c(
        "window.sub_view",
        "sub-view",
        "palette-sub-view",
        sub_view,
        sub_view_state,
    ),
    c("window.layer", "layer-2", "layers", layer, layer_state),
    c(
        "window.layer_property",
        "layer-properties",
        "layer-property",
        layer_palette_tab,
        layer_state,
    ),
    c(
        "window.history",
        "history",
        "palette-history",
        layer_palette_tab,
        layer_state,
    ),
    c(
        "window.timeline",
        "timeline",
        "palette-timeline",
        timeline,
        timeline_state,
    ),
    c(
        "window.cels",
        "animation-cels",
        "palette-cels",
        layer_palette_tab,
        layer_state,
    ),
    c(
        "window.material",
        "material",
        "palette-materials",
        color_set,
        color_set_state,
    ),
    c(
        "window.auto_actions",
        "auto-action-palette",
        "palette-auto-action",
        open_own,
        own_state,
    ),
    c(
        "window.item_bank",
        "item-bank",
        "palette-item-bank",
        open_own,
        own_state,
    ),
    c(
        "window.info",
        "information",
        "palette-info",
        open_own,
        own_state,
    ),
    family(
        "window.workspace.use.",
        "workspace",
        "layout-style",
        workspace_use,
        doc_open,
    ),
    // The numbered list of open canvases under Window, Canvas, as Clip
    // Studio Paint has it (gap CC-6): `window.canvas.0` is the first.
    // Hidden until the menu lists the canvases (P3-2 builds that list and
    // then drops the flag); Auto Actions and scripts can run it already.
    family(
        "window.canvas.",
        "go-to-canvas",
        "picture",
        canvas,
        canvas_state,
    )
    .with(Flags::HIDDEN),
];

#[cfg(test)]
mod tests {
    use sue_core::{Arena, EntityId, UndoStack};
    use sue_rice::RiceSchema;
    use sue_wm::icons::Icon;
    use sue_wm::{Theme, World};

    use super::*;
    use crate::commands::registry;

    /// Runs `work` with an applet context over an empty world, and hands
    /// back the world and the history it ran on.
    fn with_context<R>(
        work: impl FnOnce(&mut AppletContext<'_>) -> R,
    ) -> (R, World, UndoStack<World>) {
        let rice = RiceSchema::new();
        let theme = Theme::from_rice(&rice);
        let mut world = World {
            arena: Arena::new(),
            rice,
        };
        let mut history = UndoStack::new();
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
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        let out = work(&mut cx);
        (out, world, history)
    }

    /// Sue Paint with `count` canvases open, the first one current.
    fn paint_with(count: usize) -> SuePaint {
        SuePaint {
            docs: (0..count).map(|_| Doc::new(EntityId::new())).collect(),
            active: 0,
            ..SuePaint::default()
        }
    }

    /// The name a test runs for an entry: a family as its first member.
    fn runnable(command: &Command<SuePaint>) -> String {
        if command.is_family() {
            format!("{}0", command.name)
        } else {
            command.name.to_owned()
        }
    }

    fn checked(paint: &SuePaint, cx: &AppletContext<'_>, name: &str) -> bool {
        registry().state(paint, &Facts::of(cx), name).checked == Some(true)
    }

    fn enabled(paint: &SuePaint, cx: &AppletContext<'_>, name: &str) -> bool {
        registry().state(paint, &Facts::of(cx), name).enabled
    }

    #[test]
    fn every_entry_has_its_label_icon_and_group() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        for command in COMMANDS {
            assert!(command.name.starts_with("window."), "{}", command.name);
            assert_eq!(command.group, "window", "{}", command.name);
            assert!(
                sue_lang::has("paint", command.label),
                "{}: no English label {:?}",
                command.name,
                command.label
            );
            assert!(
                Icon::from_name(command.icon).is_some(),
                "{}: the icon {:?} is not in the set",
                command.name,
                command.icon
            );
            assert!(registry().has(&runnable(command)), "{}", command.name);
        }
        assert_eq!(
            registry().label("window.canvas.0").as_deref(),
            Some("Go to Canvas")
        );
    }

    #[test]
    fn the_default_keys_are_the_ones_the_group_always_had() {
        let keys: Vec<(&str, &str)> = COMMANDS
            .iter()
            .filter(|command| !command.chords.is_empty())
            .map(|command| (command.name, command.chords))
            .collect();
        assert_eq!(
            keys,
            [
                ("window.next_tab", "Ctrl+Tab"),
                ("window.prev_tab", "Ctrl+Shift+Tab"),
                ("window.hide_all", "Tab"),
                ("window.hide_menus", "Shift+Tab"),
            ]
        );
    }

    #[test]
    fn the_lists_and_the_flags_say_the_same() {
        for name in RECORDABLE.iter().chain(&LEAVES_CANVAS) {
            assert!(
                COMMANDS.iter().any(|command| command.name == *name),
                "{name} is listed but not an entry"
            );
        }
        for command in COMMANDS {
            let name = command.name;
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                RECORDABLE.contains(&name),
                "{name}"
            );
            assert_eq!(
                command.flags.has(Flags::LEAVES_CANVAS),
                LEAVES_CANVAS.contains(&name),
                "{name}"
            );
            // The flag says what the old check said, until P3-14 takes
            // the old check away.
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                crate::extras::recordable(&runnable(command)),
                "{name}"
            );
            assert!(!command.flags.has(Flags::PALETTE), "{name}");
        }
        let hidden: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.flags.has(Flags::HIDDEN))
            .map(|command| command.name)
            .collect();
        assert_eq!(hidden, ["window.canvas."]);
    }

    #[test]
    fn no_window_command_changes_a_document() {
        // Every command of the group, those that leave the canvas too,
        // runs on a studio with two canvases; the arena and the undo
        // history stay as they were, so no command needs an undo pair.
        let mut paint = paint_with(2);
        let ((), world, history) = with_context(|cx| {
            for command in COMMANDS {
                let name = runnable(command);
                let entities = cx.world.arena.len();
                let revision = cx.world.arena.revision();
                let steps = cx.history.len();
                assert!(registry().run(&mut paint, cx, &name), "{name}");
                assert_eq!(cx.world.arena.len(), entities, "{name}");
                assert_eq!(cx.world.arena.revision(), revision, "{name}");
                assert_eq!(cx.history.len(), steps, "{name}");
                paint.dialog = None;
            }
        });
        assert!(world.arena.is_empty());
        assert!(!history.can_undo());
    }

    #[test]
    fn the_canvases_go_round_and_the_list_picks_one() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            // One canvas: nowhere to go.
            assert!(!enabled(&paint, cx, "window.next_tab"));
            assert!(!enabled(&paint, cx, "window.prev_tab"));
            assert!(checked(&paint, cx, "window.canvas.0"));
            assert!(!enabled(&paint, cx, "window.canvas.1"));
            paint = paint_with(3);
            assert!(enabled(&paint, cx, "window.next_tab"));
            registry().run(&mut paint, cx, "window.prev_tab");
            assert_eq!(paint.active, 2, "back from the first is the last");
            registry().run(&mut paint, cx, "window.next_tab");
            assert_eq!(paint.active, 0, "on from the last is the first");
            registry().run(&mut paint, cx, "window.next_tab");
            assert_eq!(paint.active, 1);
            // The list: the current canvas is checked, a number past the
            // last is grayed and does nothing.
            assert!(checked(&paint, cx, "window.canvas.1"));
            assert!(!checked(&paint, cx, "window.canvas.0"));
            registry().run(&mut paint, cx, "window.canvas.2");
            assert_eq!(paint.active, 2);
            assert!(checked(&paint, cx, "window.canvas.2"));
            registry().run(&mut paint, cx, "window.canvas.7");
            assert!(!enabled(&paint, cx, "window.canvas.7"));
            assert!(!enabled(&paint, cx, "window.canvas.x"));
            assert_eq!(paint.active, 2);
            // New Window: a second view of the current canvas after it,
            // the canvases tiled; Cascade puts them back in tabs.
            let picture = paint.docs[2].picture;
            registry().run(&mut paint, cx, "window.new");
            assert_eq!(paint.docs.len(), 4);
            assert_eq!(paint.active, 3);
            assert_eq!(paint.docs[3].picture, picture);
            assert!(paint.tiled && checked(&paint, cx, "window.tile"));
            assert!(enabled(&paint, cx, "window.cascade"));
            registry().run(&mut paint, cx, "window.cascade");
            assert!(!paint.tiled && !enabled(&paint, cx, "window.cascade"));
            registry().run(&mut paint, cx, "window.tile");
            assert!(paint.tiled);
        });
        // With no canvas, New Window and the workspaces are grayed.
        let paint = paint_with(0);
        with_context(|cx| {
            for name in [
                "window.new",
                "window.workspace.save",
                "window.hide_menus",
                "window.workspace.use.0",
            ] {
                assert!(!enabled(&paint, cx, name), "{name}");
            }
        });
    }

    #[test]
    fn each_palette_item_shows_its_palette_and_is_checked_while_shown() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            // A palette of its own, or a palette with tabs: a first run
            // shows it checked, a second hides it.
            for name in [
                "window.command_bar",
                "window.tool",
                "window.sub_tool",
                "window.tool_property",
                "window.brush_size",
                "window.color_wheel",
                "window.color_slider",
                "window.intermediate",
                "window.approximate",
                "window.color_set",
                "window.color_history",
                "window.material",
                "window.navigator",
                "window.layer",
                "window.timeline",
            ] {
                let before = checked(&paint, cx, name);
                registry().run(&mut paint, cx, name);
                assert_eq!(checked(&paint, cx, name), !before, "{name}");
                if before {
                    registry().run(&mut paint, cx, name);
                    assert!(checked(&paint, cx, name), "{name}");
                }
                registry().run(&mut paint, cx, name);
                assert!(!checked(&paint, cx, name), "{name}: a second run hides");
                registry().run(&mut paint, cx, name);
                assert!(checked(&paint, cx, name), "{name}: and shows again");
            }
            // One tab of a palette at a time: the sliders leave the wheel.
            registry().run(&mut paint, cx, "window.color_slider");
            assert!(!checked(&paint, cx, "window.color_wheel"));
            // Tabs that only show: a second run leaves them shown.
            for name in [
                "window.brush_grid",
                "window.sub_view",
                "window.layer_property",
                "window.history",
                "window.cels",
            ] {
                registry().run(&mut paint, cx, name);
                assert!(checked(&paint, cx, name), "{name}");
                registry().run(&mut paint, cx, name);
                assert!(checked(&paint, cx, name), "{name} stays");
            }
            assert!(!checked(&paint, cx, "window.layer"), "Cels is the tab");
            // The palettes of their own come into the dock and stay,
            // whether the dock had them already or not.
            for name in ["window.info", "window.auto_actions", "window.item_bank"] {
                for _ in 0..2 {
                    registry().run(&mut paint, cx, name);
                    assert!(checked(&paint, cx, name), "{name}");
                }
            }
        });
    }

    #[test]
    fn hiding_everything_and_the_default_workspace() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            assert!(!checked(&paint, cx, "window.hide_all"));
            registry().run(&mut paint, cx, "window.hide_all");
            assert!(!paint.any_panel());
            assert!(checked(&paint, cx, "window.hide_all"));
            assert_eq!(
                paint.panels.command_bar,
                Panels::default().command_bar,
                "the command bar stays as it was"
            );
            registry().run(&mut paint, cx, "window.hide_all");
            assert!(paint.panels.tool && paint.panels.layer && paint.panels.timeline);
            registry().run(&mut paint, cx, "window.hide_menus");
            assert!(paint.menus_hidden);
            // Reset to Default with none chosen: Sue Paint's own palettes
            // and the menu bar back.
            paint.panels.tool = false;
            registry().run(&mut paint, cx, "window.workspace.reset");
            assert_eq!(paint.panels, Panels::default());
            assert!(!paint.menus_hidden);
            // Use Current Workspace as Default (here with no data folder,
            // so kept for this run only), then Reset goes back to it.
            paint.panels.navigator = false;
            registry().run(&mut paint, cx, "window.workspace.default");
            assert!(paint.default_workspace.is_some());
            paint.panels.navigator = true;
            registry().run(&mut paint, cx, "window.workspace.reset");
            assert!(!paint.panels.navigator);
            // The dialogs.
            registry().run(&mut paint, cx, "window.workspace.save");
            assert!(matches!(paint.dialog, Some(Dialog::Workspace(_))));
            registry().run(&mut paint, cx, "window.workspace.manage");
            assert!(matches!(paint.dialog, Some(Dialog::Workspaces)));
        });
    }

    #[test]
    fn the_interface_items_check_the_one_in_use() {
        let mut paint = paint_with(1);
        paint.interface = "tablet".to_owned();
        with_context(|cx| {
            for (name, on) in [
                ("window.interface.auto", false),
                ("window.interface.desktop", false),
                ("window.interface.tablet", true),
                ("window.interface.phone", false),
            ] {
                assert_eq!(checked(&paint, cx, name), on, "{name}");
            }
            // Without the setting registered, choosing says it cannot.
            registry().run(&mut paint, cx, "window.interface.phone");
        });
    }
}
