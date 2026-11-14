//! The picture engine stands below every studio. Sue Writer and the shell
//! show pictures through it without Sue Paint, so it never depends on a
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

/// The code of one line: no comment, and no text inside string literals
/// (ids such as `"sue_paint.comic"` are data that files keep, not code).
fn code_of(line: &str) -> String {
    let mut code = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    quoted = false;
                    code.push(c);
                }
                _ => {}
            }
        } else if c == '"' {
            quoted = true;
            code.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            break;
        } else {
            code.push(c);
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
