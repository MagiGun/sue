//! Finds every built-in template in `templates/` and embeds it, so adding
//! a template is adding a file: no list in the code names them.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .expect("crates/sue-writer/templates exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "template"))
        .collect();
    files.sort();
    let mut out = String::from("/// Every built-in template: file name and source.\n");
    out.push_str("pub(crate) const BUILT_IN: &[(&str, &str)] = &[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let path = path.to_string_lossy().into_owned();
        writeln!(out, "    ({name:?}, include_str!({path:?})),").expect("writing to a String");
    }
    out.push_str("];\n");
    let dest =
        Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("templates.rs");
    std::fs::write(dest, out).expect("OUT_DIR is writable");
}
