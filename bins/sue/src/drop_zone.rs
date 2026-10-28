//! Drop Zone: receives files dropped on the window, routes them through the
//! sniffer into the arena, and opens them in the applet that suits them.

use std::path::Path;

use sue_core::AssetHandle;
use sue_io::{IngestOptions, IngestView, NativeJxlPipeline, ingest_bytes, meta};
use sue_platform::{CursorShape, FileDrop, InputEvent, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, Rect, ShellRequest, SueApplet, Ui, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "drop-zone";

/// One ingested (or rejected) file.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    name: String,
    detail: String,
    opened: Option<(AssetHandle, MediaClass)>,
    failed: bool,
}

/// The ingest inspector in the Left Dock.
#[derive(Debug, Default)]
pub struct DropZone {
    entries: Vec<Entry>,
}

/// Maps an ingest view onto the media class applets declare.
pub fn media_class(view: IngestView) -> MediaClass {
    match view {
        IngestView::Raster => MediaClass::Image,
        IngestView::Text => MediaClass::Text,
        IngestView::Bytes => MediaClass::Binary,
    }
}

/// `1.4 MiB`, `812 KiB`, `96 B`.
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

impl DropZone {
    /// Reads, sniffs and ingests the file at `path`, then asks the shell to
    /// open it.
    fn ingest(&mut self, path: &Path, ctx: &mut AppletContext<'_>) {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let limit_mib = ctx
            .rice()
            .get_i64("io.max_ingest_mib")
            .unwrap_or(512)
            .max(1);
        let limit = u64::try_from(limit_mib)
            .unwrap_or(u64::MAX)
            .saturating_mul(1024 * 1024);
        let entry = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => Err("folders are not ingested yet".to_owned()),
            Ok(metadata) if metadata.len() > limit => Err(format!(
                "{} is over the {limit_mib} MiB limit (io.max_ingest_mib)",
                human_bytes(metadata.len())
            )),
            Ok(_) => std::fs::read(path).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
        .and_then(|bytes| {
            let width = ctx
                .rice()
                .get_i64("io.byte_map_width")
                .and_then(|width| u32::try_from(width).ok())
                .unwrap_or(64);
            let size = bytes.len() as u64;
            let ingested = ingest_bytes(
                &mut ctx.world.arena,
                &name,
                bytes,
                &NativeJxlPipeline,
                IngestOptions {
                    byte_map_width: width,
                },
            )
            .map_err(|error| error.to_string())?;
            if let Ok(entity) = ctx.world.arena.get_mut(ingested.handle) {
                entity.set_meta("source", path.display().to_string());
            }
            let class = media_class(ingested.view);
            ctx.request(ShellRequest::OpenAsset {
                handle: ingested.handle,
                class,
            });
            let route = ctx
                .arena()
                .get(ingested.handle)
                .ok()
                .and_then(|entity| entity.meta(meta::ROUTE).map(str::to_owned))
                .unwrap_or_default();
            let mut detail = format!("{route} · {}", human_bytes(size));
            if let Some(note) = ingested.note {
                detail = format!("{detail} · {note}");
            }
            Ok(Entry {
                name: name.clone(),
                detail,
                opened: Some((ingested.handle, class)),
                failed: false,
            })
        });
        let entry = entry.unwrap_or_else(|reason| Entry {
            name,
            detail: reason,
            opened: None,
            failed: true,
        });
        self.entries.insert(0, entry);
    }
}

impl SueApplet for DropZone {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Drop Zone id is valid"),
            name: "Drop Zone".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Drop any file: JPEG XL decodes, text opens, bytes get mapped.".to_owned(),
            opens: Vec::new(),
            accepts_drops: true,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        match event {
            InputEvent::FileDrop(FileDrop::Dropped(path)) => {
                self.ingest(path, ctx);
                Ok(InputResponse::Consumed { redraw: true })
            }
            _ => Ok(InputResponse::Ignored),
        }
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let line = ui.row_height();
        let target = Rect::new(area.x, area.y, area.width, (line * 2.6).min(area.height));
        let hovering = ui.input.file_hover;
        ui.canvas.draw_box(
            target,
            BoxStyle {
                fill: if hovering {
                    theme.accent.fade(0.15)
                } else {
                    theme.background
                },
                border: if hovering {
                    theme.accent
                } else {
                    theme.muted.fade(0.6)
                },
                border_width: theme.border_width.max(1.0),
                radius: theme.radius,
            },
        );
        ui.label_centered(
            Rect::new(target.x, target.y, target.width, target.height * 0.6),
            "Drop files onto Sue",
            theme.text,
        );
        ui.label_centered(
            Rect::new(
                target.x,
                target.y + target.height * 0.45,
                target.width,
                target.height * 0.5,
            ),
            "any format · sniffed by content",
            theme.muted,
        );
        let mut y = target.bottom() + theme.padding() / 2.0;
        for (index, entry) in self.entries.iter().enumerate() {
            if y + line * 1.6 > area.bottom() {
                break;
            }
            let row = Rect::new(area.x, y, area.width, line * 1.6);
            let response = ui.interact(widget_id("drop-zone/entry", &index.to_string()), row);
            if response.hovered && entry.opened.is_some() {
                ui.canvas
                    .draw_box(row, BoxStyle::filled(theme.hover, theme.radius / 2.0));
                ui.set_cursor(CursorShape::Pointer);
            }
            let name_color = if entry.failed {
                theme.alert
            } else {
                theme.text
            };
            ui.label(
                Rect::new(row.x + 4.0, row.y, row.width - 8.0, line * 0.9),
                &entry.name,
                name_color,
            );
            let mono = theme.mono_size * 0.9;
            let shown = ui
                .fonts
                .elide(FontFace::Mono, mono, &entry.detail, row.width - 8.0);
            ui.text(
                Point::new(row.x + 4.0, row.y + line * 0.8),
                &shown,
                FontFace::Mono,
                mono,
                theme.muted,
            );
            if response.clicked
                && let Some((handle, class)) = entry.opened
                && ctx.arena().contains(handle)
            {
                ctx.request(ShellRequest::OpenAsset { handle, class });
            }
            y += line * 1.7;
        }
    }
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
    fn views_map_to_media_classes() {
        assert_eq!(media_class(IngestView::Raster), MediaClass::Image);
        assert_eq!(media_class(IngestView::Text), MediaClass::Text);
        assert_eq!(media_class(IngestView::Bytes), MediaClass::Binary);
    }
}
