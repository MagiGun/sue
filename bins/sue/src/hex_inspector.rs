//! Hex Inspector: hex rows beside a spatial byte map of any file. The byte
//! map is the entity's `Gray8` raster (one pixel per byte); the hex rows read
//! the same buffer, so nothing is copied.

use sue_core::{AssetHandle, ComponentKind};
use sue_io::meta;
use sue_platform::{CursorShape, InputEvent, Key, NamedKey, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, Rect, SueApplet, Ui, World, Zone, widget_id,
};

use crate::drop_zone::human_bytes;

/// The applet id.
pub const ID: &str = "hex-inspector";

/// Bytes per hex row.
const ROW_BYTES: usize = 16;

/// Hex rows and a byte map over one entity.
#[derive(Debug, Default)]
pub struct HexInspector {
    asset: Option<AssetHandle>,
    first_row: usize,
    visible_rows: usize,
    hovered: Option<usize>,
}

/// One hex row: offset, 16 byte pairs, and printable ASCII.
fn hex_row(offset: usize, bytes: &[u8]) -> (String, String, String) {
    let mut hex = String::with_capacity(ROW_BYTES * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index == ROW_BYTES / 2 {
            hex.push(' ');
        }
        hex.push_str(&format!("{byte:02x} "));
    }
    let ascii = bytes
        .iter()
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '·'
            }
        })
        .collect();
    (format!("{offset:08x}"), hex, ascii)
}

impl HexInspector {
    fn data<'w>(&self, world: &'w World) -> Option<(&'w [u8], usize, u32)> {
        let entity = world.arena.get(self.asset?).ok()?;
        let tile = entity.component(ComponentKind::RasterTile2D)?.as_raster()?;
        let bytes = tile.bytes()?;
        let length = entity
            .meta(meta::BYTES)
            .and_then(|length| length.parse().ok())
            .unwrap_or(bytes.len())
            .min(bytes.len());
        Some((bytes, length, tile.width()))
    }

    fn rows(length: usize) -> usize {
        length.div_ceil(ROW_BYTES).max(1)
    }
}

impl SueApplet for HexInspector {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Hex Inspector id is valid"),
            name: "Hex Inspector".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Hex rows beside a spatial byte map of any file.".to_owned(),
            opens: vec![MediaClass::Binary],
            accepts_drops: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn open(&mut self, asset: AssetHandle, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        ctx.arena()
            .get(asset)?
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster()?.bytes())
            .ok_or_else(|| AppletError::new("the asset has no byte map"))?;
        self.asset = Some(asset);
        self.first_row = 0;
        Ok(())
    }

    fn title(&self, world: &World) -> Option<String> {
        let entity = world.arena.get(self.asset?).ok()?;
        Some(format!("Hex Inspector — {}", entity.name()))
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let InputEvent::Keyboard(key) = event else {
            return Ok(InputResponse::Ignored);
        };
        let Some((_, length, _)) = self.data(ctx.world) else {
            return Ok(InputResponse::Ignored);
        };
        if !key.pressed {
            return Ok(InputResponse::Ignored);
        }
        let last = Self::rows(length).saturating_sub(1);
        let page = self.visible_rows.max(1);
        self.first_row = match key.key {
            Key::Named(NamedKey::ArrowDown) => self.first_row + 1,
            Key::Named(NamedKey::ArrowUp) => self.first_row.saturating_sub(1),
            Key::Named(NamedKey::PageDown) => self.first_row + page,
            Key::Named(NamedKey::PageUp) => self.first_row.saturating_sub(page),
            Key::Named(NamedKey::Home) => 0,
            Key::Named(NamedKey::End) => last,
            _ => return Ok(InputResponse::Ignored),
        }
        .min(last);
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let Some((bytes, length, map_width)) = self.data(ctx.world) else {
            ui.label_centered(
                area,
                "Drop any file onto Sue to inspect its bytes",
                theme.muted,
            );
            return;
        };
        let handle = self.asset.unwrap_or(AssetHandle::NULL);
        let pad = theme.padding();
        let size = theme.mono_size;
        let line = ui.fonts.line_height(FontFace::Mono, size);
        let char_width = ui.fonts.measure(FontFace::Mono, size, "0");

        // Header: name, size, route and entropy.
        let entity = ctx.world.arena.get(handle).ok();
        let lookup = |key: &str| {
            entity
                .and_then(|entity| entity.meta(key))
                .unwrap_or("")
                .to_owned()
        };
        let header = format!(
            "{} · {} · {} · entropy {} bits/byte",
            entity.map_or("", |entity| entity.name()),
            human_bytes(length as u64),
            lookup(meta::ROUTE),
            lookup(meta::ENTROPY),
        );
        ui.label(
            Rect::new(area.x, area.y, area.width, line),
            &header,
            theme.text,
        );
        let note = lookup(meta::NOTE);
        let mut top = area.y + line;
        if !note.is_empty() {
            ui.label(
                Rect::new(area.x, top, area.width, line),
                &note,
                theme.accent,
            );
            top += line;
        }
        top += pad / 2.0;

        // Byte map on the right: the whole file, one pixel per byte. It takes
        // what the hex columns leave, and the ASCII column goes first when
        // space runs out.
        let rows_total = Self::rows(length);
        let map_rows = length.div_ceil(map_width as usize).max(1) as f32;
        let body_height = (area.bottom() - top).max(line);
        let full_width = char_width * (10.0 + ROW_BYTES as f32 * 3.0 + 3.0 + ROW_BYTES as f32);
        let map_budget = (area.width - full_width - pad).clamp(area.width * 0.12, area.width * 0.3);
        let show_ascii = area.width - map_budget - pad >= full_width;
        let map_scale = (body_height / map_rows)
            .min(map_budget / map_width as f32)
            .max(f32::MIN_POSITIVE);
        let map = Rect::new(
            area.right() - map_width as f32 * map_scale,
            top,
            map_width as f32 * map_scale,
            map_rows * map_scale,
        );
        ui.canvas
            .draw_box(map.shrink(-1.0), BoxStyle::filled(theme.background, 0.0));
        ui.canvas.draw_image(map, handle, false);

        let map_response = ui.interact(widget_id("hex-inspector", "map"), map);
        if map_response.hovered || map_response.dragging {
            ui.set_cursor(CursorShape::Pointer);
        }
        if (map_response.pressed || map_response.dragging)
            && let Some(pointer) = ui.input.pointer
        {
            let column =
                (((pointer.x - map.x) / map_scale).max(0.0) as usize).min(map_width as usize - 1);
            let row = ((pointer.y - map.y) / map_scale).max(0.0) as usize;
            let offset = (row * map_width as usize + column).min(length.saturating_sub(1));
            self.first_row = (offset / ROW_BYTES).saturating_sub(self.visible_rows / 2);
        }

        // Hex rows on the left.
        let hex_area = Rect::new(area.x, top, (map.x - area.x - pad).max(0.0), body_height);
        self.visible_rows = ((hex_area.height / line).floor() as usize).max(1);
        let wheel_rows = (-ui.wheel_in(hex_area) / line).round() as isize;
        let last = rows_total.saturating_sub(1);
        self.first_row = self.first_row.saturating_add_signed(wheel_rows).min(last);

        let view_start = self.first_row * ROW_BYTES;
        let view_end = ((self.first_row + self.visible_rows) * ROW_BYTES).min(length);
        let marker_top = map.y + (view_start / map_width as usize) as f32 * map_scale;
        let marker_bottom = map.y + view_end.div_ceil(map_width as usize) as f32 * map_scale;
        ui.canvas.draw_box(
            Rect::new(
                map.x - 2.0,
                marker_top,
                map.width + 4.0,
                (marker_bottom - marker_top).max(2.0),
            ),
            BoxStyle {
                fill: theme.accent.fade(0.12),
                border: theme.accent,
                border_width: theme.border_width.max(1.0),
                radius: 0.0,
            },
        );

        let offset_x = hex_area.x;
        let hex_x = offset_x + char_width * 10.0;
        let ascii_x = hex_x + char_width * (ROW_BYTES as f32 * 3.0 + 2.0);
        ui.canvas.push_clip(hex_area);
        self.hovered = None;
        for visible in 0..self.visible_rows {
            let row = self.first_row + visible;
            let start = row * ROW_BYTES;
            if start >= length {
                break;
            }
            let chunk = &bytes[start..(start + ROW_BYTES).min(length)];
            let y = hex_area.y + visible as f32 * line;
            let (offset, hex, ascii) = hex_row(start, chunk);
            if let Some(pointer) = ui.input.pointer
                && pointer.y >= y
                && pointer.y < y + line
                && pointer.x >= hex_x
                && pointer.x < ascii_x
            {
                let column = ((pointer.x - hex_x) / (char_width * 3.0)) as usize;
                if column < chunk.len() {
                    self.hovered = Some(start + column);
                    let gap = if column >= ROW_BYTES / 2 {
                        char_width
                    } else {
                        0.0
                    };
                    let cell = Rect::new(
                        hex_x + column as f32 * char_width * 3.0 + gap - 2.0,
                        y,
                        char_width * 2.0 + 4.0,
                        line,
                    );
                    ui.canvas
                        .draw_box(cell, BoxStyle::filled(theme.accent.fade(0.3), 2.0));
                }
            }
            ui.text(
                Point::new(offset_x, y),
                &offset,
                FontFace::Mono,
                size,
                theme.muted,
            );
            ui.text(Point::new(hex_x, y), &hex, FontFace::Mono, size, theme.text);
            if show_ascii {
                ui.text(
                    Point::new(ascii_x, y),
                    &ascii,
                    FontFace::Mono,
                    size,
                    theme.muted,
                );
            }
        }
        ui.canvas.pop_clip();

        if let Some(offset) = self.hovered {
            let byte = bytes[offset];
            let status =
                format!("offset 0x{offset:08x} ({offset}) = 0x{byte:02x} · {byte} · {byte:08b}");
            let width = ui.fonts.measure(FontFace::Mono, size, &status) + pad * 2.0;
            let tip = Rect::new(
                hex_area.x,
                area.bottom() - line - pad / 2.0,
                width,
                line + pad / 2.0,
            );
            ui.canvas
                .draw_box(tip, BoxStyle::filled(theme.raised, theme.radius / 2.0));
            ui.text(
                Point::new(tip.x + pad, tip.y + pad / 4.0),
                &status,
                FontFace::Mono,
                size,
                theme.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_rows_split_and_mask_unprintables() {
        let (offset, hex, ascii) = hex_row(0x20, b"Sue\0\x01\x7fABCDEFGHIJ");
        assert_eq!(offset, "00000020");
        assert!(hex.starts_with("53 75 65 00 01 7f 41 42  43"));
        assert_eq!(ascii, "Sue···ABCDEFGHIJ");
        assert_eq!(HexInspector::rows(0), 1);
        assert_eq!(HexInspector::rows(33), 3);
    }
}
