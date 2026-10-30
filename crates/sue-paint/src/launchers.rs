//! Clip Studio Paint's launchers: small bars of buttons under what is
//! being worked on. The Transform launcher sits under a transform's box,
//! the Text launcher under the current text layer's words, and the Object
//! launcher under the current layer's drawing while the Operation tool is
//! out. View turns each of them on and off.

use egui::{Pos2, Rect, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::SuePaint;
use crate::tools::Tool;

/// A launcher button: its icon, hint and whether it is on.
pub(crate) type Button = (Icon, String, bool);

/// Draws a launcher of `buttons` centered under `corners` (screen points),
/// kept inside `viewport`. The clicked button's index.
pub(crate) fn bar(
    ui: &mut Ui,
    viewport: Rect,
    corners: &[Pos2],
    buttons: &[Button],
    theme: &Theme,
) -> Option<usize> {
    let bottom = corners
        .iter()
        .map(|corner| corner.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let middle = corners.iter().map(|corner| corner.x).sum::<f32>() / corners.len().max(1) as f32;
    let width = buttons.len() as f32 * 26.0 + 8.0;
    let bar = Rect::from_min_size(pos2(middle - width / 2.0, bottom + 10.0), vec2(width, 30.0));
    let bar = bar.translate(vec2(
        (viewport.left() + 4.0 - bar.left()).max(0.0)
            + (viewport.right() - 4.0 - bar.right()).min(0.0),
        (viewport.bottom() - 4.0 - bar.bottom()).min(0.0),
    ));
    ui.painter()
        .rect_filled(bar, theme.control_radius, theme.panel);
    ui.painter().rect_stroke(
        bar,
        theme.control_radius,
        theme.hairline(),
        egui::StrokeKind::Inside,
    );
    let mut clicked = None;
    for (index, (icon, tip, active)) in buttons.iter().enumerate() {
        let at = Rect::from_min_size(
            pos2(bar.left() + 4.0 + index as f32 * 26.0, bar.top() + 4.0),
            vec2(22.0, 22.0),
        );
        if icon_button(ui, at, *icon, true, *active, tip, theme) {
            clicked = Some(index);
        }
    }
    clicked
}

impl SuePaint {
    /// The Text launcher, under the current text layer's words: edit
    /// them, make them smaller or larger, set them upright or centered.
    pub(crate) fn text_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.text_launcher || self.gesture.is_some() || self.dialog.is_some() {
            return;
        }
        let tool = self.toolbox.tool();
        if !matches!(tool, Tool::Text | Tool::Operation) {
            return;
        }
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let Some(layer) = doc.layer else {
            return;
        };
        let Some(entity) = cx.world.arena.get_by_id(layer).ok() else {
            return;
        };
        let (Some(lettering), Some([x0, y0, x1, y1])) = (
            crate::lettering::Lettering::of(entity),
            crate::lettering::area_of(entity),
        ) else {
            return;
        };
        let view = doc.view;
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x, y)));
        let buttons: [Button; 5] = [
            (Icon::Edit, tr!("edit-text"), false),
            (Icon::ZoomOut, tr!("smaller-text"), false),
            (Icon::ZoomIn, tr!("larger-text"), false),
            (Icon::Text, tr!("vertical-text"), lettering.vertical),
            (Icon::Fit, tr!("center-text"), lettering.center),
        ];
        let Some(index) = bar(ui, viewport, &corners, &buttons, theme) else {
            return;
        };
        let mut next = lettering.clone();
        match index {
            0 => {
                self.dialog = Some(crate::dialogs::Dialog::Text(Some(layer), lettering));
                return;
            }
            1 => next.size = (next.size / 1.2).max(4.0),
            2 => next.size = (next.size * 1.2).min(2000.0),
            3 => next.vertical = !next.vertical,
            _ => next.center = !next.center,
        }
        self.put_text(cx, Some(layer), &next);
    }

    /// The Object launcher, under the current layer's drawing while the
    /// Operation tool is out: scale and turn it, flip it, its properties,
    /// or delete it.
    pub(crate) fn object_launcher(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if !self.object_launcher
            || self.gesture.is_some()
            || self.transform.is_some()
            || self.toolbox.tool() != Tool::Operation
        {
            return;
        }
        let Some(info) = self.layer_info(&cx.world.arena) else {
            return;
        };
        if cx
            .world
            .arena
            .get_by_id(info.id)
            .ok()
            .and_then(crate::lettering::Lettering::of)
            .is_some()
        {
            // Text has its own launcher.
            return;
        }
        // Where its drawing is, worked out again only when it changes.
        if self
            .object_bounds
            .is_none_or(|(layer, revision, _)| layer != info.id || revision != info.revision)
        {
            let bounds = crate::doc::raster(&cx.world.arena, info.id).and_then(|raster| {
                let bytes = raster.bytes()?;
                let width = raster.width() as usize;
                let mut found: Option<[usize; 4]> = None;
                for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
                    if pixel[3] > 0 {
                        let (x, y) = (index % width, index / width);
                        let b = found.get_or_insert([x, y, x, y]);
                        b[0] = b[0].min(x);
                        b[1] = b[1].min(y);
                        b[2] = b[2].max(x);
                        b[3] = b[3].max(y);
                    }
                }
                found
            });
            self.object_bounds = Some((
                info.id,
                info.revision,
                bounds.map(|[x0, y0, x1, y1]| {
                    let [ox, oy] = info.style.offset.map(|value| value as f32);
                    [
                        x0 as f32 + ox,
                        y0 as f32 + oy,
                        x1 as f32 + 1.0 + ox,
                        y1 as f32 + 1.0 + oy,
                    ]
                }),
            ));
        }
        let Some((_, _, Some([x0, y0, x1, y1]))) = self.object_bounds else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            .map(|[x, y]| view.to_screen(viewport, size, pos2(x, y)));
        let buttons: [Button; 5] = [
            (Icon::Fit, tr!("scale-rotate"), false),
            (Icon::FlipH, tr!("flip-horizontal"), false),
            (Icon::FlipV, tr!("flip-vertical"), false),
            (Icon::LayerProperty, tr!("layer-properties"), false),
            (Icon::Delete, tr!("delete-layer"), false),
        ];
        if let Some(index) = bar(ui, viewport, &corners, &buttons, theme) {
            let command = [
                "edit.transform.scale",
                "edit.flip_h",
                "edit.flip_v",
                "window.layer_property",
                "layer.delete",
            ][index];
            self.run(command, cx);
        }
    }
}
