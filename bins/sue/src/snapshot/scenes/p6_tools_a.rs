//! Scenes of Tool Property for the view, pick and move tools, the
//! transform's settings in Tool Property and Change Canvas Size's box on
//! the canvas (Phase 6).

use super::super::Tour;

/// Shows Tool Property and writes the picture `name`.
fn tool_property(tour: &mut Tour<'_>, name: &str) -> Result<(), String> {
    tour.paint_go(&["tab/tool/1"]);
    tour.shoot(name)
}

/// 37a to 37n. Tool Property of Zoom, the Operation tool's four sub
/// tools, Move layer, the two Eyedropper sub tools and Lasso fill; the
/// transform's settings in Tool Property; Change Canvas Size's box with
/// its handles on the canvas. The tour calls this where Sue Paint has a
/// picture open, and leaves the Pen out.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_command("tool.zoom");
    tool_property(tour, "37a-sue-paint-zoom-tool-property")?;
    tour.paint_command("tool.operation");
    for name in [
        "37b-sue-paint-object-tool-property",
        "37c-sue-paint-select-layer-tool-property",
        "37d-sue-paint-light-table-tool-property",
        "37e-sue-paint-edit-timeline-tool-property",
    ] {
        tool_property(tour, name)?;
        tour.paint_command("tool.next_sub");
    }
    tour.paint_command("tool.move_layer");
    tool_property(tour, "37f-sue-paint-move-layer-tool-property")?;
    tour.paint_command("tool.eyedropper");
    tool_property(tour, "37g-sue-paint-eyedropper-display-tool-property")?;
    tour.paint_command("tool.next_sub");
    tool_property(tour, "37h-sue-paint-eyedropper-layer-tool-property")?;
    tour.paint_command("tool.previous_sub");
    // Lasso fill: the Fill tool's sub tools, stepped through to it.
    tour.paint_command("tool.fill");
    for _ in 0..3 {
        tour.paint_command("tool.next_sub");
    }
    tool_property(tour, "37i-sue-paint-lasso-fill-tool-property")?;
    // Scale/Rotate open: its settings in Tool Property.
    tour.paint_command("tool.pen");
    tour.paint_command("edit.transform.scale");
    tool_property(tour, "37j-sue-paint-transform-tool-property")?;
    tour.paint_command("edit.transform.cancel");
    // Change Canvas Size: its box and handles on the canvas.
    tour.paint_command("edit.canvas_size");
    tour.shoot("37k-sue-paint-canvas-size-handles")?;
    // The bottom right corner dragged in: what falls outside dims.
    let corner = sue_paint::shown_buttons(&tour.camera.ctx)
        .into_iter()
        .find(|(name, _)| name == "mark.canvas_size.2")
        .map(|(_, rect)| rect.center());
    if let Some(from) = corner {
        let to = from + egui::vec2(-160.0, -90.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        tour.camera
            .input
            .push(vec![egui::Event::PointerMoved(from)]);
        tour.camera.input.push(vec![button(from, true)]);
        for step in 1..=8 {
            let at = from + (to - from) * (step as f32 / 8.0);
            tour.camera.input.push(vec![egui::Event::PointerMoved(at)]);
        }
        tour.camera.input.push(vec![button(to, false)]);
        tour.shoot("37l-sue-paint-canvas-size-dragged")?;
    }
    tour.paint_go(&["dialog/close"]);
    tour.settle();
    Ok(())
}

#[cfg(test)]
mod look {
    use super::super::super::{Options, tour_with};

    /// Writes this file's scenes alone, after the sample illustration,
    /// into the folder `SUE_P6_SHOTS` names, to look at them. Run by hand
    /// (`cargo test -p sue --bin sue p6_tools_a_scenes -- --ignored`).
    #[test]
    #[ignore = "writes pictures; run by hand to look at them"]
    fn p6_tools_a_scenes() {
        let Some(dir) = std::env::var_os("SUE_P6_SHOTS") else {
            return;
        };
        let written = tour_with(
            std::path::Path::new(&dir),
            1440,
            900,
            &Options::default(),
            |tour| {
                super::super::paint_canvas::sample(tour)?;
                super::scenes(tour)
            },
        )
        .expect("the scenes are written");
        assert!(written.len() > 10, "{written:?}");
    }
}
