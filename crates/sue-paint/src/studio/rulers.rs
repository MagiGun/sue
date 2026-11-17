//! Rulers made from paths, and perspective rulers.

use sue_model::picture;
use sue_wm::AppletContext;

use super::SuePaint;

impl SuePaint {
    /// Adds `path` to the picture's curve, figure and pen rulers, as one
    /// step named `label`, and turns Snap to Ruler on.
    pub(crate) fn add_ruler_path(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: crate::rulers::Path,
        label: &str,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let mut rulers = crate::rulers::Rulers::of(&cx.world.arena, picture);
        rulers.paths.push(path);
        let text = rulers.text(crate::rulers::PATHS);
        let edit =
            crate::rulers::edit_shown(&cx.world.arena, picture, crate::rulers::PATHS, text, label);
        let sub = self.toolbox.current().clone();
        if self.apply_ruler(cx, edit, &sub, label)
            && let Some(doc) = self.doc_mut()
        {
            doc.snap_ruler = true;
            doc.show_rulers = true;
        }
    }

    /// Lays the curve ruler through the points placed so far.
    pub(crate) fn finish_curve(&mut self, cx: &mut AppletContext<'_>) {
        let points = std::mem::take(&mut self.curve_points);
        if points.len() >= 2 {
            // The Curve kind: through the points, or a Bezier curve they
            // pull.
            let bezier = self.toolbox.current().tool == crate::tools::Tool::Ruler
                && self.toolbox.current().kind > 0;
            let path = if bezier {
                crate::rulers::Path::bezier(&points)
            } else {
                crate::rulers::Path::curve(&points)
            };
            self.add_ruler_path(cx, path, &tr!("create-curve-ruler"));
        }
    }

    /// Lays a ruler with `edit`, as one undo step named `label`: on the
    /// layer being edited, or, when the sub tool's Create at editing
    /// layer is off, on a new layer of its own that the rulers then
    /// belong to and that becomes the current layer.
    pub(crate) fn apply_ruler(
        &mut self,
        cx: &mut AppletContext<'_>,
        edit: crate::doc::Edit,
        sub: &crate::tools::SubTool,
        label: &str,
    ) -> bool {
        if sub.tool != crate::tools::Tool::Ruler || sub.at_editing_layer {
            return Self::apply(cx, edit);
        }
        let Some(picture) = self.picture() else {
            return false;
        };
        let (spawn, layer) = match crate::doc::add_raster_layer(
            &cx.world.arena,
            picture,
            self.layer(),
            tr!("ruler-layer"),
            None,
        ) {
            Ok(made) => made,
            Err(error) => {
                cx.fail(error);
                return false;
            }
        };
        let whole = crate::doc::Edit::new(label)
            .then(spawn)
            .then(sue_core::txn::SetProperty::new(
                picture,
                crate::rulers::LAYER,
                sue_core::Value::Entity(layer),
            ))
            .then(edit);
        if !Self::apply(cx, whole) {
            return false;
        }
        self.sync_docs(&cx.world.arena);
        if let Some(doc) = self.doc_mut() {
            doc.layer = Some(layer);
        }
        true
    }

    /// Picks the Ruler tool's sub tool of `mode`.
    pub(crate) fn pick_ruler(&mut self, mode: u8) {
        use crate::tools::Tool;
        self.toolbox.pick(Tool::Ruler);
        if let Some(index) = self
            .toolbox
            .subs(Tool::Ruler)
            .iter()
            .position(|sub| sub.mode == mode)
        {
            self.toolbox.pick_sub(Tool::Ruler, index);
        }
    }

    /// Lays a perspective ruler of `count` vanishing points on the
    /// picture, as Create Perspective Ruler does.
    pub(crate) fn make_perspective(&mut self, cx: &mut AppletContext<'_>, count: u32) {
        let Some(picture) = self.picture() else {
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
        let ruler = crate::rulers::Perspective::new(count, size);
        let edit = crate::rulers::edit_shown(
            &cx.world.arena,
            picture,
            crate::rulers::PERSPECTIVE,
            Some(ruler.to_text()),
            &tr!("create-perspective-ruler"),
        );
        if Self::apply(cx, edit) {
            self.pick_ruler(1);
            if let Some(doc) = self.doc_mut() {
                doc.ruler_on = true;
                doc.show_rulers = true;
            }
        }
    }
}
