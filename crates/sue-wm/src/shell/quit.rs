//! Leaving Sue: the question about work that is not saved.

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
}
