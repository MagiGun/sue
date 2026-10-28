//! Hex Inspector: hex rows beside a spatial byte map of any file, with a
//! cursor whose values (integers and floats, both byte orders) go to the
//! Inspector. The byte map is the entity's `Gray8` raster (one pixel per
//! byte); the hex rows read the same buffer, so nothing is copied.

use sue_core::{AssetHandle, ComponentKind};
use sue_io::meta;
use sue_platform::{CursorShape, InputEvent, Key, NamedKey, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, PropertyGroup, Rect, SueApplet, Ui, World, Zone, widget_id,
};

use crate::util::human_bytes;

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
    cursor: usize,
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
                '.'
            }
        })
        .collect();
    (format!("{offset:08x}"), hex, ascii)
}

/// The values the bytes at the cursor spell, for the Inspector.
fn cursor_values(bytes: &[u8], offset: usize) -> Vec<(&'static str, String)> {
    let array = |count: usize| bytes.get(offset..offset + count);
    let mut values = vec![
        ("Offset", format!("0x{offset:08x} ({offset})")),
        ("Byte", format!("0b{:08b}", bytes[offset])),
        ("uint8", bytes[offset].to_string()),
        ("int8", (bytes[offset] as i8).to_string()),
    ];
    let na = || "past the end".to_owned();
    values.push((
        "uint16 LE",
        array(2).map_or_else(na, |b| u16::from_le_bytes([b[0], b[1]]).to_string()),
    ));
    values.push((
        "int32 LE",
        array(4).map_or_else(na, |b| {
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string()
        }),
    ));
    values.push((
        "int32 BE",
        array(4).map_or_else(na, |b| {
            i32::from_be_bytes([b[0], b[1], b[2], b[3]]).to_string()
        }),
    ));
    values.push((
        "uint32 LE",
        array(4).map_or_else(na, |b| {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string()
        }),
    ));
    values.push((
        "float32 LE",
        array(4).map_or_else(na, |b| {
            format!("{:e}", f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        }),
    ));
    values.push((
        "float64 LE",
        array(8).map_or_else(na, |b| {
            let mut word = [0_u8; 8];
            word.copy_from_slice(b);
            format!("{:e}", f64::from_le_bytes(word))
        }),
    ));
    values
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

    /// Scrolls so the cursor's row is visible.
    fn follow_cursor(&mut self) {
        let row = self.cursor / ROW_BYTES;
        let page = self.visible_rows.max(1);
        if row < self.first_row {
            self.first_row = row;
        } else if row >= self.first_row + page {
            self.first_row = row + 1 - page;
        }
    }
}

impl SueApplet for HexInspector {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Hex Inspector id is valid"),
            name: "Hex Inspector".to_owned(),
            category: AppletCategory::Inspector,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Hex rows beside a spatial byte map of any file, with a cursor that reads numbers in both byte orders."
                .to_owned(),
            opens: vec![MediaClass::Binary],
            icon: "\u{1F522}".to_owned(),
            singleton: false,
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
        self.cursor = 0;
        Ok(())
    }

    fn title(&self, world: &World) -> Option<String> {
        Some(world.arena.get(self.asset?).ok()?.name().to_owned())
    }

    fn inspect(&self, world: &World) -> Vec<PropertyGroup> {
        let (Some(entity), Some((bytes, length, _))) = (
            self.asset.and_then(|asset| world.arena.get(asset).ok()),
            self.data(world),
        ) else {
            return Vec::new();
        };
        let lookup = |key: &str| entity.meta(key).unwrap_or("?").to_owned();
        let mut groups = vec![PropertyGroup::new(
            "File",
            [
                ("Name", entity.name().to_owned()),
                ("Size", human_bytes(length as u64)),
                ("Route", lookup(meta::ROUTE)),
                (
                    "Entropy",
                    format!("{} bits per byte", lookup(meta::ENTROPY)),
                ),
            ],
        )];
        if let Some(note) = entity.meta(meta::NOTE) {
            groups[0].properties.push(sue_wm::Property {
                name: "Note".to_owned(),
                value: note.to_owned(),
            });
        }
        if length > 0 {
            groups.push(PropertyGroup::new(
                "Cursor",
                cursor_values(&bytes[..length], self.cursor.min(length - 1)),
            ));
        }
        groups
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
        if !key.pressed || length == 0 {
            return Ok(InputResponse::Ignored);
        }
        let last = length - 1;
        let page = self.visible_rows.max(1) * ROW_BYTES;
        self.cursor = match key.key {
            Key::Named(NamedKey::ArrowRight) => self.cursor + 1,
            Key::Named(NamedKey::ArrowLeft) => self.cursor.saturating_sub(1),
            Key::Named(NamedKey::ArrowDown) => self.cursor + ROW_BYTES,
            Key::Named(NamedKey::ArrowUp) => self.cursor.saturating_sub(ROW_BYTES),
            Key::Named(NamedKey::PageDown) => self.cursor + page,
            Key::Named(NamedKey::PageUp) => self.cursor.saturating_sub(page),
            Key::Named(NamedKey::Home) => 0,
            Key::Named(NamedKey::End) => last,
            _ => return Ok(InputResponse::Ignored),
        }
        .min(last);
        self.follow_cursor();
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let Some((bytes, length, map_width)) = self.data(ctx.world) else {
            ui.label_centered(
                area,
                "Open or drop any file to inspect its bytes",
                theme.muted,
            );
            return;
        };
        let handle = self.asset.unwrap_or(AssetHandle::NULL);
        let pad = theme.padding;
        let area = area.shrink(pad);
        let size = theme.mono_size;
        let line = ui.fonts.line_height(FontFace::Mono, size);
        let char_width = ui.fonts.measure(FontFace::Mono, size, "0");
        let top = area.y;

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
            self.cursor = (row * map_width as usize + column).min(length.saturating_sub(1));
            self.first_row = (self.cursor / ROW_BYTES).saturating_sub(self.visible_rows / 2);
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
                border_width: theme.hairline(),
                radius: 0.0,
            },
        );

        let offset_x = hex_area.x;
        let hex_x = offset_x + char_width * 10.0;
        let ascii_x = hex_x + char_width * (ROW_BYTES as f32 * 3.0 + 2.0);
        let cell = |column: usize, y: f32| {
            let gap = if column >= ROW_BYTES / 2 {
                char_width
            } else {
                0.0
            };
            Rect::new(
                hex_x + column as f32 * char_width * 3.0 + gap - 2.0,
                y,
                char_width * 2.0 + 4.0,
                line,
            )
        };
        let page = ui.interact(widget_id("hex-inspector", "rows"), hex_area);
        if page.hovered {
            ui.set_cursor(CursorShape::Pointer);
        }
        ui.canvas.push_clip(hex_area);
        for visible in 0..self.visible_rows {
            let row = self.first_row + visible;
            let start = row * ROW_BYTES;
            if start >= length {
                break;
            }
            let chunk = &bytes[start..(start + ROW_BYTES).min(length)];
            let y = hex_area.y + visible as f32 * line;
            if (start..start + chunk.len()).contains(&self.cursor) {
                ui.canvas.draw_box(
                    cell(self.cursor - start, y),
                    BoxStyle::filled(theme.accent, theme.radius),
                );
            }
            if let Some(pointer) = ui.input.pointer
                && pointer.y >= y
                && pointer.y < y + line
                && pointer.x >= hex_x
                && pointer.x < ascii_x
            {
                let column = ((pointer.x - hex_x) / (char_width * 3.0)) as usize;
                if column < chunk.len() {
                    if start + column != self.cursor {
                        ui.canvas
                            .draw_box(cell(column, y), BoxStyle::filled(theme.hover, theme.radius));
                    }
                    if page.pressed {
                        self.cursor = start + column;
                    }
                }
            }
            let (offset, hex, ascii) = hex_row(start, chunk);
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
        assert_eq!(ascii, "Sue...ABCDEFGHIJ");
        assert_eq!(HexInspector::rows(0), 1);
        assert_eq!(HexInspector::rows(33), 3);
    }

    #[test]
    fn the_cursor_reads_numbers_in_both_byte_orders() {
        let bytes = [0x00, 0x00, 0x80, 0x3f, 0x01];
        let values = cursor_values(&bytes, 0);
        let value = |name: &str| {
            values
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        assert_eq!(value("float32 LE"), "1e0");
        assert_eq!(value("int32 BE"), "32831");
        assert_eq!(value("uint16 LE"), "0");
        assert_eq!(value("float64 LE"), "past the end");
        assert_eq!(cursor_values(&bytes, 4)[3].1, "1");
    }
}
