//! 3D layers, drawing figures and their poses, and lines and tones
//! from 3D.

use std::path::Path;

use sue_core::txn::{ReplaceComponent, SetProperty};
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
                    crate::figure::Kind::Imported => tr!("figure-kind-model"),
                };
                tr!("pose-material-name", kind = kind, number = number)
            })
            .find(|name| !taken.contains(name))
            .unwrap_or_default();
        match crate::materials::register_pose(&folder, &name, body.kind, &pose) {
            Ok(_) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.show_material(crate::materials::POSE, name);
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
        if material.kind == crate::materials::BODY {
            self.apply_body_material(cx, material);
            return;
        }
        let Some((kind, pose)) = crate::materials::pose_of(material) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        use crate::figure::{Body, Kind, Pose, Side};
        // The figure on the current layer, with its pose and body.
        let current = self.layer().and_then(|layer| {
            let model = crate::figure::of_layer(&cx.world.arena, layer)
                .and_then(|model| cx.world.arena.get_by_id(model).ok())?;
            Some((layer, Pose::of(model), Body::of(model)))
        });
        // How the pose lies on that figure: as it is on one of its own
        // kind; a hand pose on a body's hands (the one whose arm is
        // picked, else both); and any other by the joints that share a
        // name (a body pose on an imported model with the same bones).
        let fitted = current.and_then(|(layer, now, body)| {
            if body.kind == kind {
                return Some((layer, pose.clone()));
            }
            if kind == Kind::Hand && body.kind == Kind::Body {
                let picked = self.figure_joint.and_then(|joint| {
                    let joints = body.joints();
                    let name = joints.get(joint)?.name;
                    if name.starts_with("Left") {
                        Some(Side::Left)
                    } else if name.starts_with("Right") {
                        Some(Side::Right)
                    } else {
                        None
                    }
                });
                let mut next = now;
                for side in [Side::Left, Side::Right] {
                    if picked.is_none_or(|picked| picked == side) {
                        crate::figure::hand_onto_body(&body, &mut next, side, &pose);
                    }
                }
                return Some((layer, next));
            }
            let from = Body {
                kind,
                ..Body::default()
            }
            .joints();
            crate::figure::retargeted(&pose, &from, &body.joints(), &now).map(|next| (layer, next))
        });
        if let Some((layer, next)) = fitted {
            if let Some(edit) =
                crate::figure::set_pose(&cx.world.arena, layer, &next, &material.name)
            {
                Self::apply(cx, edit);
            }
            return;
        }
        let model = crate::figure::new_of(&material.name, kind)
            .with_property(crate::figure::POSE, Value::from(pose.to_text()));
        self.add_3d_layer(cx, &material.name, model);
    }

    /// Shapes the current drawing figure as body shape material
    /// `material`, as one step, else makes a new figure of that shape.
    fn apply_body_material(
        &mut self,
        cx: &mut AppletContext<'_>,
        material: &crate::materials::Material,
    ) {
        let Some(shaped) = crate::materials::body_of(material) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        let current = self.layer().filter(|layer| {
            crate::figure::of_layer(&cx.world.arena, *layer)
                .and_then(|model| cx.world.arena.get_by_id(model).ok())
                .is_some_and(|model| {
                    crate::figure::Body::of(model).kind == crate::figure::Kind::Body
                })
        });
        if current.is_some() {
            self.shape_figure(cx, &material.name, |now| {
                now.with_shape_text(&shaped.shape_text())
            });
            return;
        }
        let model = crate::figure::new_model(&material.name);
        let model = body_values(&shaped)
            .into_iter()
            .fold(model, |model, (key, value)| model.with_property(key, value));
        self.add_3d_layer(cx, &material.name, model);
    }

    /// Keeps the body shape of the figure on the current 3D layer as a body
    /// shape material, to use again (Clip Studio Paint's Register body
    /// shape as material).
    pub(crate) fn register_body_shape(&mut self, cx: &mut AppletContext<'_>) {
        let (Some(folder), Some(layer)) = (crate::materials::location(cx), self.layer()) else {
            return;
        };
        let Some(body) = crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
            .map(crate::figure::Body::of)
            .filter(|body| body.kind == crate::figure::Kind::Body)
        else {
            cx.notify(tr!("pick-a-drawing-figure-layer"));
            return;
        };
        let taken: Vec<String> = crate::materials::library(Some(&folder))
            .into_iter()
            .filter(|material| material.kind == crate::materials::BODY)
            .map(|material| material.name)
            .collect();
        let name = (1..)
            .map(|number| tr!("body-shape-material-name", number = number))
            .find(|name| !taken.contains(name))
            .unwrap_or_default();
        match crate::materials::register_body(&folder, &name, &body) {
            Ok(_) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.show_material(crate::materials::BODY, name);
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Opens the Material palette on kind `kind` with `name` picked.
    fn show_material(&mut self, kind: &str, name: String) {
        self.panels.color_set = true;
        self.set_tab = 2;
        self.material_kind = self
            .material_kinds
            .iter()
            .position(|known| known.id == kind)
            .map_or(0, |index| index as u8 + 1);
        self.hub.materials_changed();
        self.material_pick = Some(name);
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

    /// Imports the model at `path` (glTF, GLB or OBJ) as a 3D layer: the
    /// file is kept inside the model, so the picture needs nothing
    /// beside it, and kept among the 3D object materials to use again.
    pub(super) fn import_3d(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path
            .file_stem()
            .map_or_else(|| tr!("model"), |stem| stem.to_string_lossy().into_owned());
        match crate::materials::model_file(path) {
            Ok(model) => {
                self.add_3d_layer(cx, &name, model);
                if let Some(folder) = crate::materials::location(cx)
                    && crate::materials::register_model(&folder, path).is_ok()
                {
                    self.hub.materials_changed();
                }
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// Makes the figure on the current layer another of Sue's models,
    /// as one step.
    pub(crate) fn set_figure_model(
        &mut self,
        cx: &mut AppletContext<'_>,
        kind: crate::figure::Kind,
        build: crate::figure::Build,
        side: crate::figure::Side,
    ) {
        let Some(layer) = self.layer() else {
            return;
        };
        if let Some(edit) = crate::figure::set_model(
            &cx.world.arena,
            layer,
            kind,
            build,
            side,
            &tr!("change-model"),
        ) {
            self.figure_joint = None;
            Self::apply(cx, edit);
        } else {
            cx.notify(tr!("pick-a-drawing-figure-layer"));
        }
    }

    /// Pins or frees the picked hand or foot of the figure on the current
    /// layer (the end of the limb the picked joint belongs to), as one
    /// step: a pinned hand or foot stays where it is while the rest of
    /// the body is posed.
    pub(crate) fn toggle_pin(&mut self, cx: &mut AppletContext<'_>) {
        let Some((model, end)) = self.layer().and_then(|layer| {
            let model = crate::figure::of_layer(&cx.world.arena, layer)?;
            let body = crate::figure::Body::of(cx.world.arena.get_by_id(model).ok()?);
            let limb = crate::figure::limb_of(&body, self.figure_joint?)?;
            Some((model, limb[2]))
        }) else {
            cx.notify(tr!("pick-a-hand-or-foot-to-pin"));
            return;
        };
        let mut pins = cx
            .world
            .arena
            .get_by_id(model)
            .map(crate::figure::pins_of)
            .unwrap_or_default();
        match pins.iter().position(|pin| *pin == end) {
            Some(at) => {
                pins.remove(at);
            }
            None => pins.push(end),
        }
        let edit = doc::Edit::new(tr!("pin-joint")).then(SetProperty::new(
            model,
            crate::figure::PINS,
            crate::figure::pins_text(&pins),
        ));
        Self::apply(cx, edit);
    }

    /// Shows 3D layer `layer` as `view` sees it, in place, with the object
    /// placement and body shape Tool Property asked for this frame (while
    /// it is being changed; [`Self::finish_view3d`] records it). The first
    /// change of a drag keeps the layer and its model as they were.
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
        self.begin_draft(&cx.world.arena, layer);
        let model = crate::three::model_of(&cx.world.arena, layer);
        let place = self.pending_place.take().filter(|(id, _)| *id == layer);
        let shape = self.pending_shape.take().filter(|(id, _)| *id == layer);
        if let Some(model) = model
            && let Ok(entity) = cx.world.arena.get_by_id_mut(model)
        {
            let properties = entity.properties_mut();
            if let Some((_, place)) = place {
                for (key, value) in place.values() {
                    properties.set(key, value);
                }
            }
            if let Some((_, body)) = shape {
                for (key, value) in body_values(&body) {
                    properties.set(key, value);
                }
            }
        }
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            let properties = entity.properties_mut();
            for (key, value) in view.values() {
                properties.set(key, value);
            }
        }
        if let Some(draft) = self.three_draft.as_mut() {
            draft.shown = shown_now(&cx.world.arena, layer, draft.model);
        }
        // Quickly while it is dragged; finely when it is let go.
        let quick = crate::three::View3D {
            fast: true,
            ..*view
        };
        let Some(drawn) = crate::three::buffers_of(&cx.world.arena, layer, &quick, size)
            .and_then(|buffers| crate::three::draw(&buffers, &quick))
        else {
            return;
        };
        if let (Some(into), Some(from)) = (
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut),
            drawn.bytes(),
        ) && into.len() == from.len()
        {
            into.copy_from_slice(from);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Keeps 3D layer `layer` and its model as they are now, before a drag
    /// changes them in place, unless a drag of the same layer already did.
    ///
    /// A draft left by a drag that never ended (a gesture taken away, a
    /// second finger) is dropped when anything has changed the layer or
    /// its model since that drag last showed them (an undo, a command),
    /// so a drag never records against a state that is gone.
    fn begin_draft(&mut self, arena: &sue_core::Arena, layer: EntityId) {
        let model = crate::three::model_of(arena, layer);
        if self.three_draft.as_ref().is_some_and(|draft| {
            draft.layer == layer
                && draft.model == model
                && draft.shown == shown_now(arena, layer, model)
        }) {
            return;
        }
        self.three_draft = Some(Draft3d {
            layer,
            model,
            layer_props: snapshot(arena, layer, &view_keys()),
            model_props: model
                .map(|model| snapshot(arena, model, &MODEL_KEYS))
                .unwrap_or_default(),
            pixels: doc::raster(arena, layer)
                .and_then(|raster| raster.bytes())
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            shown: shown_now(arena, layer, model),
        });
    }

    /// Shows the drawing figure of 3D layer `layer` posed as `pose`
    /// (and, when its whole body is being moved, standing at `stands`),
    /// in place (while it is being posed; [`Self::finish_pose`] records
    /// it).
    pub(crate) fn preview_pose(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        pose: &crate::figure::Pose,
        stands: Option<[f32; 3]>,
    ) {
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer) else {
            return;
        };
        if let Ok(entity) = cx.world.arena.get_by_id_mut(model) {
            entity
                .properties_mut()
                .set(crate::figure::POSE, pose.to_text());
            if let Some(stands) = stands {
                stand_at(entity, stands);
            }
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
        let view = crate::three::View3D { fast: true, ..view };
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
    /// `before` (its pixels) and, when its whole body was moved, to
    /// where it `stood`, then the new pose and place.
    pub(crate) fn finish_pose(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        base: &crate::figure::Pose,
        before: &[u8],
        stood: Option<[Option<f32>; 3]>,
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
        let stands = cx
            .world
            .arena
            .get_by_id(model)
            .ok()
            .map(|entity| crate::three::Placement::of(entity).position);
        // Where it was moved to, when it was moved at all.
        let went = stood
            .zip(stands)
            .filter(|(stood, stands)| stood.map(|value| value.unwrap_or(0.0)) != *stands)
            .map(|(_, stands)| stands);
        if now == *base && went.is_none() {
            return;
        }
        if let Ok(entity) = cx.world.arena.get_by_id_mut(model) {
            entity
                .properties_mut()
                .set(crate::figure::POSE, base.to_text());
            if let (Some(stood), Some(_)) = (stood, went) {
                stand_back(entity, stood);
            }
        }
        if let Some(into) =
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
            && into.len() == before.len()
        {
            into.copy_from_slice(before);
        }
        let edit = match went {
            Some(stands) => crate::figure::set_pose_placed(
                &cx.world.arena,
                layer,
                &now,
                stands,
                &tr!("figure-move"),
            ),
            None => crate::figure::set_pose(&cx.world.arena, layer, &now, &tr!("pose")),
        };
        if let Some(edit) = edit {
            Self::apply(cx, edit);
        }
    }

    /// Records a drag of a 3D layer's camera, its object's placement or a
    /// figure's body shape as one step: everything back as it was before
    /// the drag (exactly, properties it did not have included), then the
    /// properties that changed and the layer drawn anew. `base` and
    /// `before` are the view and pixels the drag began from, used when no
    /// draft was kept.
    pub(crate) fn finish_view3d(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: EntityId,
        base: crate::three::View3D,
        before: &[u8],
    ) {
        let Some(draft) = self.three_draft.take().filter(|draft| draft.layer == layer) else {
            self.finish_view3d_plain(cx, layer, base, before);
            return;
        };
        let arena = &cx.world.arena;
        let Some(raster) = doc::raster(arena, layer) else {
            return;
        };
        let size = [raster.width(), raster.height()];
        let Some(view) = arena.get_by_id(layer).ok().map(crate::three::View3D::of) else {
            return;
        };
        let drawn = crate::three::buffers_of(arena, layer, &view, size)
            .and_then(|buffers| crate::three::draw(&buffers, &view));
        let layer_now = snapshot(arena, layer, &view_keys());
        let model_now = draft
            .model
            .map(|model| snapshot(arena, model, &MODEL_KEYS))
            .unwrap_or_default();
        let changed = |was: &[(&'static str, Option<Value>)],
                       now: Vec<(&'static str, Option<Value>)>| {
            now.into_iter()
                .zip(was.iter())
                .filter(|((_, now), (_, was))| now != was)
                .filter_map(|((key, now), _)| now.map(|now| (key, now)))
                .collect::<Vec<_>>()
        };
        let layer_changed = changed(&draft.layer_props, layer_now);
        let model_changed = changed(&draft.model_props, model_now);
        // Back as it was.
        restore(&mut cx.world.arena, layer, &draft.layer_props);
        if let Some(model) = draft.model {
            restore(&mut cx.world.arena, model, &draft.model_props);
        }
        if let Some(into) =
            doc::raster_mut(&mut cx.world.arena, layer).and_then(sue_core::RasterTile2D::bytes_mut)
            && into.len() == draft.pixels.len()
        {
            into.copy_from_slice(&draft.pixels);
        }
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
        let (Some(drawn), false) = (drawn, layer_changed.is_empty() && model_changed.is_empty())
        else {
            return;
        };
        let shaped = model_changed.iter().any(|(key, _)| BODY_KEYS.contains(key));
        let label = if shaped {
            tr!("body-shape")
        } else if model_changed.is_empty() {
            tr!("move-camera")
        } else {
            tr!("place-object")
        };
        let mut edit = doc::Edit::new(&label);
        for (key, value) in layer_changed {
            edit = edit.then(SetProperty::new(layer, key, value));
        }
        if let Some(model) = draft.model {
            for (key, value) in model_changed {
                edit = edit.then(SetProperty::new(model, key, value));
            }
        }
        Self::apply(cx, edit.then(ReplaceComponent::new(layer, drawn, &label)));
    }

    /// [`Self::finish_view3d`] without a draft: back to `base` and
    /// `before`, then the view now as one step.
    fn finish_view3d_plain(
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
        if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
            let properties = entity.properties_mut();
            for (key, value) in base.values() {
                properties.set(key, value);
            }
        }
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

    /// The current layer when it is a 3D layer, and the model it shows.
    pub(crate) fn object_3d(&self, arena: &sue_core::Arena) -> Option<(EntityId, EntityId)> {
        let layer = self
            .layer()
            .filter(|layer| crate::three::is_3d(arena, *layer))?;
        Some((layer, crate::three::model_of(arena, layer)?))
    }

    /// Places the object of the current 3D layer as `next` says, given its
    /// placement now and its model, as one step named `label`. `None`
    /// leaves it.
    pub(crate) fn place_object(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        next: impl FnOnce(crate::three::Placement, &sue_core::Entity) -> Option<crate::three::Placement>,
    ) {
        let Some((layer, model)) = self.object_3d(&cx.world.arena) else {
            cx.notify(tr!("pick-a-3d-layer"));
            return;
        };
        let Ok(entity) = cx.world.arena.get_by_id(model) else {
            return;
        };
        let now = crate::three::Placement::of(entity);
        let Some(next) = next(now, entity).filter(|next| *next != now) else {
            return;
        };
        if let Some(edit) = crate::three::set_placement(&cx.world.arena, layer, &next, label) {
            Self::apply(cx, edit);
        }
    }

    /// Put on the ground: the object lifted or lowered so its lowest point
    /// touches the ground, as one step.
    pub(crate) fn ground_object(&mut self, cx: &mut AppletContext<'_>) {
        self.place_object(cx, &tr!("put-on-ground"), |place, model| {
            let low = crate::three::lowest(model, &place)?;
            let mut grounded = place;
            grounded.position[1] -= low;
            Some(grounded)
        });
    }

    /// Shapes the drawing figure of the current layer as `next` says,
    /// given its body now, as one step named `label`.
    pub(crate) fn shape_figure(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        next: impl FnOnce(crate::figure::Body) -> Option<crate::figure::Body>,
    ) {
        let Some(layer) = self.layer() else {
            return;
        };
        let Some(model) = crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
        else {
            cx.notify(tr!("pick-a-drawing-figure-layer"));
            return;
        };
        let now = crate::figure::Body::of(model);
        let Some(next) = next(now).filter(|next| *next != now) else {
            return;
        };
        if let Some(edit) = crate::figure::set_body(&cx.world.arena, layer, &next, label) {
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

/// A 3D layer and its model as they were before a drag changed them in
/// place: the properties a drag may change (`None` where one was not
/// set) and the layer's pixels.
#[derive(Clone, Debug)]
pub(crate) struct Draft3d {
    layer: EntityId,
    model: Option<EntityId>,
    layer_props: Vec<(&'static str, Option<Value>)>,
    model_props: Vec<(&'static str, Option<Value>)>,
    pixels: Vec<u8>,
    /// The properties as the drag last showed them, to tell a draft whose
    /// drag never ended from one still going.
    shown: Shown,
}

/// A 3D layer's view properties and its model's, as [`snapshot`] reads
/// them.
type Shown = (
    Vec<(&'static str, Option<Value>)>,
    Vec<(&'static str, Option<Value>)>,
);

/// What [`Draft3d::shown`] compares: layer `layer` and model `model` now.
fn shown_now(arena: &sue_core::Arena, layer: EntityId, model: Option<EntityId>) -> Shown {
    (
        snapshot(arena, layer, &view_keys()),
        model
            .map(|model| snapshot(arena, model, &MODEL_KEYS))
            .unwrap_or_default(),
    )
}

/// The properties of a figure's body shape.
const BODY_KEYS: [&str; 9] = [
    crate::figure::GROUND,
    crate::figure::HEIGHT,
    crate::figure::HEADS,
    crate::figure::BUILD,
    crate::figure::SHAPE,
    crate::figure::FOLLOW,
    crate::figure::PARTS_TEXT,
    crate::figure::SIDE,
    crate::figure::GUIDES,
];

/// The properties of a 3D layer's model a drag may change: its placement
/// and a figure's body shape.
const MODEL_KEYS: [&str; 16] = [
    crate::figure::GROUND,
    crate::three::OBJECT_X,
    crate::three::OBJECT_Y,
    crate::three::OBJECT_Z,
    crate::three::OBJECT_YAW,
    crate::three::OBJECT_PITCH,
    crate::three::OBJECT_ROLL,
    crate::three::OBJECT_SCALE,
    crate::figure::HEIGHT,
    crate::figure::HEADS,
    crate::figure::BUILD,
    crate::figure::SHAPE,
    crate::figure::FOLLOW,
    crate::figure::PARTS_TEXT,
    crate::figure::SIDE,
    crate::figure::GUIDES,
];

/// The properties of a 3D layer's view.
fn view_keys() -> Vec<&'static str> {
    crate::three::View3D::default()
        .values()
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

/// A body shape as the properties of its model.
fn body_values(body: &crate::figure::Body) -> [(&'static str, Value); 9] {
    [
        (crate::figure::GROUND, Value::from(body.ground)),
        (crate::figure::HEIGHT, Value::from(f64::from(body.height))),
        (crate::figure::HEADS, Value::from(f64::from(body.heads))),
        (crate::figure::BUILD, Value::from(body.build.key())),
        (crate::figure::SHAPE, Value::from(body.shape.to_text())),
        (crate::figure::FOLLOW, Value::from(body.follow)),
        (crate::figure::PARTS_TEXT, Value::from(body.parts_text())),
        (crate::figure::SIDE, Value::from(body.side.key())),
        (crate::figure::GUIDES, Value::from(body.guides)),
    ]
}

/// The values of `keys` on entity `id` now.
fn snapshot(
    arena: &sue_core::Arena,
    id: EntityId,
    keys: &[&'static str],
) -> Vec<(&'static str, Option<Value>)> {
    let entity = arena.get_by_id(id).ok();
    keys.iter()
        .map(|key| {
            (
                *key,
                entity.and_then(|entity| entity.property(key)).cloned(),
            )
        })
        .collect()
}

/// Puts the values `props` back on entity `id`, taking away those it did
/// not have.
fn restore(arena: &mut sue_core::Arena, id: EntityId, props: &[(&'static str, Option<Value>)]) {
    if let Ok(entity) = arena.get_by_id_mut(id) {
        let properties = entity.properties_mut();
        for (key, value) in props {
            match value {
                Some(value) => {
                    properties.set(*key, value.clone());
                }
                None => {
                    properties.remove(key);
                }
            }
        }
    }
}

/// Stands the figure model `entity` at `position`, in place (a preview;
/// the pose's undo step records it).
fn stand_at(entity: &mut sue_core::Entity, position: [f32; 3]) {
    for (key, value) in [
        (crate::three::OBJECT_X, position[0]),
        (crate::three::OBJECT_Y, position[1]),
        (crate::three::OBJECT_Z, position[2]),
    ] {
        entity.properties_mut().set(key, f64::from(value));
    }
}

/// Puts the figure model `entity` back where it `stood` before a
/// preview moved it: a place it had no property for (nothing had moved
/// it yet) loses the property again, so undo finds the model exactly
/// as it was.
fn stand_back(entity: &mut sue_core::Entity, stood: [Option<f32>; 3]) {
    for (key, value) in [
        (crate::three::OBJECT_X, stood[0]),
        (crate::three::OBJECT_Y, stood[1]),
        (crate::three::OBJECT_Z, stood[2]),
    ] {
        let _ = match value {
            Some(value) => entity.properties_mut().set(key, f64::from(value)),
            None => entity.properties_mut().remove(key),
        };
    }
}
