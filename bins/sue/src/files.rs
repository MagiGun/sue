//! Files: a folder browser in the left drawer, with quick jumps to the
//! project and the usual home folders. Folders open in place; files open on
//! the canvas through the same path as drag and drop. Dot files and the
//! Windows legacy junctions (`NetHood`, `Start Menu` and the like) are
//! hidden unless `files.show_hidden` is on.

use std::path::{Path, PathBuf};

use egui::{Align, Layout, RichText, Sense, Ui, vec2};
use sue_platform::{DirEntryInfo, list_dir};
use sue_wm::{AppletContext, AppletManifest, AppletRole, ShellRequest, SueApplet};

use crate::util::{file_name, human_bytes};

/// The applet id.
pub const ID: &str = "files";

/// Folders in the home folder offered as quick jumps.
const JUMPS: [&str; 5] = ["Desktop", "Documents", "Downloads", "Pictures", "Music"];

/// The icon for an entry.
fn icon_for(entry: &DirEntryInfo) -> &'static str {
    if entry.is_dir {
        return "\u{1F4C1}";
    }
    let extension = Path::new(&entry.name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "jxl" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" => "\u{1F5BC}",
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => "\u{1F3B5}",
        _ => "\u{1F4C4}",
    }
}

/// A folder listing.
#[derive(Debug, Default)]
pub struct Files {
    dir: Option<PathBuf>,
    entries: Vec<DirEntryInfo>,
    error: Option<String>,
    root_setting: Option<String>,
    show_hidden: bool,
}

impl Files {
    fn load(&mut self, dir: PathBuf) {
        match list_dir(&dir, self.show_hidden) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.error = Some(error.to_string());
            }
        }
        self.dir = Some(dir);
    }

    fn project(cx: &AppletContext<'_>) -> PathBuf {
        let riced = cx
            .rice()
            .get_str("general.workspace_dir")
            .unwrap_or("")
            .trim();
        if riced.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| Self::home(cx))
        } else {
            PathBuf::from(riced)
        }
    }

    fn home(cx: &AppletContext<'_>) -> PathBuf {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    }

    /// Follows `general.workspace_dir` and `files.show_hidden`.
    fn sync(&mut self, cx: &AppletContext<'_>) {
        let setting = cx
            .rice()
            .get_str("general.workspace_dir")
            .unwrap_or("")
            .trim()
            .to_owned();
        let hidden = cx.rice().get_bool("files.show_hidden").unwrap_or(false);
        if self.root_setting.as_deref() != Some(setting.as_str()) {
            self.root_setting = Some(setting.clone());
            self.show_hidden = hidden;
            let root = if setting.is_empty() {
                Self::home(cx)
            } else {
                PathBuf::from(setting)
            };
            self.load(root);
        } else if hidden != self.show_hidden {
            self.show_hidden = hidden;
            if let Some(dir) = self.dir.clone() {
                self.load(dir);
            }
        }
    }
}

impl SueApplet for Files {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Files", AppletRole::LeftDrawer, "\u{1F4C1}");
        manifest.summary = "Browse folders and open files on the canvas.".to_owned();
        manifest.order = 10;
        manifest
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.sync(cx);
        let theme = cx.theme.clone();
        let home = Self::home(cx);
        let mut jumps: Vec<(String, PathBuf)> = vec![("Project".to_owned(), Self::project(cx))];
        jumps.extend(
            JUMPS
                .iter()
                .map(|name| ((*name).to_owned(), home.join(name)))
                .filter(|(_, path)| path.is_dir()),
        );
        let mut go = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (name, path) in &jumps {
                let here = self.dir.as_deref() == Some(path.as_path());
                if sue_wm::widgets::pill_button(ui, name, here, &theme).clicked() {
                    go = Some(path.clone());
                }
            }
        });
        ui.add_space(theme.padding * 0.5);
        if let Some(dir) = self.dir.clone() {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        dir.parent().is_some(),
                        egui::Button::new(RichText::new("Up").color(theme.muted)).frame(false),
                    )
                    .on_hover_text("Up one folder")
                    .clicked()
                {
                    go = dir.parent().map(Path::to_path_buf);
                }
                ui.label(
                    RichText::new(file_name(&dir))
                        .size(theme.ui_size)
                        .color(theme.text),
                )
                .on_hover_text(dir.display().to_string());
            });
        }
        ui.separator();
        if let Some(error) = &self.error {
            ui.label(RichText::new(format!("Cannot list this folder: {error}")).color(theme.muted));
        }
        let row_height = theme.ui_size * 1.9;
        let mut open = None;
        for entry in &self.entries {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, theme.hover);
            }
            let mut row = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink2(vec2(6.0, 0.0)))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            row.label(RichText::new(icon_for(entry)).color(theme.muted));
            row.label(RichText::new(&entry.name).color(theme.text));
            if !entry.is_dir {
                row.with_layout(Layout::right_to_left(Align::Center), |row| {
                    row.label(
                        RichText::new(human_bytes(entry.size))
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                });
            }
            if response.clicked() {
                open = Some(entry.clone());
            }
        }
        if self.entries.is_empty() && self.error.is_none() {
            ui.label(RichText::new("This folder is empty").color(theme.muted));
        }
        if let Some(entry) = open {
            if entry.is_dir {
                go = Some(entry.path);
            } else {
                cx.request(ShellRequest::OpenFile(entry.path));
            }
        }
        if let Some(dir) = go {
            self.load(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> DirEntryInfo {
        DirEntryInfo {
            name: name.to_owned(),
            path: PathBuf::from(name),
            is_dir,
            size: 0,
            modified: None,
        }
    }

    #[test]
    fn icons_follow_the_kind_of_file() {
        assert_eq!(icon_for(&entry("photos", true)), "\u{1F4C1}");
        assert_eq!(icon_for(&entry("card.jxl", false)), "\u{1F5BC}");
        assert_eq!(icon_for(&entry("song.flac", false)), "\u{1F3B5}");
        assert_eq!(icon_for(&entry("notes.txt", false)), "\u{1F4C4}");
    }

    #[test]
    fn legacy_junctions_and_dot_files_stay_hidden() {
        assert!(sue_platform::hidden_by_os(".git"));
        assert_eq!(sue_platform::hidden_by_os("NetHood"), cfg!(windows));
        assert_eq!(sue_platform::hidden_by_os("Start Menu"), cfg!(windows));
        assert!(!sue_platform::hidden_by_os("Documents"));
    }
}
