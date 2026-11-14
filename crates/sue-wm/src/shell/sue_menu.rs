//! The Sue menu: what it lists, opening it, and running what was picked.

use sue_platform::FormFactor;
use sue_platform::time::Instant;
use sue_rice::{RiceValue, SetRiceValue};

use super::{INTERFACE_KEY, PHONE_FRAME_KEY, Shell, ShellIo, layout_name, pinned, templates};
use crate::{AppletRole, FilePick, MenuItem, RiceEdit, ShellRequest, WmConfig, sue_menu};

/// An item of the Sue menu that runs `request`.
fn sue_item(
    runs: &mut Vec<(ShellRequest, bool)>,
    label: impl Into<String>,
    request: ShellRequest,
    checked: bool,
) -> MenuItem {
    runs.push((request, checked));
    MenuItem::action(label, format!("sue:{}", runs.len() - 1))
}

/// The title of a Sue menu submenu: Sue's own in the language in use,
/// an applet's own as it named it (in its language).
fn submenu_title(name: &str) -> String {
    match name {
        sue_menu::FILE => tr!("menu-file"),
        sue_menu::PROJECT => tr!("menu-project"),
        sue_menu::SEND_TO => tr!("menu-send-to"),
        other => other.to_owned(),
    }
}

impl Shell {
    /// Sets Sue's language (a tag, or [`sue_lang::SYSTEM`]) as an undoable
    /// rice edit; every studio and tool that follows Sue follows it at once.
    pub fn set_language(&mut self, setting: &str, io: &mut ShellIo<'_>) {
        if io.world.rice.get_str(crate::LANGUAGE_KEY).map(str::trim) == Some(setting) {
            return;
        }
        let txn = SetRiceValue::new(crate::LANGUAGE_KEY, RiceValue::Text(setting.to_owned()));
        io.typing.flush(io.history);
        match io.apply_sue(RiceEdit(txn)) {
            Ok(_) => {
                let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
                self.notify(
                    tr!(
                        "language-changed",
                        language = sue_lang::name(&sue_lang::current())
                    ),
                    false,
                );
            }
            Err(error) => self.notify(tr!("could-not-change-language", error = error), true),
        }
    }

    /// Sets the layout the whole app uses (`auto`, `desktop`, `tablet` or
    /// `phone`) as an undoable rice edit; the shell, the Hub and every
    /// studio and tool take it on the next frame.
    pub fn set_interface(&mut self, setting: &str, io: &mut ShellIo<'_>) {
        self.edit_display(INTERFACE_KEY, RiceValue::Text(setting.to_owned()), io);
    }

    /// Keeps the phone layout on a computer to a phone-wide column, or
    /// lets it fill the window, as an undoable rice edit.
    pub fn set_phone_frame(&mut self, on: bool, io: &mut ShellIo<'_>) {
        self.edit_display(PHONE_FRAME_KEY, RiceValue::Bool(on), io);
    }

    fn edit_display(&mut self, key: &str, value: RiceValue, io: &mut ShellIo<'_>) {
        let txn = SetRiceValue::new(key, value);
        io.typing.flush(io.history);
        match io.apply_sue(RiceEdit(txn)) {
            Ok(_) => {
                // The shell follows at once; the engine reads the rice again
                // for everything else.
                if let Ok(config) = WmConfig::from_rice(&io.world.rice) {
                    self.config = config;
                }
                let layout = layout_name(self.form_factor());
                self.notify(tr!("layout-changed", layout = layout), false);
            }
            Err(error) => self.notify(tr!("could-not-change-layout", error = error), true),
        }
    }

    /// Builds the Sue menu for what is open: the Hub, the studios and
    /// tools, new files, files, projects, Send To for what the page works
    /// on, every submenu applets add to, the pin, the palette, Settings.
    pub(super) fn build_sue_menu(&mut self, io: &ShellIo<'_>) {
        let _language = sue_lang::scope(&crate::sue_language(&io.world.rice));
        let mut runs = Vec::new();
        let run = &mut runs;
        let keymap = &self.config.keymap;
        let mut items = vec![sue_item(
            run,
            tr!("hub"),
            ShellRequest::Hub,
            self.page.is_none(),
        )];
        for (title, role) in [
            (tr!("studios"), AppletRole::Studio),
            (tr!("tools"), AppletRole::Tool),
        ] {
            let pages: Vec<MenuItem> = io
                .applets
                .with_role(role)
                .map(|manifest| {
                    sue_item(
                        run,
                        manifest.title(),
                        ShellRequest::Open(manifest.id.clone()),
                        self.page.as_ref() == Some(&manifest.id),
                    )
                })
                .collect();
            if !pages.is_empty() {
                items.push(MenuItem::sub(title, pages));
            }
        }
        items.push(MenuItem::Separator);
        // New: a submenu per kind of file, each with its exact kinds.
        let mut kinds: Vec<(String, Vec<MenuItem>)> = Vec::new();
        for (applet, template) in templates(io.applets, &io.world.rice) {
            let item = sue_item(
                run,
                template.label,
                ShellRequest::Create {
                    applet,
                    key: template.key,
                },
                false,
            );
            match kinds
                .iter_mut()
                .find(|(kind, _)| *kind == template.category)
            {
                Some((_, list)) => list.push(item),
                None => kinds.push((template.category, vec![item])),
            }
        }
        if !kinds.is_empty() {
            items.push(MenuItem::sub(
                tr!("new"),
                kinds
                    .into_iter()
                    .map(|(kind, list)| MenuItem::sub(kind, list))
                    .collect(),
            ));
        }
        // The named submenus, Sue's own first.
        let mut named: Vec<(String, Vec<MenuItem>)> = vec![
            (sue_menu::FILE.to_owned(), {
                let mut file = vec![
                    sue_item(
                        run,
                        tr!("open-file-ellipsis"),
                        ShellRequest::Command("file.open".to_owned()),
                        false,
                    )
                    .key(keymap.open_file.to_string()),
                ];
                let recent: Vec<MenuItem> = io
                    .work
                    .recent()
                    .take(10)
                    .map(|recent| {
                        let name = recent.path.file_name().map_or_else(
                            || recent.path.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        sue_item(
                            run,
                            name,
                            ShellRequest::OpenFile(recent.path.clone()),
                            false,
                        )
                    })
                    .collect();
                if !recent.is_empty() {
                    file.push(MenuItem::sub(tr!("open-recent"), recent));
                }
                file
            }),
            (
                sue_menu::PROJECT.to_owned(),
                vec![
                    sue_item(
                        run,
                        tr!("open-project-ellipsis"),
                        ShellRequest::PickFile(
                            FilePick::open(None, "open", &tr!("open-project")).extensions(&["sue"]),
                        ),
                        false,
                    ),
                    sue_item(
                        run,
                        tr!("save-project"),
                        ShellRequest::Command("file.save".to_owned()),
                        false,
                    )
                    .key(keymap.save.to_string()),
                    sue_item(
                        run,
                        tr!("save-project-as-ellipsis"),
                        ShellRequest::Command("file.save_as".to_owned()),
                        false,
                    ),
                ],
            ),
            (sue_menu::SEND_TO.to_owned(), Vec::new()),
        ];
        let add = |named: &mut Vec<(String, Vec<MenuItem>)>, menu: &str, item: MenuItem| match named
            .iter_mut()
            .find(|(name, _)| name == menu)
        {
            Some((_, list)) => list.push(item),
            None => named.push((menu.to_owned(), vec![item])),
        };
        // What the page works on, into every other studio and tool that
        // takes it, then whatever each applet offers.
        let focus_id = self
            .page
            .as_ref()
            .and_then(|page| io.applets.get(page))
            .and_then(|applet| applet.focus());
        let focus = focus_id.and_then(|id| io.world.arena.get_by_id(id).ok());
        if let (Some(id), Some(entity)) = (focus_id, focus) {
            for (manifest, applet) in io.applets.iter() {
                let current = self.page.as_ref() == Some(&manifest.id);
                if manifest.role.is_page() && !current && applet.accepts(entity) {
                    let item = sue_item(
                        run,
                        manifest.title(),
                        ShellRequest::OpenIn {
                            entity: id,
                            applet: manifest.id.clone(),
                        },
                        false,
                    );
                    add(&mut named, sue_menu::SEND_TO, item);
                }
            }
        }
        for (manifest, applet) in io.applets.iter() {
            let current = self.page.as_ref() == Some(&manifest.id);
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &manifest.id));
            for action in applet.sue_actions(focus, current) {
                let item = sue_item(run, action.label, action.request, false);
                add(&mut named, &action.menu, item);
            }
        }
        items.extend(
            named
                .into_iter()
                .filter(|(_, list)| !list.is_empty())
                .map(|(name, list)| MenuItem::sub(submenu_title(&name), list)),
        );
        // Language: Sue's own, which every studio and tool follows unless
        // set apart in its own settings.
        let setting = io
            .world
            .rice
            .get_str(crate::LANGUAGE_KEY)
            .map_or(sue_lang::SYSTEM, str::trim)
            .to_owned();
        let languages = crate::sue_language_choices()
            .into_iter()
            .map(|choice| {
                let on = choice.value == setting;
                sue_item(run, choice.label, ShellRequest::Language(choice.value), on)
            })
            .collect();
        items.push(MenuItem::sub(tr!("language-menu"), languages));
        // Layout: the device's own, or the desktop, tablet or phone
        // layout for the whole app. Phones and tablets switch only between
        // their two.
        let interface = self.config.interface.as_str();
        let chosen = |value: &str| match value {
            "desktop" | "tablet" | "phone" => interface == value,
            _ => !matches!(interface, "desktop" | "tablet" | "phone"),
        };
        let mut layouts = vec![sue_item(
            run,
            tr!("layout-automatic", layout = layout_name(self.form_factor)),
            ShellRequest::Interface("auto".to_owned()),
            chosen("auto"),
        )];
        let touch = self.form_factor.is_touch();
        for (value, form) in [
            ("desktop", FormFactor::Desktop),
            ("tablet", FormFactor::Tablet),
            ("phone", FormFactor::Mobile),
        ] {
            if touch && form == FormFactor::Desktop {
                continue;
            }
            layouts.push(sue_item(
                run,
                layout_name(form),
                ShellRequest::Interface(value.to_owned()),
                chosen(value),
            ));
        }
        if !touch && self.form_factor() == FormFactor::Mobile {
            layouts.push(MenuItem::Separator);
            layouts.push(sue_item(
                run,
                tr!("layout-phone-shaped"),
                ShellRequest::PhoneFrame(!self.config.phone_frame),
                self.config.phone_frame,
            ));
        }
        items.push(MenuItem::sub(tr!("layout-menu"), layouts));
        items.push(MenuItem::Separator);
        if let Some(page) = self.page.clone() {
            let pinned = pinned(&io.world.rice, io.applets).as_ref() == Some(&page);
            let name = io
                .applets
                .manifest(&page)
                .map_or_else(String::new, |manifest| manifest.title());
            items.push(sue_item(
                run,
                tr!("open-at-launch", name = name),
                ShellRequest::Pin((!pinned).then_some(page)),
                pinned,
            ));
        }
        let palette = tr!("command-palette", command_palette = keymap.command_palette);
        items.push(sue_item(run, palette, ShellRequest::Palette, false));
        items.push(sue_item(
            run,
            tr!("check-for-updates"),
            ShellRequest::Command("app.check_updates".to_owned()),
            false,
        ));
        items.push(
            sue_item(
                run,
                tr!("settings"),
                ShellRequest::OpenSettings(None),
                false,
            )
            .key(keymap.open_settings.to_string()),
        );
        items.push(
            sue_item(
                run,
                tr!("quit-sue"),
                ShellRequest::Command("app.quit".to_owned()),
                false,
            )
            .key(keymap.quit.to_string()),
        );
        self.sue_items = items;
        self.sue_runs = runs;
    }

    /// Opens the Sue menu under the Sue button, built for what is open.
    pub fn open_sue_menu(&mut self, io: &ShellIo<'_>) {
        if let Some(palette) = self.palette.as_mut() {
            palette.close();
        }
        self.build_sue_menu(io);
        self.sue_menu.open_dropdown();
        self.touch();
    }

    /// The items of the Sue menu (with `path` naming the submenus to go
    /// through), as labels (scripted runs).
    #[must_use]
    pub fn sue_menu_labels(&self, path: &[&str]) -> Vec<String> {
        let mut items = &self.sue_items;
        for step in path {
            let found = items.iter().find_map(|item| match item {
                MenuItem::Sub { label, items } if label == step => Some(items),
                _ => None,
            });
            match found {
                Some(next) => items = next,
                None => return Vec::new(),
            }
        }
        items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { label, .. } | MenuItem::Sub { label, .. } => Some(label.clone()),
                MenuItem::Separator => None,
            })
            .collect()
    }

    /// Picks the Sue menu's item at `path` (its submenus, then its label),
    /// as a click would (scripted runs). Returns `false` if there is none.
    pub fn sue_menu_pick(&mut self, path: &[&str], io: &mut ShellIo<'_>, now: Instant) -> bool {
        let Some((last, submenus)) = path.split_last() else {
            return false;
        };
        let mut items = &self.sue_items;
        for step in submenus {
            let found = items.iter().find_map(|item| match item {
                MenuItem::Sub { label, items } if label == step => Some(items),
                _ => None,
            });
            match found {
                Some(next) => items = next,
                None => return false,
            }
        }
        let command = items.iter().find_map(|item| match item {
            MenuItem::Action { label, command, .. } if label == last => Some(command.clone()),
            _ => None,
        });
        match command {
            Some(command) => {
                self.run_sue_item(&command, io, now);
                true
            }
            None => false,
        }
    }

    pub(super) fn run_sue_item(&mut self, command: &str, io: &mut ShellIo<'_>, now: Instant) {
        self.sue_menu.close();
        let request = command
            .strip_prefix("sue:")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| self.sue_runs.get(index))
            .map(|(request, _)| request.clone());
        if let Some(request) = request
            && let Some(leftover) = self.handle(request, io, now)
        {
            io.requests.push(leftover);
        }
        self.touch();
    }
}
