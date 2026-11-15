//! Manage Fonts: Sue's shared font library as a list a person can look
//! through, search, and see each family's faces and a line set in it.
//! Every studio that sets text opens the same list, so what it shows is
//! what every font menu of Sue offers.

use egui::{RichText, Ui, Vec2};

use super::{FontFamilyInfo, canonical, families, find, ready, resolve};
use crate::Theme;
use crate::ui::{self, Space, TextRole};

/// The size the Manage Fonts dialog opens at, before the screen holds it
/// smaller: room for the search field, twelve rows of the list and the
/// chosen family's faces and sample.
pub const MANAGE_SIZE: Vec2 = egui::vec2(480.0, 700.0);

/// What the Manage Fonts dialog holds while it is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontManager {
    /// What is typed in its search field.
    pub filter: String,
    /// The family picked in the list, or empty.
    pub selected: String,
}

impl FontManager {
    /// The dialog as it opens: the family Sue's own words are set in is
    /// picked, so its faces and sample show from the start.
    #[must_use]
    pub fn open() -> Self {
        Self {
            filter: String::new(),
            selected: super::SANS.to_owned(),
        }
    }

    /// The families the list shows now: every installed family whose
    /// name holds the search text, in any case.
    #[must_use]
    pub fn shown(&self) -> Vec<String> {
        let needle = self.filter.to_lowercase();
        families()
            .into_iter()
            .filter(|name| needle.is_empty() || name.to_lowercase().contains(&needle))
            .collect()
    }

    /// The picked family's faces, lightest first (none for a bundled
    /// family that is not installed on the device too).
    #[must_use]
    pub fn faces(&self) -> Option<FontFamilyInfo> {
        (!self.selected.is_empty())
            .then(|| find(canonical(&self.selected)))
            .flatten()
    }
}

/// Draws the Manage Fonts dialog's body into `ui`. Answers `true` when
/// Close was pressed.
pub fn manage_ui(ui: &mut Ui, manager: &mut FontManager, theme: &Theme) -> bool {
    let gap = theme.space(Space::S4);
    let hint = tr!("search-fonts");
    ui::search_field("sue-manage-fonts-search", &mut manager.filter)
        .hint(&hint)
        .show(ui, theme);
    ui.add_space(gap);
    let count = manager.shown().len();
    let status = if ready() {
        tr!("font-families-count", count = count)
    } else {
        tr!("finding-installed-fonts")
    };
    ui.label(
        RichText::new(status)
            .size(theme.text_size(TextRole::Caption))
            .color(theme.text_muted()),
    );
    ui.add_space(gap);
    if let Some(family) = super::list_ui(ui, &manager.selected, &manager.filter) {
        manager.selected = family;
    }
    ui.add_space(gap);
    if !manager.selected.is_empty() {
        let name = manager.selected.clone();
        ui.label(
            RichText::new(name.as_str())
                .size(theme.text_size(TextRole::Title))
                .color(theme.text),
        );
        let drawn = resolve(ui.ctx(), &name, false, false);
        ui.label(
            RichText::new(tr!("font-sample"))
                .font(egui::FontId::new(
                    theme.text_size(TextRole::Heading),
                    drawn.family,
                ))
                .color(theme.text),
        );
        let faces = manager.faces().map_or_else(
            || tr!("font-bundled"),
            |info| {
                let faces = info
                    .faces
                    .iter()
                    .map(|face| face.style.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                tr!("font-faces", faces = faces)
            },
        );
        ui.label(
            RichText::new(faces)
                .size(theme.text_size(TextRole::Caption))
                .color(theme.text_muted()),
        );
        ui.add_space(gap);
    }
    ui.label(
        RichText::new(tr!("fonts-come-from-the-device"))
            .size(theme.text_size(TextRole::Caption))
            .color(theme.text_muted()),
    );
    ui.add_space(gap);
    // Close at the bottom right, where every dialog of Sue has its
    // buttons.
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        ui::button("sue-manage-fonts-close", &tr!("dialog-close"))
            .show(ui, theme)
            .clicked()
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_keeps_the_families_that_hold_it_in_any_case() {
        let mut manager = FontManager::default();
        // The two bundled families are always there.
        assert!(manager.shown().len() >= 2);
        manager.filter = "hAc".to_owned();
        let shown = manager.shown();
        assert!(
            shown.iter().any(|name| name == super::super::MONO),
            "{shown:?}"
        );
        assert!(
            shown.iter().all(|name| name.to_lowercase().contains("hac")),
            "{shown:?}"
        );
        manager.filter = "no family is called this".to_owned();
        assert!(manager.shown().is_empty());
    }

    #[test]
    fn nothing_picked_has_no_faces() {
        assert_eq!(FontManager::default().faces(), None);
    }

    #[test]
    fn it_opens_on_the_family_sue_writes_in() {
        let manager = FontManager::open();
        assert_eq!(manager.selected, super::super::SANS);
        assert!(manager.filter.is_empty());
    }

    #[test]
    fn its_words_are_in_english() {
        for key in [
            "search-fonts",
            "finding-installed-fonts",
            "font-sample",
            "font-bundled",
            "fonts-come-from-the-device",
            "dialog-close",
        ] {
            assert!(sue_lang::has("wm", key), "{key}");
        }
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        assert_eq!(
            sue_lang::tr!("wm", "font-families-count", count = 1),
            "1 font family"
        );
        assert_eq!(
            sue_lang::tr!("wm", "font-families-count", count = 3),
            "3 font families"
        );
    }
}
