//! Spelling, for every studio that sets text: is a word spelled right,
//! which words of a text are not, and what might have been meant.
//!
//! English ships with Sue: the public-domain Moby word list with its
//! inflections (plurals, `-ed`, `-ing`, `-er`, `-ly` and a few prefixes),
//! and a supplement of modern words. The list, its reader and the
//! inflection rules are adapted from WordCraft's proofing crate (MIT OR
//! Apache-2.0; `data/spelling/ATTRIBUTION.md`). Words people add (a
//! project's own dictionary) are passed in by the caller.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;

/// A property of a document or project: the words its own dictionary
/// takes as right (`List` of lower-case `Text`).
pub const WORDS: &str = "sue.spelling.words";

/// The words of `value` (a [`WORDS`] property), lower case.
#[must_use]
pub fn word_set(value: Option<&sue_core::Value>) -> HashSet<String> {
    match value {
        Some(sue_core::Value::List(words)) => words
            .iter()
            .filter_map(sue_core::Value::as_str)
            .map(str::to_lowercase)
            .collect(),
        _ => HashSet::new(),
    }
}

/// The deflated word list (see `data/spelling/ATTRIBUTION.md`).
const EN_US: &[u8] = include_bytes!("../data/spelling/en-us.dic");
/// Modern words the list lacks, one base form a line.
const EN_EXTRA: &str = include_str!("../data/spelling/en-extra.txt");
/// Short words every English text uses, whatever the list says.
const COMMON: &str = "a an the and or but if of to in on at by for with from as is am are was were be been being it its we us our you your he him his she her they them their i me my mine this that these those there here what which who whom whose when where why how all any both each few more most other some such no nor not only own same so than too very can will just should now do does did done has have had having may might must shall would could up down out over under again further then once off into onto upon about above below between through during before after until while because per via vs etc ok yes oh ah hi hey bye one two three four five six seven eight nine ten first last next new old get got go goes went gone see saw seen say said make made take took come came give gave know knew think thought look want need use used find found tell told ask asked feel felt try left keep let put mean set run show turn move live play pay hear hold bring begin seem help talk start";

const MAGIC: &[u8] = b"DCHYPH1\n";
/// The most bytes the list may inflate to, and the most words kept.
const MAX_BYTES: usize = 64 << 20;
const MAX_WORDS: usize = 4_000_000;

/// A sorted word list.
#[derive(Clone, Debug, Default)]
pub struct WordList {
    words: String,
    ends: Vec<u32>,
}

impl WordList {
    /// The list from its deflated, front-coded form; `-` marks inside a
    /// word (its break points) are left out.
    ///
    /// # Errors
    /// What is wrong with the data.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_BYTES)
            .map_err(|error| format!("word list: inflating failed: {:?}", error.status))?;
        let body = raw.strip_prefix(MAGIC).ok_or("word list: bad header")?;
        let mut list = Self {
            words: String::with_capacity(body.len()),
            ends: Vec::new(),
        };
        let mut current: Vec<u8> = Vec::new();
        let mut at = 0;
        while at < body.len() {
            let shared = usize::from(body[at]);
            let end = body[at + 1..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|end| at + 1 + end)
                .ok_or("word list: cut short")?;
            if shared > current.len() {
                return Err("word list: bad prefix".to_owned());
            }
            current.truncate(shared);
            current.extend_from_slice(&body[at + 1..end]);
            at = end + 1;
            let line = std::str::from_utf8(&current).map_err(|_| "word list: not UTF-8")?;
            if list.ends.len() >= MAX_WORDS || list.words.len() + line.len() > MAX_BYTES {
                return Err("word list: too large".to_owned());
            }
            list.words.extend(line.chars().filter(|ch| *ch != '-'));
            list.ends.push(list.words.len() as u32);
        }
        Ok(list)
    }

    /// The number of words.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// `true` when it holds no words.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    fn word(&self, index: usize) -> &str {
        let start = if index == 0 {
            0
        } else {
            self.ends[index - 1] as usize
        };
        &self.words[start..self.ends[index] as usize]
    }

    /// `true` if it holds `word` exactly.
    #[must_use]
    pub fn contains(&self, word: &str) -> bool {
        let (mut low, mut high) = (0, self.len());
        while low < high {
            let middle = (low + high) / 2;
            match self.word(middle).as_bytes().cmp(word.as_bytes()) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }

    /// Every word, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> + '_ {
        (0..self.len()).map(|index| self.word(index))
    }
}

/// The English word list.
#[must_use]
pub fn english() -> &'static WordList {
    static LIST: OnceLock<WordList> = OnceLock::new();
    LIST.get_or_init(|| WordList::from_bytes(EN_US).unwrap_or_default())
}

fn extra() -> &'static HashSet<&'static str> {
    static EXTRA: OnceLock<HashSet<&'static str>> = OnceLock::new();
    EXTRA.get_or_init(|| {
        EN_EXTRA
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    })
}

fn base_known(word: &str) -> bool {
    english().contains(word) || extra().contains(word) || COMMON.split(' ').any(|c| c == word)
}

/// `true` if lower-case `word` is a known word or an inflection of one.
fn known(word: &str) -> bool {
    if base_known(word) {
        return true;
    }
    let stems: [(&str, &[&str]); 13] = [
        ("'s", &[""]),
        ("s", &[""]),
        ("es", &["", "e"]),
        ("ies", &["y"]),
        ("ed", &["", "e"]),
        ("ied", &["y"]),
        ("ing", &["", "e"]),
        ("ly", &["", "le"]),
        ("ily", &["y"]),
        ("ically", &["ic"]),
        ("er", &["", "e"]),
        ("est", &["", "e"]),
        ("ness", &[""]),
    ];
    for (suffix, adds) in stems {
        let Some(stem) = word.strip_suffix(suffix) else {
            continue;
        };
        if stem.len() < 2 {
            continue;
        }
        if adds.iter().any(|add| base_known(&format!("{stem}{add}"))) {
            return true;
        }
        // A doubled consonant: running, stopped.
        let bytes = stem.as_bytes();
        if matches!(suffix, "ing" | "ed" | "er" | "est")
            && bytes.len() >= 3
            && bytes.last() == bytes.get(bytes.len() - 2)
            && let Some(single) = stem.get(..stem.len() - 1)
            && base_known(single)
        {
            return true;
        }
    }
    ["un", "re", "non", "pre", "over", "under", "co", "mis"]
        .iter()
        .any(|prefix| {
            word.strip_prefix(prefix)
                .is_some_and(|rest| rest.len() >= 3 && base_known(rest))
        })
}

/// `true` if `word` is spelled right in English, or is one of `also`
/// (lower case: a project's own words). Numbers, words with digits and
/// all-capital abbreviations count as right.
#[must_use]
pub fn is_correct(word: &str, also: &dyn Fn(&str) -> bool) -> bool {
    let word = word
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\'')
        .trim_matches('\'');
    if word.is_empty() || word.chars().any(|ch| ch.is_numeric()) {
        return true;
    }
    let letters = word.chars().filter(|ch| ch.is_alphabetic()).count();
    if letters > 1 && word.chars().all(|ch| !ch.is_lowercase()) {
        // An abbreviation such as NASA or HTML.
        return true;
    }
    if !word.is_ascii() && word.chars().any(|ch| ch.is_alphabetic() && !ch.is_ascii()) {
        // Not English (Sue's only list so far): left alone.
        return true;
    }
    let lower = word.to_lowercase().replace('\u{2019}', "'");
    if also(&lower) {
        return true;
    }
    let parts: Vec<&str> = lower.split('-').filter(|part| !part.is_empty()).collect();
    !parts.is_empty() && parts.iter().all(|part| known(part) || also(part))
}

/// The words of `text`, as byte ranges: letters and digits, with
/// apostrophes and hyphens inside a word.
#[must_use]
pub fn words(text: &str) -> Vec<Range<usize>> {
    let inside = |ch: char| ch.is_alphanumeric() || matches!(ch, '\'' | '\u{2019}' | '-');
    let mut out = Vec::new();
    let mut start = None;
    for (at, ch) in text.char_indices() {
        match (start, inside(ch)) {
            (None, true) => start = Some(at),
            (Some(from), false) => {
                out.push(from..at);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        out.push(from..text.len());
    }
    out.into_iter()
        .filter_map(|range| {
            let word = &text[range.clone()];
            let lead = word.len() - word.trim_start_matches(['\'', '\u{2019}', '-']).len();
            let trail = word.len() - word.trim_end_matches(['\'', '\u{2019}', '-']).len();
            (range.start + lead < range.end - trail)
                .then_some(range.start + lead..range.end - trail)
        })
        .collect()
}

/// The words of `text` spelled wrong (`also` as for [`is_correct`]).
#[must_use]
pub fn misspelled(text: &str, also: &dyn Fn(&str) -> bool) -> Vec<Range<usize>> {
    words(text)
        .into_iter()
        .filter(|range| !is_correct(&text[range.clone()], also))
        .collect()
}

/// Up to `max` words that `word` may have meant, the likeliest first,
/// in its case.
#[must_use]
pub fn suggest(word: &str, max: usize) -> Vec<String> {
    let lower = word.to_lowercase();
    let length = lower.chars().count();
    if length == 0 || length > 40 {
        return Vec::new();
    }
    let target: Vec<char> = lower.chars().collect();
    let mut scored: Vec<(usize, bool, usize, &str)> = Vec::new();
    for known in english().iter().chain(extra().iter().copied()) {
        let size = known.chars().count();
        if size + 2 < length || size > length + 2 {
            continue;
        }
        let distance = distance(&target, known);
        if distance <= 2 {
            let other_start = known.chars().next() != target.first().copied();
            scored.push((distance, other_start, size.abs_diff(length), known));
        }
    }
    let mut found: Vec<(usize, bool, usize, String)> = scored
        .into_iter()
        .map(|(distance, other, size, known)| (distance, other, size, known.to_owned()))
        .collect();
    // An inflected word: what the stem may have meant, inflected the same
    // way (recieved: received).
    for suffix in ["ing", "ed", "es", "s", "er", "est", "ly"] {
        let Some(stem) = lower.strip_suffix(suffix) else {
            continue;
        };
        if stem.chars().count() < 3 {
            continue;
        }
        let stem_chars: Vec<char> = stem.chars().collect();
        for base in english().iter().chain(extra().iter().copied()) {
            let size = base.chars().count();
            if size + 1 < stem_chars.len() || size > stem_chars.len() + 2 {
                continue;
            }
            if distance(&stem_chars, base) > 1 {
                continue;
            }
            let dropped = base
                .strip_suffix('e')
                .filter(|_| suffix.starts_with(['e', 'i']))
                .unwrap_or(base);
            let candidate = format!("{dropped}{suffix}");
            if known(&candidate) {
                let away = distance(&target, &candidate);
                if away <= 2 {
                    let other = candidate.chars().next() != target.first().copied();
                    found.push((
                        away,
                        other,
                        candidate.chars().count().abs_diff(length),
                        candidate,
                    ));
                }
            }
        }
    }
    found.sort();
    found.dedup_by(|a, b| a.3 == b.3);
    found
        .into_iter()
        .filter(|(distance, ..)| *distance > 0)
        .take(max)
        .map(|(_, _, _, known)| in_case_of(word, &known))
        .collect()
}

/// `known` (lower case) in the case of `word`.
fn in_case_of(word: &str, known: &str) -> String {
    let capital = word.chars().next().is_some_and(char::is_uppercase);
    let all = word.chars().count() > 1 && word.chars().all(|ch| !ch.is_lowercase());
    if all {
        known.to_uppercase()
    } else if capital {
        let mut chars = known.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    } else {
        known.to_owned()
    }
}

/// Optimal string alignment distance, giving up at 3.
fn distance(a: &[char], b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut before = vec![0_usize; m + 1];
    let mut last: Vec<usize> = (0..=m).collect();
    let mut row = vec![0_usize; m + 1];
    for i in 1..=n {
        row[0] = i;
        let mut least = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (last[j] + 1).min(row[j - 1] + 1).min(last[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(before[j - 2] + 1);
            }
            row[j] = value;
            least = least.min(value);
        }
        if least > 2 {
            return 3;
        }
        std::mem::swap(&mut before, &mut last);
        std::mem::swap(&mut last, &mut row);
    }
    last[m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_words_and_their_inflections_are_known() {
        assert!(english().len() > 100_000, "{}", english().len());
        let none = |_: &str| false;
        for word in [
            "lighthouse",
            "Storm",
            "running",
            "stopped",
            "stories",
            "quickly",
            "keeper's",
            "don't",
            "well-known",
            "NASA",
            "1990s",
            "email",
            "unhappy",
        ] {
            assert!(is_correct(word, &none), "{word}");
        }
        for word in ["lighthose", "recieve", "climbd"] {
            assert!(!is_correct(word, &none), "{word}");
        }
        let mine = |word: &str| word == "mara";
        assert!(is_correct("Mara", &mine));
        let text = "The keepr climbd the stairs.";
        let wrong: Vec<&str> = misspelled(text, &none)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(wrong, vec!["keepr", "climbd"]);
    }

    #[test]
    fn suggestions_are_near_and_in_case() {
        let found = suggest("Recieved", 5);
        assert!(found.contains(&"Received".to_owned()), "{found:?}");
        let found = suggest("recieve", 3);
        assert_eq!(
            found.first().map(String::as_str),
            Some("receive"),
            "{found:?}"
        );
        assert!(suggest("", 3).is_empty());
        assert_eq!(
            words("'Tis a well-known fact."),
            vec![1..4, 5..6, 7..17, 18..22]
        );
    }
}
