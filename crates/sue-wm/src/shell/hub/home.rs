//! The Hub's Home: New and Open, a card per studio (and per tool), the
//! work to jump back into, and the buttons of Your work, which open the
//! Files place already sorted.
//!
//! With no work to jump back into (a first run), Home does not leave a
//! blank half page: Start Something New offers the studios' kinds of
//! work, each one click from a new file, and Good to Know shows three
//! things worth knowing on day one (the command search, opening files
//! by dropping them, and Settings), each a button that does it.

use egui::{Align, Layout, Rect, Ui, UiBuilder, Vec2, vec2};

use super::kit::{self, Studio};
use super::{HubAct, HubButton, WorkShelf};
use crate::files::{Item, Type, texture_key};
use crate::icons::Icon;
use crate::ui::{Density, ListRow, Space, TextRole, Thumbnail, ThumbnailCard, Tip};
use crate::{AppletId, AppletRole, ItemState, MenuItem, ShellRequest, Theme};

use super::super::{Shell, ShellIo, pinned, templates};

/// The widest Home grows; past it the page keeps a calm column in the
/// middle.
const COLUMN: f32 = 1240.0;

impl Shell {
    /// Home, in `pane`.
    pub(super) fn home_ui(
        &mut self,
        ui: &mut Ui,
        pane: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let margin = Self::hub_margin(narrow, theme);
        let gap = theme.space(if narrow { Space::S4 } else { Space::S5 });
        egui::ScrollArea::vertical()
            .id_salt("sue-hub-home")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let room = (ui.available_width() - margin * 2.0).max(0.0);
                let width = room.min(COLUMN);
                let left = pane.left() + margin + (room - width) / 2.0;
                let top = ui.cursor().top() + margin * if narrow { 1.0 } else { 0.8 };
                let mut column = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_size(
                            egui::pos2(left, top),
                            vec2(width, f32::INFINITY),
                        ))
                        .layout(Layout::top_down(Align::Min)),
                );
                column.spacing_mut().item_spacing = Vec2::splat(gap);
                self.home_header(&mut column, width, io, theme, narrow, acts);
                column.add_space(gap);
                self.home_studios(&mut column, width, io, theme, narrow, acts);
                // On a narrow screen Your work comes before the pictures,
                // so its buttons are in sight without scrolling.
                if narrow {
                    self.home_work(&mut column, width, io, theme, narrow, acts);
                    column.add_space(gap);
                    if !self.home_jump(&mut column, width, io, theme, narrow, acts) {
                        self.home_start(&mut column, width, io, theme, narrow, acts);
                        self.home_tips(&mut column, width, theme, narrow, acts);
                    }
                } else if self.home_jump(&mut column, width, io, theme, narrow, acts) {
                    self.home_work(&mut column, width, io, theme, narrow, acts);
                } else {
                    // A first run: kinds of work to start, Your work, then
                    // what is good to know.
                    self.home_start(&mut column, width, io, theme, narrow, acts);
                    self.home_work(&mut column, width, io, theme, narrow, acts);
                    column.add_space(gap);
                    self.home_tips(&mut column, width, theme, narrow, acts);
                }
                // Room at the end for the notices, so nothing has to stay
                // under them: the page scrolls clear of the stack.
                column.add_space(margin.max(self.toasts.covered()));
                // The column's height, so the scroll area knows it.
                let used = column.min_rect();
                ui.allocate_space(vec2(
                    ui.available_width(),
                    used.bottom() - ui.cursor().top(),
                ));
            });
    }

    /// The page's name, then New and Open.
    fn home_header(
        &mut self,
        ui: &mut Ui,
        width: f32,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let height = Density::of(ui)
            .icon_button()
            .max(theme.text_size(TextRole::Display) * 1.4);
        let (row, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
        ui.painter().text(
            row.left_center(),
            egui::Align2::LEFT_CENTER,
            if narrow {
                tr!("hub-home")
            } else {
                tr!("hub-welcome")
            },
            theme.text_font(TextRole::Display),
            theme.text,
        );
        let mut buttons = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
        );
        buttons.spacing_mut().item_spacing.x = theme.space(Space::S4);
        let open = crate::ui::button("sue-hub-open", &tr!("hub-open"))
            .icon(Icon::FileOpen)
            .hint(&tr!("hub-open-hint"))
            .show(&mut buttons, theme);
        self.hub.buttons.push((HubButton::Open, open.rect));
        if open.clicked() {
            acts.push(HubAct::request(ShellRequest::Command(
                "file.open".to_owned(),
            )));
        }
        let new = crate::ui::button("sue-hub-new", &tr!("hub-new"))
            .primary()
            .icon(Icon::Plus)
            .hint(&tr!("hub-new-hint"))
            .show(&mut buttons, theme);
        self.hub.buttons.push((HubButton::New, new.rect));
        // In reading order: New, then Open.
        self.hub.walk.push(new.clone());
        self.hub.walk.push(open.clone());
        // New's menu: every studio's kinds of work, by category.
        let all = templates(io.applets, &io.world.rice);
        let mut kinds: Vec<(String, Vec<MenuItem>)> = Vec::new();
        for (index, (_, template)) in all.iter().enumerate() {
            let item = MenuItem::action(template.label.clone(), format!("hub-new:{index}"));
            match kinds
                .iter_mut()
                .find(|(kind, _)| *kind == template.category)
            {
                Some((_, list)) => list.push(item),
                None => kinds.push((template.category.clone(), vec![item])),
            }
        }
        let items: Vec<MenuItem> = kinds
            .into_iter()
            .map(|(kind, list)| MenuItem::sub(kind, list))
            .collect();
        let picked = self.hub.new_menu.dropdown(
            ui,
            new.rect,
            new.clicked(),
            &items,
            &|_| ItemState::on(),
            theme,
        );
        if let Some((applet, template)) = picked
            .and_then(|command| command.strip_prefix("hub-new:")?.parse::<usize>().ok())
            .and_then(|index| all.get(index))
        {
            acts.push(HubAct::request(ShellRequest::Create {
                applet: applet.clone(),
                key: template.key.clone(),
            }));
        }
    }

    /// A card per studio, then per tool, each under its heading.
    fn home_studios(
        &mut self,
        ui: &mut Ui,
        width: f32,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let density = Density::of(ui);
        let gap = ui.spacing().item_spacing.x;
        let pinned = pinned(&io.world.rice, io.applets);
        for (title, role) in [
            (tr!("studios"), AppletRole::Studio),
            (tr!("tools"), AppletRole::Tool),
        ] {
            let cards: Vec<(AppletId, String, String, Icon)> = io
                .applets
                .with_role(role)
                .map(|manifest| {
                    let _language =
                        sue_lang::scope(&crate::applet_language(&io.world.rice, &manifest.id));
                    (
                        manifest.id.clone(),
                        manifest.title(),
                        manifest.blurb(),
                        manifest.icon,
                    )
                })
                .collect();
            if cards.is_empty() {
                continue;
            }
            kit::heading(ui, width, &title, None, theme);
            let (columns, each, height) = if narrow {
                let height = density.icon_button() * 1.5 + theme.card_padding() * 2.0;
                (1, width, height)
            } else {
                let least = theme.space(Space::S9) * 7.0;
                let (columns, each) = kit::grid(width, least, gap);
                let height = density.row() * 1.6
                    + theme.card_padding() * 2.0
                    + theme.space(Space::S5)
                    + theme.text_size(TextRole::Title) * 1.4
                    + theme.space(Space::S2)
                    + theme.text_size(TextRole::Caption) * 2.8;
                (columns, each, height)
            };
            for row in cards.chunks(columns) {
                ui.horizontal(|ui| {
                    for (id, name, summary, icon) in row {
                        let out = kit::studio_card(
                            ui,
                            vec2(each, height),
                            &Studio {
                                name,
                                summary,
                                icon: *icon,
                                pinned: pinned.as_ref() == Some(id),
                            },
                            id.as_str(),
                            theme,
                        );
                        self.hub_boxes.push((id.clone(), out.card, out.pin));
                        self.hub.walk.push(out.response.clone());
                        if out.toggle_pin {
                            acts.push(HubAct::Pin(id.clone()));
                        } else if out.open {
                            acts.push(HubAct::Applet(id.clone()));
                        }
                    }
                });
            }
            ui.add_space(gap);
        }
    }

    /// The newest work, as cards with pictures, and All Files. Shown only
    /// when there is any, so a first run is not a page of empty boxes;
    /// returns whether it showed.
    fn home_jump(
        &mut self,
        ui: &mut Ui,
        width: f32,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) -> bool {
        let count = self.config.recent_count;
        if count == 0 {
            return false;
        }
        let studios = super::files::studios(io);
        let mut recent: Vec<Item> = crate::files::items(io.work, &studios)
            .into_iter()
            .filter(|item| item.kind != Type::Unsaved)
            .collect();
        recent.sort_by_key(|item| std::cmp::Reverse(item.when));
        if recent.is_empty() {
            return false;
        }
        let density = Density::of(ui);
        let gap = ui.spacing().item_spacing.x;
        if let Some(all) = kit::heading(
            ui,
            width,
            &tr!("hub-jump-back-in"),
            Some((&tr!("hub-all-files"), Icon::ChevronRight)),
            theme,
        ) {
            self.hub.buttons.push((HubButton::AllFiles, all.rect));
            self.hub.walk.push(all.clone());
            if all.clicked() {
                acts.push(HubAct::Work(WorkShelf::Recent));
            }
        }
        let least = if narrow {
            theme.space(Space::S9) * 3.5
        } else {
            theme.space(Space::S9) * 4.5
        };
        let (columns, each) = kit::grid(width, least, gap);
        // One row: a strip of the newest, All Files has the rest.
        let shown = columns.min(count).min(recent.len());
        let footer = ListRow::height(theme, density, true);
        let size = vec2(each, (each * 0.75).round() + footer);
        let now = crate::unix_now();
        for chunk in recent[..shown].chunks(columns) {
            ui.horizontal(|ui| {
                for item in chunk {
                    let thumbnail = self
                        .hub
                        .thumbs
                        .get_for(&item.path, item.studio.as_ref())
                        .and_then(|(at, thumb)| {
                            io.textures
                                .texture(
                                    texture_key(&item.path.display().to_string()),
                                    at.max(1),
                                    thumb.size,
                                    4,
                                    &thumb.pixels,
                                    false,
                                )
                                .map(|texture| Thumbnail::Picture(texture, thumb.size))
                        })
                        .unwrap_or(Thumbnail::Placeholder(item.kind.glyph().icon()));
                    let detail = crate::ago(item.when, now);
                    let out = ThumbnailCard::new(("sue-hub-jump", &item.path), &item.title)
                        .detail(&detail)
                        .thumbnail(thumbnail)
                        .size(size)
                        .show(ui, theme);
                    let response = out
                        .response
                        .on_hover_text(sue_platform::shown_path(&item.path));
                    self.hub
                        .buttons
                        .push((HubButton::Jump(item.path.clone()), response.rect));
                    self.hub.walk.push(response.clone());
                    if response.clicked() {
                        acts.push(HubAct::OpenFile(item.path.clone()));
                    }
                }
            });
        }
        ui.add_space(gap);
        true
    }

    /// Start Something New: a row per studio of its first kinds of work,
    /// each a button that makes the file and opens its studio. New has
    /// them all.
    fn home_start(
        &mut self,
        ui: &mut Ui,
        width: f32,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        // Each studio's kinds of work, in its own order; the one that
        // asks for a size first stays in New's menu.
        let mut lists: Vec<(AppletId, String, Icon, Vec<crate::FileTemplate>)> = Vec::new();
        for (id, template) in templates(io.applets, &io.world.rice) {
            if template.key == "custom" {
                continue;
            }
            if let Some((.., list)) = lists.iter_mut().find(|(known, ..)| *known == id) {
                list.push(template);
                continue;
            }
            let Some(manifest) = io.applets.manifest(&id) else {
                continue;
            };
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &id));
            let (title, icon) = (manifest.title(), manifest.icon);
            lists.push((id, title, icon, vec![template]));
        }
        if lists.is_empty() {
            return;
        }
        let density = Density::of(ui);
        let gap = ui.spacing().item_spacing.x;
        kit::heading(ui, width, &tr!("hub-start-new"), None, theme);
        // A row per studio, in the columns of the studio cards above, so
        // each row reads as that studio's: its first kinds of work.
        let (columns, each) = if narrow {
            (2, ((width - gap) / 2.0).floor())
        } else {
            kit::grid(width, theme.space(Space::S9) * 7.0, gap)
        };
        let rows: Vec<Vec<(AppletId, String, Icon, crate::FileTemplate)>> = lists
            .into_iter()
            .map(|(id, studio, icon, list)| {
                list.into_iter()
                    .take(columns)
                    .map(|template| (id.clone(), studio.clone(), icon, template))
                    .collect()
            })
            .collect();
        let height = density.icon_button() + theme.space(Space::S5) * 2.0;
        for row in &rows {
            ui.horizontal(|ui| {
                for (id, studio, icon, template) in row {
                    let salt = format!("start-{}-{}", id.as_str(), template.key);
                    let help = tr!(
                        "hub-start-new-tip",
                        kind = template.label.clone(),
                        studio = studio.clone()
                    );
                    let tip = Tip::new(&template.label).help(&help);
                    let response = kit::work_button(
                        ui,
                        vec2(each, height),
                        (*icon, &template.label, studio),
                        false,
                        &tip,
                        &salt,
                        theme,
                    );
                    let button = HubButton::Start(id.clone(), template.key.clone());
                    self.hub.buttons.push((button, response.rect));
                    self.hub.walk.push(response.clone());
                    if response.clicked() {
                        acts.push(HubAct::request(ShellRequest::Create {
                            applet: id.clone(),
                            key: template.key.clone(),
                        }));
                    }
                }
            });
        }
        ui.add_space(gap);
    }

    /// Good to Know: the command search (with its key), opening files by
    /// dropping them, and Settings, each a tile that does it.
    fn home_tips(
        &mut self,
        ui: &mut Ui,
        width: f32,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let density = Density::of(ui);
        let gap = ui.spacing().item_spacing.x;
        kit::heading(ui, width, &tr!("hub-good-to-know"), None, theme);
        // On the Hub no studio keeps a key for itself.
        let keymap = &self.config.keymap;
        let key = Some(Self::shown_key(
            &[],
            &[&keymap.command_palette, &keymap.command_palette_alt],
        ))
        .filter(|key| !key.is_empty());
        let search = key.as_ref().map_or_else(
            || tr!("hub-tip-search-plain"),
            |key| tr!("hub-tip-search-detail", key = key.clone()),
        );
        let tips = [
            (
                HubButton::FindCommands,
                Icon::Search,
                tr!("hub-tip-search"),
                search,
                ShellRequest::Palette,
            ),
            (
                HubButton::OpenFiles,
                Icon::FileOpen,
                tr!("hub-tip-open"),
                tr!("hub-tip-open-detail"),
                ShellRequest::Command("file.open".to_owned()),
            ),
            (
                HubButton::MakeItYours,
                Icon::Settings,
                tr!("hub-tip-yours"),
                tr!("hub-tip-yours-detail"),
                ShellRequest::OpenSettings(None),
            ),
        ];
        let (columns, each) = if narrow {
            (1, width)
        } else {
            kit::balanced_grid(width, theme.space(Space::S9) * 5.0, gap, tips.len())
        };
        let height = density.icon_button() + theme.space(Space::S5) * 2.0;
        for row in tips.chunks(columns) {
            ui.horizontal(|ui| {
                for (button, icon, name, detail, request) in row {
                    let salt = format!("{button:?}");
                    // The search's tip names its key too.
                    let tip = match (button, &key) {
                        (HubButton::FindCommands, Some(key)) => Tip::new(name).shortcut(key),
                        _ => Tip::new(name),
                    };
                    let response = kit::work_button(
                        ui,
                        vec2(each, height),
                        (*icon, name, detail),
                        false,
                        &tip,
                        &salt,
                        theme,
                    );
                    self.hub.buttons.push((button.clone(), response.rect));
                    self.hub.walk.push(response.clone());
                    if response.clicked() {
                        acts.push(HubAct::request(request.clone()));
                    }
                }
            });
        }
    }

    /// Your work: Recent, Projects, Unsaved work, Favorites and, where
    /// the system has a file manager, Folder.
    fn home_work(
        &mut self,
        ui: &mut Ui,
        width: f32,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        narrow: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let density = Density::of(ui);
        let gap = ui.spacing().item_spacing.x;
        kit::heading(ui, width, &tr!("hub-your-work"), None, theme);
        let studios = super::files::studios(io);
        let items = crate::files::items(io.work, &studios);
        let count = |shelf: WorkShelf| match shelf {
            WorkShelf::Recent => items
                .iter()
                .filter(|item| item.kind != Type::Unsaved)
                .count(),
            WorkShelf::Projects => items
                .iter()
                .filter(|item| item.kind == Type::Project)
                .count(),
            WorkShelf::Unsaved => items
                .iter()
                .filter(|item| item.kind == Type::Unsaved)
                .count(),
            WorkShelf::Favorites => items.iter().filter(|item| item.favorite).count(),
        };
        let mut buttons: Vec<(HubButton, Icon, String, String, bool)> = WorkShelf::ALL
            .into_iter()
            .map(|shelf| {
                let n = count(shelf);
                let detail = if shelf == WorkShelf::Unsaved && n > 0 {
                    tr!("hub-to-recover", count = n)
                } else {
                    tr!("hub-count", count = n)
                };
                let attention = shelf == WorkShelf::Unsaved && n > 0;
                (
                    HubButton::Work(shelf),
                    shelf.icon(),
                    shelf.label(),
                    detail,
                    attention,
                )
            })
            .collect();
        let home = io.catalog.folders.get("home").cloned();
        if sue_platform::file_manager().is_some() && home.is_some() {
            buttons.push((
                HubButton::Folder,
                Icon::FolderReveal,
                tr!("hub-folder"),
                tr!("hub-folder-detail"),
                false,
            ));
        }
        let least = theme.space(Space::S9) * 5.0;
        let (columns, each) = if narrow {
            (2, ((width - gap) / 2.0).floor())
        } else {
            kit::balanced_grid(width, least, gap, buttons.len())
        };
        let height = density.icon_button() + theme.space(Space::S5) * 2.0;
        for row in buttons.chunks(columns) {
            ui.horizontal(|ui| {
                for (button, icon, name, detail, attention) in row {
                    let salt = format!("{button:?}");
                    let response = kit::work_button(
                        ui,
                        vec2(each, height),
                        (*icon, name, detail),
                        *attention,
                        &Tip::new(name),
                        &salt,
                        theme,
                    );
                    self.hub.buttons.push((button.clone(), response.rect));
                    self.hub.walk.push(response.clone());
                    if response.clicked() {
                        match button {
                            HubButton::Work(shelf) => acts.push(HubAct::Work(*shelf)),
                            HubButton::Folder => {
                                if let Some(home) = &home {
                                    acts.push(HubAct::request(ShellRequest::Command(format!(
                                        "reveal:{}",
                                        home.display()
                                    ))));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });
        }
    }
}
