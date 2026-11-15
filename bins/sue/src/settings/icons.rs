//! Appearance, Icons: the icon packs a person installed, a preview sheet
//! of each, install from a folder or an archive, remove, choose, single
//! icons replaced one by one, and Sue's own set written out as a template
//! for pack makers. Everything takes effect on the next frame, with no
//! restart (`sue_wm::icons::pack`, format in `docs/icon-packs.md`).

/// Text from the language database's `settings` namespace.
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use std::fmt;
use std::path::Path;

use egui::{RichText, Sense, Ui, vec2};
use sue_wm::icons::pack::{self, IconHome, IconSet, Pack};
use sue_wm::icons::{Icon, IconStyle};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use super::{ID, Settings};

/// What a pick of a file or folder is for.
const PICK_FOLDER: &str = "icon-pack-folder";
const PICK_ARCHIVE: &str = "icon-pack-archive";
const PICK_EXPORT: &str = "icon-pack-export";
const PICK_OVERRIDE: &str = "icon-override";

/// The Icons rows between frames.
#[derive(Default)]
pub(super) struct IconsUi {
    /// The installed packs, the one chosen and the icons replaced one by
    /// one, as read from the icon home; `None` until first shown or after
    /// a change, so the page reads the disk only then, not each frame.
    packs: Option<Installed>,
    /// The pack whose sheet is open (its id; empty for Sue's own), and its
    /// set, kept while the sheet is open rather than made each frame.
    sheet: Option<(String, IconSet)>,
    /// What the last change said: installed, removed, or what failed.
    message: Option<String>,
    /// What could not be read when the set in use was last loaded.
    notes: Vec<String>,
    /// The set in use must be loaded again and shown (after a change made
    /// outside a frame).
    stale: bool,
    /// The icon chosen for a single replacement, and whether its solid
    /// twin.
    replace: usize,
    solid: bool,
    /// The pack Remove was clicked once for; a second click removes it.
    confirm_remove: Option<String>,
}

impl fmt::Debug for IconsUi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IconsUi")
            .field("packs", &self.packs.as_ref().map(|found| found.packs.len()))
            .field("sheet", &self.sheet.as_ref().map(|(id, _)| id))
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

/// What the icon home holds, read once per change.
#[derive(Clone)]
struct Installed {
    packs: Vec<Pack>,
    chosen: Option<String>,
    overrides: Vec<(Icon, IconStyle)>,
}

impl Installed {
    fn read(home: &IconHome) -> Self {
        Self {
            packs: home.packs(),
            chosen: home.chosen(),
            overrides: home.overrides(),
        }
    }
}

/// The icon home Settings works on.
fn home(cx: &AppletContext<'_>) -> Option<IconHome> {
    cx.catalog.folders.get("icons").map(IconHome::new)
}

fn settings_id() -> Option<AppletId> {
    AppletId::new(ID).ok()
}

impl IconsUi {
    /// A file or folder picked for one of the rows.
    pub(super) fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        let Some(home) = home(cx) else {
            return;
        };
        let result = match pick.purpose.as_str() {
            PICK_FOLDER | PICK_ARCHIVE => home.install(path).map(|pack| {
                tr!(
                    "icons-installed",
                    name = pack.manifest.name.as_str(),
                    count = pack.notes.len()
                )
            }),
            PICK_EXPORT => {
                let bytes = pack::export_archive(&pack::sue_manifest());
                sue_platform::write_atomic(path, &bytes)
                    .map(|()| tr!("icons-exported", path = path.display().to_string()))
                    .map_err(|error| pack::PackError::Io(error.to_string()))
            }
            PICK_OVERRIDE => {
                let icon = Icon::ALL[self.replace.min(Icon::ALL.len() - 1)];
                let style = if self.solid && icon.has_solid() {
                    IconStyle::Solid
                } else {
                    IconStyle::Line
                };
                home.set_override_from(icon, style, path)
                    .map(|notes| tr!("icons-replaced", name = icon.name(), count = notes.len()))
            }
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
            .map(pack::Note::message)
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

/// A small sheet of `set`'s icons, Sue's own drawn where the pack has none.
/// Gives the icon clicked: the one to replace on its own.
fn sheet(ui: &mut Ui, set: &IconSet, theme: &Theme) -> Option<Icon> {
    let mut clicked = None;
    let side = theme.ui_size * 1.6;
    let columns = ((ui.available_width() / (side + 4.0)).floor() as usize).max(1);
    for row in Icon::ALL.chunks(columns) {
        ui.horizontal(|ui| {
            for icon in row {
                let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
                if response.hovered() {
                    let radius = theme.radius(sue_wm::ui::tokens::Radius::Sm);
                    ui.painter().rect_filled(rect, radius, theme.hover);
                }
                let drawn = set.drawing(*icon, IconStyle::Line).is_some();
                let tint = if drawn || set.is_empty() {
                    theme.text
                } else {
                    theme.muted
                };
                pack::paint_from(
                    ui.painter(),
                    rect.shrink(2.0),
                    set,
                    *icon,
                    IconStyle::Line,
                    tint,
                );
                if response.on_hover_text(icon.name()).clicked() {
                    clicked = Some(*icon);
                }
            }
        });
    }
    clicked
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
        let Installed {
            packs,
            chosen,
            overrides,
        } = state
            .packs
            .get_or_insert_with(|| Installed::read(&home))
            .clone();
        ui.add(egui::Label::new(RichText::new(tr!("icons-note")).color(theme.muted)).wrap());
        // Which set Sue draws with: its own, or a pack.
        let mut choose: Option<Option<String>> = None;
        ui.horizontal_wrapped(|ui| {
            if sue_wm::widgets::pill_button(ui, &tr!("icons-sues-own"), chosen.is_none(), &theme)
                .clicked()
            {
                choose = Some(None);
            }
            for found in &packs {
                let on = chosen.as_deref() == Some(found.id.as_str());
                if sue_wm::widgets::pill_button(ui, &found.manifest.name, on, &theme).clicked() {
                    choose = Some(Some(found.id.clone()));
                }
            }
        });
        if let Some(choice) = choose {
            match home.choose(choice.as_deref()) {
                Ok(()) => {
                    state.packs = None;
                    state.show_again(ui, &home);
                }
                Err(error) => state.message = Some(tr!("icons-failed", error = error.message())),
            }
        }
        if let Some(message) = &state.message {
            ui.add(egui::Label::new(RichText::new(message).color(theme.text)).wrap());
        }
        if !state.notes.is_empty() {
            ui.label(
                RichText::new(tr!("icons-not-read", count = state.notes.len()))
                    .size(theme.small_size)
                    .color(theme.muted),
            )
            .on_hover_text(state.notes.join("\n"));
        }
        ui.add_space(theme.padding * 0.5);
        // Each pack: what it says about itself, and what to do with it.
        let mut remove = None;
        let mut open_sheet = None;
        let mut picked = None;
        let sue_sheet = state.sheet.as_ref().is_some_and(|(id, _)| id.is_empty());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr!("icons-sues-own")).color(theme.text));
            let label = if sue_sheet {
                tr!("icons-hide-sheet")
            } else {
                tr!("icons-show-sheet")
            };
            if sue_wm::ui::button("sue-icons-sheet-sue", &label)
                .ghost()
                .show(ui, &theme)
                .clicked()
            {
                open_sheet = Some(String::new());
            }
        });
        for found in &packs {
            let manifest = &found.manifest;
            ui.add_space(4.0);
            ui.label(RichText::new(&manifest.name).color(theme.text).strong());
            ui.label(
                RichText::new(tr!(
                    "icons-about",
                    author = manifest.author.as_str(),
                    licence = manifest.licence.as_str(),
                    version = manifest.version.as_str(),
                    count = found.drawn().len(),
                    notes = found.notes.len()
                ))
                .size(theme.small_size)
                .color(theme.muted),
            );
            let open = state.sheet.as_ref().is_some_and(|(id, _)| id == &found.id);
            ui.horizontal_wrapped(|ui| {
                let label = if open {
                    tr!("icons-hide-sheet")
                } else {
                    tr!("icons-show-sheet")
                };
                if sue_wm::ui::button(("sue-icons-sheet", found.id.as_str()), &label)
                    .ghost()
                    .show(ui, &theme)
                    .clicked()
                {
                    open_sheet = Some(found.id.clone());
                }
                let asked = state.confirm_remove.as_deref() == Some(found.id.as_str());
                let label = if asked {
                    tr!("icons-remove-confirm")
                } else {
                    tr!("icons-remove")
                };
                if sue_wm::ui::button(("sue-icons-remove", found.id.as_str()), &label)
                    .ghost()
                    .show(ui, &theme)
                    .clicked()
                {
                    if asked {
                        remove = Some(found.id.clone());
                    } else {
                        state.confirm_remove = Some(found.id.clone());
                    }
                }
            });
            if open && let Some((_, set)) = &state.sheet {
                picked = sheet(ui, set, &theme).or(picked);
            }
        }
        if sue_sheet && let Some((_, set)) = &state.sheet {
            picked = sheet(ui, set, &theme).or(picked);
        }
        // A click on an icon of a sheet makes it the one to replace.
        if let Some(icon) = picked
            && let Some(index) = Icon::ALL.iter().position(|known| *known == icon)
        {
            state.replace = index;
        }
        if let Some(id) = open_sheet {
            let open = state.sheet.as_ref().is_some_and(|(open, _)| *open == id);
            state.sheet = if open {
                None
            } else if id.is_empty() {
                Some((id, IconSet::sue()))
            } else {
                packs
                    .iter()
                    .find(|found| found.id == id)
                    .map(|found| (id.clone(), found.set()))
            };
        }
        if let Some(id) = remove {
            state.confirm_remove = None;
            state.message = Some(match home.remove(&id) {
                Ok(()) => tr!("icons-removed"),
                Err(error) => tr!("icons-failed", error = error.message()),
            });
            state.packs = None;
            state.sheet = None;
            state.show_again(ui, &home);
        }
        ui.add_space(theme.padding * 0.5);
        // Packs in, Sue's own out.
        ui.horizontal_wrapped(|ui| {
            if sue_wm::ui::button("sue-icons-install-folder", &tr!("icons-install-folder"))
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
            if sue_wm::ui::button("sue-icons-install-archive", &tr!("icons-install-archive"))
                .show(ui, &theme)
                .clicked()
            {
                pick(cx, PICK_ARCHIVE, &tr!("icons-install-archive"), |pick| {
                    pick.extensions(&["zip"])
                });
            }
            if sue_wm::ui::button("sue-icons-export", &tr!("icons-export"))
                .ghost()
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
        ui.add_space(theme.padding * 0.5);
        // One icon replaced on its own.
        ui.label(RichText::new(tr!("icons-replace-one")).color(theme.text));
        let state = &mut self.icons;
        ui.horizontal_wrapped(|ui| {
            let current = Icon::ALL[state.replace.min(Icon::ALL.len() - 1)];
            egui::ComboBox::from_id_salt("sue-icons-replace")
                .selected_text(current.name())
                .height(320.0)
                .show_ui(ui, |ui| {
                    for (index, icon) in Icon::ALL.iter().enumerate() {
                        if ui
                            .add(egui::Button::selectable(
                                index == state.replace,
                                icon.name(),
                            ))
                            .clicked()
                        {
                            state.replace = index;
                        }
                    }
                });
            if current.has_solid() {
                let picked = sue_wm::widgets::segmented(
                    ui,
                    &[tr!("icons-line"), tr!("icons-solid")],
                    Some(usize::from(state.solid)),
                    &theme,
                );
                if let Some(index) = picked {
                    state.solid = index == 1;
                }
            }
            if sue_wm::ui::button("sue-icons-replace-pick", &tr!("icons-choose-svg"))
                .show(ui, &theme)
                .clicked()
            {
                pick(cx, PICK_OVERRIDE, &tr!("icons-choose-svg"), |pick| {
                    pick.extensions(&["svg"])
                });
            }
        });
        let mut give_back = None;
        for (icon, style) in overrides {
            ui.horizontal_wrapped(|ui| {
                let side = theme.ui_size * 1.4;
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                sue_wm::icons::paint_styled(ui.painter(), rect, icon, style, theme.text);
                let name = pack::file_name(icon, style);
                ui.label(RichText::new(&name).color(theme.text));
                if sue_wm::ui::button(
                    ("sue-icons-give-back", name.as_str()),
                    &tr!("icons-give-back"),
                )
                .ghost()
                .show(ui, &theme)
                .clicked()
                {
                    give_back = Some((icon, style));
                }
            });
        }
        if let Some((icon, style)) = give_back {
            let state = &mut self.icons;
            if let Err(error) = home.clear_override(icon, style) {
                state.message = Some(tr!("icons-failed", error = error.message()));
            }
            state.packs = None;
            state.show_again(ui, &home);
        }
    }
}
