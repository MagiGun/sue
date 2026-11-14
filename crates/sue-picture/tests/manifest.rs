//! The picture engine stands below every studio. Sue Writer shows
//! pictures through it without Sue Paint, so it never depends on a
//! studio, and it holds no interface code of its own: the studios draw
//! the interface, the engine works on the arena and its pixels.

use std::path::{Path, PathBuf};

/// The names under `[section]` of the manifest.
fn names(manifest: &str, section: &str) -> Vec<String> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != section)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let name = line.split(['=', ' ']).next().unwrap_or(line);
            name.trim_end_matches(".workspace").to_owned()
        })
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the sources are there") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_engine_depends_on_no_studio_and_no_interface_crate() {
    let manifest = include_str!("../Cargo.toml");
    assert_eq!(
        names(manifest, "[dependencies]"),
        [
            "sha2",
            "sue-brush",
            "sue-core",
            "sue-io",
            "sue-lang",
            "sue-model",
            "sue-platform",
            "sue-type"
        ],
        "sue-picture works on pictures; the studios and the shell stand on it"
    );
    for line in manifest.lines().map(str::trim) {
        // A dependency written as a table of its own would not be in the
        // lists read here.
        assert!(
            !line.starts_with("[dependencies.")
                && !line.starts_with("[dev-dependencies.")
                && !line.starts_with("[target."),
            "{line}: name a dependency on one line under its section"
        );
    }
    for name in names(manifest, "[dev-dependencies]") {
        assert!(
            !["egui", "sue-wm", "sue-paint", "sue-writer"].contains(&name.as_str()),
            "{name} is not for the engine's tests"
        );
    }
}

/// The length of the character literal at the start of `rest` (`'a'`,
/// `'"'`, `'\''`, `'\u{2014}'`), or `None` for a lifetime or a label.
fn char_literal(rest: &[char]) -> Option<usize> {
    match rest {
        ['\'', '\\', ..] => rest[2..]
            .iter()
            .skip(1)
            .position(|&c| c == '\'')
            .map(|at| at + 4),
        ['\'', _, '\'', ..] => Some(3),
        _ => None,
    }
}

/// The code of one line: no comment, and no text inside string or
/// character literals (ids such as `"sue_paint.comic"` are data that files
/// keep, not code). A literal's quotes stay, its contents go.
fn code_of(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut code = String::new();
    let mut at = 0;
    while at < chars.len() {
        let rest = &chars[at..];
        let after_word = at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
        // A raw string, `r"..."` or `r#"..."#`, ends at its own quote and
        // hashes; a backslash in it is just a backslash.
        if rest[0] == 'r' && !after_word {
            let hashes = rest[1..].iter().take_while(|&&c| c == '#').count();
            if rest.get(1 + hashes) == Some(&'"') {
                let close: Vec<char> = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let body = at + 2 + hashes;
                let end = (body..=chars.len().saturating_sub(close.len()))
                    .find(|&from| chars[from..].starts_with(&close))
                    .map_or(chars.len(), |from| from + close.len());
                code.push_str("\"\"");
                at = end;
                continue;
            }
        }
        if let Some(length) = char_literal(rest) {
            code.push_str("''");
            at += length;
            continue;
        }
        match rest[0] {
            '"' => {
                let mut end = at + 1;
                while end < chars.len() && chars[end] != '"' {
                    end += if chars[end] == '\\' { 2 } else { 1 };
                }
                code.push_str("\"\"");
                at = end + 1;
            }
            '/' if rest.get(1) == Some(&'/') => break,
            c => {
                code.push(c);
                at += 1;
            }
        }
    }
    code
}

#[test]
fn code_of_drops_comments_and_quoted_text() {
    assert_eq!(code_of("let a = 1; // sue_paint"), "let a = 1; ");
    assert_eq!(
        code_of(r#"pub const C: &str = "sue_paint.comic"; // x"#),
        r#"pub const C: &str = ""; "#
    );
    assert_eq!(code_of(r#"f("a\"b", egui::X)"#), r#"f("", egui::X)"#);
    // A quote in a character literal opens no string, so the code after
    // it is still read.
    assert_eq!(
        code_of(r#"if c == '"' { egui::X } // y"#),
        "if c == '' { egui::X } "
    );
    assert_eq!(
        code_of(r"[',', '\'', '\n'], sue_wm"),
        "['', '', ''], sue_wm"
    );
    assert_eq!(code_of(r"fn f<'a>(x: &'a str)"), r"fn f<'a>(x: &'a str)");
    // A raw string ends at its own quote and hashes.
    assert_eq!(
        code_of(r##"f(r#"a "quoted" egui"#, sue_wm::X)"##),
        r#"f("", sue_wm::X)"#
    );
    assert_eq!(
        code_of(r#"for (number, line) in x"#),
        "for (number, line) in x"
    );
}

#[test]
fn the_engine_calls_no_interface_code() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() >= 31, "the 30 engine modules and lib.rs");
    for path in files {
        let source = std::fs::read_to_string(&path).expect("a source file reads");
        for (number, line) in source.lines().enumerate() {
            let code = code_of(line);
            for word in ["egui", "sue_wm", "sue_paint", "AppletContext", "SuePaint"] {
                assert!(
                    !code
                        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                        .any(|token| token == word),
                    "{}:{}: the engine names {word}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
}
