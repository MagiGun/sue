//! Files: a folder browser in the Left Dock. Folders open in place; files
//! open in the applet that fits them, through the same path as drag and drop.

use std::path::{Path, PathBuf};

use sue_platform::{CursorShape, DirEntryInfo, InputEvent, Point, list_dir};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, Rect, ShellRequest, SueApplet, Ui, Zone, widget_id,
};

use crate::util::{file_name, human_bytes};

/// The applet id.
pub const ID: &str = "files";

/// A folder listing.
#[derive(Debug, Default)]
pub struct Files {
    dir: Option<PathBuf>,
    entries: Vec<DirEntryInfo>,
    error: Option<String>,
    scroll: f32,
    /// The rice values the listing was made with, to follow changes.
    root_setting: Option<String>,
    show_hidden: bool,
}

/// The icon for a file name.
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
        self.scroll = 0.0;
    }

    /// Follows `general.workspace_dir` and `files.show_hidden`.
    fn sync(&mut self, ctx: &AppletContext<'_>) {
        let setting = ctx
            .rice()
            .get_str("general.workspace_dir")
            .unwrap_or("")
            .trim()
            .to_owned();
        let hidden = ctx.rice().get_bool("files.show_hidden").unwrap_or(false);
        if self.root_setting.as_deref() != Some(setting.as_str()) {
            let root = if setting.is_empty() {
                ctx.catalog.folders.get("home").cloned().unwrap_or_default()
            } else {
                PathBuf::from(&setting)
            };
            self.root_setting = Some(setting);
            self.show_hidden = hidden;
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
        AppletManifest {
            id: AppletId::new(ID).expect("the Files id is valid"),
            name: "Files".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Browse folders and open any file in the applet that fits it.".to_owned(),
            opens: Vec::new(),
            icon: "\u{1F4C1}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        self.sync(ctx);
        Ok(())
    }

    fn handle_input(
        &mut self,
        _event: &InputEvent,
        _ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        Ok(InputResponse::Ignored)
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        self.sync(ctx);
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        let Some(dir) = self.dir.clone() else {
            return;
        };

        // Toolbar: up, the folder name, refresh.
        let up = Rect::new(area.x, area.y, row, row);
        let parent = dir.parent().map(Path::to_path_buf);
        if parent.is_some()
            && ui.icon_button(
                widget_id("files", "up"),
                up,
                "\u{2191}",
                false,
                "Parent folder",
            )
            && let Some(parent) = parent
        {
            self.load(parent);
        }
        let refresh = Rect::new(area.right() - row, area.y, row, row);
        if ui.icon_button(
            widget_id("files", "refresh"),
            refresh,
            "\u{1F504}",
            false,
            "Refresh",
        ) && let Some(dir) = self.dir.clone()
        {
            self.load(dir);
        }
        let title = Rect::new(
            up.right() + pad / 2.0,
            area.y,
            refresh.x - up.right() - pad,
            row,
        );
        let folder = file_name(&dir);
        let response = ui.interact(widget_id("files", "title"), title);
        ui.label(title, &folder, theme.text);
        ui.tooltip(response, title, &dir.display().to_string());

        let list = Rect::new(
            area.x,
            area.y + row + pad / 2.0,
            area.width,
            (area.height - row - pad / 2.0).max(0.0),
        );
        ui.hline(list.x, list.y - pad / 4.0, list.width);
        if let Some(error) = &self.error {
            ui.wrapped(
                list.x,
                list.y + pad / 2.0,
                list.width,
                &format!("Cannot read this folder: {error}"),
                theme.small_size,
                theme.alert,
            );
            return;
        }
        if self.entries.is_empty() {
            ui.label_sized(
                Rect::new(list.x, list.y, list.width, row),
                "This folder is empty",
                theme.small_size,
                theme.muted,
            );
            return;
        }
        let content = self.entries.len() as f32 * row;
        self.scroll =
            (self.scroll - ui.wheel_in(list)).clamp(0.0, (content - list.height).max(0.0));
        let first = (self.scroll / row).floor() as usize;
        let visible = (list.height / row).ceil() as usize + 1;
        let mut open = None;
        ui.canvas.push_clip(list);
        for index in first..(first + visible).min(self.entries.len()) {
            let entry = &self.entries[index];
            let rect = Rect::new(
                list.x,
                list.y + index as f32 * row - self.scroll,
                list.width,
                row,
            );
            let response = ui.interact(
                widget_id("files/entry", &entry.path.display().to_string()),
                rect,
            );
            if response.hovered {
                ui.canvas
                    .draw_box(rect, BoxStyle::filled(theme.hover, theme.radius));
                ui.set_cursor(CursorShape::Pointer);
            }
            let icon_width = row;
            ui.label_centered_sized(
                Rect::new(rect.x, rect.y, icon_width, row),
                icon_for(entry),
                theme.ui_size,
                if entry.is_dir {
                    theme.accent
                } else {
                    theme.muted
                },
            );
            let size = if entry.is_dir {
                String::new()
            } else {
                human_bytes(entry.size)
            };
            let size_width = ui.fonts.measure(FontFace::Ui, theme.small_size, &size);
            ui.label(
                Rect::new(
                    rect.x + icon_width,
                    rect.y,
                    (rect.width - icon_width - size_width - pad * 1.5).max(0.0),
                    row,
                ),
                &entry.name,
                theme.text,
            );
            if !size.is_empty() {
                ui.text(
                    Point::new(
                        rect.right() - size_width - pad / 2.0,
                        rect.y + (row - ui.small_line_height()) / 2.0,
                    ),
                    &size,
                    FontFace::Ui,
                    theme.small_size,
                    theme.muted,
                );
            }
            if response.clicked {
                open = Some((entry.path.clone(), entry.is_dir));
            }
        }
        ui.canvas.pop_clip();
        match open {
            Some((path, true)) => self.load(path),
            Some((path, false)) => ctx.request(ShellRequest::OpenFile(path)),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_kind_of_file() {
        let entry = |name: &str, is_dir| DirEntryInfo {
            name: name.to_owned(),
            path: PathBuf::from(name),
            is_dir,
            size: 0,
            modified: None,
        };
        assert_eq!(icon_for(&entry("Music", true)), "\u{1F4C1}");
        assert_eq!(icon_for(&entry("song.FLAC", false)), "\u{1F3B5}");
        assert_eq!(icon_for(&entry("photo.jxl", false)), "\u{1F5BC}");
        assert_eq!(icon_for(&entry("notes.md", false)), "\u{1F4C4}");
    }
}
