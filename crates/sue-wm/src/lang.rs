//! Which language each part of Sue speaks. Sue has one global language
//! (`general.language`: a tag, or `system` for the system's), chosen from
//! the Sue menu or Settings. Every studio and tool also gets a setting of
//! its own in its rice section (`sue_paint.language`): `sue` follows the
//! global language, a tag sets it apart. The shell reads both and opens a
//! [`sue_lang::scope`] around each studio and tool it calls, so all of them
//! read the one language database, each in its own language.

use sue_rice::{RiceDeclaration, RiceParameter, RiceSchema};

use crate::{AppletId, Choice};

/// The rice key of Sue's global language.
pub const LANGUAGE_KEY: &str = "general.language";

/// The rice section a studio or tool keeps its settings in: its id with
/// underscores, e.g. `sue_paint` for `sue-paint`.
#[must_use]
pub fn rice_section(id: &AppletId) -> String {
    id.as_str().replace('-', "_")
}

/// The rice key of a studio's or tool's own language, e.g.
/// `sue_paint.language`.
#[must_use]
pub fn language_key(id: &AppletId) -> String {
    format!("{}.language", rice_section(id))
}

/// The language Sue speaks, from the rice.
#[must_use]
pub fn sue_language(rice: &RiceSchema) -> String {
    sue_lang::resolve(rice.get_str(LANGUAGE_KEY).unwrap_or(sue_lang::SYSTEM))
}

/// The language `id` speaks: its own setting, or Sue's when it follows
/// Sue (or has no setting, as windows do).
#[must_use]
pub fn applet_language(rice: &RiceSchema, id: &AppletId) -> String {
    let own = rice.get_str(&language_key(id)).unwrap_or(sue_lang::FOLLOW);
    sue_lang::resolve_for(own, &sue_language(rice))
}

/// The declaration of a studio's or tool's language setting.
#[must_use]
pub fn language_declaration(id: &AppletId) -> RiceDeclaration {
    RiceDeclaration::new(
        language_key(id),
        RiceParameter::Text {
            value: sue_lang::FOLLOW.to_owned(),
        },
        "Language: sue follows Sue's language, a language tag sets this one apart",
    )
    .labeled("Language")
}

/// The choices of Sue's language: the system's, then every language by
/// its own name.
#[must_use]
pub fn sue_language_choices() -> Vec<Choice> {
    let mut choices = vec![Choice::new(
        sue_lang::SYSTEM,
        tr!(
            "language-system",
            language = sue_lang::name(&sue_lang::system())
        ),
    )];
    choices.extend(
        sue_lang::languages()
            .into_iter()
            .map(|language| Choice::new(language.tag, language.name)),
    );
    choices
}

/// The choices of a studio's or tool's language: follow Sue (whose
/// language is `sue`), then every language by its own name.
#[must_use]
pub fn applet_language_choices(sue: &str) -> Vec<Choice> {
    let mut choices = vec![Choice::new(
        sue_lang::FOLLOW,
        tr!("language-follow-sue", language = sue_lang::name(sue)),
    )];
    choices.extend(
        sue_lang::languages()
            .into_iter()
            .map(|language| Choice::new(language.tag, language.name)),
    );
    choices
}
