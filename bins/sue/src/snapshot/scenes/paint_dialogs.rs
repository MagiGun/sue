//! Scenes of Sue Paint's menus, dialogs and galleries.

use super::super::Tour;

/// 08l and 08m. Quick Filters and Quick Effects: their galleries.
/// 08n. Advanced Fill.
pub(super) fn galleries(tour: &mut Tour<'_>) -> Result<(), String> {
    for (command, name) in [
        ("filter.quick.layer", "08l-sue-paint-quick-filters"),
        ("filter.effects.layer", "08m-sue-paint-quick-effects"),
        ("edit.advanced_fill", "08n-sue-paint-advanced-fill"),
    ] {
        tour.paint_command(command);
        tour.shoot(name)?;
        tour.paint_go(&["dialog/close"]);
    }
    Ok(())
}

/// Scene 10. The Layer menu; 11. New, 11b. Pen Pressure and 11f. Manage
/// Fonts. (11e, the old Preferences dialog, is gone: File, Preferences
/// opens Settings, scene 04d.)
pub(super) fn menu_and_dialogs(tour: &mut Tour<'_>) -> Result<(), String> {
    for (target, name) in [
        ("menu/Layer", "10-sue-paint-menu"),
        ("dialog/new", "11-sue-paint-new"),
        ("dialog/pressure", "11b-sue-paint-pressure"),
    ] {
        tour.paint_go(&["dialog/close", target]);
        tour.shoot(name)?;
    }
    // 11f. Manage Fonts: Sue's shared font list.
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("file.fonts");
    tour.shoot("11f-sue-paint-manage-fonts")?;
    Ok(())
}

/// 11c. The Page Manager of a manga of five pages.
pub(super) fn pages(tour: &mut Tour<'_>) -> Result<(), String> {
    for command in [
        "story.add",
        "story.add",
        "story.add",
        "story.add",
        "story.first",
    ] {
        tour.paint_command(command);
    }
    tour.paint_go(&["dialog/close", "dialog/pages"]);
    tour.shoot("11c-sue-paint-pages")?;
    tour.paint_go(&["dialog/close"]);
    Ok(())
}

/// Every piece of text the next frame draws, with its rect.
fn texts(tour: &mut Tour<'_>) -> Vec<(String, egui::Rect)> {
    let output = tour.camera.frame(&mut tour.runtime);
    tour.camera
        .painter
        .apply_textures(output.textures_delta.clone());
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                found.push((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                ));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
}

/// Clicks the last `text` drawn (the dialog's, over the studio's).
fn click(tour: &mut Tour<'_>, text: &str) -> Result<(), String> {
    let at = texts(tour)
        .into_iter()
        .filter(|(found, _)| found == text)
        .map(|(_, rect)| rect.center())
        .next_back()
        .ok_or_else(|| format!("no {text:?} is drawn"))?;
    tour.camera.click(at);
    tour.settle();
    Ok(())
}

/// 11p8a to 11p8h. The dialogs by family: New for an animation and for a
/// comic in monochrome with every setting, Convert Layer, Convert to
/// Lines and Tones, Grid Settings, Text, Change Canvas Size and Expand
/// Selected Area.
pub(super) fn families(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["dialog/close", "dialog/new"]);
    tour.settle();
    click_named(tour, "Animation")?;
    tour.shoot("11p8a-sue-paint-new-animation")?;
    click_named(tour, "Comic")?;
    click_named(tour, "Show all comic settings")?;
    click(tour, "Monochrome")?;
    tour.shoot("11p8b-sue-paint-new-comic")?;
    for (command, name) in [
        ("layer.convert", "11p8c-sue-paint-convert-layer"),
        ("filter.lines_tones", "11p8d-sue-paint-lines-tones"),
        ("view.grid_settings", "11p8e-sue-paint-grid-settings"),
        ("layer.new.text", "11p8f-sue-paint-text"),
        ("edit.canvas_size", "11p8g-sue-paint-canvas-size"),
    ] {
        tour.paint_go(&["dialog/close"]);
        tour.paint_command(command);
        tour.shoot(name)?;
    }
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("select.all");
    tour.paint_command("select.expand");
    tour.shoot("11p8h-sue-paint-expand")?;
    tour.paint_go(&["dialog/close"]);
    tour.paint_command("select.none");
    Ok(())
}

/// Clicks the control named `name`, found by what it tells assistive
/// technology (an icon button has no text drawn).
fn click_named(tour: &mut Tour<'_>, name: &str) -> Result<(), String> {
    tour.camera.ctx.enable_accesskit();
    let output = tour.camera.frame(&mut tour.runtime);
    tour.camera
        .painter
        .apply_textures(output.textures_delta.clone());
    let at = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter(|(_, node)| node.label() == Some(name))
        .find_map(|(_, node)| node.bounds())
        .map(|bounds| {
            egui::pos2(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            )
        })
        .ok_or_else(|| format!("no control is named {name:?}"))?;
    tour.camera.ctx.disable_accesskit();
    tour.camera.click(at);
    tour.settle();
    Ok(())
}
