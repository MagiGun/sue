//! The design system's guard rails over the interface code: ratchets,
//! counts that may only go down.
//!
//! - **Literal styling, per area.** Colors, font sizes, corner radii,
//!   spacing, sizes, stroke widths and `gamma_multiply` written as numbers
//!   in interface code instead of read from the theme's tokens
//!   (`sue_wm::ui::tokens`). They are counted per area, an area being a
//!   folder of interface code together with the file it was cut from
//!   (`panels.rs` and `panels/`), so a pure move never changes a count.
//!   The six areas are those of the 0.0.8 plan, each held since the cuts
//!   of Phase 2 made its folder. A count that rises fails the test; a
//!   count that falls passes and asks for [`AREAS`] to be lowered, so that
//!   the ground won stays won.
//! - **Font characters used as icons**, per crate (decision D7 of the
//!   0.0.8 plan): arrows, geometric shapes, dingbats, emoji and the like
//!   written into the interface code's strings and characters. Icons are
//!   drawn by `sue_wm::icons`; these counts fall to zero as each surface
//!   moves to it, and none may come back.
//!
//! What counts is read from the code itself by a small lexer: comments,
//! test modules and what [`PICTURE`] names (functions that draw picture
//! content, or only the colors of overlays) are left out. The counts are
//! a floor, not an exact census: what they promise is that the same code
//! always gives the same numbers.

use std::fmt;
use std::path::{Path, PathBuf};

/// The repository's root folder.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bins/sue sits two levels below the repository")
        .to_path_buf()
}

// ----- Counts ----------------------------------------------------------------

/// Literal styling found in some code, one column per kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    /// `Color32::from_*` with a number in it, and named `Color32`
    /// constants other than `TRANSPARENT`.
    color: u32,
    /// Font sizes: `FontId::*(n)`, `.size(n)`, `.font(n)`, `.mono(n)`.
    font: u32,
    /// Corner radii of rectangles and radii of circles.
    radius: u32,
    /// `add_space(n)`, margins, item spacing.
    space: u32,
    /// `vec2(..)`, `Vec2::splat(n)`, widths and heights.
    size: u32,
    /// `Stroke::new(n, ..)`.
    stroke: u32,
    /// `gamma_multiply(n)` and `linear_multiply(n)`: colors faded by a
    /// number instead of a color role.
    gamma: u32,
}

impl Counts {
    /// A row of the table, in the order of the fields.
    const fn new(row: [u32; 7]) -> Self {
        let [color, font, radius, space, size, stroke, gamma] = row;
        Self {
            color,
            font,
            radius,
            space,
            size,
            stroke,
            gamma,
        }
    }

    /// The columns with their names.
    fn columns(self) -> [(&'static str, u32); 7] {
        [
            ("color", self.color),
            ("font", self.font),
            ("radius", self.radius),
            ("space", self.space),
            ("size", self.size),
            ("stroke", self.stroke),
            ("gamma", self.gamma),
        ]
    }

    fn bump(&mut self, column: Column) {
        let cell = match column {
            Column::Color => &mut self.color,
            Column::Font => &mut self.font,
            Column::Radius => &mut self.radius,
            Column::Space => &mut self.space,
            Column::Size => &mut self.size,
            Column::Stroke => &mut self.stroke,
            Column::Gamma => &mut self.gamma,
        };
        *cell += 1;
    }

    fn add(&mut self, other: Self) {
        self.color += other.color;
        self.font += other.font;
        self.radius += other.radius;
        self.space += other.space;
        self.size += other.size;
        self.stroke += other.stroke;
        self.gamma += other.gamma;
    }
}

impl fmt::Display for Counts {
    /// The row as it is written in [`AREAS`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let numbers: Vec<String> = self.columns().iter().map(|(_, n)| n.to_string()).collect();
        write!(f, "[{}]", numbers.join(", "))
    }
}

// ----- The tables ------------------------------------------------------------

/// One area of interface code under the styling ratchet.
struct Area {
    /// The folder, from the repository root, without its `/`. The file of
    /// the same name beside it (`<folder>.rs`) belongs to the area too.
    folder: &'static str,
    /// The most literal styling the area may hold, column by column as in
    /// [`Counts`]: color, font, radius, space, size, stroke, gamma.
    baseline: Counts,
}

/// The areas, with what each held when it was taken in. Lower a row when
/// its area is cleaned up; never raise one.
///
/// The rows were counted at 0.0.8's Phase 2 (`v008/p2-4`, 3b8bfa7) and
/// counted again on the merged Phase 2 line, after the cuts of P2-2 and
/// P2-3 had made every folder: the same numbers. Every area is held. For
/// comparison, `code-ui-polish.md` 2.1 counted these files at 823b060
/// with a coarser rule (six columns, `vec2` positions included).
const AREAS: [Area; 6] = [
    Area {
        folder: "crates/sue-paint/src/panels",
        baseline: Counts::new([10, 23, 34, 8, 92, 11, 10]),
    },
    Area {
        folder: "crates/sue-paint/src/canvas",
        baseline: Counts::new([3, 2, 35, 2, 33, 44, 3]),
    },
    Area {
        folder: "crates/sue-paint/src/dialogs",
        baseline: Counts::new([6, 2, 15, 17, 81, 0, 3]),
    },
    Area {
        folder: "crates/sue-paint/src/studio",
        baseline: Counts::new([0, 0, 0, 0, 2, 0, 0]),
    },
    Area {
        folder: "crates/sue-writer/src/ui",
        baseline: Counts::new([16, 13, 29, 29, 60, 27, 6]),
    },
    Area {
        folder: "crates/sue-wm/src/shell",
        baseline: Counts::new([0, 2, 9, 0, 17, 0, 2]),
    },
];

/// What a [`PICTURE`] entry leaves out of the count.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Exempt {
    /// Every column: the function draws the picture itself.
    All,
    /// The color and gamma columns only: the function draws interface
    /// over the picture (handles, guides, marks) whose colors must read
    /// on any picture, while its sizes, radii and strokes are interface
    /// like any other.
    Colors,
}

/// Functions that draw picture content rather than interface: the
/// document, thumbnails, color fields, and the colors of what is drawn
/// over the picture on the canvas. Each entry is an area's folder (its
/// file and every file under it, so a cut does not lose the entry), a
/// function name in it, and what of it is left out.
///
/// A function that is mostly interface stays counted, even if it also
/// draws a small picture (the Navigator panel with its view frame, the
/// brush preview with its ground, both named as chrome by
/// `code-ui-polish.md` 2.3).
const PICTURE: &[(&str, &str, Exempt)] = &[
    // The picture itself.
    ("crates/sue-paint/src/canvas", "draw_picture", Exempt::All),
    // What is drawn over it: page frames and marks, grids, selection,
    // handles and rulers. Their colors must read on any picture.
    ("crates/sue-paint/src/canvas", "page_frames", Exempt::Colors),
    ("crates/sue-paint/src/canvas", "overlays", Exempt::Colors),
    (
        "crates/sue-paint/src/canvas",
        "ruler_overlay",
        Exempt::Colors,
    ),
    // Color fields, and small copies of the picture.
    ("crates/sue-paint/src/panels", "color_wheel", Exempt::All),
    (
        "crates/sue-paint/src/panels",
        "layer_thumbnail",
        Exempt::All,
    ),
    // The miniature page of a template on Sue Writer's start page.
    ("crates/sue-writer/src/ui", "template_thumb", Exempt::All),
];

/// `true` when the file `name` (from the repository root) belongs to the
/// area whose folder is `folder`.
fn in_area(name: &str, folder: &str) -> bool {
    name.strip_prefix(folder)
        .is_some_and(|rest| rest == ".rs" || rest.starts_with('/'))
}

/// The crates whose code draws Sue's interface, for the guard against
/// font characters used as icons.
const INTERFACE: [&str; 5] = [
    "crates/sue-wm/src",
    "crates/sue-paint/src",
    "crates/sue-writer/src",
    "crates/sue-type/src",
    "bins/sue/src",
];

/// The most font characters each crate of [`INTERFACE`] may still use as
/// icons, in the same order. Lower a number as icons move to
/// `sue_wm::icons`; never raise one.
///
/// At 3b8bfa7 they are: the close crosses of the shell's windows and of
/// Settings, the anchor dots of Change Canvas Size, the emoji of the two
/// studios' manifests and Settings' gear, and in Sue Writer the label
/// dots, the outliner's tick and the find bar's arrows and cross.
const GLYPH_BASELINE: [u32; 5] = [1, 2, 6, 0, 1];

/// Characters that are text where they stand, not icons: a file or a
/// folder from the repository root, the character (`None` for every
/// one), and why.
const GLYPH_TEXT: &[(&str, Option<char>, &str)] = &[
    (
        "bins/sue/src/ui_lint.rs",
        None,
        "the scanner names the characters it looks for",
    ),
    (
        "crates/sue-type/src",
        Some('\u{25E6}'),
        "the bullet of a second level list in a typeset document",
    ),
    (
        "crates/sue-type/src",
        Some('\u{25AA}'),
        "the bullet of a third level list in a typeset document",
    ),
    (
        "bins/sue/src/selfcheck.rs",
        Some('\u{2192}'),
        "the arrows of the self-check's report on the console",
    ),
];

/// `true` when `c` in the file `name` is listed in [`GLYPH_TEXT`].
fn is_text_glyph(name: &str, c: char) -> bool {
    GLYPH_TEXT.iter().any(|(place, text, _)| {
        (name == *place || in_area(name, place)) && text.is_none_or(|text| text == c)
    })
}

/// `true` for a character that is a picture, not a letter: what a font
/// draws as an icon.
fn is_icon_glyph(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00D7 // multiplication sign, used as a close cross
            | 0x2190..=0x21FF // arrows
            | 0x2300..=0x23FF // miscellaneous technical
            | 0x2460..=0x24FF // enclosed alphanumerics
            | 0x2500..=0x25FF // box drawing, blocks, geometric shapes
            | 0x2600..=0x27BF // miscellaneous symbols, dingbats
            | 0x27F0..=0x27FF // supplemental arrows
            | 0x2900..=0x297F // supplemental arrows
            | 0x2B00..=0x2BFF // miscellaneous symbols and arrows
            | 0xE000..=0xF8FF // private use (icon fonts)
            | 0x1F000..=0x1FAFF // emoji and pictographs
    )
}

// ----- The lexer -------------------------------------------------------------

/// Source code made ready for counting.
struct Lexed {
    /// The code with comments blanked and every string and character
    /// literal's content blanked, so that no text is mistaken for code.
    /// It keeps the source's length and line breaks.
    code: Vec<u8>,
    /// The decoded content of every string and character literal, with
    /// the byte offset where it starts.
    literals: Vec<(usize, String)>,
}

/// Decodes the escapes of a (non-raw) literal's content.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') if chars.peek() == Some(&'{') => {
                chars.next();
                let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
                if let Some(c) = u32::from_str_radix(&hex.replace('_', ""), 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    out.push(c);
                }
            }
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Splits `source` into code and literals.
fn lex(source: &str) -> Lexed {
    let bytes = source.as_bytes();
    let mut code = bytes.to_vec();
    let mut literals = Vec::new();
    let blank = |code: &mut Vec<u8>, from: usize, to: usize| {
        for byte in &mut code[from..to] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    };
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        if rest.starts_with(b"//") {
            let end = rest
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |n| i + n);
            blank(&mut code, i, end);
            i = end;
        } else if rest.starts_with(b"/*") {
            // Block comments nest in Rust.
            let mut depth = 0;
            let mut j = i;
            while j < bytes.len() {
                if bytes[j..].starts_with(b"/*") {
                    depth += 1;
                    j += 2;
                } else if bytes[j..].starts_with(b"*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            blank(&mut code, i, j);
            i = j;
        } else if (rest.starts_with(b"r#") || rest.starts_with(b"r\"") || rest.starts_with(b"br"))
            && (i == 0 || !ident(bytes[i - 1]))
            && raw_string(rest).is_some()
        {
            let (start, end) = raw_string(rest).expect("checked above");
            literals.push((i, source[i + start..i + end].to_owned()));
            blank(&mut code, i + start, i + end);
            i += end;
        } else if rest[0] == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            let end = j.min(bytes.len());
            literals.push((i, unescape(&source[i + 1..end])));
            blank(&mut code, i + 1, end);
            i = end + 1;
        } else if rest[0] == b'\'' {
            match char_literal(rest) {
                Some(len) => {
                    literals.push((i, unescape(&source[i + 1..i + len - 1])));
                    blank(&mut code, i + 1, i + len - 1);
                    i += len;
                }
                // A lifetime or a label.
                None => i += 1,
            }
        } else {
            // Step over a whole character, so that `i` stays on a boundary.
            i += source[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    Lexed { code, literals }
}

/// The content's start and the literal's end, from the start of a raw
/// string (`r"..."`, `r#"..."#`, `br"..."`).
fn raw_string(rest: &[u8]) -> Option<(usize, usize)> {
    let mut j = usize::from(rest.starts_with(b"b")) + 1;
    let hashes = rest[j..].iter().take_while(|b| **b == b'#').count();
    j += hashes;
    if rest.get(j) != Some(&b'"') {
        return None;
    }
    let start = j + 1;
    let mut close = vec![b'"'];
    close.extend(std::iter::repeat_n(b'#', hashes));
    let end = rest[start..]
        .windows(close.len())
        .position(|window| window == close.as_slice())
        .map_or(rest.len(), |n| start + n);
    Some((start, (end + close.len()).min(rest.len())))
}

/// The length of the character literal `rest` starts with, or `None` for
/// a lifetime.
fn char_literal(rest: &[u8]) -> Option<usize> {
    if rest.get(1) == Some(&b'\\') {
        // The escaped character may itself be a quote: `'\''`.
        let close = rest.iter().skip(3).position(|b| *b == b'\'')?;
        return Some(close + 4);
    }
    let text = std::str::from_utf8(&rest[1..rest.len().min(6)])
        .or_else(|error| std::str::from_utf8(&rest[1..1 + error.valid_up_to()]))
        .ok()?;
    let c = text.chars().next()?;
    (rest.get(1 + c.len_utf8()) == Some(&b'\'')).then_some(c.len_utf8() + 2)
}

/// The end of the bracketed group that opens at `open` (one past its
/// closing bracket), in lexed code.
fn group_end(code: &[u8], open: usize) -> usize {
    let mut depth = 0_i32;
    for (j, byte) in code.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// Where every function called `name` in lexed `code` starts, and the end
/// of its body.
fn function_spans(code: &[u8], name: &str) -> Vec<(usize, usize)> {
    let text = String::from_utf8_lossy(code);
    let pattern = format!("fn {name}");
    let mut spans = Vec::new();
    for (at, _) in text.match_indices(&pattern) {
        let next = code.get(at + pattern.len()).copied().unwrap_or(b' ');
        if next.is_ascii_alphanumeric() || next == b'_' {
            continue;
        }
        if let Some(brace) = code[at..].iter().position(|b| *b == b'{') {
            spans.push((at, group_end(code, at + brace)));
        }
    }
    spans
}

/// Blanks every item marked `#[cfg(test)]` (a test module or function)
/// and the body of every function of `picture`.
fn drop_unseen(code: &mut [u8], picture: &[&str]) {
    let text = String::from_utf8_lossy(code).into_owned();
    let mut spans = Vec::new();
    for (at, _) in text.match_indices("#[cfg(test)]") {
        let after = &code[at..];
        let semi = after.iter().position(|b| *b == b';');
        let brace = after.iter().position(|b| *b == b'{');
        let end = match (brace, semi) {
            (Some(brace), Some(semi)) if semi < brace => at + semi + 1,
            (Some(brace), _) => group_end(code, at + brace),
            (None, Some(semi)) => at + semi + 1,
            (None, None) => code.len(),
        };
        spans.push((at, end));
    }
    for name in picture {
        spans.extend(function_spans(code, name));
    }
    for (from, to) in spans {
        for byte in &mut code[from..to] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
}

// ----- Counting --------------------------------------------------------------

/// `true` when `token` is a number written out: `4`, `4.0`, `0.5_f32`.
fn is_number(token: &str) -> bool {
    let digits = token.trim_start_matches('-');
    for prefix in ["0x", "0o", "0b"] {
        if let Some(rest) = digits.strip_prefix(prefix) {
            return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        }
    }
    let body = [
        "f32", "f64", "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "usize", "isize",
    ]
    .iter()
    .find_map(|suffix| digits.strip_suffix(suffix))
    .map_or(digits, |body| body.trim_end_matches('_'));
    body.starts_with(|c: char| c.is_ascii_digit())
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '_' || c == 'e' || c == 'E')
}

/// `true` when `expression` has a number at its own level, outside any
/// call or index inside it: `8.0`, `0.45 * appear`, `rect.width() - 8.0`.
fn has_number(expression: &str) -> bool {
    let mut depth = 0_i32;
    let mut token = String::new();
    // The token after a `.` that follows a name is a field (`x.0`).
    let mut field = false;
    let mut found = false;
    let mut flush = |token: &mut String, field: &mut bool, depth: i32| {
        if token.is_empty() {
            return;
        }
        if depth == 0 && !*field && is_number(token) {
            found = true;
        }
        *field = false;
        token.clear();
    };
    for c in expression.chars() {
        match c {
            '(' | '[' | '{' => {
                flush(&mut token, &mut field, depth);
                depth += 1;
            }
            ')' | ']' | '}' => {
                flush(&mut token, &mut field, depth);
                depth -= 1;
            }
            '.' if !token.is_empty() && token.chars().all(|c| c.is_ascii_digit()) => {
                token.push(c);
            }
            '.' => {
                let named = !token.is_empty();
                flush(&mut token, &mut field, depth);
                field = named || field;
            }
            c if c.is_ascii_alphanumeric() || c == '_' => token.push(c),
            _ => flush(&mut token, &mut field, depth),
        }
    }
    flush(&mut token, &mut field, depth);
    found
}

/// The arguments of the call whose `(` is at `open`, split at the commas
/// of its own level.
fn arguments(code: &[u8], open: usize) -> Vec<String> {
    let end = group_end(code, open);
    let inner = String::from_utf8_lossy(&code[open + 1..end.saturating_sub(1)]).into_owned();
    let mut out = Vec::new();
    let mut depth = 0_i32;
    let mut current = String::new();
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// Every call of `name` in `code` (`name` not preceded by a letter, a
/// digit or `_`, then `(`), as the offset of its `(`.
fn calls<'a>(code: &'a [u8], text: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(name).filter_map(move |(at, _)| {
        let before = at.checked_sub(1).map(|i| code[i]);
        if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        let open = at + name.len();
        (code.get(open) == Some(&b'(')).then_some(open)
    })
}

/// A column of [`Counts`].
#[derive(Clone, Copy)]
enum Column {
    Color,
    Font,
    Radius,
    Space,
    Size,
    Stroke,
    Gamma,
}

/// A call that is counted: its name (a method when it starts with `.`),
/// the argument that must hold a number (`None`: any of them), and the
/// column it counts in.
type Rule = (&'static str, Option<usize>, Column);

/// The literal styling in lexed `code`.
fn count(code: &[u8]) -> Counts {
    let text = String::from_utf8_lossy(code).into_owned();
    let mut counts = Counts::default();
    // Calls counted when the argument at `index` has a number in it, or
    // with `None`, when any argument has.
    use Column::{Color, Font, Gamma, Radius, Size, Space, Stroke};
    let rules: [Rule; 37] = [
        ("Color32::from_rgb", None, Color),
        ("Color32::from_rgba_unmultiplied", None, Color),
        ("Color32::from_rgba_premultiplied", None, Color),
        ("Color32::from_gray", None, Color),
        ("Color32::from_black_alpha", None, Color),
        ("Color32::from_white_alpha", None, Color),
        ("Color32::from_additive_luminance", None, Color),
        ("FontId::proportional", Some(0), Font),
        ("FontId::monospace", Some(0), Font),
        ("FontId::new", Some(0), Font),
        (".size", Some(0), Font),
        (".font", Some(0), Font),
        (".mono", Some(0), Font),
        ("CornerRadius::same", Some(0), Radius),
        (".corner_radius", Some(0), Radius),
        (".rect_filled", Some(1), Radius),
        (".rect_stroke", Some(1), Radius),
        (".rect", Some(1), Radius),
        ("RectShape::filled", Some(1), Radius),
        ("RectShape::stroke", Some(1), Radius),
        ("RectShape::new", Some(1), Radius),
        (".circle_filled", Some(1), Radius),
        (".circle_stroke", Some(1), Radius),
        (".add_space", Some(0), Space),
        ("Margin::same", Some(0), Space),
        ("Margin::symmetric", None, Space),
        (".inner_margin", Some(0), Space),
        (".outer_margin", Some(0), Space),
        ("vec2", None, Size),
        ("Vec2::splat", Some(0), Size),
        (".desired_width", Some(0), Size),
        (".min_width", Some(0), Size),
        (".max_width", Some(0), Size),
        (".exact_width", Some(0), Size),
        ("Stroke::new", Some(0), Stroke),
        (".gamma_multiply", Some(0), Gamma),
        (".linear_multiply", Some(0), Gamma),
    ];
    for (name, index, column) in rules {
        // `.size(` and the like are methods: look for the name after the dot.
        let (bare, method) = name
            .strip_prefix('.')
            .map_or((name, false), |bare| (bare, true));
        for open in calls(code, &text, bare) {
            let start = open - bare.len();
            if method && start.checked_sub(1).map(|i| code[i]) != Some(b'.') {
                continue;
            }
            let arguments = arguments(code, open);
            let literal = match index {
                Some(index) => arguments.get(index).is_some_and(|a| has_number(a)),
                None => arguments.iter().any(|a| has_number(a)),
            };
            if literal {
                counts.bump(column);
            }
        }
    }
    // Named colors, and item spacing set to numbers.
    for (at, _) in text.match_indices("Color32::") {
        let name: String = text[at + 9..]
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || *c == '_')
            .collect();
        let after = text[at + 9 + name.len()..].chars().next();
        let constant = !name.is_empty()
            && after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
            && name != "TRANSPARENT";
        if constant {
            counts.color += 1;
        }
    }
    for (at, _) in text.match_indices("item_spacing") {
        let rest = text[at + 12..]
            .trim_start_matches(['.', 'x', 'y'])
            .trim_start();
        if let Some(value) = rest.strip_prefix('=').filter(|r| !r.starts_with('=')) {
            let value = value.split(';').next().unwrap_or_default();
            if has_number(value) {
                counts.space += 1;
            }
        }
    }
    counts
}

/// The Rust files under `dir`, left out test modules kept in files of
/// their own (`tests.rs`, `tests/`).
fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        if name == "tests" || name == "tests.rs" {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// The path of `path` from the repository root, with `/`.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The files of an area: its folder, and the file it was cut from.
fn area_files(root: &Path, area: &Area) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let file = root.join(format!("{}.rs", area.folder));
    if file.is_file() {
        files.push(file);
    }
    rust_files(&root.join(area.folder), &mut files);
    files.sort();
    files
}

/// Lexes `path` and blanks what is not counted in it.
fn read_lexed(root: &Path, path: &Path) -> Lexed {
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut lexed = lex(&source);
    let name = relative(root, path);
    drop_unseen(&mut lexed.code, &picture(&name, Exempt::All));
    lexed
}

/// The functions of [`PICTURE`] in the file `name` that leave out `what`.
fn picture(name: &str, what: Exempt) -> Vec<&'static str> {
    PICTURE
        .iter()
        .filter(|(folder, _, exempt)| *exempt == what && in_area(name, folder))
        .map(|(_, function, _)| *function)
        .collect()
}

/// The literal styling in lexed `code`, less the colors of the functions
/// `colors_only` names.
fn count_file(code: &[u8], colors_only: &[&str]) -> Counts {
    let mut counts = count(code);
    for function in colors_only {
        for (from, to) in function_spans(code, function) {
            let inside = count(&code[from..to]);
            counts.color = counts.color.saturating_sub(inside.color);
            counts.gamma = counts.gamma.saturating_sub(inside.gamma);
        }
    }
    counts
}

/// The literal styling an area holds now.
fn area_counts(root: &Path, area: &Area) -> Counts {
    let mut total = Counts::default();
    for path in area_files(root, area) {
        let lexed = read_lexed(root, &path);
        let colors_only = picture(&relative(root, &path), Exempt::Colors);
        total.add(count_file(&lexed.code, &colors_only));
    }
    total
}

/// Font characters used as icons in the code under `dir`, with where.
fn glyphs(root: &Path, dir: &str) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(&root.join(dir), &mut files);
    files.sort();
    let mut found = Vec::new();
    for path in files {
        let name = relative(root, &path);
        let lexed = read_lexed(root, &path);
        for (at, literal) in &lexed.literals {
            // A literal inside a blanked test module is not counted.
            if lexed.code.get(*at) == Some(&b' ') {
                continue;
            }
            let line = lexed.code[..*at].iter().filter(|b| **b == b'\n').count() + 1;
            for c in literal
                .chars()
                .filter(|c| is_icon_glyph(*c) && !is_text_glyph(&name, *c))
            {
                found.push(format!("{name}:{line}: U+{:04X} {c}", u32::from(c)));
            }
        }
    }
    found
}

// ----- The tests -------------------------------------------------------------

#[test]
fn literal_styling_never_grows() {
    let root = repository();
    let mut report = Vec::new();
    let mut grown = Vec::new();
    for area in &AREAS {
        let now = area_counts(&root, area);
        report.push(format!("{}: {now} (table {})", area.folder, area.baseline));
        for ((column, was), (_, is)) in area.baseline.columns().into_iter().zip(now.columns()) {
            if is > was {
                grown.push(format!(
                    "{}: {column} literals rose from {was} to {is}",
                    area.folder
                ));
            } else if is < was {
                println!(
                    "{}: {column} literals fell from {was} to {is}; lower its row in AREAS",
                    area.folder
                );
            }
        }
    }
    let report = report.join("\n");
    println!("{report}");
    assert!(
        grown.is_empty(),
        "Literal styling grew in interface code. Read the value from the \
         theme's tokens (sue_wm::ui::tokens) instead, or, for picture \
         content, add the function to PICTURE in bins/sue/src/ui_lint.rs.\n{}\n\nNow:\n{report}",
        grown.join("\n")
    );
}

#[test]
fn every_area_of_the_plan_is_counted_and_none_was_lost() {
    let root = repository();
    for area in &AREAS {
        let file = root.join(format!("{}.rs", area.folder));
        assert!(
            file.is_file() || root.join(area.folder).is_dir(),
            "{} is gone: move its row in AREAS to where its code went, \
             or the ratchet stops counting it",
            area.folder
        );
        assert!(
            !area_files(&root, area).is_empty(),
            "{} has no code",
            area.folder
        );
    }
}

#[test]
fn every_picture_exception_names_a_function_that_exists() {
    let root = repository();
    for (folder, function, _) in PICTURE {
        let area = Area {
            folder,
            baseline: Counts::default(),
        };
        let pattern = format!("fn {function}");
        let found = area_files(&root, &area).iter().any(|path| {
            let source = std::fs::read_to_string(path).unwrap_or_default();
            source.match_indices(&pattern).any(|(at, _)| {
                source[at + pattern.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '_')
            })
        });
        assert!(
            found,
            "PICTURE names fn {function} in {folder}, which has no such function"
        );
    }
}

#[test]
fn font_characters_are_not_coming_back_as_icons() {
    let root = repository();
    let mut grown = Vec::new();
    let mut report = Vec::new();
    for (dir, allowed) in INTERFACE.into_iter().zip(GLYPH_BASELINE) {
        let found = glyphs(&root, dir);
        let count = u32::try_from(found.len()).expect("fewer than 4 billion");
        report.push(format!("{dir}: {count} (table {allowed})"));
        if count > allowed {
            grown.push(format!(
                "{dir}: {count}, more than {allowed}:\n{}",
                found.join("\n")
            ));
        }
    }
    let report = report.join("\n");
    println!("{report}");
    assert!(
        grown.is_empty(),
        "Font characters are used as icons. Draw the icon with \
         sue_wm::icons (decision D7) instead.\n{}\n\nNow:\n{report}",
        grown.join("\n\n")
    );
}

/// Interface text lives in the language files, so an icon written as a
/// character could come in through them as well as through the code.
/// None holds one, in any language.
#[test]
fn language_files_hold_no_font_characters_as_icons() {
    fn ftl_files(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                ftl_files(&path, found);
            } else if path.extension().is_some_and(|e| e == "ftl") {
                found.push(path);
            }
        }
    }
    let root = repository();
    let mut files = Vec::new();
    ftl_files(&root.join("crates/sue-lang/locales"), &mut files);
    files.sort();
    assert!(files.len() > 10, "the language files are where they were");
    let mut found = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("a language file reads");
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            for c in line.chars().filter(|c| is_icon_glyph(*c)) {
                found.push(format!(
                    "{}:{}: U+{:04X} {c}",
                    relative(&root, &path),
                    index + 1,
                    u32::from(c)
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "Font characters are used as icons in the language files. Draw \
         the icon with sue_wm::icons (decision D7) instead.\n{}",
        found.join("\n")
    );
}

mod lexer {
    use super::*;

    fn counted(source: &str) -> Counts {
        let mut lexed = lex(source);
        drop_unseen(&mut lexed.code, &["checkerboard"]);
        count(&lexed.code)
    }

    #[test]
    fn numbers_are_told_from_names() {
        for yes in [
            "4",
            "4.0",
            "0.5_f32",
            "12u8",
            "-1.0",
            "1e3",
            "0.45 * appear",
        ] {
            assert!(has_number(yes), "{yes}");
        }
        for no in [
            "theme.radius",
            "x.0",
            "size",
            "f32::MAX",
            "a.max(4.0)",
            "rect.width()",
        ] {
            assert!(!has_number(no), "{no}");
        }
        assert!(has_number("rect.width() - 8.0"));
    }

    #[test]
    fn each_kind_of_literal_lands_in_its_column() {
        let source = r#"
            fn ui(ui: &mut Ui, painter: &Painter, theme: &Theme) {
                let a = Color32::from_rgb(200, 40, 40);
                let b = Color32::WHITE;
                let c = Color32::TRANSPARENT;
                let d = Color32::from_gray(theme.level);
                let e = RichText::new("x").size(13.0);
                let f = theme.font(12.0);
                painter.rect_filled(rect, 4.0, theme.panel);
                painter.rect_filled(rect, theme.control_radius, theme.panel);
                painter.circle_filled(center, 3.0, theme.text);
                ui.add_space(8.0);
                ui.add_space(theme.padding);
                let g = egui::vec2(20.0, theme.row);
                let h = Vec2::splat(theme.icon);
                let i = Stroke::new(1.5, theme.border);
                let j = theme.text.gamma_multiply(0.45 * appear);
                ui.spacing_mut().item_spacing.x = 2.0;
            }
        "#;
        assert_eq!(counted(source), Counts::new([2, 2, 2, 2, 1, 1, 1]));
    }

    #[test]
    fn comments_strings_tests_and_pictures_are_not_counted() {
        let source = r#"
            // painter.rect_filled(rect, 4.0, theme.panel);
            /* ui.add_space(8.0); /* nested */ vec2(1.0, 2.0) */
            fn label() -> &'static str { "ui.add_space(8.0)" }
            fn checkerboard(painter: &Painter) { painter.rect_filled(r, 0.0, Color32::WHITE); }
            #[cfg(test)]
            mod tests { fn t() { let v = vec2(3.0, 4.0); } }
            fn kept<'a>(x: &'a str) { let c = 'x'; ui.add_space(2.0); }
        "#;
        assert_eq!(counted(source), Counts::new([0, 0, 0, 1, 0, 0, 0]));
    }

    #[test]
    fn an_overlay_leaves_out_its_colors_only() {
        let source = r"
            fn overlays(painter: &Painter) {
                let violet = Color32::from_rgb(170, 120, 255);
                painter.circle_stroke(at, 4.0, Stroke::new(1.5, violet.gamma_multiply(0.4)));
            }
            fn panel(ui: &mut Ui) { let red = Color32::from_rgb(220, 40, 40); }
        ";
        let lexed = lex(source);
        assert_eq!(
            count_file(&lexed.code, &["overlays"]),
            Counts::new([1, 0, 1, 0, 0, 1, 0]),
            "the overlay's sizes and strokes count, its colors do not"
        );
        assert_eq!(
            count_file(&lexed.code, &[]),
            Counts::new([2, 0, 1, 0, 0, 1, 1])
        );
    }

    #[test]
    fn an_area_is_its_file_and_its_folder() {
        let folder = "crates/sue-paint/src/panels";
        assert!(in_area("crates/sue-paint/src/panels.rs", folder));
        assert!(in_area("crates/sue-paint/src/panels/layers.rs", folder));
        assert!(in_area("crates/sue-paint/src/panels/tool/brush.rs", folder));
        assert!(!in_area("crates/sue-paint/src/panels_old.rs", folder));
        assert!(!in_area("crates/sue-paint/src/panel.rs", folder));
    }

    #[test]
    fn a_character_literal_may_hold_a_quote() {
        let lexed = lex(r"let q = '\''; let b = '\\'; ui.add_space(4.0);");
        let texts: Vec<&str> = lexed.literals.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(texts, ["'", "\\"]);
        assert_eq!(count(&lexed.code).space, 1);
    }

    #[test]
    fn literals_are_decoded_for_the_glyph_guard() {
        let lexed = lex(r##"let a = "\u{2699} Settings"; let b = '\u{25B6}'; let r = r#"→"#;"##);
        let text: String = lexed.literals.iter().map(|(_, s)| s.as_str()).collect();
        let glyphs: Vec<char> = text.chars().filter(|c| is_icon_glyph(*c)).collect();
        assert_eq!(glyphs, ['\u{2699}', '\u{25B6}', '\u{2192}']);
        assert!(!is_icon_glyph('A') && !is_icon_glyph('\u{2026}') && !is_icon_glyph('\u{00B0}'));
    }
}
