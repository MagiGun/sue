//! The keys that work everywhere in Sue.

use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::{ShellRequest, keys};

impl Shell {
    pub(super) fn global_keys(&mut self, ctx: &egui::Context, io: &mut ShellIo<'_>, now: Instant) {
        if io.applets.captures_keys() {
            return;
        }
        let keymap = self.config.keymap.clone();
        let typing = ctx.text_edit_focused();
        // Chords the open studio uses itself stay with it.
        let reserved: Vec<sue_rice::Keybind> = self
            .page
            .as_ref()
            .and_then(|page| io.applets.get(page))
            .map(|applet| applet.reserved_keys())
            .unwrap_or_default();
        let take = |input: &mut egui::InputState, bind: &sue_rice::Keybinds| {
            keys::consume_except(input, bind, &reserved)
        };
        let (undo, redo, settings, open, save, quit) = ctx.input_mut(|input| {
            let redo = take(input, &keymap.redo) || take(input, &keymap.redo_alt);
            (
                take(input, &keymap.undo),
                redo,
                take(input, &keymap.open_settings),
                take(input, &keymap.open_file),
                take(input, &keymap.save),
                take(input, &keymap.quit),
            )
        });
        if undo {
            self.undo(io, false);
        }
        if redo {
            self.undo(io, true);
        }
        if settings {
            self.handle(ShellRequest::OpenSettings(None), io, now);
        }
        if open {
            // The page opens files its own way first.
            let handled = match self.page.clone() {
                Some(page) => {
                    let view = self.view(now, 0.0);
                    let theme = self.theme.clone();
                    let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &page));
                    let (applets, mut cx) = split(io, &theme, view);
                    applets
                        .get_mut(&page)
                        .is_some_and(|applet| applet.command("file.open", &mut cx))
                }
                None => false,
            };
            if !handled {
                io.requests
                    .push(ShellRequest::Command("file.open".to_owned()));
            }
        }
        if save {
            // The page saves its own way first (Sue Paint writes the
            // picture's .clip); else the project is saved.
            let handled = match self.page.clone() {
                Some(page) => {
                    let view = self.view(now, 0.0);
                    let theme = self.theme.clone();
                    let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &page));
                    let (applets, mut cx) = split(io, &theme, view);
                    applets
                        .get_mut(&page)
                        .is_some_and(|applet| applet.command("file.save", &mut cx))
                }
                None => false,
            };
            if !handled {
                io.requests
                    .push(ShellRequest::Command("file.save".to_owned()));
            }
        }
        if quit {
            io.requests
                .push(ShellRequest::Command("app.quit".to_owned()));
        }
        if typing || self.palette.is_some() {
            return;
        }
        let (palette, escape) = ctx.input_mut(|input| {
            (
                take(input, &keymap.command_palette) || take(input, &keymap.command_palette_alt),
                self.window.is_some()
                    && self.window_appear.target() > 0.0
                    && input.key_pressed(egui::Key::Escape),
            )
        });
        if palette {
            self.open_palette();
        }
        if escape {
            self.close_window();
        }
    }
}
