//! Per-OS locations for configuration, data and caches.

use std::path::{Path, PathBuf};

use crate::{OsFamily, PlatformError};

/// Where Sue keeps its files on this device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformPaths {
    /// User configuration (rice files, keymaps).
    pub config_dir: PathBuf,
    /// Durable data (packages, models, generations).
    pub data_dir: PathBuf,
    /// Disposable caches (thumbnails, tiles).
    pub cache_dir: PathBuf,
    /// The user's home folder, where the Files panel starts.
    pub home_dir: PathBuf,
}

impl PlatformPaths {
    /// Resolves the directories of this desktop OS from the process
    /// environment.
    ///
    /// # Errors
    /// As [`resolve`](Self::resolve); always on mobile, which needs a sandbox
    /// root.
    pub fn detect() -> Result<Self, PlatformError> {
        Self::resolve(OsFamily::current(), |name| std::env::var(name).ok(), None)
    }

    /// Resolves the directories for `os`.
    ///
    /// `env` looks up environment variables; it is injected so resolution is a
    /// pure, testable function. `sandbox_root` is the app sandbox that Android
    /// and iOS runtimes provide; it is required there and ignored elsewhere.
    ///
    /// # Errors
    /// [`PlatformError::MissingEnvironment`] if a required variable is unset, or
    /// [`PlatformError::MissingSandbox`] on mobile without a sandbox root.
    pub fn resolve<F>(
        os: OsFamily,
        env: F,
        sandbox_root: Option<&Path>,
    ) -> Result<Self, PlatformError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let var = |name: &'static str| {
            env(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .ok_or(PlatformError::MissingEnvironment(name))
        };
        match os {
            OsFamily::Windows => {
                let roaming = var("APPDATA")?;
                let local = var("LOCALAPPDATA").unwrap_or_else(|_| roaming.clone());
                // %APPDATA% is <home>\AppData\Roaming when USERPROFILE is unset.
                let home_dir = var("USERPROFILE").unwrap_or_else(|_| {
                    roaming
                        .parent()
                        .and_then(Path::parent)
                        .map_or_else(|| roaming.clone(), Path::to_path_buf)
                });
                Ok(Self {
                    config_dir: roaming.join("Sue"),
                    data_dir: local.join("Sue"),
                    cache_dir: local.join("Sue").join("cache"),
                    home_dir,
                })
            }
            OsFamily::MacOs => {
                let home_dir = var("HOME")?;
                let library = home_dir.join("Library");
                let support = library.join("Application Support").join("Sue");
                Ok(Self {
                    config_dir: support.clone(),
                    data_dir: support,
                    cache_dir: library.join("Caches").join("Sue"),
                    home_dir,
                })
            }
            OsFamily::Android | OsFamily::Ios => {
                let root = sandbox_root.ok_or(PlatformError::MissingSandbox)?;
                Ok(Self {
                    config_dir: root.join("config"),
                    data_dir: root.join("data"),
                    cache_dir: root.join("cache"),
                    home_dir: root.join("documents"),
                })
            }
            OsFamily::Linux | OsFamily::Other => {
                // The XDG spec requires relative paths in these variables to be
                // ignored. `has_root` is the POSIX notion of absolute on any host.
                let xdg = |name: &str| env(name).map(PathBuf::from).filter(|path| path.has_root());
                let home = || var("HOME");
                let config = match xdg("XDG_CONFIG_HOME") {
                    Some(path) => path,
                    None => home()?.join(".config"),
                };
                let data = match xdg("XDG_DATA_HOME") {
                    Some(path) => path,
                    None => home()?.join(".local").join("share"),
                };
                let cache = match xdg("XDG_CACHE_HOME") {
                    Some(path) => path,
                    None => home()?.join(".cache"),
                };
                let home_dir = home().unwrap_or_else(|_| {
                    config
                        .parent()
                        .map_or_else(|| config.clone(), Path::to_path_buf)
                });
                Ok(Self {
                    config_dir: config.join("sue"),
                    data_dir: data.join("sue"),
                    cache_dir: cache.join("sue"),
                    home_dir,
                })
            }
        }
    }

    /// Directory holding `.suerice` files.
    #[must_use]
    pub fn rice_dir(&self) -> PathBuf {
        self.config_dir.join("rice")
    }

    /// Content-addressed package store and generations.
    #[must_use]
    pub fn packages_dir(&self) -> PathBuf {
        self.data_dir.join("packages")
    }

    /// Local AI models.
    #[must_use]
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// Notes and other documents Sue saves on its own.
    #[must_use]
    pub fn notes_dir(&self) -> PathBuf {
        self.data_dir.join("notes")
    }

    /// Fonts Sue keeps itself (installed from a studio), searched with the
    /// system's.
    #[must_use]
    pub fn fonts_dir(&self) -> PathBuf {
        self.data_dir.join("fonts")
    }
}

/// The folders this OS installs fonts in, system-wide and for the user,
/// that exist on this device.
#[must_use]
pub fn system_font_dirs() -> Vec<PathBuf> {
    font_dirs(OsFamily::current(), |name| std::env::var(name).ok())
        .into_iter()
        .filter(|dir| dir.is_dir())
        .collect()
}

/// The folders `os` installs fonts in, from its environment (`env` looks
/// variables up, so this stays a pure, testable function).
pub fn font_dirs<F>(os: OsFamily, env: F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let var = |name: &str| {
        env(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match os {
        OsFamily::Windows => {
            let windows = var("WINDIR")
                .or_else(|| var("SystemRoot"))
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
            let mut dirs = vec![windows.join("Fonts")];
            if let Some(local) = var("LOCALAPPDATA") {
                dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
            }
            dirs
        }
        OsFamily::MacOs | OsFamily::Ios => {
            let mut dirs = vec![
                PathBuf::from("/System/Library/Fonts"),
                PathBuf::from("/Library/Fonts"),
            ];
            if let Some(home) = var("HOME") {
                dirs.push(home.join("Library").join("Fonts"));
            }
            dirs
        }
        OsFamily::Android => vec![PathBuf::from("/system/fonts")],
        OsFamily::Linux | OsFamily::Other => {
            let mut dirs = vec![
                PathBuf::from("/usr/share/fonts"),
                PathBuf::from("/usr/local/share/fonts"),
            ];
            match var("XDG_DATA_HOME").filter(|path| path.has_root()) {
                Some(data) => dirs.push(data.join("fonts")),
                None => {
                    if let Some(home) = var("HOME") {
                        dirs.push(home.join(".local").join("share").join("fonts"));
                    }
                }
            }
            if let Some(home) = var("HOME") {
                dirs.push(home.join(".fonts"));
            }
            dirs
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|&(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn fonts_are_found_where_each_os_keeps_them() {
        let windows = font_dirs(
            OsFamily::Windows,
            env_of(&[
                ("WINDIR", r"C:\Windows"),
                ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
            ]),
        );
        assert_eq!(windows[0], Path::new(r"C:\Windows").join("Fonts"));
        assert!(windows[1].ends_with(Path::new("Microsoft").join("Windows").join("Fonts")));
        let linux = font_dirs(OsFamily::Linux, env_of(&[("HOME", "/home/a")]));
        assert!(linux.contains(&Path::new("/home/a/.local/share/fonts").to_path_buf()));
        let mac = font_dirs(OsFamily::MacOs, env_of(&[("HOME", "/Users/a")]));
        assert_eq!(mac.len(), 3);
    }

    #[test]
    fn windows_uses_appdata() {
        let env = env_of(&[
            ("APPDATA", r"C:\Users\a\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
        ]);
        let paths = PlatformPaths::resolve(OsFamily::Windows, env, None).unwrap();
        assert_eq!(
            paths.config_dir,
            Path::new(r"C:\Users\a\AppData\Roaming").join("Sue")
        );
        assert_eq!(
            paths.cache_dir,
            Path::new(r"C:\Users\a\AppData\Local")
                .join("Sue")
                .join("cache")
        );
        assert_eq!(paths.packages_dir(), paths.data_dir.join("packages"));
        assert_eq!(paths.home_dir, Path::new(r"C:\Users\a"));
    }

    #[test]
    fn linux_prefers_absolute_xdg_and_ignores_relative() {
        let env = env_of(&[
            ("HOME", "/home/a"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "relative/data"),
        ]);
        let paths = PlatformPaths::resolve(OsFamily::Linux, env, None).unwrap();
        assert_eq!(paths.config_dir, Path::new("/cfg").join("sue"));
        assert_eq!(
            paths.data_dir,
            Path::new("/home/a")
                .join(".local")
                .join("share")
                .join("sue")
        );
        assert_eq!(
            paths.cache_dir,
            Path::new("/home/a").join(".cache").join("sue")
        );
        assert_eq!(paths.home_dir, Path::new("/home/a"));
    }

    #[test]
    fn macos_uses_library() {
        let paths =
            PlatformPaths::resolve(OsFamily::MacOs, env_of(&[("HOME", "/Users/a")]), None).unwrap();
        assert!(
            paths
                .config_dir
                .ends_with(Path::new("Library").join("Application Support").join("Sue"))
        );
        assert!(paths.cache_dir.ends_with(Path::new("Caches").join("Sue")));
    }

    #[test]
    fn mobile_needs_a_sandbox() {
        assert_eq!(
            PlatformPaths::resolve(OsFamily::Android, env_of(&[]), None),
            Err(PlatformError::MissingSandbox)
        );
        let root = Path::new("/data/user/0/app.sue/files");
        let paths = PlatformPaths::resolve(OsFamily::Ios, env_of(&[]), Some(root)).unwrap();
        assert_eq!(paths.models_dir(), root.join("data").join("models"));
    }

    #[test]
    fn missing_home_is_reported() {
        assert_eq!(
            PlatformPaths::resolve(OsFamily::Linux, env_of(&[]), None),
            Err(PlatformError::MissingEnvironment("HOME"))
        );
    }
}
