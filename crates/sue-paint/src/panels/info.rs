//! The Information palette: the canvas and what is under the pointer.

use egui::{Align2, Rect, Sense, Ui, pos2};
use sue_wm::{AppletContext, Theme};

use crate::studio::SuePaint;

impl SuePaint {
    /// The Info tab: the canvas and what is under the pointer.
    pub(crate) fn info_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let painter = ui.painter().with_clip_rect(body);
        let mut lines = Vec::new();
        let viewport = self.viewport;
        if let Some(doc) = self.doc() {
            let arena = &cx.world.arena;
            let entity = arena.get_by_id(doc.picture).ok();
            let size = entity.and_then(sue_model::picture::size).unwrap_or([0, 0]);
            let dpi = entity
                .and_then(|entity| entity.property(sue_model::picture::DPI))
                .and_then(sue_core::Value::as_f64)
                .unwrap_or(72.0);
            lines.push(tr!(
                "canvas-x-px-dpi",
                value = size[0],
                value2 = size[1],
                dpi = sue_lang::number(dpi, 0)
            ));
            lines.push(tr!(
                "print-size-x-cm",
                value = sue_lang::number(f64::from(size[0]) / dpi * 2.54, 1),
                value2 = sue_lang::number(f64::from(size[1]) / dpi * 2.54, 1)
            ));
            lines.push(tr!(
                "layers-2",
                count = crate::doc::layers(arena, doc.picture).len()
            ));
            lines.push(tr!(
                "zoom-percent-angle",
                value = sue_lang::number(f64::from(doc.view.zoom * 100.0), 0),
                rotation = sue_lang::number(f64::from(doc.view.rotation), 0)
            ));
            if let Some(pointer) = self.pointer.filter(|pointer| viewport.contains(*pointer)) {
                let at = doc.view.to_picture(viewport, size, pointer);
                lines.push(tr!(
                    "pointer",
                    x = sue_lang::number(f64::from(at.x), 0),
                    y = sue_lang::number(f64::from(at.y), 0)
                ));
                let (pixels, shown, _) = doc.compositor.display();
                if at.x >= 0.0
                    && at.y >= 0.0
                    && (at.x as u32) < shown[0]
                    && (at.y as u32) < shown[1]
                {
                    let index = ((at.y as u32 * shown[0] + at.x as u32) * 4) as usize;
                    if let Some(rgba) = pixels.get(index..index + 4) {
                        lines.push(tr!(
                            "color-r-g-b-a",
                            value = rgba[0],
                            value2 = rgba[1],
                            value3 = rgba[2],
                            value4 = rgba[3],
                            value5 = format!("{:02X}", rgba[0]),
                            value6 = format!("{:02X}", rgba[1]),
                            value7 = format!("{:02X}", rgba[2])
                        ));
                    }
                }
            } else {
                lines.push(tr!("pointer-off-the-canvas"));
            }
        } else {
            lines.push(tr!("no-canvas"));
        }
        let font = theme.font(theme.small_size + 0.5);
        for (row, line) in lines.iter().enumerate() {
            painter.text(
                pos2(body.left() + 10.0, body.top() + 10.0 + row as f32 * 18.0),
                Align2::LEFT_TOP,
                line,
                font.clone(),
                theme.text,
            );
        }
        // Keep it current while the pointer moves.
        let _ = ui.interact(body, ui.scope_id().with("sue-paint-info"), Sense::hover());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}
