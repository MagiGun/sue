//! # sue-lang: Sue's language database
//!
//! Every string Sue shows lives here, in every language, as
//! [Project Fluent](https://projectfluent.org) messages. The database is one
//! for the whole app: each part of Sue reads its own namespace (`sue`, `wm`,
//! `paint`, `writer` and so on), and each language is a folder of `.ftl`
//! files under `crates/sue-lang/locales/<tag>/`, one file per namespace.
//! The build finds the folders, so adding a language is adding a folder
//! (no list in the code names them), and plug-ins add their own messages,
//! or whole languages, at runtime with [`add_resource`].
//!
//! English (`en`) is the source language and the fallback: a message a
//! language lacks is read in English, never shown blank or as its key.
//!
//! Which language a lookup reads is the thread's current one: the global
//! language ([`set_language`]), or, inside a [`scope`], the scope's. The
//! shell opens a scope around each studio and tool, so one can follow Sue's
//! language and another speak its own while both read this one database.
//!
//! Parts of Sue read text with the [`tr!`] macro, usually through a
//! one-line wrapper that names their namespace:
//!
//! ```
//! macro_rules! tr {
//!     ($($arg:tt)*) => { sue_lang::tr!("language", $($arg)*) };
//! }
//! assert_eq!(tr!("language-name"), "English");
//! ```

mod check;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use intl_memoizer::Memoizable;
use intl_memoizer::concurrent::IntlLangMemoizer;
use unic_langid::LanguageIdentifier;

pub use check::{Report, VariableMismatch, check, english_keys, variables_of};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// The source language and the fallback.
pub const ENGLISH: &str = "en";

/// The global language setting that follows the system's language.
pub const SYSTEM: &str = "system";

/// A studio's or tool's language setting that follows Sue's.
pub const FOLLOW: &str = "sue";

/// The namespace that describes each language itself: its name in its own
/// script (`language-name`), whether it waits for a native speaker's review
/// (`language-status = draft`), and how it writes numbers
/// (`number-decimal`, `number-group`, `number-group-min`).
pub const LANGUAGE: &str = "language";

type Bundle = fluent_bundle::bundle::FluentBundle<FluentResource, IntlLangMemoizer>;

/// One namespace of one language: the messages and the sources they came
/// from (the checks read the sources).
struct Namespace {
    bundle: Bundle,
    sources: Vec<String>,
}

/// One language: its namespaces.
struct Language {
    tag: String,
    id: LanguageIdentifier,
    namespaces: BTreeMap<String, Namespace>,
}

impl Language {
    fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_owned(),
            id: tag.parse().unwrap_or_default(),
            namespaces: BTreeMap::new(),
        }
    }

    /// Adds `source` to `namespace`; a message already there is replaced.
    /// Returns the parse errors, if any (what parsed is kept).
    fn add(&mut self, namespace: &str, source: &str) -> Vec<String> {
        let (resource, errors) = match FluentResource::try_new(source.to_owned()) {
            Ok(resource) => (resource, Vec::new()),
            Err((resource, errors)) => (
                resource,
                errors
                    .into_iter()
                    .map(|error| {
                        let line = source[..error.pos.start.min(source.len())].lines().count();
                        format!("{}/{namespace}.ftl:{line}: {error}", self.tag)
                    })
                    .collect(),
            ),
        };
        let id = self.id.clone();
        let entry = self
            .namespaces
            .entry(namespace.to_owned())
            .or_insert_with(|| {
                let mut bundle = Bundle::new_concurrent(vec![id]);
                // Bidi isolation marks would show as boxes in the
                // interface's fonts; no right-to-left language ships yet.
                bundle.set_use_isolating(false);
                bundle.set_formatter(Some(format_number));
                Namespace {
                    bundle,
                    sources: Vec::new(),
                }
            });
        entry.bundle.add_resource_overriding(resource);
        entry.sources.push(source.to_owned());
        errors
    }

    /// The message `key` of `namespace` (or `message.attribute`),
    /// formatted, if this language has it.
    fn format(&self, namespace: &str, key: &str, args: &[(&str, Arg)]) -> Option<String> {
        let bundle = &self.namespaces.get(namespace)?.bundle;
        let pattern = match key.split_once('.') {
            Some((id, attribute)) => bundle.get_message(id)?.get_attribute(attribute)?.value(),
            None => bundle.get_message(key)?.value()?,
        };
        let mut fluent = FluentArgs::with_capacity(args.len());
        for (name, value) in args {
            fluent.set(*name, value.to_fluent());
        }
        let mut errors = Vec::new();
        let text = bundle.format_pattern(pattern, Some(&fluent), &mut errors);
        Some(text.into_owned())
    }

    fn has(&self, namespace: &str, key: &str) -> bool {
        let Some(space) = self.namespaces.get(namespace) else {
            return false;
        };
        match key.split_once('.') {
            Some((id, attribute)) => space
                .bundle
                .get_message(id)
                .is_some_and(|message| message.get_attribute(attribute).is_some()),
            None => space.bundle.has_message(key),
        }
    }
}

/// The database: every language, the global language and the system's.
struct Database {
    languages: Vec<Language>,
    /// The tag lookups outside any scope read.
    global: String,
    /// The best language for the system's own, else English.
    system: String,
    /// Parse errors of the built-in files (the tests require none).
    errors: Vec<String>,
}

impl Database {
    fn built_in() -> Self {
        let mut database = Self {
            languages: Vec::new(),
            global: ENGLISH.to_owned(),
            system: ENGLISH.to_owned(),
            errors: Vec::new(),
        };
        for (tag, namespace, source) in BUILT_IN {
            let errors = database.language_mut(tag).add(namespace, source);
            database.errors.extend(errors);
        }
        database.sort();
        database.refresh_numbers();
        database
    }

    fn sort(&mut self) {
        self.languages
            .sort_by(|a, b| (a.tag != ENGLISH, &a.tag).cmp(&(b.tag != ENGLISH, &b.tag)));
    }

    fn language(&self, tag: &str) -> Option<&Language> {
        self.languages
            .iter()
            .find(|language| language.tag.eq_ignore_ascii_case(tag))
    }

    fn language_mut(&mut self, tag: &str) -> &mut Language {
        let index = match self
            .languages
            .iter()
            .position(|language| language.tag.eq_ignore_ascii_case(tag))
        {
            Some(index) => index,
            None => {
                self.languages.push(Language::new(tag));
                self.languages.len() - 1
            }
        };
        &mut self.languages[index]
    }

    /// The languages a lookup in `tag` tries, in order: `tag` itself (or
    /// the closest one Sue has), others of the same language and script,
    /// then English.
    fn chain(&self, tag: &str) -> Vec<&Language> {
        let mut chain: Vec<&Language> = Vec::with_capacity(3);
        let wanted: LanguageIdentifier = tag.parse().unwrap_or_default();
        if let Some(exact) = self.language(tag) {
            chain.push(exact);
        }
        for language in &self.languages {
            if close(&wanted, &language.id) && !chain.iter().any(|seen| seen.tag == language.tag) {
                chain.push(language);
            }
        }
        if let Some(english) = self.language(ENGLISH)
            && !chain.iter().any(|seen| seen.tag == ENGLISH)
        {
            chain.push(english);
        }
        chain
    }

    /// The closest language Sue has to `tag`, if any.
    fn best(&self, tag: &str) -> Option<String> {
        let first = self
            .chain(tag)
            .first()
            .map(|language| language.tag.clone())?;
        let english_only = first == ENGLISH && primary(tag) != ENGLISH;
        (!english_only).then_some(first)
    }

    fn format(&self, tag: &str, namespace: &str, key: &str, args: &[(&str, Arg)]) -> String {
        self.chain(tag)
            .into_iter()
            .find_map(|language| language.format(namespace, key, args))
            .unwrap_or_else(|| words_of(key))
    }

    /// Reads every language's number separators into [`NUMBERS`].
    fn refresh_numbers(&self) {
        let mut numbers = NUMBERS.write().unwrap_or_else(PoisonError::into_inner);
        numbers.clear();
        for language in &self.languages {
            let read = |key: &str| language.format(LANGUAGE, key, &[]);
            let style = NumberStyle {
                decimal: read("number-decimal").unwrap_or_else(|| ".".to_owned()),
                group: read("number-group").unwrap_or_else(|| ",".to_owned()),
                group_min: read("number-group-min")
                    .and_then(|min| min.trim().parse().ok())
                    .unwrap_or(1),
            };
            numbers.insert(language.tag.to_ascii_lowercase(), style);
        }
    }
}

/// `true` if `available` serves people who asked for `wanted`: the same
/// language, and the same script (Chinese in Taiwan, Hong Kong and Macau
/// is written in traditional characters).
fn close(wanted: &LanguageIdentifier, available: &LanguageIdentifier) -> bool {
    if wanted.language != available.language {
        return false;
    }
    let script_of = |id: &LanguageIdentifier| {
        id.script
            .map(|script| script.as_str().to_owned())
            .or_else(|| {
                (id.language.as_str() == "zh").then(|| {
                    let traditional = id
                        .region
                        .is_some_and(|region| matches!(region.as_str(), "TW" | "HK" | "MO"));
                    if traditional { "Hant" } else { "Hans" }.to_owned()
                })
            })
    };
    match (script_of(wanted), script_of(available)) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

fn primary(tag: &str) -> String {
    tag.split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// A key in words, for the one case nothing has it (a plug-in's missing
/// message): `open-file` reads `Open file`.
fn words_of(key: &str) -> String {
    let words = key.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

static DATABASE: LazyLock<RwLock<Database>> = LazyLock::new(|| RwLock::new(Database::built_in()));

/// Bumped whenever the global language or the messages change.
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn read() -> RwLockReadGuard<'static, Database> {
    DATABASE.read().unwrap_or_else(PoisonError::into_inner)
}

fn write() -> RwLockWriteGuard<'static, Database> {
    DATABASE.write().unwrap_or_else(PoisonError::into_inner)
}

thread_local! {
    static SCOPES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

// ----- Numbers -----------------------------------------------------------------

/// How a language writes numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NumberStyle {
    decimal: String,
    group: String,
    /// Group thousands only with at least this many digits before the
    /// first group (Spanish writes `1000` but `10 000`: 2).
    group_min: usize,
}

impl Default for NumberStyle {
    fn default() -> Self {
        Self {
            decimal: ".".to_owned(),
            group: ",".to_owned(),
            group_min: 1,
        }
    }
}

impl NumberStyle {
    fn of(tag: &str) -> Self {
        let numbers = NUMBERS.read().unwrap_or_else(PoisonError::into_inner);
        numbers
            .get(&tag.to_ascii_lowercase())
            .or_else(|| numbers.get(&primary(tag)))
            .cloned()
            .unwrap_or_default()
    }

    /// `value` with exactly `decimals` digits after the separator, or as
    /// many as it needs (up to six) when `decimals` is `None`.
    fn format(&self, value: f64, decimals: Option<usize>, grouping: bool) -> String {
        if !value.is_finite() {
            return value.to_string();
        }
        let plain = match decimals {
            Some(decimals) => format!("{:.*}", decimals, value.abs()),
            None => {
                let text = format!("{:.6}", value.abs());
                let text = text.trim_end_matches('0');
                text.trim_end_matches('.').to_owned()
            }
        };
        let (whole, fraction) = plain.split_once('.').unwrap_or((&plain, ""));
        let mut out = String::new();
        if value.is_sign_negative() && plain.bytes().any(|byte| matches!(byte, b'1'..=b'9')) {
            out.push('-');
        }
        if grouping && whole.len() >= 3 + self.group_min {
            for (index, digit) in whole.chars().enumerate() {
                if index > 0 && (whole.len() - index) % 3 == 0 {
                    out.push_str(&self.group);
                }
                out.push(digit);
            }
        } else {
            out.push_str(whole);
        }
        if !fraction.is_empty() {
            out.push_str(&self.decimal);
            out.push_str(fraction);
        }
        out
    }
}

static NUMBERS: LazyLock<RwLock<HashMap<String, NumberStyle>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

impl Memoizable for NumberStyle {
    type Args = ();
    type Error = ();

    fn construct(lang: LanguageIdentifier, (): ()) -> Result<Self, ()> {
        Ok(Self::of(&lang.to_string()))
    }
}

/// Fluent's number formatter: every number in a message is written the
/// way its language writes numbers.
fn format_number(value: &FluentValue<'_>, memoizer: &IntlLangMemoizer) -> Option<String> {
    let FluentValue::Number(number) = value else {
        return None;
    };
    let decimals = number.options.minimum_fraction_digits;
    let grouping = number.options.use_grouping;
    memoizer
        .with_try_get::<NumberStyle, _, _>((), |style| {
            style.format(number.value, decimals, grouping)
        })
        .ok()
}

/// `value` written the way the current language writes numbers, with
/// `decimals` digits after the decimal separator.
#[must_use]
pub fn number(value: f64, decimals: usize) -> String {
    NumberStyle::of(&current()).format(value, Some(decimals), true)
}

// ----- Arguments ---------------------------------------------------------------

/// A value for a message's variable: text, or a number (which picks plural
/// forms and is written the language's way).
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// Text, shown as it is.
    Text(String),
    /// A number.
    Number(f64),
}

impl Arg {
    fn to_fluent(&self) -> FluentValue<'_> {
        match self {
            Self::Text(text) => FluentValue::from(text.as_str()),
            Self::Number(value) => FluentValue::from(*value),
        }
    }
}

impl From<&str> for Arg {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<String> for Arg {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&String> for Arg {
    fn from(text: &String) -> Self {
        Self::Text(text.clone())
    }
}

impl From<std::borrow::Cow<'_, str>> for Arg {
    fn from(text: std::borrow::Cow<'_, str>) -> Self {
        Self::Text(text.into_owned())
    }
}

impl From<char> for Arg {
    fn from(text: char) -> Self {
        Self::Text(text.to_string())
    }
}

macro_rules! number_args {
    ($($kind:ty),*) => {$(
        impl From<$kind> for Arg {
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless, reason = "numbers people read")]
            fn from(value: $kind) -> Self {
                Self::Number(value as f64)
            }
        }
        impl From<&$kind> for Arg {
            #[allow(clippy::cast_precision_loss, clippy::cast_lossless, reason = "numbers people read")]
            fn from(value: &$kind) -> Self {
                Self::Number(*value as f64)
            }
        }
    )*};
}

number_args!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

/// How [`tr!`] turns any value into an [`Arg`]: numbers stay numbers (so
/// they pick plural forms), anything else that displays becomes its text.
#[doc(hidden)]
pub mod args {
    use std::fmt::Display;

    use super::Arg;

    /// A value on its way to becoming an [`Arg`].
    pub struct Wrap<'v, T: ?Sized>(pub &'v T);

    /// Numbers, picked first.
    pub trait ViaNumber {
        /// The argument.
        fn arg(&self) -> Arg;
    }

    /// Anything that displays.
    pub trait ViaDisplay {
        /// The argument.
        fn arg(&self) -> Arg;
    }

    // `(&Wrap(&value)).arg()` finds a number's impl without borrowing
    // again, and only borrows again (to reach the display impl) when the
    // value is no number.
    macro_rules! numbers {
        ($($kind:ty),*) => {$(
            impl ViaNumber for Wrap<'_, $kind> {
                fn arg(&self) -> Arg {
                    Arg::from(*self.0)
                }
            }
            impl ViaNumber for Wrap<'_, &$kind> {
                fn arg(&self) -> Arg {
                    Arg::from(**self.0)
                }
            }
        )*};
    }

    numbers!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

    impl<T: Display + ?Sized> ViaDisplay for &Wrap<'_, T> {
        fn arg(&self) -> Arg {
            Arg::Text(self.0.to_string())
        }
    }
}

// ----- Lookups -----------------------------------------------------------------

/// The message `key` of `namespace` in the current language, else in the
/// closest one, else in English. `message.attribute` reads one of the
/// message's attributes.
#[must_use]
pub fn text(namespace: &str, key: &str) -> String {
    text_with(namespace, key, &[])
}

/// [`text`] with values for the message's variables.
#[must_use]
pub fn text_with(namespace: &str, key: &str, args: &[(&str, Arg)]) -> String {
    let tag = current();
    read().format(&tag, namespace, key, args)
}

/// [`text`] for a key made at runtime (a setting's name, a plug-in's
/// tool): `fallback` when no language has it.
#[must_use]
pub fn text_or(namespace: &str, key: &str, fallback: &str) -> String {
    let tag = current();
    let database = read();
    database
        .chain(&tag)
        .into_iter()
        .find_map(|language| language.format(namespace, key, &[]))
        .unwrap_or_else(|| fallback.to_owned())
}

/// `true` if English has message `key` in `namespace`.
#[must_use]
pub fn has(namespace: &str, key: &str) -> bool {
    read()
        .language(ENGLISH)
        .is_some_and(|english| english.has(namespace, key))
}

/// Reads a message: `tr!("namespace", "key")`, or with variables,
/// `tr!("namespace", "key", name = value, count = 3)`. A variable's value
/// is a number (which picks plural forms) or anything that displays.
#[macro_export]
macro_rules! tr {
    ($namespace:expr, $key:expr $(,)?) => {
        $crate::text($namespace, $key)
    };
    ($namespace:expr, $key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        #[allow(unused_imports)]
        use $crate::args::{ViaDisplay as _, ViaNumber as _};
        $crate::text_with(
            $namespace,
            $key,
            &[$((stringify!($name), (&$crate::args::Wrap(&$value)).arg())),+],
        )
    }};
}

// ----- Which language ----------------------------------------------------------

/// The language lookups on this thread read now: the innermost [`scope`],
/// else the global language.
#[must_use]
pub fn current() -> String {
    SCOPES
        .with(|scopes| scopes.borrow().last().cloned())
        .unwrap_or_else(|| read().global.clone())
}

/// The global language (outside any scope).
#[must_use]
pub fn global() -> String {
    read().global.clone()
}

/// Sets the global language from a setting: a language tag, or
/// [`SYSTEM`] for the system's.
pub fn set_language(setting: &str) {
    let tag = resolve(setting);
    let mut database = write();
    if database.global != tag {
        database.global = tag;
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

/// Tells the database the system's languages, most preferred first (from
/// `sue_platform::system_languages`). The first Sue has becomes what
/// [`SYSTEM`] means; with none, English.
pub fn set_system<S: AsRef<str>>(tags: &[S]) {
    let mut database = write();
    let found = tags
        .iter()
        .find_map(|tag| database.best(tag.as_ref()))
        .unwrap_or_else(|| ENGLISH.to_owned());
    database.system = found;
}

/// The language [`SYSTEM`] stands for.
#[must_use]
pub fn system() -> String {
    read().system.clone()
}

/// The language a setting names: [`SYSTEM`] (or nothing) is the system's;
/// a tag is the closest language Sue has to it, else English.
#[must_use]
pub fn resolve(setting: &str) -> String {
    let database = read();
    let setting = setting.trim();
    if setting.is_empty() || setting.eq_ignore_ascii_case(SYSTEM) {
        return database.system.clone();
    }
    database.best(setting).unwrap_or_else(|| ENGLISH.to_owned())
}

/// The language a studio or tool speaks, from its own setting: [`FOLLOW`]
/// (or nothing) is `global`, anything else as [`resolve`] reads it.
#[must_use]
pub fn resolve_for(setting: &str, global: &str) -> String {
    let setting = setting.trim();
    if setting.is_empty() || setting.eq_ignore_ascii_case(FOLLOW) {
        global.to_owned()
    } else {
        resolve(setting)
    }
}

/// While alive, lookups on this thread read `tag` (see [`scope`]).
#[derive(Debug)]
#[must_use = "the scope ends when it is dropped"]
pub struct Scope(());

impl Drop for Scope {
    fn drop(&mut self) {
        SCOPES.with(|scopes| {
            scopes.borrow_mut().pop();
        });
    }
}

/// Makes this thread's lookups read `tag` until the returned [`Scope`] is
/// dropped. Scopes nest; the innermost wins.
pub fn scope(tag: &str) -> Scope {
    SCOPES.with(|scopes| scopes.borrow_mut().push(tag.to_owned()));
    Scope(())
}

/// Changes whenever the global language or the messages change, so a
/// part of Sue that keeps text can tell when to read it again.
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

// ----- Languages ---------------------------------------------------------------

/// A language Sue has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageInfo {
    /// Its tag, e.g. `pt-BR`.
    pub tag: String,
    /// Its name in its own script, e.g. `Português (Brasil)`.
    pub name: String,
    /// `true` while it waits for a native speaker's review.
    pub draft: bool,
}

/// Every language Sue has, English first, then by tag.
#[must_use]
pub fn languages() -> Vec<LanguageInfo> {
    read()
        .languages
        .iter()
        .map(|language| LanguageInfo {
            tag: language.tag.clone(),
            name: language
                .format(LANGUAGE, "language-name", &[])
                .unwrap_or_else(|| language.tag.clone()),
            draft: language
                .format(LANGUAGE, "language-status", &[])
                .is_some_and(|status| status.trim() == "draft"),
        })
        .collect()
}

/// The name of language `tag` in its own script, else the tag.
#[must_use]
pub fn name(tag: &str) -> String {
    let database = read();
    database
        .language(tag)
        .and_then(|language| language.format(LANGUAGE, "language-name", &[]))
        .unwrap_or_else(|| tag.to_owned())
}

/// Every namespace any language has.
#[must_use]
pub fn namespaces() -> Vec<String> {
    let database = read();
    let mut names: Vec<String> = database
        .languages
        .iter()
        .flat_map(|language| language.namespaces.keys().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Adds Fluent messages for `namespace` in language `tag` (a new language
/// too): how plug-ins bring their own text, or a new language, at runtime.
/// A message already there is replaced.
///
/// # Errors
/// The parse errors, one line each; whatever parsed is added anyway.
pub fn add_resource(tag: &str, namespace: &str, source: &str) -> Result<(), Vec<String>> {
    let mut database = write();
    let errors = database.language_mut(tag).add(namespace, source);
    database.sort();
    database.refresh_numbers();
    GENERATION.fetch_add(1, Ordering::Relaxed);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Parse errors in the built-in files (none in a good build).
#[must_use]
pub fn built_in_errors() -> Vec<String> {
    read().errors.clone()
}

/// Every character `tag`'s messages use beyond ASCII, so the interface can
/// make sure its fonts cover them.
#[must_use]
pub fn characters(tag: &str) -> String {
    let database = read();
    let Some(language) = database.language(tag) else {
        return String::new();
    };
    let mut seen: Vec<char> = language
        .namespaces
        .values()
        .flat_map(|space| space.sources.iter())
        .flat_map(|source| source.chars())
        .filter(|ch| !ch.is_ascii() && !ch.is_whitespace())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.into_iter().collect()
}

/// The sources of every namespace of `tag`, and English's, read at once,
/// for the checks.
type Sources = BTreeMap<String, Vec<String>>;

fn sources(tag: &str) -> Option<(Sources, Sources)> {
    let database = read();
    let of = |language: &Language| -> Sources {
        language
            .namespaces
            .iter()
            .map(|(name, space)| (name.clone(), space.sources.clone()))
            .collect()
    };
    let own = of(database.language(tag)?);
    let english = database.language(ENGLISH).map(of).unwrap_or_default();
    Some((own, english))
}

#[cfg(test)]
mod tests;
