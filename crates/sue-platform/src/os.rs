//! Hand-offs to the operating system: the default web browser, the file
//! manager, the system's own open, save and folder dialogs, and the
//! clipboard.

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

/// The name of the file manager [`reveal_file`] shows files in, for
/// words such as "Show it in Explorer"; `None` where Sue cannot show a
/// file in one (phones and tablets).
#[must_use]
pub fn file_manager() -> Option<&'static str> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        None
    } else if cfg!(target_os = "windows") {
        Some("Explorer")
    } else if cfg!(target_os = "macos") {
        Some("Finder")
    } else {
        Some("the file manager")
    }
}

/// Where a file or folder is, in the words people know it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownPlace {
    /// Its name, e.g. `Picture.png`.
    pub name: String,
    /// The folder it is in, where the system can say it, e.g. `Download`
    /// or `Documents/Exports`.
    pub folder: Option<String>,
}

impl std::fmt::Display for ShownPlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.folder {
            Some(folder) => write!(f, "{} in {folder}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// What people chose, for a path a system picker staged in Sue's cache
/// on a phone or tablet: writes to it go to that place (see
/// [`deliver_picked_files`]). `None` for every other path, so always on
/// desktops.
#[must_use]
pub fn shown_place(path: &Path) -> Option<ShownPlace> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        crate::picks::native_shown(path)
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = path;
        None
    }
}

/// A path as people should read it: what they chose for a path a system
/// picker staged (see [`shown_place`]), else the path itself.
#[must_use]
pub fn shown_path(path: &Path) -> String {
    shown_place(path).map_or_else(|| path.display().to_string(), |shown| shown.to_string())
}

/// A file-type filter for the native dialogs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFilter {
    /// What the user sees, e.g. `Rice files`.
    pub name: String,
    /// Extensions without the dot, e.g. `["suerice"]`.
    pub extensions: Vec<String>,
}

/// How a system file dialog answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picking {
    /// The person chose this path.
    Chosen(PathBuf),
    /// The dialog closed without a choice.
    Cancelled,
    /// The dialog is open beside Sue (phones and tablets); its answer
    /// comes from [`poll_picks`].
    Waiting,
    /// The system could not show the dialog or hand the file over.
    Failed(String),
}

/// News from the system's file dialogs on phones and tablets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickNews {
    /// The dialog that said [`Picking::Waiting`] closed with this answer.
    Answered(Picking),
    /// A file written to a chosen path could not reach the place the
    /// person chose; what people should read.
    Failed(String),
}

/// Asks the person for a file to open with the system's own dialog.
/// Desktops block until it closes; phones and tablets answer
/// [`Picking::Waiting`] and copy the chosen document into Sue's cache,
/// where writes go back to it ([`deliver_picked_files`]).
#[must_use]
pub fn pick_open_file(title: &str, start: Option<&Path>, filters: &[FileFilter]) -> Picking {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(start) = start.filter(|start| start.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        for filter in filters {
            dialog = dialog.add_filter(filter.name.clone(), &filter.extensions);
        }
        dialog
            .pick_file()
            .map_or(Picking::Cancelled, Picking::Chosen)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, start);
        crate::picks::native().show(crate::picks::Kind::Open, &extensions(filters), "")
    }
}

/// Asks the person for a folder with the system's own dialog. Desktops
/// block until it closes; phones and tablets answer [`Picking::Waiting`].
#[must_use]
pub fn pick_folder(title: &str, start: Option<&Path>) -> Picking {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(start) = start.filter(|start| start.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        dialog
            .pick_folder()
            .map_or(Picking::Cancelled, Picking::Chosen)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, start);
        crate::picks::native().show(crate::picks::Kind::Folder, &[], "")
    }
}

/// Asks the person where to save a file with the system's own dialog.
/// Desktops block until it closes. Android asks first and answers
/// [`Picking::Waiting`]; iOS answers a path in Sue's cache at once and
/// asks where to export it once it is written. Either way, call
/// [`deliver_picked_files`] after writing.
#[must_use]
pub fn pick_save_file(
    title: &str,
    start: Option<&Path>,
    file_name: &str,
    filters: &[FileFilter],
) -> Picking {
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
        dialog
            .save_file()
            .map_or(Picking::Cancelled, Picking::Chosen)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, start);
        crate::picks::native().show(crate::picks::Kind::Save, &extensions(filters), file_name)
    }
}

/// Every extension the filters take, for the mobile pickers, which offer
/// one list of kinds.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn extensions(filters: &[FileFilter]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for extension in filters.iter().flat_map(|filter| &filter.extensions) {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        if !extension.is_empty() && !all.contains(&extension) {
            all.push(extension);
        }
    }
    all
}

/// Answers of dialogs that said [`Picking::Waiting`], and files that
/// could not reach the place chosen for them. Always empty on desktops.
#[must_use]
pub fn poll_picks() -> Vec<PickNews> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        crate::picks::native().poll()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        Vec::new()
    }
}

/// Hands files written to chosen paths over to the place the person
/// chose, where the system keeps that place apart from Sue (Android's
/// documents, iOS's Files). Call it after writing; it only looks at the
/// files picks staged, and does nothing on desktops.
pub fn deliver_picked_files() {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    crate::picks::native().deliver();
}

/// `true` while a system dialog is open beside Sue or a picked file is
/// being copied, so the caller keeps calling [`poll_picks`]. Never on
/// desktops.
#[must_use]
pub fn picks_busy() -> bool {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        crate::picks::native().busy()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        false
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

    #[test]
    fn desktops_show_paths_as_they_are() {
        let path = std::env::temp_dir().join("Picture.png");
        assert_eq!(shown_place(&path), None);
        assert_eq!(shown_path(&path), path.display().to_string());
        assert!(file_manager().is_some());
        let shown = ShownPlace {
            name: "Picture.png".to_owned(),
            folder: None,
        };
        assert_eq!(shown.to_string(), "Picture.png");
    }
}
