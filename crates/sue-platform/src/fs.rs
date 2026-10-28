//! Crash-safe file writes.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `path` atomically: a temporary file in the same directory
/// is written, flushed to disk and renamed over `path`. A crash leaves either
/// the old file or the new one, never a torn one. Parent directories are
/// created as needed.
///
/// # Errors
/// Any I/O error, or [`io::ErrorKind::InvalidInput`] if `path` has no file name.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        // Best effort: the temporary file is garbage either way.
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_replace_files_atomically() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-fs-test-{}-{unique}", std::process::id()));
        let file = dir.join("nested").join("default.suerice");
        write_atomic(&file, b"gap = 8\n").unwrap();
        write_atomic(&file, b"gap = 12\n").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"gap = 12\n");
        let entries = fs::read_dir(file.parent().unwrap()).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(entries, 1, "no temporary files are left behind");
        assert!(write_atomic(Path::new(""), b"").is_err());
    }
}
