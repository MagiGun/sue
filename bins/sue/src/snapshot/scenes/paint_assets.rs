//! Scenes of Sue Paint's materials, templates, palette colors, Webtoon
//! Display Settings, Auto Action steps and the interface color's density
//! (0.0.8 Phase 6). The tour calls this file where Sue Paint has the
//! sample illustration open; each scene puts back what it changed.

use sue_rice::RiceValue;

use super::super::Tour;

/// p6a to p6l.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    // p6a to p6d. The Material palette open at one folder each, by its
    // folder buttons.
    for (kind, name) in [
        ("sue_paint.balloon", "p6a-sue-paint-material-balloons"),
        ("sue_paint.effect", "p6b-sue-paint-material-effect-lines"),
        ("sue_paint.template", "p6c-sue-paint-material-templates"),
        ("sue_paint.background3d", "p6d-sue-paint-material-3d"),
    ] {
        tour.paint_command(&format!("material.folder.{kind}"));
        tour.shoot(name)?;
    }
    // The Material palette goes again, so the Layer palette shows.
    tour.paint_command("window.material");
    // p6e. Palette colors in the parent folder and on the canvas.
    tour.paint_command("layer.palette_color.red");
    tour.set("sue_paint.palette_in_folder", RiceValue::Bool(true));
    tour.set("sue_paint.palette_on_canvas", RiceValue::Bool(true));
    tour.shoot("p6e-sue-paint-palette-colors-on-canvas")?;
    tour.set("sue_paint.palette_in_folder", RiceValue::Bool(false));
    tour.set("sue_paint.palette_on_canvas", RiceValue::Bool(false));
    tour.paint_command("edit.undo");
    // p6f. Where each screen of a 16:10 display ends.
    tour.set("sue_paint.webtoon_guides", RiceValue::Bool(true));
    tour.set(
        "sue_paint.webtoon_screen",
        RiceValue::Text("desktop".to_owned()),
    );
    tour.shoot("p6f-sue-paint-webtoon-screen-guides")?;
    tour.set("sue_paint.webtoon_guides", RiceValue::Bool(false));
    tour.set(
        "sue_paint.webtoon_screen",
        RiceValue::Text("tall-phone".to_owned()),
    );
    // p6g to p6i. Webtoon Display Settings, New with its Template row,
    // Register Material, Template.
    tour.paint_command("view.webtoon_display");
    tour.shoot("p6g-sue-paint-webtoon-display-settings")?;
    tour.paint_go(&["dialog/close", "dialog/new"]);
    tour.shoot("p6h-sue-paint-new-with-template")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("edit.material.template");
    tour.shoot("p6i-sue-paint-register-template")?;
    tour.paint_go(&["dialog/close"]);
    // p6j. An action recorded with a step that opens a dialog: each step
    // has its on box and its dialog box.
    tour.paint_command("auto.new");
    tour.paint_command("filter.gaussian");
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("view.fit_width");
    tour.paint_command("auto.record");
    tour.paint_go(&["extras/show/auto_action"]);
    tour.shoot("p6j-sue-paint-auto-action-dialog-boxes")?;
    tour.paint_command("auto.delete");
    tour.paint_command("view.fit");
    // p6k and p6l. The interface color, denser and lighter.
    tour.set("sue_paint.theme_density", RiceValue::Int(60));
    tour.shoot("p6k-sue-paint-density-darker")?;
    tour.set("sue_paint.theme_density", RiceValue::Int(-60));
    tour.shoot("p6l-sue-paint-density-lighter")?;
    tour.set("sue_paint.theme_density", RiceValue::Int(0));
    Ok(())
}
