//! The Window menu's commands: canvases, workspaces, the interface and
//! the palettes.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "window", super::run).state(super::state)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "window", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("window.next_tab", "next-canvas").keys("Ctrl+Tab"),
    c("window.prev_tab", "previous-canvas").keys("Ctrl+Shift+Tab"),
    c("window.new", "new-window").with(Flags::LEAVES_CANVAS),
    c("window.cascade", "cascade"),
    c("window.tile", "tile"),
    c("window.workspace.save", "register-workspace-ellipsis"),
    c("window.workspace.manage", "manage-workspaces-ellipsis"),
    c(
        "window.workspace.default",
        "use-current-workspace-as-default",
    ),
    c("window.workspace.reset", "reset-workspace-to-default"),
    c("window.interface.auto", "automatic-follows-the-device").with(Flags::LEAVES_CANVAS),
    c("window.interface.desktop", "desktop").with(Flags::LEAVES_CANVAS),
    c("window.interface.tablet", "tablet").with(Flags::LEAVES_CANVAS),
    c("window.interface.phone", "phone").with(Flags::LEAVES_CANVAS),
    c("window.hide_all", "hide-all-palettes").keys("Tab"),
    c("window.hide_menus", "hide-menu-bar").keys("Shift+Tab"),
    c("window.command_bar", "command-bar"),
    c("window.tool", "tool"),
    c("window.sub_tool", "tool-group"),
    c("window.tool_property", "tool-settings"),
    c("window.brush_size", "tool-sliders"),
    c("window.brush_grid", "brush-size"),
    c("window.color_wheel", "color-wheel"),
    c("window.color_slider", "color-slider"),
    c("window.color_set", "color-set"),
    c("window.color_history", "color-history"),
    c("window.intermediate", "intermediate-color"),
    c("window.approximate", "approximate-color"),
    c("window.navigator", "navigator"),
    c("window.sub_view", "sub-view"),
    c("window.layer", "layer-2"),
    c("window.layer_property", "layer-properties"),
    c("window.history", "history"),
    c("window.timeline", "timeline"),
    c("window.cels", "animation-cels"),
    c("window.material", "material"),
    c("window.auto_actions", "auto-action-palette"),
    c("window.item_bank", "item-bank"),
    c("window.info", "information"),
    family("window.workspace.use.", "workspace"),
];
