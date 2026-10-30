//! Reading the text of a PDF back into rich text: the pages in order,
//! their lines joined into paragraphs, and big lines as headings. Fonts
//! with a ToUnicode map (every modern writer's, Sue's own included) and
//! the standard single-byte encodings are read; pictures and layout are
//! left behind, as a word processor that opens a PDF for editing does
//! with what it cannot keep.
//!
//! Written from the PDF 1.7 reference (ISO 32000-1): objects, compressed
//! and object streams, cross-reference streams, the page tree, content
//! stream text operators and CMaps.

use std::collections::HashMap;

use sue_core::Value;
use sue_model::{TextMarks, mark_kind as kind};

/// A PDF value, as far as reading text needs.
#[derive(Clone, Debug, PartialEq)]
enum Obj {
    Null,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    Name(String),
    Array(Vec<Obj>),
    Dict(HashMap<String, Obj>),
    Ref(u32),
    /// A content stream operator.
    Op(String),
}

impl Obj {
    fn get(&self, key: &str) -> Option<&Obj> {
        match self {
            Self::Dict(dict) => dict.get(key),
            _ => None,
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name),
            _ => None,
        }
    }
}

/// Reads PDF syntax from bytes.
struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

fn is_white(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | b'\x0c' | b'\0')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

impl<'a> Lexer<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn skip(&mut self) {
        while let Some(&byte) = self.bytes.get(self.at) {
            if is_white(byte) {
                self.at += 1;
            } else if byte == b'%' {
                while self
                    .bytes
                    .get(self.at)
                    .is_some_and(|b| *b != b'\n' && *b != b'\r')
                {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    fn word(&mut self) -> &'a [u8] {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| !is_white(*b) && !is_delimiter(*b))
        {
            self.at += 1;
        }
        &self.bytes[start..self.at]
    }

    /// The next value (a reference `n g R` folds into one), `None` at the
    /// end or at a closing bracket.
    fn value(&mut self) -> Option<Obj> {
        self.skip();
        let byte = *self.bytes.get(self.at)?;
        match byte {
            b'/' => {
                self.at += 1;
                let word = self.word();
                let mut name = Vec::with_capacity(word.len());
                let mut i = 0;
                while i < word.len() {
                    // `#20`: a byte written in hex.
                    if word[i] == b'#'
                        && let Some(hex) = word
                            .get(i + 1..i + 3)
                            .and_then(|h| std::str::from_utf8(h).ok())
                            .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        name.push(hex);
                        i += 3;
                        continue;
                    }
                    name.push(word[i]);
                    i += 1;
                }
                Some(Obj::Name(String::from_utf8_lossy(&name).into_owned()))
            }
            b'(' => Some(Obj::Str(self.literal())),
            b'<' if self.bytes.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                let mut dict = HashMap::new();
                loop {
                    self.skip();
                    if self.bytes.get(self.at..self.at + 2) == Some(b">>") {
                        self.at += 2;
                        break;
                    }
                    match self.value()? {
                        Obj::Name(key) => {
                            let value = self.value().unwrap_or(Obj::Null);
                            dict.insert(key, value);
                        }
                        _ => return Some(Obj::Dict(dict)),
                    }
                }
                Some(Obj::Dict(dict))
            }
            b'<' => {
                self.at += 1;
                let mut digits = Vec::new();
                while let Some(&b) = self.bytes.get(self.at) {
                    self.at += 1;
                    if b == b'>' {
                        break;
                    }
                    if b.is_ascii_hexdigit() {
                        digits.push(b);
                    }
                }
                if digits.len() % 2 == 1 {
                    digits.push(b'0');
                }
                Some(Obj::Str(
                    digits
                        .chunks(2)
                        .filter_map(|pair| {
                            std::str::from_utf8(pair)
                                .ok()
                                .and_then(|h| u8::from_str_radix(h, 16).ok())
                        })
                        .collect(),
                ))
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip();
                    match self.bytes.get(self.at) {
                        Some(b']') => {
                            self.at += 1;
                            break;
                        }
                        None => break,
                        _ => match self.value() {
                            Some(item) => items.push(item),
                            None => break,
                        },
                    }
                }
                Some(Obj::Array(items))
            }
            b']' | b'>' | b')' | b'{' | b'}' => {
                self.at += 1;
                None
            }
            _ => {
                let word = self.word();
                if word.is_empty() {
                    self.at += 1;
                    return Some(Obj::Null);
                }
                let text = String::from_utf8_lossy(word);
                if let Ok(number) = text.parse::<f64>() {
                    // `n g R`: a reference.
                    let back = self.at;
                    if number.fract() == 0.0 && number >= 0.0 {
                        self.skip();
                        let generation = self.word();
                        if !generation.is_empty() && generation.iter().all(u8::is_ascii_digit) {
                            self.skip();
                            if self.word() == b"R" {
                                return Some(Obj::Ref(number as u32));
                            }
                        }
                    }
                    self.at = back;
                    return Some(Obj::Num(number));
                }
                Some(match word {
                    b"true" => Obj::Bool(true),
                    b"false" => Obj::Bool(false),
                    b"null" => Obj::Null,
                    _ => Obj::Op(text.into_owned()),
                })
            }
        }
    }

    fn literal(&mut self) -> Vec<u8> {
        self.at += 1;
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(&b) = self.bytes.get(self.at) {
            self.at += 1;
            match b {
                b'\\' => {
                    let Some(&next) = self.bytes.get(self.at) else {
                        break;
                    };
                    self.at += 1;
                    match next {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if self.bytes.get(self.at) == Some(&b'\n') {
                                self.at += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut value = u32::from(next - b'0');
                            for _ in 0..2 {
                                match self.bytes.get(self.at) {
                                    Some(&d @ b'0'..=b'7') => {
                                        value = value * 8 + u32::from(d - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(value as u8);
                        }
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                _ => out.push(b),
            }
        }
        out
    }
}

/// A PDF's objects, by number, and their streams.
#[derive(Default)]
struct File {
    objects: HashMap<u32, Obj>,
    streams: HashMap<u32, Vec<u8>>,
    trailer: HashMap<String, Obj>,
}

/// The decoded data of a stream with dictionary `dict`.
fn decode(dict: &Obj, raw: &[u8]) -> Option<Vec<u8>> {
    let filters: Vec<String> = match dict.get("Filter") {
        Some(Obj::Name(name)) => vec![name.clone()],
        Some(Obj::Array(items)) => items
            .iter()
            .filter_map(|item| item.name().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    };
    let mut data = raw.to_vec();
    for filter in filters {
        data = match filter.as_str() {
            "FlateDecode" | "Fl" => {
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, 1 << 28)
                    .or_else(|_| miniz_oxide::inflate::decompress_to_vec_with_limit(&data, 1 << 28))
                    .ok()?
            }
            _ => return None,
        };
    }
    // PNG predictors on cross-reference streams.
    if let Some(parms) = dict.get("DecodeParms")
        && parms.get("Predictor").and_then(Obj::num).unwrap_or(1.0) >= 10.0
    {
        let columns = parms.get("Columns").and_then(Obj::num).unwrap_or(1.0) as usize;
        let mut out = Vec::new();
        let mut previous = vec![0_u8; columns];
        for row in data.chunks(columns + 1) {
            let (kind, row) = row.split_first()?;
            let mut line = row.to_vec();
            line.resize(columns, 0);
            for i in 0..columns {
                let left = if i > 0 { line[i - 1] } else { 0 };
                let up = previous[i];
                line[i] = match kind {
                    1 => line[i].wrapping_add(left),
                    2 => line[i].wrapping_add(up),
                    3 => line[i].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                    4 => {
                        let corner = if i > 0 { previous[i - 1] } else { 0 };
                        let p = i16::from(left) + i16::from(up) - i16::from(corner);
                        let (a, b, c) = (
                            (p - i16::from(left)).abs(),
                            (p - i16::from(up)).abs(),
                            (p - i16::from(corner)).abs(),
                        );
                        let pick = if a <= b && a <= c {
                            left
                        } else if b <= c {
                            up
                        } else {
                            corner
                        };
                        line[i].wrapping_add(pick)
                    }
                    _ => line[i],
                };
            }
            out.extend_from_slice(&line);
            previous = line;
        }
        data = out;
    }
    Some(data)
}

impl File {
    fn read(bytes: &[u8]) -> Self {
        let mut file = Self::default();
        // Every `n g obj` in the file, in order (later ones are updates).
        let mut at = 0;
        while let Some(found) = find(bytes, b" obj", at) {
            at = found + 4;
            let Some(number) = object_number(bytes, found) else {
                continue;
            };
            let mut lexer = Lexer::new(bytes, at);
            let Some(value) = lexer.value() else {
                continue;
            };
            lexer.skip();
            if bytes[lexer.at..].starts_with(b"stream") {
                let mut data_at = lexer.at + 6;
                if bytes.get(data_at) == Some(&b'\r') {
                    data_at += 1;
                }
                if bytes.get(data_at) == Some(&b'\n') {
                    data_at += 1;
                }
                let length = value.get("Length").and_then(Obj::num).map(|n| n as usize);
                let end = match length {
                    Some(length)
                        if bytes
                            .get(data_at + length..)
                            .is_some_and(|rest| starts_after_white(rest, b"endstream")) =>
                    {
                        data_at + length
                    }
                    _ => find(bytes, b"endstream", data_at).unwrap_or(bytes.len()),
                };
                let raw = &bytes[data_at.min(end)..end];
                if let Some(data) = decode(&value, raw) {
                    file.streams.insert(number, data);
                }
                at = end;
            }
            if value.get("Type").and_then(Obj::name) == Some("XRef")
                && let Obj::Dict(dict) = &value
            {
                for (key, item) in dict {
                    file.trailer.entry(key.clone()).or_insert(item.clone());
                }
            }
            file.objects.insert(number, value);
        }
        // The classic trailer.
        let mut at = 0;
        while let Some(found) = find(bytes, b"trailer", at) {
            at = found + 7;
            if let Some(Obj::Dict(dict)) = Lexer::new(bytes, at).value() {
                for (key, item) in dict {
                    file.trailer.insert(key, item);
                }
            }
        }
        // Objects packed in object streams.
        let packed: Vec<u32> = file
            .objects
            .iter()
            .filter(|(_, value)| value.get("Type").and_then(Obj::name) == Some("ObjStm"))
            .map(|(number, _)| *number)
            .collect();
        for number in packed {
            let (Some(dict), Some(data)) = (file.objects.get(&number), file.streams.get(&number))
            else {
                continue;
            };
            let count = dict.get("N").and_then(Obj::num).unwrap_or(0.0) as usize;
            let first = dict.get("First").and_then(Obj::num).unwrap_or(0.0) as usize;
            let mut header = Lexer::new(data, 0);
            let mut entries = Vec::new();
            for _ in 0..count {
                let (Some(Obj::Num(id)), Some(Obj::Num(offset))) = (header.value(), header.value())
                else {
                    break;
                };
                entries.push((id as u32, first + offset as usize));
            }
            let data = data.clone();
            for (id, offset) in entries {
                if let Some(value) = Lexer::new(&data, offset).value() {
                    file.objects.entry(id).or_insert(value);
                }
            }
        }
        file
    }

    fn resolve<'a>(&'a self, value: &'a Obj) -> &'a Obj {
        let mut value = value;
        for _ in 0..8 {
            match value {
                Obj::Ref(number) => match self.objects.get(number) {
                    Some(found) => value = found,
                    None => return &Obj::Null,
                },
                _ => return value,
            }
        }
        value
    }

    fn get<'a>(&'a self, value: &'a Obj, key: &str) -> Option<&'a Obj> {
        self.resolve(value)
            .get(key)
            .map(|found| self.resolve(found))
    }

    /// The pages, in order: each page's dictionary and its object number.
    fn pages(&self) -> Vec<u32> {
        fn walk(file: &File, node: u32, out: &mut Vec<u32>, depth: usize) {
            if depth > 64 || out.contains(&node) {
                return;
            }
            let Some(value) = file.objects.get(&node) else {
                return;
            };
            match value.get("Type").and_then(Obj::name) {
                Some("Pages") => {
                    if let Some(Obj::Array(kids)) = file.get(value, "Kids") {
                        for kid in kids {
                            if let Obj::Ref(number) = kid {
                                walk(file, *number, out, depth + 1);
                            }
                        }
                    }
                }
                _ => out.push(node),
            }
        }
        let mut out = Vec::new();
        let root = self.trailer.get("Root").cloned().unwrap_or(Obj::Null);
        if let Some(Obj::Ref(pages)) = self.resolve(&root).get("Pages") {
            walk(self, *pages, &mut out, 0);
        }
        if out.is_empty() {
            let mut all: Vec<u32> = self
                .objects
                .iter()
                .filter(|(_, value)| value.get("Type").and_then(Obj::name) == Some("Page"))
                .map(|(number, _)| *number)
                .collect();
            all.sort_unstable();
            out = all;
        }
        out
    }

    /// A page's value `key`, looked up the page tree when it inherits.
    fn inherited(&self, page: u32, key: &str) -> Option<&Obj> {
        let mut node = self.objects.get(&page)?;
        for _ in 0..32 {
            if let Some(found) = node.get(key) {
                return Some(self.resolve(found));
            }
            node = self.resolve(node.get("Parent")?);
        }
        None
    }

    /// The bytes of a page's content streams.
    fn contents(&self, page: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let refs: Vec<u32> = match self.objects.get(&page).and_then(|p| p.get("Contents")) {
            Some(Obj::Ref(number)) => match self.objects.get(number) {
                Some(Obj::Array(items)) => items
                    .iter()
                    .filter_map(|item| match item {
                        Obj::Ref(n) => Some(*n),
                        _ => None,
                    })
                    .collect(),
                _ => vec![*number],
            },
            Some(Obj::Array(items)) => items
                .iter()
                .filter_map(|item| match item {
                    Obj::Ref(n) => Some(*n),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        for number in refs {
            if let Some(data) = self.streams.get(&number) {
                out.extend_from_slice(data);
                out.push(b'\n');
            }
        }
        out
    }

    /// How font `font` turns its codes into text.
    fn font(&self, font: &Obj) -> Font {
        let mut map = HashMap::new();
        let mut two_bytes = false;
        if let Some(Obj::Name(encoding)) = self.get(font, "Encoding") {
            two_bytes = encoding.starts_with("Identity") || encoding.ends_with("-H");
        }
        if let Some(Obj::Ref(number)) = self.resolve(font).get("ToUnicode")
            && let Some(data) = self.streams.get(number)
        {
            let (read, wide) = cmap(data);
            map = read;
            two_bytes |= wide;
        }
        let bold = self
            .get(font, "BaseFont")
            .and_then(Obj::name)
            .is_some_and(|name| name.contains("Bold") || name.contains("Black"));
        Font {
            map,
            two_bytes,
            bold,
        }
    }
}

fn starts_after_white(bytes: &[u8], word: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|b| !is_white(*b))
        .unwrap_or(bytes.len());
    bytes[start..].starts_with(word)
}

fn find(bytes: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

/// The object number before ` obj` at `at` (`n g obj`).
fn object_number(bytes: &[u8], at: usize) -> Option<u32> {
    let mut i = at;
    let digits = |i: &mut usize| {
        let end = *i;
        while *i > 0 && bytes[*i - 1].is_ascii_digit() {
            *i -= 1;
        }
        (*i < end).then(|| {
            std::str::from_utf8(&bytes[*i..end])
                .ok()?
                .parse::<u32>()
                .ok()
        })?
    };
    digits(&mut i)?;
    if i == 0 || !is_white(bytes[i - 1]) {
        return None;
    }
    while i > 0 && is_white(bytes[i - 1]) {
        i -= 1;
    }
    digits(&mut i)
}

/// The code-to-text pairs of a ToUnicode CMap, and whether its codes take
/// two bytes.
fn cmap(data: &[u8]) -> (HashMap<u32, String>, bool) {
    let mut map = HashMap::new();
    let mut wide = false;
    let text = |bytes: &[u8]| {
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&units)
    };
    let code = |bytes: &[u8]| bytes.iter().fold(0_u32, |n, b| (n << 8) | u32::from(*b));
    let mut lexer = Lexer::new(data, 0);
    let mut stack: Vec<Obj> = Vec::new();
    while let Some(value) = {
        lexer.skip();
        if lexer.at >= data.len() {
            None
        } else {
            Some(lexer.value().unwrap_or(Obj::Null))
        }
    } {
        match value {
            Obj::Op(op) if op == "endbfchar" => {
                for pair in stack.chunks(2) {
                    if let [Obj::Str(from), Obj::Str(to)] = pair {
                        wide |= from.len() > 1;
                        map.insert(code(from), text(to));
                    }
                }
                stack.clear();
            }
            Obj::Op(op) if op == "endbfrange" => {
                for triple in stack.chunks(3) {
                    match triple {
                        [Obj::Str(low), Obj::Str(high), Obj::Str(to)] => {
                            wide |= low.len() > 1;
                            let base: Vec<u16> = to
                                .chunks(2)
                                .map(|p| u16::from_be_bytes([p[0], *p.get(1).unwrap_or(&0)]))
                                .collect();
                            for (step, c) in
                                (code(low)..=code(high).min(code(low) + 0xffff)).enumerate()
                            {
                                let mut units = base.clone();
                                if let Some(last) = units.last_mut() {
                                    *last = last.wrapping_add(step as u16);
                                }
                                map.insert(c, String::from_utf16_lossy(&units));
                            }
                        }
                        [Obj::Str(low), Obj::Str(high), Obj::Array(each)] => {
                            wide |= low.len() > 1;
                            for (c, to) in (code(low)..=code(high)).zip(each) {
                                if let Obj::Str(to) = to {
                                    map.insert(c, text(to));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                stack.clear();
            }
            Obj::Op(op) if op.starts_with("begin") => stack.clear(),
            Obj::Op(_) => {}
            other => stack.push(other),
        }
    }
    (map, wide)
}

/// How a font's codes read as text.
struct Font {
    map: HashMap<u32, String>,
    two_bytes: bool,
    bold: bool,
}

impl Font {
    fn text(&self, bytes: &[u8]) -> String {
        if self.two_bytes {
            bytes
                .chunks(2)
                .map(|pair| {
                    let code = pair.iter().fold(0_u32, |n, b| (n << 8) | u32::from(*b));
                    self.map.get(&code).cloned().unwrap_or_default()
                })
                .collect()
        } else {
            bytes
                .iter()
                .map(|byte| {
                    self.map
                        .get(&u32::from(*byte))
                        .cloned()
                        .unwrap_or_else(|| win_ansi(*byte).to_string())
                })
                .collect()
        }
    }
}

/// A byte of the Windows (and near enough, the standard) encoding.
fn win_ansi(byte: u8) -> char {
    match byte {
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201c}',
        0x94 => '\u{201d}',
        0x95 => '\u{2022}',
        0x85 => '\u{2026}',
        0x80 => '\u{20ac}',
        0x96 | 0x97 => '-',
        0x20..=0x7e | 0xa0..=0xff => char::from(byte),
        _ => ' ',
    }
}

/// A line of text on a page: where it sits, how big, and what it says.
struct Line {
    y: f64,
    size: f64,
    bold: bool,
    text: String,
}

/// Adds `text` set at `at` to the page's lines: onto the last line when
/// it sits on the same baseline (with a space for a wide gap after
/// `last_end`), else as a new line.
fn push(
    lines: &mut Vec<Line>,
    text: String,
    (x, y): (f64, f64),
    size: f64,
    bold: bool,
    last_end: Option<(f64, f64)>,
) {
    if text.is_empty() {
        return;
    }
    let height = size.abs().max(1.0);
    match lines.last_mut() {
        Some(line) if (line.y - y).abs() < height * 0.4 => {
            let gap = last_end.map_or(0.0, |(end, _)| x - end);
            if gap > height * 0.25 && !line.text.ends_with(' ') && !text.starts_with(' ') {
                line.text.push(' ');
            }
            line.text.push_str(&text);
            line.bold &= bold;
        }
        _ => lines.push(Line {
            y,
            size: height,
            bold,
            text,
        }),
    }
}

/// The lines of one page's content.
fn page_lines(file: &File, page: u32) -> Vec<Line> {
    let fonts: HashMap<String, Font> = file
        .inherited(page, "Resources")
        .and_then(|resources| file.get(resources, "Font"))
        .and_then(|fonts| match fonts {
            Obj::Dict(dict) => Some(
                dict.iter()
                    .map(|(name, font)| (name.clone(), file.font(font)))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let data = file.contents(page);
    let mut lexer = Lexer::new(&data, 0);
    let mut operands: Vec<Obj> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    let mut font: Option<&Font> = None;
    let mut size = 12.0;
    // The text matrix's scale and position, and the line start.
    let (mut scale, mut x, mut y, mut leading) = (1.0, 0.0_f64, 0.0_f64, 0.0);
    let mut last_end: Option<(f64, f64)> = None;
    loop {
        lexer.skip();
        if lexer.at >= data.len() {
            break;
        }
        let Some(value) = lexer.value() else {
            continue;
        };
        let Obj::Op(op) = value else {
            operands.push(value);
            continue;
        };
        let n = |i: usize| operands.get(i).and_then(Obj::num).unwrap_or(0.0);
        match op.as_str() {
            "BI" => {
                // An inline picture: skip to its end.
                lexer.at = find(&data, b"EI", lexer.at).map_or(data.len(), |at| at + 2);
            }
            "BT" => {
                scale = 1.0;
                x = 0.0;
                y = 0.0;
            }
            "Tf" => {
                if let Some(Obj::Name(name)) = operands.first() {
                    font = fonts.get(name);
                }
                size = n(1);
            }
            "TL" => leading = n(0),
            "Tm" => {
                scale = (n(0) * n(0) + n(1) * n(1)).sqrt().max(0.01);
                x = n(4);
                y = n(5);
                last_end = None;
            }
            "Td" | "TD" => {
                x += n(0) * scale;
                y += n(1) * scale;
                if op == "TD" {
                    leading = -n(1);
                }
                last_end = None;
            }
            "T*" => {
                y -= leading * scale;
                last_end = None;
            }
            "Tj" | "'" | "\"" | "TJ" => {
                if op == "'" || op == "\"" {
                    y -= leading * scale;
                }
                let shown = size * scale;
                let bold = font.is_some_and(|f| f.bold);
                let mut text = String::new();
                let mut advance = 0.0;
                let add = |bytes: &[u8], text: &mut String| {
                    let piece = font.map_or_else(
                        || bytes.iter().map(|b| win_ansi(*b)).collect(),
                        |f| f.text(bytes),
                    );
                    // Glyph widths are not read: half the size a character.
                    text.push_str(&piece);
                    piece.chars().count() as f64 * shown * 0.5
                };
                match operands.last() {
                    Some(Obj::Str(bytes)) => advance += add(bytes, &mut text),
                    Some(Obj::Array(items)) => {
                        for item in items {
                            match item {
                                Obj::Str(bytes) => advance += add(bytes, &mut text),
                                // A shift back of more than an eighth of the size is
                                // the room of a space (writers that set no space
                                // glyphs, Sue's among them, move the pen instead).
                                Obj::Num(kern) if *kern < -125.0 && !text.ends_with(' ') => {
                                    text.push(' ');
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                push(&mut lines, text, (x, y), shown, bold, last_end);
                last_end = Some((x + advance, y));
                x += advance;
            }
            _ => {}
        }
        operands.clear();
    }
    lines.retain(|line| !line.text.trim().is_empty());
    lines
}

/// Reads a PDF's text as rich text: a paragraph for each block of lines,
/// headings where the type is big, and the pages in order.
///
/// # Errors
/// If it is not a PDF, or none of its text can be read.
pub fn from_pdf(bytes: &[u8]) -> Result<(String, TextMarks), String> {
    if !bytes.starts_with(b"%PDF") {
        return Err("not a PDF".to_owned());
    }
    let file = File::read(bytes);
    let pages: Vec<Vec<Line>> = file
        .pages()
        .into_iter()
        .map(|page| page_lines(&file, page))
        .collect();
    // The body size: the size most of the text is set in.
    let mut sizes: HashMap<u32, usize> = HashMap::new();
    for line in pages.iter().flatten() {
        *sizes.entry((line.size * 2.0).round() as u32).or_default() += line.text.len();
    }
    let body = sizes
        .into_iter()
        // A tie goes to the smaller size: headings are the big ones.
        .max_by_key(|(size, count)| (*count, std::cmp::Reverse(*size)))
        .map_or(12.0, |(size, _)| f64::from(size) / 2.0);
    // Running heads and page numbers are not the text: a line of only a
    // number, or a small first or last line that (numbers aside) is at
    // the same place on most pages.
    let key = |line: &Line| {
        let words: String = line.text.chars().filter(|c| !c.is_ascii_digit()).collect();
        ((line.y / 4.0).round() as i64, words.trim().to_owned())
    };
    let mut seen: HashMap<(i64, String), usize> = HashMap::new();
    for lines in &pages {
        let mut once: Vec<(i64, String)> = lines.iter().map(key).collect();
        once.sort();
        once.dedup();
        for one in once {
            *seen.entry(one).or_default() += 1;
        }
    }
    let count = pages.len();
    let pages: Vec<Vec<Line>> = pages
        .into_iter()
        .map(|lines| {
            let last = lines.len().saturating_sub(1);
            lines
                .into_iter()
                .enumerate()
                .filter(|(index, line)| {
                    let number = line
                        .text
                        .trim()
                        .trim_end_matches('.')
                        .chars()
                        .all(|c| c.is_ascii_digit());
                    let edge = *index == 0 || *index == last;
                    let repeated = count >= 3
                        && edge
                        && line.size <= body * 1.05
                        && seen.get(&key(line)).copied().unwrap_or(0) * 2 > count;
                    !number && !repeated
                })
                .map(|(_, line)| line)
                .collect()
        })
        .collect();
    let all: Vec<&Line> = pages.iter().flatten().collect();
    if all.is_empty() {
        return Err("no text found in the PDF".to_owned());
    }
    let mut text = String::new();
    let mut marks = TextMarks::new();
    let mut paragraph = String::new();
    let mut paragraph_size = body;
    let mut paragraph_bold = false;
    let flush = |paragraph: &mut String,
                 size: f64,
                 bold: bool,
                 text: &mut String,
                 marks: &mut TextMarks| {
        let line = paragraph.trim();
        if line.is_empty() {
            paragraph.clear();
            return;
        }
        let start = text.len();
        text.push_str(line);
        text.push('\n');
        let style = if size >= body * 1.9 {
            Some("title")
        } else if size >= body * 1.45 {
            Some("heading1")
        } else if size >= body * 1.2 {
            Some("heading2")
        } else if bold && line.len() < 80 && size > body * 0.99 {
            Some("heading3")
        } else {
            None
        };
        if let Some(style) = style {
            marks.set_paragraphs(
                text,
                start..start,
                kind::STYLE,
                Some(Value::Text(style.into())),
            );
        }
        paragraph.clear();
    };
    for lines in &pages {
        let mut previous: Option<&Line> = None;
        for line in lines {
            let new_paragraph = previous.is_none_or(|before| {
                let gap = before.y - line.y;
                let size_changed = (before.size - line.size).abs() > body * 0.15;
                size_changed || gap > before.size.max(line.size) * 1.75 || gap < 0.0
            });
            if new_paragraph {
                flush(
                    &mut paragraph,
                    paragraph_size,
                    paragraph_bold,
                    &mut text,
                    &mut marks,
                );
                paragraph_size = line.size;
                paragraph_bold = line.bold;
            } else if paragraph.ends_with('-') && !paragraph.ends_with(" -") {
                // A word broken across lines.
                paragraph.pop();
            } else {
                paragraph.push(' ');
            }
            paragraph.push_str(line.text.trim());
            previous = Some(line);
        }
        // A page ends its paragraph unless it ends mid-sentence.
        if !paragraph
            .trim_end()
            .ends_with(|c: char| c.is_alphanumeric() || c == ',')
        {
            flush(
                &mut paragraph,
                paragraph_size,
                paragraph_bold,
                &mut text,
                &mut marks,
            );
        } else {
            paragraph.push(' ');
        }
    }
    flush(
        &mut paragraph,
        paragraph_size,
        paragraph_bold,
        &mut text,
        &mut marks,
    );
    Ok((text, marks))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small PDF in the old style: one page, a heading and two
    /// paragraphs in a standard font, with a classic trailer.
    fn simple_pdf() -> Vec<u8> {
        let content = b"BT /F1 24 Tf 72 720 Td (A Title) Tj ET\n\
BT /F1 12 Tf 72 680 Td 14 TL (The first paragraph runs on) Tj T* (across two lines.) Tj ET\n\
BT /F1 12 Tf 72 620 Td [(Second) -300 (paragraph.)] TJ ET";
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
                .to_owned(),
            format!("<< /Length {} >>\nstream\n", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        for (index, object) in objects.iter().enumerate() {
            pdf.extend_from_slice(format!("{} 0 obj\n{object}", index + 1).as_bytes());
            if index == 3 {
                pdf.extend_from_slice(content);
                pdf.extend_from_slice(b"\nendstream");
            }
            pdf.extend_from_slice(b"\nendobj\n");
        }
        pdf.extend_from_slice(b"trailer\n<< /Root 1 0 R /Size 6 >>\n%%EOF\n");
        pdf
    }

    #[test]
    fn a_simple_pdf_reads_as_paragraphs_and_a_heading() {
        let (text, marks) = from_pdf(&simple_pdf()).unwrap();
        assert_eq!(
            text,
            "A Title\nThe first paragraph runs on across two lines.\nSecond paragraph.\n"
        );
        assert_eq!(
            marks.paragraph_value(&text, 0, kind::STYLE),
            Some(&Value::Text("title".into()))
        );
        assert_eq!(marks.paragraph_value(&text, 1, kind::STYLE), None);
    }

    #[test]
    fn a_tounicode_map_reads_two_byte_codes() {
        let cmap_data = b"/CIDInit /ProcSet findresource begin\n1 begincodespacerange <0000> <FFFF> endcodespacerange\n2 beginbfchar\n<0001> <0048>\n<0002> <0069>\nendbfchar\n1 beginbfrange\n<0010> <0012> <0061>\nendbfrange\nend";
        let (map, wide) = cmap(cmap_data);
        assert!(wide);
        let font = Font {
            map,
            two_bytes: true,
            bold: false,
        };
        assert_eq!(font.text(&[0, 1, 0, 2, 0, 0x10, 0, 0x12]), "Hiac");
    }

    #[test]
    fn what_is_not_a_pdf_is_refused() {
        assert!(from_pdf(b"hello").is_err());
        assert!(from_pdf(b"%PDF-1.4\n%%EOF").is_err());
    }
}
