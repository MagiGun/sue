//! Materials, brush tips made from images, imported brushes, and color
//! sets saved to the settings.

use std::path::Path;

use sue_wm::AppletContext;

use super::SuePaint;
use crate::doc;

impl SuePaint {
    /// Keeps the current layer as a picture material named `name`.
    pub(crate) fn register_material(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let (Some(folder), Some(layer)) = (crate::materials::location(cx), self.layer()) else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        match crate::materials::register(&folder, name, raster) {
            Ok(_) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.panels.color_set = true;
                self.set_tab = 2;
                self.material_kind = self
                    .material_kinds
                    .iter()
                    .position(|kind| kind.id == crate::materials::IMAGE)
                    .map_or(0, |index| index as u8 + 1);
                self.start.materials_changed();
                self.material_pick = Some(name.to_owned());
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Writes the color sets to Sue Paint's data folder.
    pub(crate) fn save_color_sets(&self, cx: &mut AppletContext<'_>) {
        if let Err(error) = self
            .color_sets
            .save(crate::colorsets::folder(cx).as_deref())
        {
            cx.fail(tr!("cannot-save-the-color-sets", error = error));
        }
    }

    /// Edit, Create Brush Tip from Image: the current layer (inside the
    /// selection) becomes a brush tip shape, kept in Sue Paint's data
    /// folder, and a new Brush sub tool paints with it.
    pub(crate) fn make_brush_tip(&mut self, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let (Some(layer), Some(selection)) = (self.layer(), self.selection_or_all(arena)) else {
            return;
        };
        let Some(raster) = doc::raster(arena, layer) else {
            return;
        };
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let (w, h) = (raster.width(), raster.height());
        let mask = selection.mask();
        let (mut min, mut max) = ([u32::MAX; 2], [0_u32; 2]);
        for y in 0..h {
            for x in 0..w {
                let index = (y * w + x) as usize;
                if bytes[index * 4 + 3] > 0 && mask.get(index).is_some_and(|cover| *cover > 0) {
                    min = [min[0].min(x), min[1].min(y)];
                    max = [max[0].max(x + 1), max[1].max(y + 1)];
                }
            }
        }
        if max[0] <= min[0] || max[1] <= min[1] {
            cx.fail(tr!("draw-tips-shape-layer-first"));
            return;
        }
        let (cw, ch) = (max[0] - min[0], max[1] - min[1]);
        let mut cut = Vec::with_capacity((cw * ch * 4) as usize);
        for y in min[1]..max[1] {
            for x in min[0]..max[0] {
                let index = (y * w + x) as usize;
                let mut pixel = [
                    bytes[index * 4],
                    bytes[index * 4 + 1],
                    bytes[index * 4 + 2],
                    bytes[index * 4 + 3],
                ];
                pixel[3] = ((u32::from(pixel[3]) * u32::from(mask[index])) / 255) as u8;
                cut.extend_from_slice(&pixel);
            }
        }
        // Coverage comes from the alpha: dark paint on clear.
        for pixel in cut.chunks_mut(4) {
            if pixel[3] == 255 {
                pixel[3] = 254;
            }
        }
        let name = tr!("tip", value = crate::brush::shapes().len() + 1);
        let Some(shape) = crate::brush::TipShape::from_rgba(&name, &cut, cw, ch) else {
            return;
        };
        if let Some(folder) = crate::materials::location(cx).map(|folder| folder.join("tips"))
            && let Ok(png) = sue_io::encode_png_rgba(cw, ch, &cut)
        {
            let _ = sue_platform::fs::create_dir_all(&folder)
                .and_then(|()| sue_platform::fs::write(folder.join(format!("{name}.png")), png));
        }
        let id = crate::brush::register_shape(shape);
        let tip = crate::brush::Tip {
            size: (cw.max(ch) as f32).clamp(8.0, 200.0),
            shape: id,
            spacing: 0.25,
            ..crate::brush::Tip::default()
        };
        self.toolbox.add_sub(crate::tools::SubTool::new(
            crate::tools::Tool::Brush,
            &name,
            0,
            tip,
        ));
        cx.notify(tr!("new-brush-sub-tool", name = name));
    }

    /// Add from downloads: the brushes in a file people downloaded (Clip
    /// Studio Paint's `.sut`, Photoshop's `.abr`, GIMP's `.gbr` and `.gih`,
    /// or a picture) become sub tools of the tool out (the Brush tool when
    /// it does not paint). Their tips are kept with the other brush tips in
    /// the materials location, so they come back next time.
    pub(crate) fn import_brushes(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let brushes = match sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| sue_io::brushes::read(&file, &bytes))
        {
            Ok(brushes) => brushes,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let tool = self.toolbox.tool();
        let tool = if tool.paints() && tool != crate::tools::Tool::Eraser {
            tool
        } else {
            crate::tools::Tool::Brush
        };
        let folder = crate::materials::location(cx).map(|folder| folder.join("tips"));
        let mut added = 0;
        for brush in brushes {
            let Some(tip) = crate::decoration::imported_tip(&brush, folder.as_deref()) else {
                continue;
            };
            self.toolbox
                .add_sub(crate::tools::SubTool::new(tool, &brush.name, 0, tip));
            added += 1;
        }
        if added == 0 {
            cx.fail(tr!("no-brush-in-file", file = file.as_str()));
        } else {
            cx.notify(tr!(
                "brushes-added-from",
                count = added,
                file = file.as_str()
            ));
        }
    }
}
