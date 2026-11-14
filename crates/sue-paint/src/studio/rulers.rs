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
        if Self::apply(
            cx,
            crate::rulers::edit(picture, crate::rulers::PATHS, text, label),
        ) && let Some(doc) = self.doc_mut()
        {
            doc.snap_ruler = true;
            doc.show_rulers = true;
        }
    }

    /// Lays the curve ruler through the points placed so far.
    pub(crate) fn finish_curve(&mut self, cx: &mut AppletContext<'_>) {
        let points = std::mem::take(&mut self.curve_points);
        if points.len() >= 2 {
            self.add_ruler_path(
                cx,
                crate::rulers::Path::curve(&points),
                &tr!("create-curve-ruler"),
            );
        }
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
        let edit = crate::rulers::edit(
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
