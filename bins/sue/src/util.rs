//! Small helpers shared by the applets and the runtime.

use std::path::{Path, PathBuf};

use sue_io::IngestView;
use sue_wm::MediaClass;

/// Maps an ingest view onto the media class applets declare.
pub fn media_class(view: IngestView) -> MediaClass {
    match view {
        IngestView::Raster => MediaClass::Image,
        IngestView::Text => MediaClass::Text,
        IngestView::Audio => MediaClass::Audio,
        IngestView::Bytes => MediaClass::Binary,
    }
}

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

/// `3:07`, or `1:02:03` past an hour.
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
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
    fn clocks_read_like_players() {
        assert_eq!(clock(0.4), "0:00");
        assert_eq!(clock(187.9), "3:07");
        assert_eq!(clock(3723.0), "1:02:03");
    }

    #[test]
    fn views_map_to_media_classes() {
        assert_eq!(media_class(IngestView::Raster), MediaClass::Image);
        assert_eq!(media_class(IngestView::Text), MediaClass::Text);
        assert_eq!(media_class(IngestView::Audio), MediaClass::Audio);
        assert_eq!(media_class(IngestView::Bytes), MediaClass::Binary);
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
