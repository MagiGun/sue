//! Phase 6, tools B: Tool Property for the tools the full guide's tour
//! shows from the Figure tool to Correct Line (00:36:03 to 00:38:12).

use super::Tour;

/// The rect of `text` where the next frame draws it, the topmost.
fn text_rect(tour: &mut Tour<'_>, text: &str) -> Option<egui::Rect> {
    let output = tour.camera.frame(&mut tour.runtime);
    let mut found: Option<egui::Rect> = None;
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(shown) = shape
                && shown.galley.text() == text
            {
                let rect = egui::Rect::from_min_size(shown.pos, shown.galley.size());
                if found.is_none_or(|was| rect.top() < was.top()) {
                    found = Some(rect);
                }
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    tour.camera.painter.apply_textures(output.textures_delta);
    found
}

/// Clicks `text` where it is drawn.
fn click_text(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    let rect = text_rect(tour, text).ok_or_else(|| format!("{text:?} is not on screen"))?;
    tour.camera.click(rect.center());
    tour.settle();
    Ok(())
}

/// A drag with the main button from `from` to `to`.
fn drag(tour: &mut Tour<'_>, from: egui::Pos2, to: egui::Pos2) {
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
    for step in 1..=12 {
        let at = from + (to - from) * (step as f32 / 12.0);
        tour.camera.input.push(vec![egui::Event::PointerMoved(at)]);
    }
    tour.camera.input.push(vec![button(to, false)]);
    tour.settle();
}

/// The sub tools whose Tool Property each picture shows: the tool's
/// command, the sub tool's command and the picture's name.
const ROWS: [(&str, &str, &str); 10] = [
    (
        "tool.pick.figure",
        "subtool.figure.Rectangle",
        "36p1-sue-paint-figure-rectangle-rows",
    ),
    (
        "tool.pick.figure",
        "subtool.figure.Dense_saturated_line",
        "36p2-sue-paint-saturated-line-rows",
    ),
    (
        "tool.pick.frame",
        "subtool.frame_border.Rectangle_Frame",
        "36p3-sue-paint-frame-rows",
    ),
    (
        "tool.pick.frame",
        "subtool.frame_border.Frame_Border_Division",
        "36p4-sue-paint-frame-division-rows",
    ),
    (
        "tool.pick.ruler",
        "subtool.ruler.Linear_Ruler",
        "36p5-sue-paint-linear-ruler-rows",
    ),
    (
        "tool.pick.ruler",
        "subtool.ruler.Curve_Ruler",
        "36p6-sue-paint-curve-ruler-rows",
    ),
    (
        "tool.pick.text",
        "subtool.text.Text",
        "36p7-sue-paint-text-style-rows",
    ),
    (
        "tool.pick.balloon",
        "subtool.balloon.Ellipse_Balloon",
        "36p8-sue-paint-balloon-rows",
    ),
    (
        "tool.correct_line",
        "subtool.correct_line.Pinch_vector_line",
        "36p9-sue-paint-pinch-rows",
    ),
    (
        "tool.correct_line",
        "subtool.correct_line.Redraw_vector_line",
        "36p10-sue-paint-redraw-rows",
    ),
];

/// 36p1 to 36p11. Tool Property of each sub tool of [`ROWS`], then a
/// guide and a scaled linear ruler on the canvas, undone after; the tools
/// are put back as they were found (the first sub tool of each, the
/// Operation tool picked, the Sub Tool palette in front).
pub(super) fn tool_rows(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.settle();
    click_text(tour, "Tool Property")?;
    for (tool, sub, name) in ROWS {
        tour.paint_command(tool);
        tour.paint_command(sub);
        tour.shoot(name)?;
    }
    // 36p11. A guide and a linear ruler with scale marks on the canvas.
    let middle = tour.runtime.shell.page_rect().center() + egui::vec2(8.0, -63.0);
    tour.paint_command("tool.pick.ruler");
    tour.paint_command("subtool.ruler.Guide");
    drag(
        tour,
        middle + egui::vec2(-200.0, 60.0),
        middle + egui::vec2(200.0, 64.0),
    );
    tour.paint_command("subtool.ruler.Linear_Ruler");
    tour.settle();
    click_text(tour, "Scale")?;
    drag(
        tour,
        middle + egui::vec2(-180.0, -80.0),
        middle + egui::vec2(160.0, -40.0),
    );
    tour.shoot("36p11-sue-paint-guide-and-ruler-scale")?;
    click_text(tour, "Scale")?;
    tour.paint_command("edit.undo");
    tour.paint_command("edit.undo");
    tour.settle();
    for (tool, first) in [
        ("tool.pick.figure", "subtool.figure.Straight_Line"),
        ("tool.pick.frame", "subtool.frame_border.Rectangle_Frame"),
        ("tool.pick.ruler", "subtool.ruler.Linear_Ruler"),
        ("tool.pick.balloon", "subtool.balloon.Ellipse_Balloon"),
        ("tool.correct_line", "subtool.correct_line.Control_point"),
    ] {
        tour.paint_command(tool);
        tour.paint_command(first);
    }
    tour.paint_command("tool.operation");
    click_text(tour, "Sub Tool")?;
    Ok(())
}
