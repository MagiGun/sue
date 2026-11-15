//! Leaving Sue: the question about work that is not saved, then the
//! one about exports that still run.

use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::ui::{Answer, Dialog, Tone, confirm};
use crate::{ShellRequest, Theme};

impl Shell {
    /// Quitting asks, for each piece of work not saved, whether to save
    /// it, with Save, Do Not Save and Cancel. When none is left, asks the
    /// runtime to quit with `app.quit.checked`.
    pub(super) fn quit_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        let Some(skipped) = self.quitting.clone() else {
            return;
        };
        // A save that asks where uses the system's dialog, over this one;
        // once it is saved it leaves the list, and if not it is asked again.
        let next = io.applets.iter().find_map(|(manifest, applet)| {
            applet
                .unsaved(&io.world.arena)
                .into_iter()
                .find(|(key, _)| !skipped.contains(&(manifest.id.clone(), key.clone())))
                .map(|(key, name)| (manifest.id.clone(), manifest.title(), key, name))
        });
        let Some((id, studio, key, name)) = next else {
            if self.exports_running_ui(ctx, io, theme) {
                return;
            }
            self.quitting = None;
            io.requests
                .push(ShellRequest::Command("app.quit.checked".to_owned()));
            return;
        };
        // One question in Sue's dialog host, titled by the studio: Save
        // accepts (Enter), Do Not Save is the other answer, and Cancel,
        // the close button or Escape keeps Sue open.
        let dialog = Dialog::new("sue-unsaved", &studio, theme)
            .accept(tr!("save"))
            .alternate(tr!("do-not-save"))
            .cancel();
        let message = tr!("unsaved-save-it", name = name.as_str());
        let answer = confirm(ctx, dialog, Tone::Warning, &message);
        match answer {
            Some(Answer::Cancel) => self.quitting = None,
            Some(Answer::Alternate) => {
                if let Some(skipped) = self.quitting.as_mut() {
                    skipped.push((id, key));
                }
            }
            Some(Answer::Accept) => {
                let view = self.view(now, 0.0);
                let theme = self.theme.clone();
                let (applets, mut cx) = split(io, &theme, view);
                if let Some(applet) = applets.get_mut(&id) {
                    applet.save_unsaved(&key, &mut cx);
                }
            }
            Some(Answer::Extra) | None => {}
        }
        if answer.is_some() {
            self.touch();
        }
    }

    /// Work a person waits for (an export) that still runs when every
    /// piece of work not saved was asked about: Keep Exporting (Enter,
    /// and Cancel) keeps Sue open; Stop and Quit stops it, so nothing of
    /// it is written, and quits. Returns `true` while it asks.
    fn exports_running_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) -> bool {
        let running: Vec<(u64, String)> = io
            .transfers
            .list()
            .iter()
            .filter(|transfer| transfer.noticed && !transfer.finished())
            .map(|transfer| (transfer.id, transfer.label.clone()))
            .collect();
        if running.is_empty() || self.quit_stops_work {
            return false;
        }
        let title = tr!("quit-sue");
        let dialog = Dialog::new("sue-quit-exporting", &title, theme)
            .accept(tr!("keep-exporting"))
            .alternate(tr!("stop-and-quit"))
            .cancel();
        let message = match running.as_slice() {
            [(_, name)] => tr!("quit-stops-export", name = name.as_str()),
            _ => tr!("quit-stops-exports", count = running.len()),
        };
        match confirm(ctx, dialog, Tone::Warning, &message) {
            Some(Answer::Alternate) => {
                for (id, _) in &running {
                    io.transfers.cancel(*id);
                }
                self.quit_stops_work = true;
                self.touch();
                false
            }
            Some(Answer::Accept | Answer::Cancel) => {
                self.quitting = None;
                self.touch();
                true
            }
            Some(Answer::Extra) | None => true,
        }
    }
}
