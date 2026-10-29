//! Changing the case of text, as Format, Text, Change Case does in Word and
//! LibreOffice Writer.

/// A case to change text to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// ALL CAPITALS.
    Upper,
    /// all small letters.
    Lower,
    /// The first letter of each sentence a capital, the rest small.
    Sentence,
    /// The First Letter Of Every Word A Capital.
    Title,
    /// cAPITALS AND SMALL LETTERS SWAPPED.
    Toggle,
}

impl Case {
    /// Every case, with what people read for it.
    pub const ALL: [(Self, &'static str, &'static str); 5] = [
        (Self::Sentence, "sentence", "Sentence case"),
        (Self::Lower, "lower", "lowercase"),
        (Self::Upper, "upper", "UPPERCASE"),
        (Self::Title, "title", "Capitalize Every Word"),
        (Self::Toggle, "toggle", "tOGGLE cASE"),
    ];

    /// The case named `name` (`upper`, `lower`, `sentence`, `title`,
    /// `toggle`).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, known, _)| *known == name)
            .map(|(case, _, _)| *case)
    }
}

/// `text` in `case`. `sentence_start` says whether `text` begins a
/// sentence (the start of a paragraph, or after a full stop).
#[must_use]
pub fn change(text: &str, case: Case, sentence_start: bool) -> String {
    match case {
        Case::Upper => text.to_uppercase(),
        Case::Lower => text.to_lowercase(),
        Case::Toggle => text
            .chars()
            .flat_map(|ch| {
                if ch.is_uppercase() {
                    ch.to_lowercase().collect::<Vec<_>>()
                } else {
                    ch.to_uppercase().collect()
                }
            })
            .collect(),
        Case::Title => {
            let mut out = String::with_capacity(text.len());
            let mut word_start = true;
            for ch in text.chars() {
                if ch.is_alphanumeric() {
                    if word_start {
                        out.extend(ch.to_uppercase());
                    } else {
                        out.extend(ch.to_lowercase());
                    }
                    word_start = false;
                } else {
                    // An apostrophe inside a word ("don't") keeps it one.
                    word_start = !matches!(ch, '\'' | '\u{2019}');
                    out.push(ch);
                }
            }
            out
        }
        Case::Sentence => {
            let mut out = String::with_capacity(text.len());
            let mut start = sentence_start;
            for ch in text.chars() {
                if ch.is_alphabetic() {
                    if start {
                        out.extend(ch.to_uppercase());
                    } else {
                        out.extend(ch.to_lowercase());
                    }
                    start = false;
                } else {
                    // Quotes and brackets leave a sentence's start as it is.
                    if matches!(ch, '.' | '!' | '?' | '\n') {
                        start = true;
                    } else if !ch.is_whitespace()
                        && !"\"'\u{201C}\u{201D}\u{2018}\u{2019}()[]".contains(ch)
                    {
                        start = false;
                    }
                    out.push(ch);
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_changes_case_by_letters_words_and_sentences() {
        let text = "the storm CAME. it didn't stop! \u{201C}why?\u{201D} she said";
        assert_eq!(change("Mixed Case", Case::Upper, false), "MIXED CASE");
        assert_eq!(change("Mixed Case", Case::Lower, false), "mixed case");
        assert_eq!(change("Mixed Case", Case::Toggle, false), "mIXED cASE");
        assert_eq!(
            change(text, Case::Title, false),
            "The Storm Came. It Didn't Stop! \u{201C}Why?\u{201D} She Said"
        );
        assert_eq!(
            change(text, Case::Sentence, true),
            "The storm came. It didn't stop! \u{201C}Why?\u{201D} She said"
        );
        assert_eq!(
            change("MIDDLE WORDS", Case::Sentence, false),
            "middle words"
        );
        assert_eq!(change("stra\u{df}e", Case::Upper, false), "STRASSE");
        assert_eq!(Case::parse("title"), Some(Case::Title));
        assert_eq!(Case::parse("shout"), None);
    }
}
