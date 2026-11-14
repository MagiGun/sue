//! The Window menu's commands: canvases, workspaces, the interface and
//! the palettes.

use sue_core::{Arena, UndoStack};
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState, World};

use super::{Basis, flag, when};
use crate::dialogs::Dialog;
use crate::studio::{Doc, Panels, SuePaint};

/// Does what a Window command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_window(call.name, cx);
}

/// How an item for a Window command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_window(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Window command looks right now.
    fn state_window(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "window.command_bar" => flag(self.panels.command_bar),
            "window.tool" => flag(self.panels.tool),
            "window.sub_tool" => flag(self.panels.sub_tool),
            "window.tool_property" => flag(self.panels.tool_property),
            "window.brush_size" => flag(self.panels.brush_size),
            "window.color_wheel" => flag(self.panels.color && self.color_tab == 0),
            "window.color_slider" => flag(self.panels.color && self.color_tab == 1),
            "window.intermediate" => flag(self.panels.color && self.color_tab == 2),
            "window.approximate" => flag(self.panels.color && self.color_tab == 3),
            "window.color_set" => flag(self.panels.color_set && self.set_tab == 0),
            "window.material" => flag(self.panels.color_set && self.set_tab == 2),
            "window.color_history" => flag(self.panels.color_set && self.set_tab == 1),
            "window.navigator" => flag(self.panels.navigator),
            "window.sub_view" => flag(self.panels.navigator && self.nav_tab == 1),
            "window.cels" => flag(self.panels.layer && self.layer_tab == 3),
            "window.tile" => flag(self.tiled),
            "window.brush_grid" => flag(self.panels.sub_tool && self.tool_tab == 2),
            "window.info" => flag(self.dock.has(crate::dock::Palette::Info)),
            "window.auto_actions" => flag(self.dock.has(crate::dock::Palette::AutoAction)),
            "window.item_bank" => flag(self.dock.has(crate::dock::Palette::ItemBank)),
            interface if interface.starts_with("window.interface.") => {
                flag(self.interface == interface["window.interface.".len()..])
            }
            "window.cascade" => when(self.tiled),
            "window.new" => when(has_doc),
            "window.layer" => flag(self.panels.layer && self.layer_tab == 0),
            "window.layer_property" => flag(self.panels.layer && self.layer_tab == 1),
            "window.history" => flag(self.panels.layer && self.layer_tab == 2),
            "window.timeline" => flag(self.panels.timeline),
            "window.hide_all" => flag(!self.any_panel()),
            "window.next_tab" | "window.prev_tab" => when(self.docs.len() > 1),
            _ => when(has_doc),
        }
    }

    /// Does what a Window command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_window(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "window.brush_grid" => {
                self.panels.sub_tool = true;
                self.tool_tab = 2;
            }
            "window.history" => {
                self.panels.layer = true;
                self.layer_tab = 2;
            }
            interface if interface.starts_with("window.interface.") => {
                let chosen = interface["window.interface.".len()..].to_owned();
                match cx.edit_rice(sue_rice::SetRiceValue::new(
                    "display.interface",
                    sue_rice::RiceValue::Text(chosen),
                )) {
                    Ok(_) => cx.notify(tr!("interface-changes-now-window-interface-changes")),
                    Err(error) => cx.fail(tr!("cannot-change-the-interface", error = error)),
                }
            }
            "window.next_tab" | "window.prev_tab" => {
                let count = self.docs.len().max(1);
                self.active = if command == "window.next_tab" {
                    (self.active + 1) % count
                } else {
                    (self.active + count - 1) % count
                };
            }
            "window.hide_all" => {
                let show = !self.any_panel();
                let command_bar = self.panels.command_bar;
                self.panels = Panels {
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
            "window.command_bar" => self.panels.command_bar = !self.panels.command_bar,
            "window.hide_menus" => self.menus_hidden = !self.menus_hidden,
            "window.workspace.save" => {
                self.dialog = Some(Dialog::Workspace(tr!(
                    "workspace-2",
                    value = self.workspaces.len() + 1
                )))
            }
            "window.workspace.manage" => self.dialog = Some(Dialog::Workspaces),
            used if used.starts_with("window.workspace.use.") => {
                if let Ok(index) = used["window.workspace.use.".len()..].parse::<usize>() {
                    self.use_workspace(index);
                }
            }
            "window.sub_view" => {
                self.panels.navigator = true;
                self.nav_tab = 1;
            }
            // Palettes of their own, opened from the strip.
            "window.info" | "window.auto_actions" | "window.item_bank" => {
                use crate::dock::Palette;
                let palette = match command {
                    "window.info" => Palette::Info,
                    "window.auto_actions" => Palette::AutoAction,
                    _ => Palette::ItemBank,
                };
                if !self.dock.has(palette) {
                    self.dock.iconize(palette);
                }
                if self.dock.iconized.contains(&palette) {
                    self.dock.popped = Some(palette);
                }
                self.dock.choose(palette);
            }
            "window.cels" => {
                self.panels.layer = true;
                self.layer_tab = 3;
            }
            "window.new" => {
                if let Some(doc) = self.doc() {
                    let mut view = Doc::new(doc.picture);
                    view.view = doc.view;
                    view.layer = doc.layer;
                    view.frame = doc.frame;
                    view.track = doc.track;
                    self.docs.insert(self.active + 1, view);
                    self.active += 1;
                    self.tiled = true;
                }
            }
            "window.tile" => self.tiled = true,
            "window.cascade" => self.tiled = false,
            // Reset to Default: the workspace people chose as the default
            // (Use Current Workspace as Default), or Sue Paint's own when
            // there is none or the palettes already are as it says.
            "window.workspace.reset" => {
                let saved = self
                    .default_workspace
                    .clone()
                    .filter(|space| space.lines != self.workspace_now("").lines);
                if let Some(space) = saved {
                    self.apply_layout(&space.lines);
                } else {
                    self.dock = crate::dock::Dock::default();
                    self.panels = Panels::default();
                    self.menus_hidden = false;
                }
            }
            "window.workspace.default" => self.set_default_workspace(cx),
            "window.tool" => self.panels.tool = !self.panels.tool,
            "window.sub_tool" => self.panels.sub_tool = !self.panels.sub_tool,
            "window.tool_property" => self.panels.tool_property = !self.panels.tool_property,
            "window.brush_size" => self.panels.brush_size = !self.panels.brush_size,
            "window.color_wheel"
            | "window.color_slider"
            | "window.intermediate"
            | "window.approximate" => {
                let tab = match command {
                    "window.color_slider" => 1,
                    "window.intermediate" => 2,
                    "window.approximate" => 3,
                    _ => 0,
                };
                if self.panels.color && self.color_tab == tab {
                    self.panels.color = false;
                } else {
                    self.panels.color = true;
                    self.color_tab = tab;
                }
            }
            "window.color_set" | "window.color_history" | "window.material" => {
                let tab = match command {
                    "window.color_history" => 1,
                    "window.material" => 2,
                    _ => 0,
                };
                if self.panels.color_set && self.set_tab == tab {
                    self.panels.color_set = false;
                } else {
                    self.panels.color_set = true;
                    self.set_tab = tab;
                }
            }
            "window.navigator" => self.panels.navigator = !self.panels.navigator,
            "window.layer" => {
                if self.panels.layer && self.layer_tab == 0 {
                    self.panels.layer = false;
                } else {
                    self.panels.layer = true;
                    self.layer_tab = 0;
                }
            }
            "window.timeline" => self.panels.timeline = !self.panels.timeline,
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "window", run).state(state)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "window", run).state(state)
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
    // Layer Properties is the Layer menu's, so its Layer command runs it.
    Command::new(
        "window.layer_property",
        "layer-properties",
        "window",
        super::layer::run,
    )
    .state(state),
    c("window.history", "history"),
    c("window.timeline", "timeline"),
    c("window.cels", "animation-cels"),
    c("window.material", "material"),
    c("window.auto_actions", "auto-action-palette"),
    c("window.item_bank", "item-bank"),
    c("window.info", "information"),
    family("window.workspace.use.", "workspace"),
];
