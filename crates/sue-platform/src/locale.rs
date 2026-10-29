//! The languages the system is set to, so Sue starts in one of them.

/// The system's languages, most preferred first, as language tags such as
/// `pt-BR` (Windows' display languages, the preferred languages on macOS
/// and iOS, the system locale on Android, `LANGUAGE`, `LC_ALL`,
/// `LC_MESSAGES` and `LANG` elsewhere).
#[must_use]
pub fn system_languages() -> Vec<String> {
    let mut tags: Vec<String> = sys_locale::get_locales()
        .filter_map(|tag| normalize_language(&tag))
        .collect();
    // `LANGUAGE` lists several, colon separated, where it is set.
    if let Ok(list) = std::env::var("LANGUAGE") {
        for tag in list.split(':').filter_map(normalize_language).rev() {
            tags.retain(|seen| *seen != tag);
            tags.insert(0, tag);
        }
    }
    tags.dedup();
    tags
}

/// A system locale name as a language tag: `de_DE.UTF-8@euro` reads
/// `de-DE`. `None` for the neutral `C` and `POSIX` locales and for empty
/// names.
#[must_use]
pub fn normalize_language(name: &str) -> Option<String> {
    let name = name.split(['.', '@']).next().unwrap_or_default().trim();
    if name.is_empty() || name.eq_ignore_ascii_case("c") || name.eq_ignore_ascii_case("posix") {
        return None;
    }
    Some(name.replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_names_become_tags() {
        assert_eq!(
            normalize_language("de_DE.UTF-8@euro").as_deref(),
            Some("de-DE")
        );
        assert_eq!(normalize_language("pt-BR").as_deref(), Some("pt-BR"));
        assert_eq!(normalize_language("C.UTF-8"), None);
        assert_eq!(normalize_language("POSIX"), None);
        assert_eq!(normalize_language(""), None);
    }

    #[test]
    fn the_system_names_its_languages_as_tags() {
        for tag in system_languages() {
            assert!(!tag.contains(['_', '.', '@']), "{tag}");
        }
    }
}
