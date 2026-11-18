//! Sue VE's page of Settings, in named groups, and its keyboard
//! shortcuts grouped as its menus are.

use sue_wm::{KeyMap, SettingsGroup, SettingsItem, SettingsPage};

use crate::studio::{ID, KEYS_SECTION, rice};

/// The page, in the language in use, titled `title` (Sue VE's name).
pub(crate) fn page(title: &str) -> SettingsPage {
    let param = |path: &str| SettingsItem::Param(path.to_owned());
    let mut page = SettingsPage::new(
        ID,
        title,
        vec![
            SettingsGroup::new(
                tr!("settings-group-playback"),
                vec![
                    param(rice::QUALITY),
                    param(rice::HARDWARE),
                    param(rice::CACHE_MB),
                ],
            ),
            SettingsGroup::new(tr!("settings-group-media"), vec![param(rice::EMBED)]),
        ],
    );
    page.keys = Some(keymap(title));
    page
}

/// Sue VE's keyboard shortcuts, grouped as its menus are.
pub(crate) fn keymap(title: &str) -> KeyMap {
    let registry = crate::commands::registry();
    KeyMap::from_menus(
        ID,
        title,
        KEYS_SECTION,
        &crate::menus::menus(),
        crate::commands::shortcut_table(),
        |command| registry.label(command),
        |_| tr!("keys-group-other"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_wm::SueApplet;

    #[test]
    fn the_page_shows_every_setting_of_sue_ve_once() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let page = page("Sue VE");
        let shown: Vec<&str> = page
            .claims()
            .filter_map(|item| match item {
                SettingsItem::Param(path) => Some(path.as_str()),
                _ => None,
            })
            .collect();
        for declaration in crate::SueVe::default().rice_declarations() {
            let path = declaration.path.as_str();
            if path.starts_with(KEYS_SECTION) {
                continue;
            }
            assert_eq!(
                shown.iter().filter(|known| **known == path).count(),
                1,
                "{path}"
            );
        }
        let keys = page.keys.expect("a key map");
        for shortcut in crate::commands::shortcut_table() {
            assert!(
                keys.find(shortcut.command).is_some(),
                "{}",
                shortcut.command
            );
        }
    }
}
