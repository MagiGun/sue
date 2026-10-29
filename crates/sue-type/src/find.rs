//! Finding and replacing in text, for every studio that edits it: plain
//! text with or without matching case and whole words, or a regular
//! expression whose replacement can name what it caught (`$1`, `${name}`).

use std::ops::Range;

use regex::{Regex, RegexBuilder};

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    /// The text, or the regular expression.
    pub text: String,
    /// Capitals and small letters must match.
    pub case: bool,
    /// Only whole words.
    pub words: bool,
    /// `text` is a regular expression.
    pub regex: bool,
}

/// A query ready to search with.
#[derive(Clone, Debug)]
pub struct Finder {
    query: Query,
    regex: Option<Regex>,
}

impl Finder {
    /// Prepares `query`.
    ///
    /// # Errors
    /// What is wrong with its regular expression, in words.
    pub fn new(query: &Query) -> Result<Self, String> {
        let regex = if query.regex && !query.text.is_empty() {
            let pattern = if query.words {
                format!(r"\b(?:{})\b", query.text)
            } else {
                query.text.clone()
            };
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!query.case)
                .multi_line(true)
                .build()
                .map_err(|error| match error {
                    regex::Error::Syntax(_) => "That regular expression is not valid".to_owned(),
                    _ => "That regular expression is too large".to_owned(),
                })?;
            Some(regex)
        } else {
            None
        };
        Ok(Self {
            query: query.clone(),
            regex,
        })
    }

    /// Every match in `text`, in order, none of them empty.
    #[must_use]
    pub fn matches(&self, text: &str) -> Vec<Range<usize>> {
        if self.query.text.is_empty() {
            return Vec::new();
        }
        match &self.regex {
            Some(regex) => regex
                .find_iter(text)
                .map(|found| found.range())
                .filter(|range| !range.is_empty())
                .collect(),
            None => plain(&self.query, text),
        }
    }

    /// What replaces match `range` of `text`: `with` itself, or for a
    /// regular expression `with` with `$1` and `${name}` filled in from
    /// what it caught.
    #[must_use]
    pub fn replacement(&self, text: &str, range: Range<usize>, with: &str) -> String {
        let Some(regex) = &self.regex else {
            return with.to_owned();
        };
        // The match again, anchored where it was found, for its groups.
        let Some(caught) = regex
            .captures_at(text, range.start)
            .filter(|caught| caught.get(0).is_some_and(|whole| whole.range() == range))
        else {
            return with.to_owned();
        };
        let mut out = String::new();
        caught.expand(with, &mut out);
        out
    }

    /// `true` if `selected` is exactly one match (Replace replaces the
    /// selection only then).
    #[must_use]
    pub fn is_match(&self, selected: &str) -> bool {
        !selected.is_empty() && self.matches(selected).first() == Some(&(0..selected.len()))
    }
}

/// Plain text matches, case folded unless asked not to.
fn plain(query: &Query, text: &str) -> Vec<Range<usize>> {
    let (hay, needle) = if query.case {
        (text.to_owned(), query.text.clone())
    } else {
        (text.to_lowercase(), query.text.to_lowercase())
    };
    // Lowercasing may change lengths; only use it when it did not.
    let (hay, needle) = if hay.len() == text.len() {
        (hay, needle)
    } else {
        (text.to_owned(), query.text.clone())
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = hay[from..].find(&needle) {
        let start = from + at;
        let end = start + needle.len();
        let boundary = |at: usize, before: bool| {
            let ch = if before {
                text[..at].chars().next_back()
            } else {
                text[at..].chars().next()
            };
            ch.is_none_or(|ch| !ch.is_alphanumeric())
        };
        if !query.words || (boundary(start, true) && boundary(end, false)) {
            out.push(start..end);
        }
        from = end.max(start + 1);
        while from < hay.len() && !hay.is_char_boundary(from) {
            from += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str, case: bool, words: bool, regex: bool) -> Query {
        Query {
            text: text.to_owned(),
            case,
            words,
            regex,
        }
    }

    #[test]
    fn plain_text_matches_by_case_and_whole_words() {
        let text = "The cat sat. Cats scatter; the CAT.";
        let all = Finder::new(&query("cat", false, false, false)).unwrap();
        assert_eq!(all.matches(text).len(), 4);
        let words = Finder::new(&query("cat", false, true, false)).unwrap();
        assert_eq!(words.matches(text), vec![4..7, 31..34]);
        let case = Finder::new(&query("cat", true, true, false)).unwrap();
        assert_eq!(case.matches(text), vec![4..7]);
        assert!(case.is_match("cat"));
        assert!(!case.is_match("cats"));
        assert_eq!(case.replacement(text, 4..7, "dog"), "dog");
    }

    #[test]
    fn regular_expressions_match_and_fill_in_their_groups() {
        let text = "Smith, John\nDoe, Jane\n";
        let names = Finder::new(&query(r"^(\w+), (\w+)$", true, false, true)).unwrap();
        let found = names.matches(text);
        assert_eq!(found, vec![0..11, 12..21]);
        assert_eq!(
            names.replacement(text, found[1].clone(), "$2 $1"),
            "Jane Doe"
        );
        let named =
            Finder::new(&query(r"(?P<last>\w+), (?P<first>\w+)", true, false, true)).unwrap();
        assert_eq!(
            named.replacement(text, 0..11, "${first} ${last}"),
            "John Smith"
        );
        // Case and whole words apply to expressions too.
        let digits = Finder::new(&query(r"\d+", false, true, true)).unwrap();
        assert_eq!(digits.matches("a1 22 b333c"), vec![3..5]);
        let folded = Finder::new(&query("jo.n", false, false, true)).unwrap();
        assert_eq!(folded.matches(text), vec![7..11]);
        assert!(Finder::new(&query("(unclosed", true, false, true)).is_err());
        // An expression that can match nothing finds no empty matches.
        let empty = Finder::new(&query("x*", true, false, true)).unwrap();
        assert_eq!(empty.matches("axxb"), vec![1..3]);
    }
}
