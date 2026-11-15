//! Layout Styles: which design Sue Paint's workspace follows. A style says
//! which palettes there are and where they sit until people move them;
//! a workspace only moves and shows palettes inside the style in use.
//!
//! The styles are a registry ([`STYLES`], read through
//! [`sue_wm::dock::DockPanel::styles`]), so adding one is adding an entry.
//! The one in use is the setting [`KEY`], a person's preference like the
//! theme, never part of a document. Its id is saved data (the settings
//! file, the command `window.layout_style.<id>` and the layout files), so
//! it never changes; its name is one language key.
//!
//! "Clip Studio" (`classic`) arranges the palettes as Clip Studio Paint
//! does. "Sue Paint" (`sue_paint`) is listed for later, grayed out: it is
//! not built, so it can never be the style in use ([`current`]).

use sue_rice::RiceSchema;
use sue_wm::Choice;
use sue_wm::dock::{DockPanel, LayoutStyle};

use crate::dock::Palette;

/// The setting that names the style in use.
pub(crate) const KEY: &str = "sue_paint.layout_style";

/// The "Clip Studio" style: Clip Studio Paint's arrangement.
pub(crate) const CLASSIC: &str = "classic";

/// The "Sue Paint" style, listed for later and not built yet.
pub(crate) const SUE_PAINT: &str = "sue_paint";

/// The commands that choose a style: `window.layout_style.<id>`.
pub(crate) const COMMAND_PREFIX: &str = "window.layout_style.";

/// Every layout style, the default one first.
pub(crate) static STYLES: [LayoutStyle<Palette>; 2] = [
    LayoutStyle {
        id: CLASSIC,
        label: "layout-style-classic",
        available: true,
        layout: crate::dock::classic_layout,
    },
    LayoutStyle {
        id: SUE_PAINT,
        label: "layout-style-sue-paint",
        available: false,
        // Not built: until it is, it would look like the default.
        layout: crate::dock::classic_layout,
    },
];

/// The style the settings name; an unknown one, or one that is not built
/// yet, is the default (someone can still type it into the settings
/// file).
#[must_use]
pub(crate) fn current(rice: &RiceSchema) -> &'static LayoutStyle<Palette> {
    let id = rice.get_str(KEY).unwrap_or(CLASSIC);
    Palette::style(id).unwrap_or(&STYLES[0])
}

/// The style a registry id names, built or not.
#[must_use]
pub(crate) fn by_id(id: &str) -> Option<&'static LayoutStyle<Palette>> {
    STYLES.iter().find(|style| style.id == id)
}

/// The name people read for `style`.
#[must_use]
pub(crate) fn label(style: &LayoutStyle<Palette>) -> String {
    sue_lang::text_or("paint", style.label, style.id)
}

/// Every style as a choice for Settings: the ones not built yet are shown
/// grayed out, so they can be seen but not chosen.
#[must_use]
pub(crate) fn choices() -> Vec<Choice> {
    STYLES
        .iter()
        .map(|style| Choice::new(style.id, label(style)).enabled(style.available))
        .collect()
}

/// The name of the file in Sue Paint's data folder that keeps `file` for
/// the style `id`: the default style keeps the names Sue Paint always
/// used (`layout.txt`), another style puts its id before the extension
/// (`layout.sue_paint.txt`), so switching styles and back loses nothing.
#[must_use]
pub(crate) fn file_for(id: &str, file: &str) -> String {
    if id == CLASSIC {
        return file.to_owned();
    }
    match file.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem}.{id}.{extension}"),
        None => format!("{file}.{id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rice_naming(id: Option<&str>) -> RiceSchema {
        match id {
            Some(id) => RiceSchema::parse(&format!("[sue_paint]\nlayout_style = \"{id}\"\n"))
                .expect("a settings file with the style"),
            None => RiceSchema::new(),
        }
    }

    #[test]
    fn the_first_style_is_built_and_is_the_default() {
        assert_eq!(STYLES[0].id, CLASSIC);
        assert!(STYLES[0].available);
        assert_eq!(Palette::styles()[0].id, CLASSIC);
        assert_eq!(current(&rice_naming(None)).id, CLASSIC);
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        assert_eq!(label(&STYLES[0]), "Clip Studio");
        assert_eq!(label(&STYLES[1]), "Sue Paint (Coming Later)");
    }

    #[test]
    fn a_style_that_is_not_built_is_never_the_one_in_use() {
        assert!(!by_id(SUE_PAINT).is_some_and(|style| style.available));
        for named in [SUE_PAINT, "no-such-style", ""] {
            assert_eq!(current(&rice_naming(Some(named))).id, CLASSIC, "{named}");
        }
        let choices = choices();
        assert_eq!(choices.len(), STYLES.len());
        assert!(choices[0].enabled);
        assert!(!choices[1].enabled, "listed, grayed out");
    }

    #[test]
    fn ids_are_names_a_settings_file_and_a_command_can_hold() {
        for style in &STYLES {
            assert!(
                style.id.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                style.id
            );
            assert!(sue_lang::has("paint", style.label), "{}", style.label);
        }
        assert_eq!(file_for(CLASSIC, "layout.txt"), "layout.txt");
        assert_eq!(file_for(SUE_PAINT, "layout.txt"), "layout.sue_paint.txt");
        assert_eq!(
            file_for(SUE_PAINT, "default_workspace.txt"),
            "default_workspace.sue_paint.txt"
        );
    }
}
