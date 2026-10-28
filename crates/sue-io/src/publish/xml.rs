//! Just enough XML for documents: escaping text for writing, and a reader
//! that turns markup into open tags, close tags and text (for Word and
//! HTML imports). It is forgiving: what it does not understand it skips.

/// Escapes `text` for XML content and attribute values.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters XML 1.0 cannot carry.
            ch if (ch as u32) < 0x20 && !matches!(ch, '\t' | '\n' | '\r') => {}
            ch => out.push(ch),
        }
    }
    out
}

/// One piece of markup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XmlEvent {
    /// `<name a="b">` (or `<name/>` when `empty`).
    Open {
        /// The tag name, with its prefix (`w:p`).
        name: String,
        /// Its attributes, unescaped.
        attrs: Vec<(String, String)>,
        /// `true` for a self-closing tag.
        empty: bool,
    },
    /// `</name>`.
    Close(String),
    /// Text between tags, unescaped.
    Text(String),
}

impl XmlEvent {
    /// The value of attribute `key` of an open tag.
    #[must_use]
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Self::Open { attrs, .. } => attrs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str()),
            _ => None,
        }
    }
}

/// Resolves entities and character references.
#[must_use]
pub fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{A0}'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(ch) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reads `xml` into events. Comments, declarations and processing
/// instructions are skipped; CDATA becomes text.
#[must_use]
pub fn parse(xml: &str) -> Vec<XmlEvent> {
    let mut events = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            events.push(XmlEvent::Text(unescape(rest)));
            break;
        };
        if lt > 0 {
            events.push(XmlEvent::Text(unescape(&rest[..lt])));
        }
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |end| &body[end + 3..]);
            continue;
        }
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            events.push(XmlEvent::Text(body[..end].to_owned()));
            rest = body.get(end + 3..).unwrap_or("");
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        // A tag: find its end outside quotes.
        let bytes = rest.as_bytes();
        let mut quote = None;
        let mut end = None;
        for (index, byte) in bytes.iter().enumerate().skip(1) {
            match (quote, byte) {
                (None, b'"' | b'\'') => quote = Some(*byte),
                (Some(open), _) if *byte == open => quote = None,
                (None, b'>') => {
                    end = Some(index);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            break;
        };
        let inner = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = inner.strip_prefix('/') {
            events.push(XmlEvent::Close(name.trim().to_owned()));
            continue;
        }
        let empty = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let name_end = inner
            .find(|ch: char| ch.is_whitespace())
            .unwrap_or(inner.len());
        let name = inner[..name_end].to_owned();
        let mut attrs = Vec::new();
        let mut text = &inner[name_end..];
        while let Some(eq) = text.find('=') {
            let key = text[..eq].trim().to_owned();
            let after = text[eq + 1..].trim_start();
            let Some(open) = after.chars().next().filter(|ch| *ch == '"' || *ch == '\'') else {
                break;
            };
            let body = &after[1..];
            let close = body.find(open).unwrap_or(body.len());
            attrs.push((key, unescape(&body[..close])));
            text = body.get(close + 1..).unwrap_or("");
        }
        events.push(XmlEvent::Open { name, attrs, empty });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_reads_back_into_tags_and_text() {
        let xml = r#"<?xml version="1.0"?><!-- note --><w:p a="1 &amp; 2"><w:t xml:space='preserve'>a &lt;b&gt; &#x41;</w:t><w:br/></w:p>"#;
        let events = parse(xml);
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].attr("a"), Some("1 & 2"));
        assert_eq!(events[2], XmlEvent::Text("a <b> A".to_owned()));
        assert!(matches!(&events[4], XmlEvent::Open { name, empty: true, .. } if name == "w:br"));
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
        assert_eq!(unescape("&unknown; &amp"), "&unknown; &amp");
    }
}
