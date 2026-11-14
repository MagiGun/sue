//! Reading a `.clip`: from the layer rows of its database, through each
//! layer's mipmap to its stored bitmap, to [`Psd`] records.

use std::collections::HashMap;

use super::Clip;
use super::container::chunks;
use super::offscreen::{Bitmap, bitmap_of, decode_bitmap};
use super::schema::blend_of;
use crate::psd::{Psd, PsdKind, PsdLayer};
use crate::sqlite::{self, Table, Value};

/// A table's rows by `MainId`.
struct Rows<'a> {
    table: Option<&'a Table>,
    by_id: HashMap<i64, usize>,
}

impl<'a> Rows<'a> {
    fn new(tables: &'a [Table], name: &str) -> Self {
        let table = tables.iter().find(|table| table.name == name);
        let by_id = table
            .and_then(|table| {
                let key = table.column("MainId")?;
                Some(
                    table
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(i, (_, row))| Some((row.get(key)?.int()?, i)))
                        .collect(),
                )
            })
            .unwrap_or_default();
        Self { table, by_id }
    }

    fn get(&self, id: i64, column: &str) -> Option<&'a Value> {
        let table = self.table?;
        let row = &table.rows.get(*self.by_id.get(&id)?)?.1;
        row.get(table.column(column)?)
    }

    fn int(&self, id: i64, column: &str) -> i64 {
        self.get(id, column).and_then(Value::int).unwrap_or(0)
    }
}

struct Source<'a> {
    exta: HashMap<String, &'a [u8]>,
    layers: Rows<'a>,
    mipmaps: Rows<'a>,
    levels: Rows<'a>,
    offscreens: Rows<'a>,
    width: u32,
    height: u32,
}

impl Source<'_> {
    /// The bitmap and pixels of a layer's mipmap (`LayerRenderMipmap` or
    /// `LayerLayerMaskMipmap`).
    fn pixels(&self, mipmap: i64) -> Result<Option<(Bitmap, Vec<u8>)>, String> {
        if mipmap == 0 {
            return Ok(None);
        }
        let level = self.mipmaps.int(mipmap, "BaseMipmapInfo");
        let offscreen = self.levels.int(level, "Offscreen");
        let Some(attribute) = self
            .offscreens
            .get(offscreen, "Attribute")
            .and_then(Value::bytes)
        else {
            return Ok(None);
        };
        let Some(bitmap) = bitmap_of(attribute) else {
            return Err("a layer's bitmap description is broken".to_owned());
        };
        let id = self
            .offscreens
            .get(offscreen, "BlockData")
            .and_then(Value::bytes)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        let pixels = decode_bitmap(&bitmap, self.exta.get(&id).copied())?;
        Ok(Some((bitmap, pixels)))
    }

    /// The record of one pixel layer, cut to the canvas.
    #[allow(clippy::cast_possible_truncation)]
    fn layer(&self, id: i64) -> Result<Option<PsdLayer>, String> {
        let name = self
            .layers
            .get(id, "LayerName")
            .and_then(Value::text)
            .unwrap_or_default()
            .to_owned();
        // Correction layers keep only their mask as pixels, and gradient
        // and fill layers are drawn from their settings: Sue cannot redo
        // those yet, so they come in as empty layers in their places.
        let generated = ["FilterLayerInfo", "GradationFillInfo"]
            .iter()
            .any(|column| self.layers.get(id, column).and_then(Value::bytes).is_some());
        if generated {
            let mut layer = PsdLayer::pixels(&name, 0, 0, 0, 0, Vec::new());
            self.style(id, &mut layer);
            return Ok(Some(layer));
        }
        let Some((bitmap, mut pixels)) = self.pixels(self.layers.int(id, "LayerRenderMipmap"))?
        else {
            return Ok(None);
        };
        if bitmap.channels != 4 {
            return Ok(None);
        }
        let left =
            self.layers.int(id, "LayerOffsetX") + self.layers.int(id, "LayerRenderOffscrOffsetX");
        let top =
            self.layers.int(id, "LayerOffsetY") + self.layers.int(id, "LayerRenderOffscrOffsetY");
        // A paper layer draws in its own color.
        if self.layers.int(id, "DrawColorEnable") != 0 && self.layers.int(id, "LayerType") == 1584 {
            let channel = |column: &str| (self.layers.int(id, column) >> 24) as u8;
            let color = [
                channel("DrawColorMainRed"),
                channel("DrawColorMainGreen"),
                channel("DrawColorMainBlue"),
            ];
            for pixel in pixels.as_chunks_mut::<4>().0 {
                if pixel[3] > 0 {
                    pixel[..3].copy_from_slice(&color);
                }
            }
        }
        // A layer mask, when on, cuts the alpha.
        let mask_on = self.layers.int(id, "LayerVisibility") & 2 != 0;
        if mask_on
            && let Some((mask, values)) =
                self.pixels(self.layers.int(id, "LayerLayerMaskMipmap"))?
        {
            let mx = self.layers.int(id, "LayerOffsetX")
                + self.layers.int(id, "LayerMaskOffsetX")
                + self.layers.int(id, "LayerMaskOffscrOffsetX");
            let my = self.layers.int(id, "LayerOffsetY")
                + self.layers.int(id, "LayerMaskOffsetY")
                + self.layers.int(id, "LayerMaskOffscrOffsetY");
            for y in 0..bitmap.height {
                for x in 0..bitmap.width {
                    let (cx, cy) = (left + x as i64 - mx, top + y as i64 - my);
                    let value = if cx >= 0
                        && cy >= 0
                        && (cx as usize) < mask.width
                        && (cy as usize) < mask.height
                    {
                        values[cy as usize * mask.width + cx as usize]
                    } else {
                        mask.fill[0]
                    };
                    let a = &mut pixels[(y * bitmap.width + x) * 4 + 3];
                    *a = ((u16::from(*a) * u16::from(value) + 127) / 255) as u8;
                }
            }
        }
        // Cut to the canvas.
        let (cw, ch) = (i64::from(self.width), i64::from(self.height));
        let (x0, y0) = (left.max(0), top.max(0));
        let (x1, y1) = (
            (left + bitmap.width as i64).min(cw),
            (top + bitmap.height as i64).min(ch),
        );
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let mut cut = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let sy = (y0 - top) as usize + y;
            let sx = (x0 - left) as usize;
            let start = (sy * bitmap.width + sx) * 4;
            cut.extend_from_slice(&pixels[start..start + w * 4]);
        }
        let mut layer = PsdLayer::pixels(&name, x0 as i32, y0 as i32, w as u32, h as u32, cut);
        self.style(id, &mut layer);
        Ok(Some(layer))
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn style(&self, id: i64, layer: &mut PsdLayer) {
        let opacity = self.layers.int(id, "LayerOpacity").clamp(0, 256);
        layer.opacity = ((opacity * 255 + 128) / 256) as u8;
        layer.visible = self.layers.int(id, "LayerVisibility") & 1 != 0;
        layer.clip = self.layers.int(id, "LayerClip") != 0;
        layer.blend = blend_of(self.layers.int(id, "LayerComposite"));
    }

    /// The records of the folder `folder`'s children, bottom first.
    fn children(&self, folder: i64, depth: usize, out: &mut Vec<PsdLayer>) -> Result<(), String> {
        if depth > 64 {
            return Err("the folders nest too deep".to_owned());
        }
        let mut id = self.layers.int(folder, "LayerFirstChildIndex");
        let mut seen = 0;
        while id != 0 {
            seen += 1;
            if seen > 100_000 {
                return Err("the layer list loops".to_owned());
            }
            if self.layers.int(id, "LayerFolder") != 0 {
                out.push(PsdLayer::folder("</Layer group>", PsdKind::FolderEnd));
                self.children(id, depth + 1, out)?;
                let name = self
                    .layers
                    .get(id, "LayerName")
                    .and_then(Value::text)
                    .unwrap_or_default()
                    .to_owned();
                let open = self.layers.int(id, "LayerFolder") & 16 == 0;
                let mut record = PsdLayer::folder(&name, PsdKind::FolderOpen { open });
                self.style(id, &mut record);
                out.push(record);
            } else if let Some(layer) = self.layer(id)? {
                out.push(layer);
            }
            id = self.layers.int(id, "LayerNextIndex");
        }
        Ok(())
    }
}

/// Reads a Clip Studio file.
///
/// # Errors
/// When `bytes` is not a `.clip`, or its database or bitmaps are broken.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn decode_clip(bytes: &[u8]) -> Result<Clip, String> {
    let (exta, database) = chunks(bytes)?;
    let tables = sqlite::read(database)?;
    let canvas = tables
        .iter()
        .find(|table| table.name == "Canvas")
        .and_then(|table| table.rows.first().map(|(_, row)| (table, row)))
        .ok_or("the file has no canvas")?;
    let value = |column: &str| canvas.0.column(column).and_then(|i| canvas.1.get(i));
    let width = value("CanvasWidth")
        .and_then(Value::real)
        .unwrap_or(0.0)
        .round();
    let height = value("CanvasHeight")
        .and_then(Value::real)
        .unwrap_or(0.0)
        .round();
    if !(1.0..=300_000.0).contains(&width) || !(1.0..=300_000.0).contains(&height) {
        return Err(format!(
            "a canvas of {width} by {height} pixels is not valid"
        ));
    }
    let dpi = value("CanvasResolution")
        .and_then(Value::real)
        .unwrap_or(72.0);
    let root = value("CanvasRootFolder").and_then(Value::int).unwrap_or(0);
    let source = Source {
        exta,
        layers: Rows::new(&tables, "Layer"),
        mipmaps: Rows::new(&tables, "Mipmap"),
        levels: Rows::new(&tables, "MipmapInfo"),
        offscreens: Rows::new(&tables, "Offscreen"),
        width: width as u32,
        height: height as u32,
    };
    let mut layers = Vec::new();
    source.children(root, 0, &mut layers)?;
    let sue = tables
        .iter()
        .find(|table| table.name == "SueProject")
        .and_then(|table| {
            let data = table.column("Data")?;
            table.rows.first()?.1.get(data)?.bytes().map(<[u8]>::to_vec)
        });
    Ok(Clip {
        psd: Psd {
            width: width as u32,
            height: height as u32,
            layers,
            merged: Vec::new(),
        },
        dpi,
        sue,
    })
}
