//! The Hub's Files place: the shared browser over the work of every
//! studio, its details popup, and the menu of upkeep that came from Sue
//! Paint's launcher (rebuild thumbnails, forget missing files, clear
//! finished transfers).

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, vec2};

use super::{HubAct, HubButton, HubRow, WorkShelf};
use crate::files::{Event, Item, ProjectKey, ProjectsState, Shelf, Spot, Studio, Type};
use crate::icons::Icon;
use crate::ui::{Density, Space, TextRole};
use crate::{AppletRole, ItemState, MenuItem, ShellRequest, ShellView, Theme};

use super::super::{Shell, ShellIo, split};

/// The upkeep menu's commands.
const REBUILD: &str = "hub-upkeep:thumbnails";
const FORGET: &str = "hub-upkeep:missing";
const CLEAR: &str = "hub-upkeep:transfers";

/// Every studio (and tool that fills the page) whose work the Hub lists:
/// a file belongs to the studio it was last worked on in.
pub(super) fn studios<'a>(io: &ShellIo<'_>) -> Vec<Studio<'a>> {
    io.applets
        .manifests()
        .filter(|manifest| manifest.role == AppletRole::Studio || manifest.role.is_page())
        .map(|manifest| {
            let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, &manifest.id));
            Studio {
                id: manifest.id.clone(),
                name: manifest.title(),
                opens: None,
            }
        })
        .collect()
}

/// The browser's state for a shelf of Your work.
pub(super) fn state_for(shelf: WorkShelf) -> ProjectsState {
    let mut state = ProjectsState::default();
    match shelf {
        WorkShelf::Recent => {}
        WorkShelf::Projects => {
            state.types.insert(Type::Project as u8);
        }
        WorkShelf::Unsaved => {
            state.types.insert(Type::Unsaved as u8);
        }
        WorkShelf::Favorites => state.shelf = Shelf::Favorite,
    }
    state
}

/// The work that is gone from disk: every remembered file (not kept work)
/// whose file is missing.
pub(super) fn missing(items: &[Item]) -> Vec<Item> {
    items
        .iter()
        .filter(|item| item.kind != Type::Unsaved && !sue_platform::fs::exists(&item.path))
        .cloned()
        .collect()
}

/// The Files place's menu.
fn upkeep_items() -> [MenuItem; 3] {
    [
        MenuItem::action(tr!("hub-rebuild-thumbnails"), REBUILD),
        MenuItem::action(tr!("hub-forget-missing"), FORGET),
        MenuItem::action(tr!("start-clear-completed"), CLEAR),
    ]
}

impl Shell {
    /// Runs the entry of the Files place's menu that reads `label`, as a
    /// click on it does (scripted runs and tests). Returns `false` when
    /// the menu has no such entry.
    pub fn hub_files_menu_pick(&mut self, label: &str, io: &mut ShellIo<'_>) -> bool {
        let command = upkeep_items().into_iter().find_map(|item| match item {
            MenuItem::Action {
                label: name,
                command,
                ..
            } if name == label => Some(command),
            _ => None,
        });
        match command {
            Some(command) => {
                self.hub_upkeep(&command, io);
                true
            }
            None => false,
        }
    }

    /// The Files place, in `pane`. `full` shows popups as sheets.
    #[allow(clippy::too_many_arguments, reason = "one place of the Hub")]
    pub(super) fn files_ui(
        &mut self,
        ui: &mut Ui,
        pane: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        view: ShellView,
        full: bool,
        acts: &mut Vec<HubAct>,
    ) {
        let narrow = pane.width() < super::NARROW;
        let margin = Self::hub_margin(narrow, theme);
        let density = Density::of(ui);
        // The title row: the place's name, and its menu at the right.
        let row = Rect::from_min_size(
            pane.min + vec2(margin, margin * if narrow { 1.0 } else { 0.6 }),
            vec2(pane.width() - margin * 2.0, density.icon_button()),
        );
        ui.painter().text(
            row.left_center(),
            egui::Align2::LEFT_CENTER,
            tr!("hub-files"),
            theme.text_font(TextRole::Display),
            theme.text,
        );
        let mut tools = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let more = crate::ui::icon_button("sue-hub-upkeep", Icon::More, &tr!("hub-upkeep"))
            .show(&mut tools, theme);
        self.hub.buttons.push((HubButton::Upkeep, more.rect));
        self.hub.walk.push(more.clone());
        let items = upkeep_items();
        let picked = self.hub.upkeep.dropdown(
            ui,
            more.rect,
            more.clicked(),
            &items,
            &|_| ItemState::on(),
            theme,
        );
        if let Some(command) = picked {
            self.hub_upkeep(&command, io);
        }

        // The browser under it.
        let body = Rect::from_min_max(
            egui::pos2(
                pane.left()
                    + if narrow {
                        margin
                    } else {
                        margin - theme.space(Space::S5)
                    },
                row.bottom() + theme.space(Space::S4),
            ),
            egui::pos2(
                pane.right()
                    - if narrow {
                        margin
                    } else {
                        margin - theme.space(Space::S5)
                    },
                pane.bottom(),
            ),
        );
        let mut body_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(body)
                .layout(Layout::top_down(Align::Min)),
        );
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let studios = studios(io);
        self.hub.browse.compact = narrow;
        self.hub.browse.hint = Some(tr!("hub-search-files"));
        self.hub.browse.discard_armed = self.discard_armed.take();
        let events = {
            let (_, mut cx) = split(io, theme, view.clone());
            crate::files::browse(
                &mut body_ui,
                body,
                (&mut self.hub.files, &mut self.hub.browse),
                &mut self.hub.thumbs,
                &studios,
                &mut cx,
                theme,
            )
        };
        self.discard_armed = self.hub.browse.discard_armed.take();
        // The arrow keys walk on from the menu to the cards.
        self.hub.walk.append(&mut self.hub.browse.focus);
        for (spot, rect) in &self.hub.browse.drawn {
            let row = match spot {
                Spot::Card(ProjectKey::File(path)) => HubRow::Open(path.clone()),
                Spot::Card(ProjectKey::Kept(id)) | Spot::Recover(id) => HubRow::Recover(id.clone()),
                Spot::Discard(id) => HubRow::Discard(id.clone()),
            };
            self.hub_rows.push((row, *rect));
        }
        for event in events {
            match event {
                Event::Open(item) => match item.key {
                    ProjectKey::Kept(id) => {
                        acts.push(HubAct::request(ShellRequest::Recover(id)));
                    }
                    ProjectKey::File(path) => acts.push(HubAct::OpenFile(path)),
                },
                Event::Details(key) => self.hub.details = Some((key, false)),
            }
        }

        // What an item is, over the page.
        if let Some((key, mut armed)) = self.hub.details.take() {
            let all = crate::files::items(io.work, &studios);
            if let Some(item) = all.into_iter().find(|item| item.key == key) {
                let (_, mut cx) = split(io, theme, view.clone());
                let (outcome, open) = crate::files::project_popup(
                    ui.ctx(),
                    full,
                    &item,
                    &mut self.hub.thumbs,
                    &mut armed,
                    &studios,
                    &mut cx,
                    theme,
                );
                if open {
                    acts.push(HubAct::request(match item.key {
                        ProjectKey::Kept(id) => ShellRequest::Recover(id),
                        ProjectKey::File(path) => ShellRequest::OpenFile(path),
                    }));
                }
                if outcome != crate::start::PopupOutcome::Closed {
                    self.hub.details = Some((key, armed));
                }
            }
        }
        let _ = ui.allocate_rect(pane, Sense::hover());
    }

    /// Runs an entry of the Files place's menu.
    fn hub_upkeep(&mut self, command: &str, io: &mut ShellIo<'_>) {
        match command {
            REBUILD => {
                self.hub.thumbs.clear();
                self.notify(tr!("hub-thumbnails-rebuilt"), false);
            }
            FORGET => {
                let studios = studios(io);
                let gone = missing(&crate::files::items(io.work, &studios));
                for item in &gone {
                    io.requests.push(ShellRequest::Forget(item.path.clone()));
                }
                self.notify(tr!("hub-forgot", count = gone.len()), false);
            }
            CLEAR => io.transfers.clear_completed(),
            _ => {}
        }
        self.touch();
    }
}
