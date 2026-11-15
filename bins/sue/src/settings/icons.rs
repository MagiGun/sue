//! Appearance, Icons: the icon sets to draw with (Sue's own and every
//! pack a person installed), each a card with its own preview, install
//! from a folder or an archive, remove, choose, single icons replaced one
//! by one, and Sue's own set written out as a template for pack makers.
//! Everything takes effect on the next frame, with no restart
//! (`sue_wm::icons::pack`, format in `docs/icon-packs.md`).
//!
//! What a pack's notes say is told apart: a file that could not be read
//! is a problem and is counted as one; a filled twin the pack leaves out
//! is optional (Sue's own twin is drawn for the state that is on), so it
//! is said plainly and never counted as something that failed.

/// Text from the language database's `settings` namespace.
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use std::fmt;
use std::path::Path;

use egui::{Label, Rect, RichText, Sense, Ui, Vec2};
use sue_wm::icons::pack::{self, IconHome, IconSet, Note, Pack, Problem};
use sue_wm::icons::{Icon, IconStyle};
use sue_wm::ui::{self, Density, Radius, RowAction, Space, TextRole, Tip};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use super::{ID, Settings};

/// What a pick of a file or folder is for.
const PICK_FOLDER: &str = "icon-pack-folder";
const PICK_ARCHIVE: &str = "icon-pack-archive";
const PICK_EXPORT: &str = "icon-pack-export";
const PICK_OVERRIDE: &str = "icon-override";

/// How many rows of its icons a card shows before Show All Icons.
const PREVIEW_ROWS: usize = 2;

/// The Icons rows between frames.
#[derive(Default)]
pub(super) struct IconsUi {
    /// The installed packs (each with its set, made once), the one
    /// chosen and the icons replaced one by one, as read from the icon
    /// home; `None` until first shown or after a change, so the page
    /// reads the disk only then, not each frame.
    packs: Option<Installed>,
    /// The set whose whole sheet is open: its id, empty for Sue's own.
    sheet: Option<String>,
    /// The set whose notes are open: its id.
    details: Option<String>,
    /// What the last change said: installed, removed, or what failed.
    message: Option<String>,
    /// The problems found when the set in use was last loaded, in words.
    notes: Vec<String>,
    /// The set in use must be loaded again and shown (after a change made
    /// outside a frame).
    stale: bool,
    /// The name of the icon chosen for a single replacement, as typed.
    replace: String,
    /// The solid twin of that icon is the one replaced.
    solid: bool,
    /// The pack Remove was clicked once for; a second click removes it.
    confirm_remove: Option<String>,
}

impl fmt::Debug for IconsUi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IconsUi")
            .field("packs", &self.packs.as_ref().map(|found| found.packs.len()))
            .field("sheet", &self.sheet)
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

/// What the icon home holds, read once per change.
#[derive(Clone)]
struct Installed {
    /// Each pack with its drawings as a set, for its preview.
    packs: Vec<(Pack, IconSet)>,
    chosen: Option<String>,
    overrides: Vec<(Icon, IconStyle)>,
    /// Sue's own set, for its preview.
    sue: IconSet,
}

impl Installed {
    fn read(home: &IconHome) -> Self {
        Self {
            packs: home
                .packs()
                .into_iter()
                .map(|found| {
                    let set = found.set();
                    (found, set)
                })
                .collect(),
            chosen: home.chosen(),
            overrides: home.overrides(),
            sue: IconSet::sue(),
        }
    }
}

/// `true` for a note that tells of something that could not be read or
/// used. A filled twin left out is not one: the twin is optional.
pub(super) fn is_problem(note: &Note) -> bool {
    !matches!(note.problem, Problem::NoSolid { .. })
}

/// The notes of `notes` that are problems.
fn problems(notes: &[Note]) -> Vec<&Note> {
    notes.iter().filter(|note| is_problem(note)).collect()
}

/// The icons whose filled twins `notes` say were left out.
fn twins_left_out(notes: &[Note]) -> Vec<String> {
    notes
        .iter()
        .filter_map(|note| match &note.problem {
            Problem::NoSolid { name } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// What a change that read `notes` says after `done`: the problems, if
/// there were any.
fn with_problems(done: String, notes: &[Note]) -> String {
    let count = problems(notes).len();
    if count == 0 {
        done
    } else {
        format!("{done} {}", tr!("icons-files-not-read", count = count))
    }
}

/// The icon home Settings works on.
fn home(cx: &AppletContext<'_>) -> Option<IconHome> {
    cx.catalog.folders.get("icons").map(IconHome::new)
}

fn settings_id() -> Option<AppletId> {
    AppletId::new(ID).ok()
}

/// The icon named `name`.
fn icon_named(name: &str) -> Option<Icon> {
    Icon::from_name(name.trim())
}

impl IconsUi {
    /// A file or folder picked for one of the rows.
    pub(super) fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        let Some(home) = home(cx) else {
            return;
        };
        let result = match pick.purpose.as_str() {
            PICK_FOLDER | PICK_ARCHIVE => home.install(path).map(|pack| {
                with_problems(
                    tr!("icons-pack-installed", name = pack.manifest.name.as_str()),
                    &pack.notes,
                )
            }),
            PICK_EXPORT => {
                let bytes = pack::export_archive(&pack::sue_manifest());
                sue_platform::write_atomic(path, &bytes)
                    .map(|()| tr!("icons-exported", path = path.display().to_string()))
                    .map_err(|error| pack::PackError::Io(error.to_string()))
            }
            PICK_OVERRIDE => match icon_named(&self.replace) {
                Some(icon) => {
                    let style = if self.solid && icon.has_solid() {
                        IconStyle::Solid
                    } else {
                        IconStyle::Line
                    };
                    home.set_override_from(icon, style, path).map(|notes| {
                        with_problems(tr!("icons-icon-replaced", name = icon.name()), &notes)
                    })
                }
                None => return,
            },
            _ => return,
        };
        self.message = Some(match result {
            Ok(said) => said,
            Err(error) => tr!("icons-failed", error = error.message()),
        });
        self.packs = None;
        self.sheet = None;
        self.stale = true;
    }

    /// Loads the set in use again and shows it from the next frame on.
    fn show_again(&mut self, ui: &Ui, home: &IconHome) {
        self.notes = pack::refresh(ui.ctx(), home)
            .iter()
            .filter(|note| is_problem(note))
            .map(Note::message)
            .collect();
        self.stale = false;
    }
}

/// Asks the system's dialog for a file or a folder for `purpose`.
fn pick(
    cx: &mut AppletContext<'_>,
    purpose: &str,
    title: &str,
    how: impl FnOnce(FilePick) -> FilePick,
) {
    let pick = how(FilePick::open(settings_id(), purpose, title));
    cx.request(ShellRequest::PickFile(pick));
}

/// Text in `role`, muted or not, wrapping at the edge.
fn text(ui: &mut Ui, words: &str, role: TextRole, muted: bool, theme: &Theme) {
    let color = if muted {
        theme.text_muted()
    } else {
        theme.text
    };
    ui.add(
        Label::new(
            RichText::new(words)
                .font(theme.text_font(role))
                .color(color),
        )
        .wrap(),
    );
}

/// A grid of `icons` drawn from `set` (Sue's own where it has none, in
/// the muted color, unless `set` is Sue's own), each a cell that names
/// its icon on hover and shows focus. Gives the icon clicked: the one to
/// replace on its own.
fn sheet(ui: &mut Ui, salt: &str, set: &IconSet, icons: &[Icon], theme: &Theme) -> Option<Icon> {
    let density = Density::of(ui);
    let side = density.icon_button();
    let gap = theme.space(Space::S1);
    let columns = ((ui.available_width() + gap) / (side + gap))
        .floor()
        .max(1.0) as usize;
    let mut clicked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(gap);
        for row in icons.chunks(columns) {
            ui.horizontal(|ui| {
                for icon in row {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
                    let id = ui.make_persistent_id((salt, icon.name()));
                    let response = ui.interact(rect, id, Sense::click());
                    cell(ui, rect, &response, set, *icon, theme, density);
                    let response = ui::tooltip(response, &Tip::new(icon.name()), theme);
                    if response.clicked() {
                        clicked = Some(*icon);
                    }
                }
            });
        }
    });
    clicked
}

/// One cell of a sheet.
fn cell(
    ui: &Ui,
    rect: Rect,
    response: &egui::Response,
    set: &IconSet,
    icon: Icon,
    theme: &Theme,
    density: Density,
) {
    let radius = theme.radius(Radius::Md);
    if response.hovered() {
        ui.painter().rect_filled(rect, radius, theme.fill_hover());
    }
    let drawn = set.is_empty() || set.drawing(icon, IconStyle::Line).is_some();
    let tint = if drawn {
        theme.text
    } else {
        theme.text_muted()
    };
    let picture = Rect::from_center_size(rect.center(), Vec2::splat(density.icon()));
    pack::paint_from(ui.painter(), picture, set, icon, IconStyle::Line, tint);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, icon.name()));
    ui::focus_ring(ui, response, radius, theme);
}

/// The icons a card previews: the ones a pack draws, or the first of
/// Sue's own.
fn preview_icons(set: &IconSet, columns: usize) -> Vec<Icon> {
    let most = columns * PREVIEW_ROWS;
    if set.is_empty() {
        Icon::ALL.iter().copied().take(most).collect()
    } else {
        Icon::ALL
            .iter()
            .copied()
            .filter(|icon| set.drawing(*icon, IconStyle::Line).is_some())
            .take(most)
            .collect()
    }
}

/// How many cells fit across `ui`.
fn columns_of(ui: &Ui, theme: &Theme) -> usize {
    let side = Density::of(ui).icon_button();
    let gap = theme.space(Space::S1);
    ((ui.available_width() + gap) / (side + gap))
        .floor()
        .max(1.0) as usize
}

/// What a card asks for this frame.
#[derive(Default)]
struct CardPress {
    choose: bool,
    sheet: bool,
    details: bool,
    remove: bool,
    picked: Option<Icon>,
}

/// One icon set as it is shown: Sue's own or a pack.
struct SetCard<'a> {
    /// Its id; empty for Sue's own.
    id: &'a str,
    name: &'a str,
    /// Who made it, its licence, its version and how many icons it
    /// draws; `None` for Sue's own.
    about: Option<String>,
    set: &'a IconSet,
    notes: &'a [Note],
    chosen: bool,
    sheet_open: bool,
    details_open: bool,
    asked_remove: bool,
}

impl SetCard<'_> {
    fn show(&self, ui: &mut Ui, theme: &Theme) -> (CardPress, egui::Response) {
        let mut press = CardPress::default();
        let mut radio = None;
        ui::Card::new().show(ui, theme, |ui| {
            ui.set_width(ui.available_width());
            let response =
                ui::radio(("sue-icons-use", self.id), self.name, self.chosen).show(ui, theme);
            let help = tr!("icons-use-hint");
            let response = ui::tooltip(response, &Tip::new(self.name).help(&help), theme);
            press.choose = response.clicked() && !self.chosen;
            radio = Some(response);
            let about = self.about.clone().unwrap_or_else(|| tr!("icons-sue-about"));
            text(ui, &about, TextRole::Caption, true, theme);
            self.notes_ui(ui, theme, &mut press);
            ui.add_space(theme.space(Space::S2));
            // Its own icons, a couple of rows of them.
            let preview = preview_icons(self.set, columns_of(ui, theme));
            if !self.sheet_open {
                press.picked = sheet(
                    ui,
                    &format!("preview-{}", self.id),
                    self.set,
                    &preview,
                    theme,
                );
            } else {
                press.picked = sheet(
                    ui,
                    &format!("sheet-{}", self.id),
                    self.set,
                    Icon::ALL,
                    theme,
                );
            }
            ui.add_space(theme.space(Space::S2));
            ui.horizontal_wrapped(|ui| {
                let label = if self.sheet_open {
                    tr!("icons-hide-sheet")
                } else {
                    tr!("icons-show-sheet")
                };
                let help = tr!("icons-show-sheet-hint");
                press.sheet = ui::button(("sue-icons-sheet", self.id), &label)
                    .ghost()
                    .icon(Icon::ViewGridSmall)
                    .hint(&help)
                    .show(ui, theme)
                    .clicked();
                if !self.id.is_empty() {
                    let label = if self.asked_remove {
                        tr!("icons-remove-confirm")
                    } else {
                        tr!("icons-remove")
                    };
                    let help = tr!("icons-remove-hint");
                    let button = ui::button(("sue-icons-remove", self.id), &label).hint(&help);
                    let button = if self.asked_remove {
                        button.danger()
                    } else {
                        button.ghost().icon(Icon::Delete)
                    };
                    press.remove = button.show(ui, theme).clicked();
                }
            });
        });
        (press, radio.expect("the card draws its radio button"))
    }

    /// What its notes say: the problems, counted, with a button that
    /// lists them; the filled twins it leaves out, in a plain line.
    fn notes_ui(&self, ui: &mut Ui, theme: &Theme, press: &mut CardPress) {
        let twins = twins_left_out(self.notes);
        if !twins.is_empty() {
            text(
                ui,
                &tr!("icons-twins-left-out", names = twins.join(", ")),
                TextRole::Caption,
                true,
                theme,
            );
        }
        let problems = problems(self.notes);
        if problems.is_empty() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let (mark, _) = ui.allocate_exact_size(
                Vec2::splat(theme.text_size(TextRole::Caption)),
                Sense::hover(),
            );
            sue_wm::icons::paint(ui.painter(), mark, Icon::Warning, theme.text);
            text(
                ui,
                &tr!("icons-files-not-read", count = problems.len()),
                TextRole::Caption,
                false,
                theme,
            );
            let label = if self.details_open {
                tr!("icons-hide-details")
            } else {
                tr!("icons-show-details")
            };
            press.details = ui::link(("sue-icons-details", self.id), &label)
                .show(ui, theme)
                .clicked();
        });
        if self.details_open {
            for note in problems {
                text(ui, &note.message(), TextRole::Caption, true, theme);
            }
        }
    }
}

impl Settings {
    /// Appearance, Icons.
    pub(super) fn icons_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let Some(home) = home(cx) else {
            return;
        };
        let theme = cx.theme.clone();
        let state = &mut self.icons;
        if state.stale {
            state.show_again(ui, &home);
        }
        let installed = state
            .packs
            .get_or_insert_with(|| Installed::read(&home))
            .clone();
        text(ui, &tr!("icons-note"), TextRole::Body, true, &theme);
        if let Some(message) = &state.message {
            text(ui, message, TextRole::Body, false, &theme);
        }
        if !state.notes.is_empty() {
            let line = tr!("icons-in-use-not-read", count = state.notes.len());
            let response = ui.add(
                Label::new(
                    RichText::new(&line)
                        .font(theme.text_font(TextRole::Caption))
                        .color(theme.text_muted()),
                )
                .wrap(),
            );
            let all = state.notes.join("\n");
            ui::tooltip(response, &Tip::new(&line).help(&all), &theme);
        }
        ui.add_space(theme.space(Space::S3));
        // Each set to draw with, as a card: Sue's own, then every pack.
        let own_name = tr!("icons-sues-own");
        let mut cards = vec![SetCard {
            id: "",
            name: &own_name,
            about: None,
            set: &installed.sue,
            notes: &[],
            chosen: installed.chosen.is_none(),
            sheet_open: state.sheet.as_deref() == Some(""),
            details_open: false,
            asked_remove: false,
        }];
        for (found, set) in &installed.packs {
            let manifest = &found.manifest;
            cards.push(SetCard {
                id: &found.id,
                name: &manifest.name,
                about: Some(tr!(
                    "icons-pack-about",
                    author = manifest.author.as_str(),
                    licence = manifest.licence.as_str(),
                    version = manifest.version.as_str(),
                    count = found.drawn().len()
                )),
                set,
                notes: &found.notes,
                chosen: installed.chosen.as_deref() == Some(found.id.as_str()),
                sheet_open: state.sheet.as_deref() == Some(found.id.as_str()),
                details_open: state.details.as_deref() == Some(found.id.as_str()),
                asked_remove: state.confirm_remove.as_deref() == Some(found.id.as_str()),
            });
        }
        let mut choose: Option<Option<String>> = None;
        let mut radios = Vec::new();
        for card in &cards {
            let (press, radio) = card.show(ui, &theme);
            radios.push(radio);
            ui.add_space(theme.space(Space::S3));
            let id = card.id.to_owned();
            if press.choose {
                choose = Some((!id.is_empty()).then(|| id.clone()));
            }
            if press.sheet {
                state.sheet = if card.sheet_open {
                    None
                } else {
                    Some(id.clone())
                };
            }
            if press.details {
                state.details = if card.details_open {
                    None
                } else {
                    Some(id.clone())
                };
            }
            if press.remove {
                if card.asked_remove {
                    state.confirm_remove = None;
                    state.message = Some(match home.remove(&id) {
                        Ok(()) => tr!("icons-removed"),
                        Err(error) => tr!("icons-failed", error = error.message()),
                    });
                    state.packs = None;
                    state.sheet = None;
                    state.stale = true;
                } else {
                    state.confirm_remove = Some(id.clone());
                }
            }
            // A click on an icon of a sheet makes it the one to replace.
            if let Some(icon) = press.picked {
                icon.name().clone_into(&mut state.replace);
                state.solid = false;
            }
        }
        // The arrow keys walk the sets; choosing follows them.
        if let Some(index) = ui::rove(ui, &radios, ui::Roving::Vertical)
            && let Some(card) = cards.get(index)
            && !card.chosen
        {
            choose = Some((!card.id.is_empty()).then(|| card.id.to_owned()));
        }
        if let Some(choice) = choose {
            match home.choose(choice.as_deref()) {
                Ok(()) => {
                    state.packs = None;
                    state.show_again(ui, &home);
                }
                Err(error) => state.message = Some(tr!("icons-failed", error = error.message())),
            }
        }
        // Packs in, Sue's own out.
        ui.horizontal_wrapped(|ui| {
            let help = tr!("icons-install-folder-hint");
            if ui::button("sue-icons-install-folder", &tr!("icons-install-folder"))
                .icon(Icon::FolderOpen)
                .hint(&help)
                .show(ui, &theme)
                .clicked()
            {
                pick(
                    cx,
                    PICK_FOLDER,
                    &tr!("icons-install-folder"),
                    FilePick::folder,
                );
            }
            let help = tr!("icons-install-archive-hint");
            if ui::button("sue-icons-install-archive", &tr!("icons-install-archive"))
                .icon(Icon::Import)
                .hint(&help)
                .show(ui, &theme)
                .clicked()
            {
                pick(cx, PICK_ARCHIVE, &tr!("icons-install-archive"), |pick| {
                    pick.extensions(&["zip"])
                });
            }
            let help = tr!("icons-export-hint");
            if ui::button("sue-icons-export", &tr!("icons-export"))
                .ghost()
                .icon(Icon::Export)
                .hint(&help)
                .show(ui, &theme)
                .clicked()
            {
                let pick = FilePick::save(
                    settings_id(),
                    PICK_EXPORT,
                    &tr!("icons-export"),
                    "sue-icons.zip",
                )
                .extensions(&["zip"]);
                cx.request(ShellRequest::PickFile(pick));
            }
        });
        ui.add_space(theme.space(Space::S5));
        self.replace_ui(ui, &installed, &home, cx);
    }

    /// One icon replaced on its own, and the icons replaced so far.
    fn replace_ui(
        &mut self,
        ui: &mut Ui,
        installed: &Installed,
        home: &IconHome,
        cx: &mut AppletContext<'_>,
    ) {
        let theme = cx.theme.clone();
        ui::heading(ui, &tr!("icons-replace-heading").to_uppercase(), &theme);
        text(ui, &tr!("icons-replace-one"), TextRole::Body, true, &theme);
        ui.add_space(theme.space(Space::S2));
        let state = &mut self.icons;
        let names: Vec<&str> = Icon::ALL.iter().map(|icon| icon.name()).collect();
        let current = icon_named(&state.replace);
        ui.horizontal_wrapped(|ui| {
            let label = tr!("icons-replace-which");
            let response = ui::combo("sue-icons-replace", &mut state.replace, &names)
                .hint(&label)
                .label(&label)
                .show(ui, &theme);
            ui::tooltip(response, &Tip::new(&label), &theme);
            if current.is_some_and(Icon::has_solid) {
                let options = [tr!("icons-line"), tr!("icons-solid")];
                let strip = ui::segmented(
                    "sue-icons-replace-style",
                    &options,
                    Some(usize::from(state.solid)),
                )
                .show(ui, &theme);
                if let Some(index) = strip.picked {
                    state.solid = index == 1;
                }
                super::strip_tip(ui, &strip.response, &Tip::new(&tr!("icons-style")), &theme);
            }
            let help = tr!("icons-choose-svg-hint");
            if ui::button("sue-icons-replace-pick", &tr!("icons-choose-svg"))
                .icon(Icon::Replace)
                .enabled(current.is_some())
                .hint(&help)
                .show(ui, &theme)
                .clicked()
            {
                pick(cx, PICK_OVERRIDE, &tr!("icons-choose-svg"), |pick| {
                    pick.extensions(&["svg"])
                });
            }
        });
        if installed.overrides.is_empty() {
            return;
        }
        ui.add_space(theme.space(Space::S3));
        let mut give_back = None;
        let label = tr!("icons-give-back");
        let actions = [RowAction::new(Icon::Reset, &label)];
        ui::group(ui, &theme, |ui| {
            for (icon, style) in &installed.overrides {
                let name = pack::file_name(*icon, *style);
                let row = ui::list_row(("sue-icons-override", name.as_str()), &name)
                    .icon(*icon)
                    .actions(&actions)
                    .show(ui, &theme);
                if row.action == Some(0) {
                    give_back = Some((*icon, *style));
                }
            }
        });
        if let Some((icon, style)) = give_back {
            let state = &mut self.icons;
            if let Err(error) = home.clear_override(icon, style) {
                state.message = Some(tr!("icons-failed", error = error.message()));
            }
            state.packs = None;
            state.show_again(ui, home);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(problem: Problem) -> Note {
        Note {
            file: "heart.svg".to_owned(),
            problem,
        }
    }

    #[test]
    fn a_filled_twin_left_out_is_not_a_problem() {
        let notes = [
            note(Problem::NoSolid {
                name: "heart".to_owned(),
            }),
            note(Problem::NoSolid {
                name: "star".to_owned(),
            }),
            note(Problem::UnknownIcon {
                name: "hart".to_owned(),
            }),
        ];
        assert_eq!(problems(&notes).len(), 1);
        assert_eq!(twins_left_out(&notes), ["heart", "star"]);
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let said = with_problems("Installed.".to_owned(), &notes[..2]);
        assert_eq!(said, "Installed.");
        let said = with_problems("Installed.".to_owned(), &notes);
        assert!(said.contains("1 file could not be read"), "{said}");
    }
}
