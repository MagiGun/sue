//! The command palette: its entries and its window.

use sue_platform::time::Instant;

use super::{Shell, ShellIo, pinned, split, templates};
use crate::palette::PaletteOutcome;
use crate::{AppletId, FilePick, PaletteEntry, ShellRequest, Theme};

/// What a palette entry does.
#[derive(Clone, Debug, PartialEq)]
enum PaletteAction {
    Request(ShellRequest),
    Undo(bool),
    Applet(AppletId, String),
}

fn entry(label: &str, hint: String) -> PaletteEntry {
    PaletteEntry {
        label: label.to_owned(),
        command: label.to_owned(),
        hint,
    }
}

impl Shell {
    /// Every palette entry: the Hub, every studio and tool, the pin, Sue's
    /// own actions, new files and the applets' own commands.
    fn palette_entries(&self, io: &ShellIo<'_>) -> Vec<(PaletteEntry, PaletteAction)> {
        let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
        let keymap = &self.config.keymap;
        let request = PaletteAction::Request;
        let command = |name: &str| request(ShellRequest::Command(name.to_owned()));
        let mut entries = vec![(
            entry(&tr!("go-to-the-hub"), String::new()),
            request(ShellRequest::Hub),
        )];
        for manifest in io.applets.manifests() {
            if manifest.role.is_page() {
                entries.push((
                    entry(&tr!("open", name = manifest.title()), String::new()),
                    request(ShellRequest::Open(manifest.id.clone())),
                ));
            }
        }
        let pinned = pinned(&io.world.rice, io.applets);
        if let Some(page) = self.page.clone()
            && pinned.as_ref() != Some(&page)
        {
            let name = io
                .applets
                .manifest(&page)
                .map_or_else(String::new, |manifest| manifest.title());
            entries.push((
                entry(&tr!("pin-open-it-at-launch", name = name), String::new()),
                request(ShellRequest::Pin(Some(page))),
            ));
        }
        if pinned.is_some() {
            entries.push((
                entry(&tr!("unpin-open-hub-launch"), String::new()),
                request(ShellRequest::Pin(None)),
            ));
        }
        entries.extend([
            (
                entry(&tr!("open-settings"), keymap.open_settings.to_string()),
                request(ShellRequest::OpenSettings(None)),
            ),
            (
                entry(&tr!("undo"), keymap.undo.to_string()),
                PaletteAction::Undo(false),
            ),
            (
                entry(&tr!("redo"), keymap.redo.to_string()),
                PaletteAction::Undo(true),
            ),
            (
                entry(&tr!("open-file"), keymap.open_file.to_string()),
                command("file.open"),
            ),
            (
                entry(&tr!("open-project"), String::new()),
                request(ShellRequest::PickFile(
                    FilePick::open(None, "open", &tr!("open-project")).extensions(&["sue"]),
                )),
            ),
            (
                entry(&tr!("save-project"), keymap.save.to_string()),
                command("file.save"),
            ),
            (
                entry(&tr!("save-project-as"), String::new()),
                command("file.save_as"),
            ),
            (
                entry(&tr!("quit-sue"), keymap.quit.to_string()),
                command("app.quit"),
            ),
        ]);
        for (applet, template) in templates(io.applets, &io.world.rice) {
            entries.push((
                entry(
                    &tr!(
                        "new-file-kind",
                        category = template.category,
                        label = template.label
                    ),
                    String::new(),
                ),
                request(ShellRequest::Create {
                    applet,
                    key: template.key,
                }),
            ));
        }
        for (manifest, applet) in io.applets.iter() {
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &manifest.id));
            for command in applet.commands() {
                entries.push((
                    PaletteEntry {
                        label: format!("{}: {}", manifest.title(), command.label),
                        command: command.command.clone(),
                        hint: command.hint.clone(),
                    },
                    PaletteAction::Applet(manifest.id.clone(), command.command),
                ));
            }
        }
        entries
    }

    pub(super) fn palette_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        if self.palette.is_none() {
            return;
        }
        let entries = self.palette_entries(io);
        let shown: Vec<PaletteEntry> = entries.iter().map(|(entry, _)| entry.clone()).collect();
        let area = self.page_rect;
        let outcome = self
            .palette
            .as_mut()
            .map_or(PaletteOutcome::Open, |palette| {
                palette.ui(ctx, area, &shown, theme)
            });
        self.touch();
        if let PaletteOutcome::Run(index) = outcome
            && let Some((_, action)) = entries.into_iter().nth(index)
        {
            match action {
                PaletteAction::Request(request) => {
                    if let Some(request) = self.handle(request, io, now) {
                        io.requests.push(request);
                    }
                }
                PaletteAction::Undo(redo) => self.undo(io, redo),
                PaletteAction::Applet(id, command) => {
                    let view = self.view(now, 0.0);
                    let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &id));
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id)
                        && !applet.command(&command, &mut cx)
                    {
                        cx.fail(tr!("is-not-available", command = command));
                    }
                }
            }
        }
    }
}
