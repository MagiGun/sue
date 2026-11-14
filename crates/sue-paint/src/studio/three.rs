//! 3D layers, drawing figures and their poses, and lines and tones
//! from 3D.

use std::path::Path;

use sue_core::txn::SetProperty;
use sue_core::{EntityId, Value};
use sue_model::picture;
use sue_wm::AppletContext;

use super::SuePaint;
use crate::doc;
use crate::filters::Filter;

impl SuePaint {
    /// Keeps the pose of the figure, hand or head on the current 3D layer
    /// as a pose material, to use again (Clip Studio Paint's Register
    /// pose as material).
    pub(crate) fn register_pose(&mut self, cx: &mut AppletContext<'_>) {
        let (Some(folder), Some(layer)) = (crate::materials::location(cx), self.layer()) else {
            return;
        };
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
        else {
            cx.notify(tr!("pick-a-drawing-figure-layer"));
            return;
        };
        let (pose, body) = (
            crate::figure::Pose::of(model),
            crate::figure::Body::of(model),
        );
        // The next free name for this kind.
        let taken: Vec<String> = crate::materials::library(Some(&folder))
            .into_iter()
            .filter(|material| material.kind == crate::materials::POSE)
            .map(|material| material.name)
            .collect();
        let name = (1..)
            .map(|number| {
                let kind = match body.kind {
                    crate::figure::Kind::Body => tr!("figure-kind-body"),
                    crate::figure::Kind::Hand => tr!("figure-kind-hand"),
                    crate::figure::Kind::Head => tr!("figure-kind-head"),
                };
                tr!("pose-material-name", kind = kind, number = number)
            })
            .find(|name| !taken.contains(name))
            .unwrap_or_default();
        match crate::materials::register_pose(&folder, &name, body.kind, &pose) {
            Ok(_) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.panels.color_set = true;
                self.set_tab = 2;
                self.material_kind = self
                    .material_kinds
                    .iter()
                    .position(|kind| kind.id == crate::materials::POSE)
                    .map_or(0, |index| index as u8 + 1);
                self.start.materials_changed();
                self.material_pick = Some(name);
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Puts pose material `material` on the current figure, hand or head
    /// when it is of the pose's kind, else on a new one of that kind.
    pub(super) fn apply_pose_material(
        &mut self,
        cx: &mut AppletContext<'_>,
        material: &crate::materials::Material,
    ) {
        let Some((kind, pose)) = crate::materials::pose_of(material) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        let current = self.layer().filter(|layer| {
            crate::figure::of_layer(&cx.world.arena, *layer)
                .and_then(|model| cx.world.arena.get_by_id(model).ok())
                .is_some_and(|model| crate::figure::Body::of(model).kind == kind)
        });
        if let Some(layer) = current {
            if let Some(edit) =
                crate::figure::set_pose(&cx.world.arena, layer, &pose, &material.name)
            {
                Self::apply(cx, edit);
            }
            return;
        }
        let model = crate::figure::new_of(&material.name, kind)
            .with_property(crate::figure::POSE, Value::from(pose.to_text()));
        self.add_3d_layer(cx, &material.name, model);
    }

    /// Adds a 3D layer showing `model` (a new model entity) above the
    /// current layer, as one step.
    pub(crate) fn add_3d_layer(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        model: sue_core::Entity,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        match crate::three::new_layer(&cx.world.arena, picture, self.layer(), name, model) {
            Ok((edit, id)) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
                self.toolbox.pick(crate::tools::Tool::Operation);
                self.toolbox.pick_sub(crate::tools::Tool::Operation, 0);
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Imports the model at `path` (OBJ) as a 3D layer.
    pub(super) fn import_3d(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        use sue_core::FormatHandler as _;
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path
            .file_stem()
            .map_or_else(|| tr!("model"), |stem| stem.to_string_lossy().into_owned());
        let bundle = sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                sue_io::ObjFormat
                    .import(&bytes, &name, &sue_core::Properties::new())
                    .map_err(|error| error.to_string())
            });
        match bundle.map(|bundle| bundle.entities.into_iter().next()) {
            Ok(Some(model)) => self.add_3d_layer(cx, &name, model),
            Ok(None) => cx.fail(tr!("file-has-no-model")),
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// Shows 3D layer `layer` as `view` sees it, in place (while it is being
    /// changed; [`Self::finish_view3d`] records it).
    pub(crate) fn preview_view3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        view: &crate::three::View3D,
    ) {
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(drawn) = crate::three::buffers_of(&cx.world.arena, layer, view, size)
            .and_then(|buffers| crate::three::draw(&buffers, view))
        else {
            return;
        };
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            let properties = entity.properties_mut();
            for (key, value) in [
                (crate::three::YAW, f64::from(view.yaw)),
                (crate::three::PITCH, f64::from(view.pitch)),
                (crate::three::DISTANCE, f64::from(view.distance)),
                (crate::three::FOV, f64::from(view.fov)),
                (crate::three::LIGHT, f64::from(view.light)),
                (crate::three::LINE_WIDTH, f64::from(view.line_width)),
                (crate::three::CREASE, f64::from(view.crease)),
                (crate::three::PAN_X, f64::from(view.pan[0])),
                (crate::three::PAN_Y, f64::from(view.pan[1])),
            ] {
                properties.set(key, value);
            }
            properties.set(crate::three::STYLE, i64::from(view.style));
        }
        if let (Some(into), Some(from)) = (
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut),
            drawn.bytes(),
        ) {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Shows the drawing figure of 3D layer `layer` posed as `pose`, in
    /// place (while it is being posed; [`Self::finish_pose`] records it).
    pub(crate) fn preview_pose(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        pose: &crate::figure::Pose,
    ) {
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer) else {
            return;
        };
        if let Ok(entity) = cx.world.arena.get_by_id_mut(model) {
            entity
                .properties_mut()
                .set(crate::figure::POSE, pose.to_text());
        }
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(view) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
        else {
            return;
        };
        let Some(drawn) = crate::three::buffers_of(&cx.world.arena, layer, &view, size)
            .and_then(|buffers| crate::three::draw(&buffers, &view))
        else {
            return;
        };
        if let (Some(into), Some(from)) = (
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut),
            drawn.bytes(),
        ) {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Records a figure's pose change as one step: back to `base` and
    /// `before` (its pixels), then the new pose.
    pub(crate) fn finish_pose(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        base: &crate::figure::Pose,
        before: &[u8],
    ) {
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer) else {
            return;
        };
        let Some(now) = cx
            .world
            .arena
            .get_by_id(model)
            .ok()
            .map(crate::figure::Pose::of)
        else {
            return;
        };
        if now == *base {
            return;
        }
        if let Ok(entity) = cx.world.arena.get_by_id_mut(model) {
            entity
                .properties_mut()
                .set(crate::figure::POSE, base.to_text());
        }
        if let Some(into) =
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
            && into.len() == before.len()
        {
            into.copy_from_slice(before);
        }
        if let Some(edit) = crate::figure::set_pose(&cx.world.arena, layer, &now, &tr!("pose")) {
            Self::apply(cx, edit);
        }
    }

    /// Records a 3D layer's view change as one step: back to `base` and
    /// `before` (its pixels), then the change.
    pub(crate) fn finish_view3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        base: crate::three::View3D,
        before: &[u8],
    ) {
        let Some(now) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
        else {
            return;
        };
        if now == base {
            return;
        }
        self.preview_view3d(cx, layer, &base);
        if let Some(into) =
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
            && into.len() == before.len()
        {
            into.copy_from_slice(before);
        }
        if let Some(edit) =
            crate::three::set_view(&cx.world.arena, layer, &now, &tr!("move-camera"))
        {
            Self::apply(cx, edit);
        }
    }

    /// Convert to Lines and Tones: the current layer's lines on a new layer
    /// (Line Extraction) and its shading on another as dots (a gray layer
    /// with the Tone effect on), the layer itself hidden, as one step.
    pub(crate) fn lines_and_tones(&mut self, cx: &mut AppletContext<'_>) {
        if let Some(layer) = self
            .layer()
            .filter(|layer| crate::three::is_3d(&cx.world.arena, *layer))
        {
            self.lines_and_tones_3d(cx, layer, false);
            return;
        }
        let (Some(picture), Some(info)) = (self.picture(), self.paintable(&cx.world.arena).ok())
        else {
            cx.fail(tr!("pick-raster-layer-convert-lines-tones"));
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let Some(size) = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
        else {
            return;
        };
        if [raster.width(), raster.height()] != size || info.style.offset != [0, 0] {
            cx.fail(tr!("move-layer-back-into-place-merge"));
            return;
        }
        let Some(bytes) = raster.bytes() else {
            return;
        };
        let mut lines = bytes.to_vec();
        Filter::LineExtraction {
            sensitivity: 0.5,
            black: 0.12,
            smooth: 1.0,
        }
        .apply(&mut lines, size[0], size[1], None, [0, 0]);
        // The shading: darker is more ink.
        let tones: Vec<u8> = bytes
            .chunks(4)
            .flat_map(|p| {
                let a = f32::from(p[3]) / 255.0;
                let l =
                    (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
                        / 255.0;
                let gray = (l * 255.0).round() as u8;
                [
                    gray,
                    gray,
                    gray,
                    ((1.0 - l) * a * 255.0).round().min(255.0) as u8,
                ]
            })
            .collect();
        let tile = |pixels: Vec<u8>| {
            sue_core::RasterTile2D::from_bytes(
                size[0],
                size[1],
                sue_core::PixelFormat::Rgba8,
                pixels,
            )
        };
        let (Ok(lines), Ok(tones)) = (tile(lines), tile(tones)) else {
            return;
        };
        let Ok((spawns, ids)) = doc::add_layers_pixels(
            &cx.world.arena,
            picture,
            Some(info.id),
            vec![(tr!("tones"), tones), (tr!("lines"), lines)],
        ) else {
            return;
        };
        let mut edit = doc::Edit::new(tr!("convert-to-lines-and-tones"));
        for spawn in spawns {
            edit = edit.then(spawn);
        }
        let edit = edit
            .then(SetProperty::new(ids[0], crate::effects::TONE, true))
            .then(SetProperty::new(info.id, picture::VISIBLE, false));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(ids[1]);
        }
    }

    /// Convert to Lines and Tones for a 3D layer: its outline and folds as
    /// lines, its shading as tone, each on a layer of its own, the 3D layer
    /// hidden, as one step (Clip Studio Paint's LT conversion of 3D).
    pub(crate) fn lines_and_tones_3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        vector: bool,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(raster) = doc::raster(&cx.world.arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(view) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .map(crate::three::View3D::of)
        else {
            return;
        };
        let Some(buffers) = crate::three::buffers_of(&cx.world.arena, layer, &view, size) else {
            cx.fail(tr!("n-3d-layer-has-no-model"));
            return;
        };
        let line = crate::three::lines(&buffers, view.line_width.max(1.0), view.crease);
        let lines: Vec<u8> = line
            .iter()
            .flat_map(|ink| [0, 0, 0, (ink * 255.0).round() as u8])
            .collect();
        let tones: Vec<u8> = (0..line.len())
            .flat_map(|at| {
                let shade = if buffers.covered(at) {
                    1.0 - buffers.light[at]
                } else {
                    0.0
                };
                [0, 0, 0, (shade.clamp(0.0, 1.0) * 255.0).round() as u8]
            })
            .collect();
        let tile = |pixels: Vec<u8>| {
            sue_core::RasterTile2D::from_bytes(
                size[0],
                size[1],
                sue_core::PixelFormat::Rgba8,
                pixels,
            )
        };
        // As vector lines: the drawn lines traced to their middles, each
        // an editable stroke on a vector layer.
        let traced = if vector {
            let traced = crate::vector::trace(&lines, size[0], size[1]);
            if traced.is_empty() {
                cx.fail(tr!("there-nothing-layer-trace"));
                return;
            }
            Some(traced)
        } else {
            None
        };
        let (Ok(lines), Ok(tones)) = (tile(lines), tile(tones)) else {
            return;
        };
        let rasters = if traced.is_some() {
            vec![(tr!("tones"), tones)]
        } else {
            vec![(tr!("tones"), tones), (tr!("lines"), lines)]
        };
        let Ok((spawns, mut ids)) =
            doc::add_layers_pixels(&cx.world.arena, picture, Some(layer), rasters)
        else {
            return;
        };
        let mut edit = doc::Edit::new(tr!("convert-to-lines-and-tones"));
        for spawn in spawns {
            edit = edit.then(spawn);
        }
        if let Some(traced) = traced {
            let Ok((spawn, id)) = doc::add_vector_lines(
                &cx.world.arena,
                picture,
                Some(ids[0]),
                tr!("lines"),
                &traced,
            ) else {
                return;
            };
            edit = edit.then(spawn);
            ids.push(id);
        }
        let edit = edit
            .then(SetProperty::new(ids[0], crate::effects::TONE, true))
            .then(SetProperty::new(layer, picture::VISIBLE, false));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(ids[1]);
        }
    }
}
