//! Line-oriented syntax for the `.suerice` TOML subset and its metadata comments.

use std::ops::Range;

use crate::RiceError;

/// A value literal.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Literal {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// One bound of a `[min..max]` range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Bound {
    Int(i64),
    Float(f64),
}

impl Bound {
    pub(crate) fn as_f64(self) -> f64 {
        match self {
            Self::Int(int) => int as f64,
            Self::Float(float) => float,
        }
    }
}

/// An `@tag` forcing a widget type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tag {
    Color,
    Keybind,
    Shader,
    Text,
}

/// Metadata parsed from a trailing comment.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Metadata {
    pub(crate) range: Option<(Option<Bound>, Option<Bound>)>,
    pub(crate) choices: Option<Vec<String>>,
    pub(crate) tag: Option<Tag>,
    pub(crate) description: Option<String>,
}

/// A `key = value` line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedEntry {
    pub(crate) key: String,
    pub(crate) literal: Literal,
    /// Byte range of the value literal within the original line.
    pub(crate) value_span: Range<usize>,
    pub(crate) metadata: Metadata,
}

/// The meaning of one source line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Line {
    /// Blank or comment-only.
    Blank,
    /// `[table]` header.
    Section(String),
    /// `key = value`.
    Entry(ParsedEntry),
}

fn syntax(line: usize, message: impl Into<String>) -> RiceError {
    RiceError::Syntax {
        line,
        message: message.into(),
    }
}

/// Parses one line. `line` is the 1-based line number, used in errors.
pub(crate) fn parse_line(raw: &str, line: usize) -> Result<Line, RiceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(Line::Blank);
    }
    if trimmed.starts_with("[[") {
        return Err(RiceError::Unsupported {
            line,
            feature: "arrays of tables",
        });
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        let close = rest
            .find(']')
            .ok_or_else(|| syntax(line, "unclosed table header"))?;
        let name = normalize_key(&rest[..close], line)?;
        let after = rest[close + 1..].trim();
        if !after.is_empty() && !after.starts_with('#') {
            return Err(syntax(line, "unexpected text after the table header"));
        }
        return Ok(Line::Section(name));
    }

    let equals = raw
        .find('=')
        .ok_or_else(|| syntax(line, "expected `key = value`"))?;
    let key = raw[..equals].trim();
    if key.starts_with(['"', '\'']) {
        return Err(RiceError::Unsupported {
            line,
            feature: "quoted keys",
        });
    }
    let key = normalize_key(key, line)?;
    let after_equals = &raw[equals + 1..];
    let value_start = equals + 1 + (after_equals.len() - after_equals.trim_start().len());
    let (literal, value_len) = parse_value(&raw[value_start..], line)?;
    let value_end = value_start + value_len;
    let tail = raw[value_end..].trim();
    let comment = match tail.strip_prefix('#') {
        Some(comment) => comment,
        None if tail.is_empty() => "",
        None => return Err(syntax(line, "unexpected text after the value")),
    };
    Ok(Line::Entry(ParsedEntry {
        key,
        literal,
        value_span: value_start..value_end,
        metadata: parse_metadata(comment, line)?,
    }))
}

/// Validates a bare or dotted key such as `gap` or `layout . gap`, and returns
/// it without the optional whitespace around dots.
pub(crate) fn normalize_key(key: &str, line: usize) -> Result<String, RiceError> {
    let bare = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    };
    let parts: Vec<&str> = key.split('.').map(str::trim).collect();
    if parts.iter().all(|part| bare(part)) {
        Ok(parts.join("."))
    } else {
        Err(syntax(line, format!("`{key}` is not a bare key")))
    }
}

/// Parses the value at the start of `rest`; returns it and its byte length.
fn parse_value(rest: &str, line: usize) -> Result<(Literal, usize), RiceError> {
    let unsupported = |feature| RiceError::Unsupported { line, feature };
    match rest.chars().next() {
        None => Err(syntax(line, "missing value")),
        Some('"') if rest.starts_with("\"\"\"") => Err(unsupported("multi-line strings")),
        Some('\'') if rest.starts_with("'''") => Err(unsupported("multi-line strings")),
        Some('"') => parse_basic_string(rest, line),
        Some('\'') => {
            let close = rest[1..]
                .find('\'')
                .ok_or_else(|| syntax(line, "unterminated literal string"))?;
            Ok((Literal::Str(rest[1..=close].to_owned()), close + 2))
        }
        Some('[') => Err(unsupported("arrays")),
        Some('{') => Err(unsupported("inline tables")),
        Some(_) => {
            let end = rest
                .find(|ch: char| ch == '#' || ch.is_whitespace())
                .unwrap_or(rest.len());
            let literal = parse_bare(&rest[..end]).map_err(|problem| match problem {
                BareProblem::Unsupported(feature) => unsupported(feature),
                BareProblem::Invalid(message) => syntax(line, message),
            })?;
            Ok((literal, end))
        }
    }
}

fn parse_basic_string(rest: &str, line: usize) -> Result<(Literal, usize), RiceError> {
    let mut text = String::new();
    let mut chars = rest.char_indices().skip(1);
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((Literal::Str(text), index + 1)),
            '\\' => {
                let (_, escape) = chars
                    .next()
                    .ok_or_else(|| syntax(line, "unterminated string"))?;
                let decoded = match escape {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'e' => '\u{1b}',
                    '"' => '"',
                    '\\' => '\\',
                    'u' | 'U' => {
                        let digits = if escape == 'u' { 4 } else { 8 };
                        let hex: String = chars.by_ref().take(digits).map(|(_, ch)| ch).collect();
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .filter(|_| hex.len() == digits)
                            .and_then(char::from_u32)
                            .ok_or_else(|| {
                                syntax(line, format!("invalid unicode escape \\{escape}{hex}"))
                            })?
                    }
                    other => return Err(syntax(line, format!("invalid escape \\{other}"))),
                };
                text.push(decoded);
            }
            ch if ch.is_control() && ch != '\t' => {
                return Err(syntax(line, "control characters must be escaped"));
            }
            ch => text.push(ch),
        }
    }
    Err(syntax(line, "unterminated string"))
}

/// Why a bare token is not a value.
#[derive(Debug)]
pub(crate) enum BareProblem {
    Unsupported(&'static str),
    Invalid(String),
}

/// Parses a bare literal: a bool, an integer or a finite float.
pub(crate) fn parse_bare(token: &str) -> Result<Literal, BareProblem> {
    let invalid = || BareProblem::Invalid(format!("`{token}` is not a valid value"));
    match token {
        "true" => return Ok(Literal::Bool(true)),
        "false" => return Ok(Literal::Bool(false)),
        "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan" => {
            return Err(BareProblem::Unsupported("non-finite floats"));
        }
        _ => {}
    }
    let bytes = token.as_bytes();
    let date_like = bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-';
    if date_like || token.contains(':') {
        return Err(BareProblem::Unsupported("dates and times"));
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = token.strip_prefix(prefix) {
            if !underscores_between_digits(digits, radix) {
                return Err(invalid());
            }
            return i64::from_str_radix(&digits.replace('_', ""), radix)
                .map(Literal::Int)
                .map_err(|_| invalid());
        }
    }
    let unsigned = token.strip_prefix(['+', '-']).unwrap_or(token);
    if unsigned.is_empty() || !underscores_between_digits(unsigned, 10) {
        return Err(invalid());
    }
    let cleaned = token.replace('_', "");
    if unsigned.contains(['.', 'e', 'E']) {
        let integer_part = unsigned.split(['.', 'e', 'E']).next().unwrap_or("");
        if integer_part.is_empty() || unsigned.ends_with('.') || has_leading_zero(integer_part) {
            return Err(invalid());
        }
        return cleaned
            .parse::<f64>()
            .ok()
            .filter(|float| float.is_finite())
            .map(Literal::Float)
            .ok_or_else(invalid);
    }
    if has_leading_zero(unsigned) || !unsigned.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
        return Err(invalid());
    }
    cleaned
        .parse::<i64>()
        .map(Literal::Int)
        .map_err(|_| invalid())
}

fn has_leading_zero(digits: &str) -> bool {
    digits.len() > 1 && digits.starts_with('0')
}

/// TOML allows `_` only between two digits of the literal's radix.
fn underscores_between_digits(text: &str, radix: u32) -> bool {
    let bytes = text.as_bytes();
    let is_digit = |index: usize| {
        bytes
            .get(index)
            .is_some_and(|&byte| char::from(byte).is_digit(radix))
    };
    bytes
        .iter()
        .enumerate()
        .filter(|&(_, &byte)| byte == b'_')
        .all(|(index, _)| index > 0 && is_digit(index - 1) && is_digit(index + 1))
}

/// Parses `[range] {choices} @tag description` from a comment.
fn parse_metadata(comment: &str, line: usize) -> Result<Metadata, RiceError> {
    let error = |message: String| RiceError::Metadata { line, message };
    let mut meta = Metadata::default();
    let mut rest = comment.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix('[') {
            let close = after
                .find(']')
                .ok_or_else(|| error("unclosed `[min..max]` range".to_owned()))?;
            if meta.range.is_some() {
                return Err(error("only one range is allowed".to_owned()));
            }
            meta.range = Some(parse_range(&after[..close]).map_err(error)?);
            rest = after[close + 1..].trim_start();
        } else if let Some(after) = rest.strip_prefix('{') {
            let close = after
                .find('}')
                .ok_or_else(|| error("unclosed `{a|b}` choice list".to_owned()))?;
            let options: Vec<String> = after[..close]
                .split('|')
                .map(|option| option.trim().to_owned())
                .collect();
            if options.iter().any(String::is_empty) {
                return Err(error("choices must not be empty".to_owned()));
            }
            if meta.choices.replace(options).is_some() {
                return Err(error("only one choice list is allowed".to_owned()));
            }
            rest = after[close + 1..].trim_start();
        } else if let Some(after) = rest.strip_prefix('@') {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            let tag = match &after[..end] {
                "color" => Tag::Color,
                "keybind" => Tag::Keybind,
                "shader" => Tag::Shader,
                "text" => Tag::Text,
                other => return Err(error(format!("unknown tag `@{other}`"))),
            };
            if meta.tag.replace(tag).is_some() {
                return Err(error("only one tag is allowed".to_owned()));
            }
            rest = after[end..].trim_start();
        } else {
            break;
        }
    }
    let description = rest.trim();
    meta.description = (!description.is_empty()).then(|| description.to_owned());
    Ok(meta)
}

fn parse_range(inner: &str) -> Result<(Option<Bound>, Option<Bound>), String> {
    let (low, high) = inner
        .split_once("..")
        .ok_or_else(|| format!("`[{inner}]` is not a `[min..max]` range"))?;
    let bound = |text: &str| -> Result<Option<Bound>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        match parse_bare(text) {
            Ok(Literal::Int(int)) => Ok(Some(Bound::Int(int))),
            Ok(Literal::Float(float)) => Ok(Some(Bound::Float(float))),
            _ => Err(format!("`{text}` is not a number")),
        }
    };
    let range = (bound(low)?, bound(high)?);
    if range == (None, None) {
        return Err("a range needs at least one bound".to_owned());
    }
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(raw: &str) -> ParsedEntry {
        match parse_line(raw, 1).unwrap() {
            Line::Entry(entry) => entry,
            other => panic!("expected an entry, got {other:?}"),
        }
    }

    #[test]
    fn classifies_lines() {
        assert_eq!(parse_line("   ", 1).unwrap(), Line::Blank);
        assert_eq!(parse_line("# note", 1).unwrap(), Line::Blank);
        assert_eq!(
            parse_line("[layout] # docks", 1).unwrap(),
            Line::Section("layout".into())
        );
        assert_eq!(
            parse_line("[a.b-c]", 1).unwrap(),
            Line::Section("a.b-c".into())
        );
    }

    #[test]
    fn records_value_spans_and_metadata() {
        let raw = "gap = 8   # [0..64] Space between panes";
        let parsed = entry(raw);
        assert_eq!(parsed.key, "gap");
        assert_eq!(parsed.literal, Literal::Int(8));
        assert_eq!(&raw[parsed.value_span.clone()], "8");
        assert_eq!(
            parsed.metadata.range,
            Some((Some(Bound::Int(0)), Some(Bound::Int(64))))
        );
        assert_eq!(
            parsed.metadata.description.as_deref(),
            Some("Space between panes")
        );
    }

    #[test]
    fn parses_strings_with_escapes() {
        let raw = r#"title = "Faust \"Enigma\" \u00e9"  # @text"#;
        let parsed = entry(raw);
        assert_eq!(parsed.literal, Literal::Str("Faust \"Enigma\" é".into()));
        assert_eq!(
            &raw[parsed.value_span.clone()],
            r#""Faust \"Enigma\" \u00e9""#
        );
        assert_eq!(parsed.metadata.tag, Some(Tag::Text));
        assert_eq!(
            entry("path = 'C:\\no\\escapes'").literal,
            Literal::Str("C:\\no\\escapes".into())
        );
        let hash = entry("accent = \"#c9a227\" # Focus ring");
        assert_eq!(hash.literal, Literal::Str("#c9a227".into()));
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_bare("1_000").unwrap(), Literal::Int(1000));
        assert_eq!(parse_bare("-17").unwrap(), Literal::Int(-17));
        assert_eq!(parse_bare("0xff").unwrap(), Literal::Int(255));
        assert_eq!(parse_bare("0b1010").unwrap(), Literal::Int(10));
        assert_eq!(parse_bare("6.5").unwrap(), Literal::Float(6.5));
        assert_eq!(parse_bare("1e3").unwrap(), Literal::Float(1000.0));
        for bad in ["007", "1__0", "_1", "1_", "1.", ".5", "0xg", "12abc"] {
            assert!(parse_bare(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn parses_choices_and_open_ranges() {
        let parsed = entry("mode = \"auto\" # {auto | balanced} Profile");
        assert_eq!(
            parsed.metadata.choices,
            Some(vec!["auto".into(), "balanced".into()])
        );
        assert_eq!(
            entry("x = 2 # [1..]").metadata.range,
            Some((Some(Bound::Int(1)), None))
        );
        assert_eq!(
            entry("x = 0.5 # [0.0..1.0]").metadata.range,
            Some((Some(Bound::Float(0.0)), Some(Bound::Float(1.0))))
        );
    }

    #[test]
    fn rejects_unsupported_toml() {
        let unsupported = |raw: &str| {
            matches!(
                parse_line(raw, 3),
                Err(RiceError::Unsupported { line: 3, .. })
            )
        };
        assert!(unsupported("list = [1, 2]"));
        assert!(unsupported("point = { x = 1 }"));
        assert!(unsupported("[[plugins]]"));
        assert!(unsupported("when = 1979-05-27"));
        assert!(unsupported("big = inf"));
        assert!(unsupported("text = \"\"\"multi"));
        assert!(unsupported("\"quoted\" = 1"));
    }

    #[test]
    fn reports_malformed_lines() {
        for raw in [
            "gap 8",
            "gap = ",
            "gap = 8 9",
            "[open",
            "bad key = 1",
            "s = \"open",
        ] {
            assert!(
                matches!(parse_line(raw, 7), Err(RiceError::Syntax { line: 7, .. })),
                "{raw}"
            );
        }
        for raw in [
            "x = 1 # @shiny",
            "x = 1 # [1..2] [3..4]",
            "x = 1 # [..]",
            "x = \"a\" # {a||b}",
        ] {
            assert!(
                matches!(parse_line(raw, 2), Err(RiceError::Metadata { line: 2, .. })),
                "{raw}"
            );
        }
    }
}
