//! The settings dialogs: Keyboard Shortcuts (with
//! what the pen and the mouse do), Preferences and the workspaces.

use egui::{RichText, Ui};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, label, ok_cancel};
use crate::studio::SuePaint;

/// The three modifier settings that came before Modifier Key Settings
/// kept its rows in a file (`crate::modifier_keys`): their rice keys,
/// labels and choices, still shown in Settings.
pub(crate) const MODIFIERS: [(&str, &str, &[&str]); 3] = [
    (
        "sue_paint.alt_tool",
        "Alt while drawing",
        &["eyedropper", "eraser", "none"],
    ),
    ("sue_paint.space_tool", "Space", &["hand", "none"]),
    (
        "sue_paint.ctrl_shift",
        "Ctrl+Shift click",
        &["pick_layer", "none"],
    ),
];

/// What the pen and mouse do, for the Keyboard Shortcuts dialog under
/// the keys Settings has: Clip Studio Paint's gestures.
pub(crate) const GESTURES: [(&str, &str); 13] = [
    ("Scroll", "Space+drag, middle drag"),
    ("Rotate the view", "Shift+Space+drag, Shift+wheel"),
    (
        "Zoom in / out by clicking",
        "Ctrl+Space+click / Alt+Space+click",
    ),
    ("Zoom", "Ctrl+wheel"),
    ("Pick a color", "Alt+click, right-click"),
    ("Pick the layer under the pointer", "Ctrl+Shift+click"),
    ("Move the layer (Object)", "Ctrl+drag"),
    ("A tool only while its key is held", "Hold the tool's key"),
    ("Change the brush size", "Ctrl+Alt+drag"),
    ("Straight line", "Shift+drag"),
    (
        "Selection: add / subtract / intersect",
        "Shift / Alt / Shift+Alt",
    ),
    ("Zoom tool: the other way", "Alt+click"),
    ("Command palette", "Command in the status bar"),
];

/// Register Workspace: its name.
pub(super) fn workspace_ui(ui: &mut Ui, name: &mut String, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let entered = ui
        .horizontal(|ui| {
            label(ui, &tr!("name"), theme);
            let field = ui.text_edit_singleline(name);
            // The name takes the keys from the start, as a person types it.
            if !field.has_focus() && ui.memory(|memory| memory.focused().is_none()) {
                field.request_focus();
            }
            // Enter in the field is OK (the field gives up the keyboard as
            // it takes the key, so either may be seen this frame).
            (field.lost_focus() || field.has_focus())
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
        })
        .inner;
    ui.label(
        RichText::new(tr!("workspace-keeps"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    if entered { Outcome::Ok } else { outcome }
}

/// Keyboard Shortcuts: the keys set now and the pen and mouse gestures.
pub(super) fn shortcuts_ui(
    ui: &mut Ui,
    cx: &AppletContext<'_>,
    size: egui::Vec2,
    change_keys: &mut bool,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    // Every command can be given a key, but the list shows the ones
    // that have one, as set now.
    let keys = {
        let section = crate::studio::KEYS_SECTION;
        let table = crate::studio::shortcut_table();
        let bound: Vec<sue_wm::shortcuts::Shortcut> = table
            .iter()
            .zip(sue_wm::shortcuts::resolve(cx.rice(), section, table))
            .filter(|(_, chords)| !chords.is_empty())
            .map(|(shortcut, _)| shortcut.clone())
            .collect();
        sue_wm::shortcuts::rows(cx.rice(), section, &bound)
    };
    egui::ScrollArea::vertical()
        .max_height(size.y - 90.0)
        .show(ui, |ui| {
            sue_wm::shortcuts::grid_ui(ui, "sue-paint-keys", &keys, theme);
            ui.add_space(12.0);
            ui.label(
                RichText::new(tr!("with-the-pen-and-mouse"))
                    .color(theme.text)
                    .strong(),
            );
            let gestures: Vec<(String, String)> = GESTURES
                .iter()
                .map(|(what, how)| (crate::localize(what), crate::localize(how)))
                .collect();
            sue_wm::shortcuts::grid_ui(ui, "sue-paint-gestures", &gestures, theme);
        });
    crate::dialogs::button_row(ui, |ui| {
        if ui.button(tr!("close")).clicked() {
            outcome = Outcome::Cancel;
        }
        if ui
            .button(tr!("change-shortcuts-ellipsis"))
            .on_hover_text(tr!("settings-keymaps-any-number-keys-each"))
            .clicked()
        {
            *change_keys = true;
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

impl SuePaint {
    /// Manage Workspaces: rename, use or delete each.
    pub(super) fn workspaces_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let mut chosen = None;
        let mut gone = None;
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                for (index, space) in self.workspaces.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut space.name);
                        if ui.button(tr!("use")).clicked() {
                            chosen = Some(index);
                        }
                        if ui.button(tr!("delete")).clicked() {
                            gone = Some(index);
                        }
                    });
                }
                if self.workspaces.is_empty() {
                    ui.label(
                        RichText::new(tr!("no-workspaces-yet-window-workspace-register"))
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                }
            });
        if let Some(index) = chosen {
            self.pick_workspace(index, cx);
        }
        if let Some(index) = gone {
            self.workspaces.remove(index);
        }
        ui.add_space(8.0);
        if ui.button(tr!("close")).clicked() {
            outcome = Outcome::Ok;
        }
        outcome
    }

    /// Keeps the workspace as it is now under `name`.
    pub(super) fn workspace_ok(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let name = if name.trim().is_empty() {
            tr!("workspace")
        } else {
            name.trim().to_owned()
        };
        let mut space = self.workspace_now(&name);
        space.lines.extend(self.workspace_settings_lines(cx.rice()));
        match self.workspaces.iter().position(|known| known.name == name) {
            Some(index) => self.workspaces[index] = space,
            None => self.workspaces.push(space),
        }
        self.save_extras(cx);
    }
}
