//! Small helpers shared by the applets and the runtime.

use std::path::{Path, PathBuf};

/// `1.4 MiB`, `812.0 KiB`, `96 B`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// `path`, or a free sibling such as `photo (2).png` when it exists.
pub fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|index| path.with_file_name(format!("{stem} ({index}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Where exports without a file of their own go: `io.convert_dir`, or the
/// home folder.
pub fn export_dir(cx: &sue_wm::AppletContext<'_>) -> PathBuf {
    let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
    if riced.is_empty() {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    } else {
        PathBuf::from(riced)
    }
}

/// The last component of `path`, for display.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_bytes(96), "96 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn free_paths_never_overwrite() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-util-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let taken = dir.join("photo.png");
        assert_eq!(free_path(&taken), taken);
        std::fs::write(&taken, b"x").unwrap();
        assert_eq!(free_path(&taken), dir.join("photo (2).png"));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(file_name(&taken), "photo.png");
    }
}
