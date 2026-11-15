//! The Window menu's commands: canvases, workspaces, the interface and
//! the palettes.
//!
//! Each command is one entry of [`COMMANDS`] with its own `run` and
//! `state`, its label, its default keys, its icon and its flags. No
//! Window command changes a document: they show and hide palettes, move
//! between canvases and keep workspaces, so none has a document undo
//! step. The interface choices change the setting `display.interface`,
//! which Sue's own history takes back like any other setting.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{doc_open, flag, when};
use crate::dialogs::Dialog;
use crate::dock::Palette;
use crate::studio::{Doc, Panels, SuePaint};

/// The commands of this group that Auto Actions may record: none. They
/// change how the studio looks, not the picture, so a recorded action
/// that replays them would only move palettes about (no `window.`
/// command was ever recorded).
pub(super) const RECORDABLE: [&str; 0] = [];

/// The commands of this group that go outside the canvas, so a test that
/// runs every command on a canvas skips them: New Window opens a second
/// view of the canvas, Canvas in Its Own Window moves the canvas out of
/// the column, and each interface choice changes the setting
/// `display.interface`, which lays the whole studio out anew.
pub(super) const LEAVES_CANVAS: [&str; 6] = [
    "window.new",
    "window.float",
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
/// in the tabs, shown at once; the canvases stay in tabs or tiled as
/// they were.
fn new_window(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc() {
        let mut view = Doc::new(doc.picture);
        view.view = doc.view;
        view.layer = doc.layer;
        view.frame = doc.frame;
        view.track = doc.track;
        paint.docs.insert(paint.active + 1, view);
        paint.active += 1;
    }
}

/// Canvas in Its Own Window: the current canvas moves into a window of
/// its own, which can go to another screen; run again (or in that
/// window), it comes back into the tabs.
fn float(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_canvas_window(cx);
}

/// Checked while the current canvas is in a window of its own.
fn float_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    match paint.doc() {
        Some(doc) => flag(doc.window.is_some()),
        None => ItemState::off(),
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
    paint.hidden_all = None;
    paint.current_workspace = None;
    if let Some(space) = saved {
        paint.apply_layout(&space.lines);
    } else {
        // The layout style's own arrangement.
        paint.dock = paint.style_layout();
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

/// A registered workspace is checked while it is the one in use, and
/// grayed when no workspace has that number.
fn workspace_use_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    match call
        .arg
        .parse::<usize>()
        .ok()
        .and_then(|index| paint.workspaces.get(index))
    {
        Some(space) => flag(paint.current_workspace.as_deref() == Some(space.name.as_str())),
        None => ItemState::off(),
    }
}

/// Reload: the workspace in use, as it was registered.
fn workspace_reload(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.reload_workspace();
}

/// Reload works while a registered workspace is in use (not one renamed
/// or deleted in Manage Workspaces since).
fn workspace_reload_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .current_workspace
            .as_ref()
            .is_some_and(|name| paint.workspaces.iter().any(|space| space.name == *name)),
    )
}

/// A workspace Sue Paint ships (Illustration, Animation).
fn workspace_preset(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    paint.use_preset(call.arg);
}

/// A preset works when Sue Paint ships one of that name.
fn workspace_preset_state(_: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    when(
        crate::extras::PRESETS
            .iter()
            .any(|preset| preset.id == call.arg),
    )
}

// Layout styles.

/// A style of Window, Layout Style: the setting `sue_paint.layout_style`,
/// which Sue's own history takes back like any other setting. A style
/// that is not built yet is never chosen.
fn layout_style(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(style) = crate::layout_style::by_id(call.arg).filter(|style| style.available) else {
        return;
    };
    if style.id == paint.layout_style {
        return;
    }
    match cx.edit_rice(sue_rice::SetRiceValue::new(
        crate::layout_style::KEY,
        sue_rice::RiceValue::Text(style.id.to_owned()),
    )) {
        Ok(_) => paint.follow_layout_style(cx),
        Err(error) => cx.fail(tr!("cannot-change-the-layout-style", error = error)),
    }
}

/// The style in use is checked; one not built yet is grayed out.
fn layout_style_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    match crate::layout_style::by_id(call.arg) {
        Some(style) if style.available => flag(style.id == paint.layout_style),
        _ => ItemState::off(),
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
        Ok(_) => cx.notify(tr!("the-interface-changes-now")),
        Err(error) => cx.fail(tr!("cannot-change-the-interface", error = error)),
    }
}

/// The interface in use is checked.
fn interface_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(paint.interface == chosen_interface(call))
}

// Showing and hiding.

/// Hide All Palettes: every palette goes, docked, floating, in a window
/// or in the strip (the command bar stays, as in Clip Studio Paint);
/// while they are hidden the item reads Show All Palettes and puts every
/// palette back exactly as it was.
fn hide_all(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_hide_all();
}

/// Hide All Palettes always works; its label says which way it goes.
fn hide_all_state(_: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::on()
}

/// Hide Menu Bar.
fn hide_menus(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.menus_hidden = !paint.menus_hidden;
}

// The palettes. Every palette is one of its own: its item shows it (at
// the front of its tabs, or open beside the strip) or, while it shows,
// hides it for good, as its Close does.

/// The palette a palette item of the Window menu shows.
fn palette_of(call: Call<'_>) -> Option<Palette> {
    Palette::of_window_command(call.name)
}

/// A palette's item: shows the palette, or hides it while it shows.
fn palette(paint: &mut SuePaint, _: &mut AppletContext<'_>, call: Call<'_>) {
    if let Some(palette) = palette_of(call) {
        paint.toggle_palette(palette);
    }
}

/// A palette's item is checked while the palette shows.
fn palette_state(paint: &SuePaint, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    flag(palette_of(call).is_some_and(|palette| paint.palette_showing(palette)))
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
    )
    .with(Flags::PALETTE),
    c(
        "window.interface.desktop",
        "desktop",
        "layout-style",
        interface,
        interface_state,
    )
    .with(Flags::PALETTE),
    c(
        "window.interface.tablet",
        "tablet",
        "layout-style",
        interface,
        interface_state,
    )
    .with(Flags::PALETTE),
    c(
        "window.interface.phone",
        "phone",
        "layout-style",
        interface,
        interface_state,
    )
    .with(Flags::PALETTE),
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
        palette,
        palette_state,
    ),
    c(
        "window.tool",
        "tool",
        "palette-tools",
        palette,
        palette_state,
    ),
    c(
        "window.sub_tool",
        "tool-group",
        "palette-sub-tools",
        palette,
        palette_state,
    ),
    c(
        "window.tool_property",
        "tool-settings",
        "palette-tool-property",
        palette,
        palette_state,
    ),
    c(
        "window.brush_size",
        "tool-sliders",
        "palette-brush-size",
        palette,
        palette_state,
    ),
    c(
        "window.brush_grid",
        "brush-size",
        "palette-brush-grid",
        palette,
        palette_state,
    ),
    c(
        "window.color_wheel",
        "color-wheel",
        "palette-color-wheel",
        palette,
        palette_state,
    ),
    c(
        "window.color_slider",
        "color-slider",
        "palette-color-sliders",
        palette,
        palette_state,
    ),
    c(
        "window.color_set",
        "color-set",
        "palette-color-set",
        palette,
        palette_state,
    ),
    c(
        "window.color_history",
        "color-history",
        "palette-color-history",
        palette,
        palette_state,
    ),
    c(
        "window.intermediate",
        "intermediate-color",
        "palette-intermediate",
        palette,
        palette_state,
    ),
    c(
        "window.approximate",
        "approximate-color",
        "palette-approximate",
        palette,
        palette_state,
    ),
    c(
        "window.navigator",
        "navigator",
        "palette-navigator",
        palette,
        palette_state,
    ),
    c(
        "window.sub_view",
        "sub-view",
        "palette-sub-view",
        palette,
        palette_state,
    ),
    c("window.layer", "layer-2", "layers", palette, palette_state),
    c(
        "window.layer_property",
        "layer-properties",
        "layer-property",
        palette,
        palette_state,
    ),
    c(
        "window.history",
        "history",
        "palette-history",
        palette,
        palette_state,
    ),
    c(
        "window.timeline",
        "timeline",
        "palette-timeline",
        palette,
        palette_state,
    ),
    c(
        "window.cels",
        "animation-cels",
        "palette-cels",
        palette,
        palette_state,
    ),
    c(
        "window.material",
        "material",
        "palette-materials",
        palette,
        palette_state,
    ),
    c(
        "window.auto_actions",
        "auto-action-palette",
        "palette-auto-action",
        palette,
        palette_state,
    ),
    c(
        "window.item_bank",
        "item-bank",
        "palette-item-bank",
        palette,
        palette_state,
    ),
    c(
        "window.info",
        "information",
        "palette-info",
        palette,
        palette_state,
    ),
    family(
        "window.workspace.use.",
        "workspace",
        "layout-style",
        workspace_use,
        workspace_use_state,
    ),
    c(
        "window.workspace.reload",
        "reload-workspace-plain",
        "reset",
        workspace_reload,
        workspace_reload_state,
    ),
    family(
        "window.workspace.preset.",
        "workspace-preset",
        "layout-style",
        workspace_preset,
        workspace_preset_state,
    ),
    family(
        crate::layout_style::COMMAND_PREFIX,
        "layout-style",
        "layout-style",
        layout_style,
        layout_style_state,
    ),
    // The numbered list of open canvases under Window, Canvas, as Clip
    // Studio Paint has it (gap CC-6): `window.canvas.0` is the first.
    family(
        "window.canvas.",
        "go-to-canvas",
        "picture",
        canvas,
        canvas_state,
    ),
    c(
        "window.float",
        "canvas-in-its-own-window",
        "external",
        float,
        float_state,
    ),
    // Quick Access (gaps WS-2, FG-38): its palette, then its sets and its
    // settings, from its palette menu and its own button.
    c(
        "window.quick_access",
        "quick-access",
        "star",
        palette,
        palette_state,
    ),
    c(
        "window.quick_access.new_set",
        "new-set",
        "plus",
        crate::bar_items::new_quick_set,
        doc_any,
    ),
    c(
        "window.quick_access.delete_set",
        "delete-set",
        "delete",
        crate::bar_items::delete_quick_set,
        crate::bar_items::delete_quick_set_state,
    ),
    c(
        "window.quick_access.settings",
        "quick-access-settings-ellipsis",
        "settings",
        crate::bar_items::quick_access_settings,
        doc_any,
    ),
    c(
        "window.sub_tool_detail",
        "sub-tool-detail",
        "wrench",
        palette,
        palette_state,
    ),
];

/// A command that works with or without a canvas.
fn doc_any(_: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    ItemState::on()
}

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
        with_rice(RiceSchema::new(), work)
    }

    /// Runs `work` with an applet context over an empty arena and the
    /// settings `rice`, and hands back the world and the history.
    fn with_rice<R>(
        rice: RiceSchema,
        work: impl FnOnce(&mut AppletContext<'_>) -> R,
    ) -> (R, World, UndoStack<World>) {
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
            // Only the interface choices are offered in the command palette:
            // the Window menu no longer lists them (the Sue menu does).
            assert_eq!(
                command.flags.has(Flags::PALETTE),
                name.starts_with("window.interface."),
                "{name}"
            );
        }
        let hidden: Vec<&str> = COMMANDS
            .iter()
            .filter(|command| command.flags.has(Flags::HIDDEN))
            .map(|command| command.name)
            .collect();
        assert!(hidden.is_empty(), "{hidden:?}");
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
            // shown as a tab of its own: the canvases are not tiled
            // (Clip Studio Paint's New Window leaves tiling alone).
            let picture = paint.docs[2].picture;
            registry().run(&mut paint, cx, "window.new");
            assert_eq!(paint.docs.len(), 4);
            assert_eq!(paint.active, 3);
            assert_eq!(paint.docs[3].picture, picture);
            assert!(!paint.tiled && !checked(&paint, cx, "window.tile"));
            assert!(!enabled(&paint, cx, "window.cascade"));
            // Tile, then New Window keeps the canvases tiled; Cascade
            // puts them back in tabs.
            registry().run(&mut paint, cx, "window.tile");
            registry().run(&mut paint, cx, "window.new");
            assert_eq!(paint.docs.len(), 5);
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

    /// Every palette command of the Window menu.
    const PALETTE_COMMANDS: [&str; 24] = [
        "window.command_bar",
        "window.quick_access",
        "window.tool",
        "window.sub_tool",
        "window.tool_property",
        "window.brush_grid",
        "window.brush_size",
        "window.color_wheel",
        "window.color_slider",
        "window.color_set",
        "window.intermediate",
        "window.approximate",
        "window.color_history",
        "window.layer",
        "window.layer_property",
        "window.cels",
        "window.timeline",
        "window.navigator",
        "window.sub_view",
        "window.history",
        "window.auto_actions",
        "window.info",
        "window.item_bank",
        "window.material",
    ];

    /// How many places `palette` is in: docked, collapsed or floating.
    fn places(paint: &SuePaint, palette: Palette) -> usize {
        usize::from(paint.dock.find(palette).is_some())
            + usize::from(paint.dock.iconized.contains(&palette))
            + paint
                .dock
                .floating
                .iter()
                .filter(|(found, _)| *found == palette)
                .count()
    }

    #[test]
    fn each_palette_item_shows_its_own_palette_and_hides_it_for_good() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            for name in PALETTE_COMMANDS {
                let palette = Palette::of_window_command(name).expect(name);
                // Open palettes start checked; the strip's icons and the
                // folded Timeline do not, and their item opens them.
                let before = checked(&paint, cx, name);
                assert_eq!(
                    before,
                    paint.dock.is_open(palette) && palette != Palette::Timeline
                );
                registry().run(&mut paint, cx, name);
                assert_eq!(checked(&paint, cx, name), !before, "{name}");
                registry().run(&mut paint, cx, name);
                assert_eq!(checked(&paint, cx, name), before, "{name}: and back");
                if !before {
                    registry().run(&mut paint, cx, name);
                }
                assert!(checked(&paint, cx, name), "{name}: shown");
                assert!(paint.palette_showing(palette), "{name}");
                assert!(paint.dock.has(palette), "{name}: as itself");
            }
            // No palette shows in two places: each one is in the dock
            // once, and the color palettes are each their own.
            for name in PALETTE_COMMANDS {
                let palette = Palette::of_window_command(name).expect(name);
                assert_eq!(places(&paint, palette), 1, "{name}");
            }
            assert!(checked(&paint, cx, "window.color_slider"));
            assert!(checked(&paint, cx, "window.color_wheel"));
        });
    }

    #[test]
    fn a_palette_closed_for_good_comes_back_where_the_style_has_it() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            // Closed from its own menu or its window's X: gone for good.
            paint.close_palette(Palette::ColorSlider);
            assert!(paint.dock.is_closed(Palette::ColorSlider));
            assert!(!checked(&paint, cx, "window.color_slider"));
            // The Window menu brings it back as a tab of the colors.
            registry().run(&mut paint, cx, "window.color_slider");
            let found = paint.dock.find(Palette::ColorSlider);
            assert!(found.is_some());
            assert_eq!(paint.dock.find(Palette::Color), found);
            // A floating palette closed leaves nothing behind.
            paint.dock.float(Palette::Info, egui::pos2(300.0, 200.0));
            paint.close_palette(Palette::Info);
            assert_eq!(places(&paint, Palette::Info), 0);
            registry().run(&mut paint, cx, "window.info");
            assert!(paint.dock.find(Palette::Info).is_some(), "back in its tabs");
            // Material, an icon in the strip, opens beside the strip.
            registry().run(&mut paint, cx, "window.material");
            assert_eq!(paint.dock.popped, Some(Palette::Material));
            assert!(checked(&paint, cx, "window.material"));
            // The folded Timeline: its item opens the row.
            assert!(!checked(&paint, cx, "window.timeline"));
            registry().run(&mut paint, cx, "window.timeline");
            assert!(!paint.dock.row_folded[1]);
            assert!(checked(&paint, cx, "window.timeline"));
        });
    }

    #[test]
    fn hiding_everything_and_the_default_workspace() {
        let mut paint = paint_with(1);
        paint.dock.float(Palette::Info, egui::pos2(300.0, 200.0));
        paint.panels.navigator = false;
        let before = (paint.panels, paint.dock.clone());
        with_context(|cx| {
            registry().run(&mut paint, cx, "window.hide_all");
            assert!(!paint.any_panel());
            assert!(paint.hidden_all.is_some());
            // Every palette goes, the strip's and the floating ones too;
            // the command bar stays, as in Clip Studio Paint.
            for &palette in <Palette as sue_wm::dock::DockPanel>::all() {
                let showing = paint.palette_showing(palette);
                assert_eq!(showing, palette == Palette::CommandBar, "{palette:?}");
            }
            // Kept as they were, in the layout file too.
            let lines = before.1.to_lines();
            assert_eq!(paint.workspace_now("").lines[..lines.len()], lines[..]);
            // Show All Palettes: exactly as before.
            registry().run(&mut paint, cx, "window.hide_all");
            assert_eq!((paint.panels, paint.dock.clone()), before);
            assert!(paint.hidden_all.is_none());
            // Showing one palette while all are hidden brings the rest too.
            registry().run(&mut paint, cx, "window.hide_all");
            registry().run(&mut paint, cx, "window.layer");
            assert!(paint.hidden_all.is_none());
            assert!(paint.palette_showing(Palette::Info));
            registry().run(&mut paint, cx, "window.hide_menus");
            assert!(paint.menus_hidden);
            // Reset to Default with none chosen: the layout style's own
            // palettes and the menu bar back.
            paint.panels.tool = false;
            registry().run(&mut paint, cx, "window.workspace.reset");
            assert_eq!(paint.panels, Panels::default());
            assert_eq!(paint.dock, crate::dock::classic_layout());
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
    fn workspaces_reload_check_the_one_in_use_and_presets_lay_out() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            assert!(!enabled(&paint, cx, "window.workspace.reload"));
            paint.panels.navigator = false;
            paint.workspaces = vec![paint.workspace_now("Inking")];
            paint.panels.navigator = true;
            registry().run(&mut paint, cx, "window.workspace.use.0");
            assert!(checked(&paint, cx, "window.workspace.use.0"));
            assert!(!paint.panels.navigator);
            assert!(!enabled(&paint, cx, "window.workspace.use.1"));
            // Moved about, then Reload: as registered.
            paint.panels.navigator = true;
            registry().run(&mut paint, cx, "window.workspace.reload");
            assert!(!paint.panels.navigator);
            // Renamed in Manage Workspaces: nothing left to reload.
            paint.workspaces[0].name = "Inks".to_owned();
            assert!(!enabled(&paint, cx, "window.workspace.reload"));
            paint.workspaces[0].name = "Inking".to_owned();
            assert!(enabled(&paint, cx, "window.workspace.reload"));
            // The Animation preset opens the Timeline with the cels by
            // the layers; Illustration is the style's own arrangement.
            registry().run(&mut paint, cx, "window.workspace.preset.animation");
            assert!(!paint.dock.row_folded[1] && paint.panels.timeline);
            assert_eq!(
                paint.dock.find(Palette::Cels),
                paint.dock.find(Palette::Layer)
            );
            assert!(!checked(&paint, cx, "window.workspace.use.0"));
            registry().run(&mut paint, cx, "window.workspace.preset.illustration");
            assert_eq!(paint.dock, crate::dock::classic_layout());
            assert!(!enabled(&paint, cx, "window.workspace.preset.none"));
        });
    }

    #[test]
    fn a_layout_style_not_built_is_grayed_and_never_chosen() {
        let mut paint = paint_with(1);
        with_context(|cx| {
            let classic = "window.layout_style.classic";
            let later = "window.layout_style.sue_paint";
            assert!(checked(&paint, cx, classic));
            assert!(!enabled(&paint, cx, later));
            let dock = paint.dock.clone();
            registry().run(&mut paint, cx, later);
            assert_eq!(paint.layout_style, crate::layout_style::CLASSIC);
            assert_eq!(paint.dock, dock);
            assert!(cx.requests.is_empty(), "{:?}", cx.requests);
            assert!(!enabled(&paint, cx, "window.layout_style.nothing"));
        });
    }

    #[test]
    fn switching_layout_styles_and_back_keeps_each_ones_palettes() {
        // The second style is not built, so the switch is made directly
        // (as the setting will make it once it is).
        let mut paint = paint_with(1);
        paint.panels.navigator = false;
        paint.dock.float(Palette::Layer, egui::pos2(200.0, 100.0));
        let classic = paint.workspace_now("").lines;
        paint.switch_layout_style(crate::layout_style::SUE_PAINT, None, false);
        assert_eq!(paint.layout_style, crate::layout_style::SUE_PAINT);
        assert!(paint.panels.navigator, "the other style's own palettes");
        assert_ne!(paint.workspace_now("").lines, classic);
        paint.panels.color = false;
        paint.switch_layout_style(crate::layout_style::CLASSIC, None, false);
        assert_eq!(paint.workspace_now("").lines, classic);
        paint.switch_layout_style(crate::layout_style::SUE_PAINT, None, false);
        assert!(!paint.panels.color, "and the other style's as left");
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
            assert!(registry().run(&mut paint, cx, "window.interface.phone"));
            assert!(
                matches!(cx.requests.as_slice(), [sue_wm::ShellRequest::Fail(_)]),
                "{:?}",
                cx.requests
            );
            assert!(cx.world.rice.get_str("display.interface").is_none());
        });
        // With it, the choice is the setting, said on the status line, and
        // one undo step takes it back (rule 7: a setting has its undo).
        let mut rice = RiceSchema::new();
        sue_wm::register_rice(&mut rice).expect("the shell's settings register");
        let ((), mut world, mut history) = with_rice(rice, |cx| {
            assert!(registry().run(&mut paint, cx, "window.interface.phone"));
            assert!(
                matches!(cx.requests.as_slice(), [sue_wm::ShellRequest::Notify(_)]),
                "{:?}",
                cx.requests
            );
        });
        assert_eq!(world.rice.get_str("display.interface"), Some("phone"));
        assert!(history.undo(&mut world).expect("the step undoes"));
        assert_eq!(world.rice.get_str("display.interface"), Some("auto"));
    }
}
