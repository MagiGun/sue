//! Hand-offs to the operating system: the default web browser, the file
//! manager, native open and save dialogs, and the clipboard.

use std::io;
use std::path::{Path, PathBuf};

/// Puts `text` on the system clipboard (`arboard`). Text fields copy and
/// paste through egui on their own; this is for buttons such as "Copy path".
///
/// # Errors
/// The clipboard's message when it is unavailable.
pub fn copy_text(text: &str) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
            .map_err(|error| error.to_string())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = text;
        Err("there is no system clipboard to reach here; select the text and copy it".to_owned())
    }
}

/// The system opener for files, folders and web addresses, where there is
/// one (desktops).
fn open_detached(target: &std::ffi::OsStr) -> io::Result<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        open::that_detached(target)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = target;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this device has no system opener wired up yet",
        ))
    }
}

/// Normalizes what a user typed into a URL bar: `example.com` becomes
/// `https://example.com`. Only `http` and `https` URLs are accepted, so a
/// URL bar can never launch a local program or file.
///
/// # Errors
/// [`io::ErrorKind::InvalidInput`] for empty text, other schemes, or text
/// with spaces or control characters.
pub fn web_url(text: &str) -> io::Result<String> {
    let text = text.trim();
    let invalid = |why: &str| io::Error::new(io::ErrorKind::InvalidInput, why.to_owned());
    if text.is_empty() {
        return Err(invalid("the address is empty"));
    }
    if text.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err(invalid("addresses cannot contain spaces"));
    }
    let lower = text.to_ascii_lowercase();
    let url = if lower.starts_with("https://") || lower.starts_with("http://") {
        text.to_owned()
    } else if lower.contains("://")
        || lower.starts_with("file:")
        || lower.starts_with("javascript:")
    {
        return Err(invalid("only http and https addresses open"));
    } else {
        format!("https://{text}")
    };
    let host = url
        .split_once("://")
        .map_or("", |(_, rest)| rest)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    if host.is_empty() || host.starts_with('.') {
        return Err(invalid("the address has no host"));
    }
    Ok(url)
}

/// Opens a web address in the system browser (see [`web_url`]).
///
/// # Errors
/// An invalid address, or the OS failing to start a browser.
pub fn open_url(text: &str) -> io::Result<String> {
    let url = web_url(text)?;
    open_detached(url.as_ref())?;
    Ok(url)
}

/// Shows a folder in the system file manager.
///
/// # Errors
/// If `dir` is not a folder, or the OS fails to open it.
pub fn reveal_folder(dir: &Path) -> io::Result<()> {
    if !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a folder", dir.display()),
        ));
    }
    open_detached(dir.as_os_str())
}

/// Opens a document (a PDF to print, a picture) in the program the
/// system opens it with.
///
/// # Errors
/// If the file is gone, or the OS fails to open it.
pub fn open_document(path: &Path) -> io::Result<()> {
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is gone", path.display()),
        ));
    }
    open_detached(path.as_os_str())
}

/// Shows a file in the system file manager, selected in its folder where
/// the system allows it (Explorer and Finder do), else just its folder.
///
/// # Errors
/// If the file is gone, or the OS fails to open the file manager.
pub fn reveal_file(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is gone", path.display()),
        ));
    }
    if path.is_dir() {
        return open_detached(path.as_os_str());
    }
    if cfg!(target_os = "windows") {
        let mut select = std::ffi::OsString::from("/select,");
        select.push(path.as_os_str());
        std::process::Command::new("explorer.exe")
            .arg(select)
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
    } else {
        let folder = path.parent().unwrap_or(path);
        open_detached(folder.as_os_str())
    }
}

/// A file-type filter for the native dialogs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    /// What the user sees, e.g. `Rice files`.
    pub name: String,
    /// Extensions without the dot, e.g. `["suerice"]`.
    pub extensions: Vec<String>,
}

/// Asks the user for a file to open with the native dialog. Blocks until the
/// dialog closes; `None` if it was cancelled.
#[must_use]
pub fn pick_open_file(
    title: &str,
    start: Option<&Path>,
    filters: &[FileFilter],
) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(start) = start.filter(|start| start.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        for filter in filters {
            dialog = dialog.add_filter(filter.name.clone(), &filter.extensions);
        }
        dialog.pick_file()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        // No system dialog is wired up on phones and tablets yet.
        let _ = (title, start, filters);
        None
    }
}

/// Asks the user for a folder with the native dialog. Blocks until the
/// dialog closes; `None` if it was cancelled.
#[must_use]
pub fn pick_folder(title: &str, start: Option<&Path>) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(start) = start.filter(|start| start.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, start);
        None
    }
}

/// Asks the user where to save a file with the native dialog. Blocks until
/// the dialog closes; `None` if it was cancelled.
#[must_use]
pub fn pick_save_file(
    title: &str,
    start: Option<&Path>,
    file_name: &str,
    filters: &[FileFilter],
) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(file_name);
        if let Some(start) = start.filter(|start| start.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        for filter in filters {
            dialog = dialog.add_filter(filter.name.clone(), &filter.extensions);
        }
        dialog.save_file()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, start, file_name, filters);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_bars_only_open_web_addresses() {
        assert_eq!(web_url("docs.rs").unwrap(), "https://docs.rs");
        assert_eq!(
            web_url("  http://example.com/a?b ").unwrap(),
            "http://example.com/a?b"
        );
        for bad in [
            "",
            "file:///C:/Windows/system32/calc.exe",
            "javascript:alert(1)",
            "ftp://example.com",
            "two words",
            "https://",
            "https:///path",
        ] {
            assert!(web_url(bad).is_err(), "{bad:?} must be rejected");
        }
    }
}
