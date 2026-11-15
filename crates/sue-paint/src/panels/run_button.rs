//! An icon button that runs one of Sue Paint's commands: the icon, the
//! name, whether it can be pressed and whether it is on all come from the
//! command registry, and its tooltip shows the keys that run it as
//! Shortcut Settings has them. The Navigator, Sub View and Auto Action
//! palettes are built from these, so a button there does what the menu
//! item does, Auto Actions can record it, and a changed key is followed.

use egui::{Rect, Ui};
use sue_wm::icons::Icon;
use sue_wm::{AppletContext, Theme};

use crate::bar_items::Item;
use crate::studio::SuePaint;

/// What a command button shows now.
pub(crate) struct Shown {
    /// The command's icon in the shared set.
    pub icon: Icon,
    /// Its name, as menus show it but without a trailing ellipsis.
    pub name: String,
    /// The keys that run it now; empty for none.
    pub keys: String,
    /// It can be pressed.
    pub enabled: bool,
    /// It is a switch that is on.
    pub on: Option<bool>,
}

impl SuePaint {
    /// How the button of `command` looks now, with `fallback` as its icon
    /// when the registry names none.
    pub(crate) fn command_look(
        &self,
        command: &str,
        fallback: Icon,
        cx: &AppletContext<'_>,
    ) -> Shown {
        let registry = crate::commands::registry();
        let icon = registry
            .find(command)
            .and_then(|(found, _)| Icon::from_name(found.icon))
            .unwrap_or(fallback);
        let name = registry
            .label(command)
            .unwrap_or_else(|| command.to_owned())
            .trim_end_matches("...")
            .to_owned();
        let state = self.state(command, &sue_wm::Facts::of(cx));
        Shown {
            icon,
            name,
            keys: self.item_keys(&Item::Command(command.to_owned()), cx),
            enabled: state.enabled,
            on: state.checked,
        }
    }

    /// The button of `command` at `rect`; runs the command when pressed.
    /// Returns `true` when it ran.
    pub(crate) fn command_button(
        &mut self,
        ui: &Ui,
        rect: Rect,
        command: &str,
        fallback: Icon,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> bool {
        let look = self.command_look(command, fallback, cx);
        let mut button = sue_wm::ui::icon_button(("sue-paint-run", command), look.icon, &look.name)
            .enabled(look.enabled)
            .shortcut(&look.keys)
            .quiet();
        if let Some(on) = look.on {
            button = button.selected(on);
        }
        let clicked = button.show_at(ui, rect, theme).clicked();
        if clicked {
            self.run(command, cx);
        }
        clicked
    }
}
