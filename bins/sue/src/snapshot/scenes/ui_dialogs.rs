//! Gallery scenes of the dialog host, the sheet and the toast stack of
//! `sue_wm::ui`. No studio shows them yet, so each is drawn alone over a
//! plain backdrop, with the context and the theme of the run: they are
//! the pictures these components are reviewed on.

use egui::{Color32, Event, Key, Modifiers, Pos2, Rect, Ui, vec2};
use sue_io::encode_png_rgba;
use sue_wm::Theme;
use sue_wm::ui::{Dialog, Sheet, Space, Toast, Toasts, Tone, confirm, form_grid};

use super::super::Tour;

/// The size of the desktop scenes, in points.
const DESKTOP: [u32; 2] = [1100, 720];

/// The size of the phone scenes, in points.
const PHONE: [u32; 2] = [430, 932];

/// How many frames a scene runs before its picture: enough for the
/// slowest transition (240 ms at 8 ms a frame) to come to rest.
const FRAMES: usize = 60;

/// The height of the backdrop's status bar, in points.
const STATUS_BAR: f32 = 24.0;

/// Scenes 28a to 28f: a form dialog, a question that destroys, the toast
/// stack, a sheet and a dialog on a phone, and a dialog too narrow for
/// a row of buttons.
pub(super) fn gallery(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.settle();
    let before = tour.camera.size;
    let touch = sue_wm::widgets::is_touch(&tour.camera.ctx);

    tour.camera.size = DESKTOP;
    shoot_ui(tour, "28a-ui-dialog-form", false, |ui, theme| {
        backdrop(ui, theme);
        Dialog::new("gallery-form", "New Layer", theme)
            .extra("Reset")
            .ok_cancel()
            .show(ui.ctx(), |ui| new_layer_form(ui, theme));
    })?;
    shoot_ui(tour, "28b-ui-dialog-confirm", false, |ui, theme| {
        backdrop(ui, theme);
        confirm(
            ui.ctx(),
            Dialog::new("gallery-confirm", "Delete Layer", theme)
                .accept("Delete")
                .cancel()
                .danger(),
            Tone::Warning,
            "Layer 3 and everything drawn on it will be deleted.",
        );
    })?;
    let mut toasts = Toasts::new("gallery-toasts");
    toasts.push(Toast::info("Autosave is on for Illustration.sue"));
    toasts.push(Toast::success("Exported Illustration.png").action("Show in Folder"));
    toasts.push(Toast::warning(
        "The canvas is larger than this computer's graphics card can show at once, \
         so it is drawn in parts",
    ));
    toasts.push(Toast::error("Brush.sut could not be read").sticky());
    shoot_ui(tour, "28c-ui-toasts", false, |ui, theme| {
        backdrop(ui, theme);
        let mut room = ui.ctx().content_rect();
        room.max.y -= STATUS_BAR;
        toasts.show(ui.ctx(), room, theme);
    })?;

    tour.camera.size = PHONE;
    shoot_ui(tour, "28d-ui-sheet-phone", true, |ui, theme| {
        backdrop(ui, theme);
        Sheet::new("gallery-sheet", "Brush", theme).show(ui.ctx(), |ui| {
            form_grid(ui, "gallery-sheet-form", theme, |form| {
                for (label, value) in [("Size", 24.0), ("Opacity", 80.0), ("Hardness", 35.0)] {
                    let mut value: f32 = value;
                    form.row(label, |ui| {
                        ui.add(egui::Slider::new(&mut value, 0.0..=100.0));
                    });
                }
            });
        });
    })?;
    shoot_ui(tour, "28e-ui-dialog-phone", true, |ui, theme| {
        backdrop(ui, theme);
        Dialog::new("gallery-phone", "Save Changes?", theme)
            .accept("Save")
            .alternate("Do Not Save")
            .cancel()
            .show(ui.ctx(), |ui| {
                ui.label("Illustration.sue has changes that were not saved.");
            });
    })?;
    tour.camera.size = DESKTOP;
    shoot_ui(tour, "28f-ui-dialog-narrow", false, |ui, theme| {
        backdrop(ui, theme);
        Dialog::new("gallery-narrow", "Close Picture", theme)
            .width(300.0)
            .extra("Help")
            .accept("Save Everything")
            .alternate("Close Without Saving")
            .cancel()
            .show(ui.ctx(), |ui| {
                ui.label("Three pictures have changes that were not saved.");
            });
    })?;

    tour.camera.size = before;
    sue_wm::widgets::set_touch(&tour.camera.ctx, touch);
    Ok(())
}

/// The body of the form dialog: a text field, a choice, a slider and a
/// check box, in a form.
fn new_layer_form(ui: &mut Ui, theme: &Theme) {
    let mut name = String::from("Layer 4");
    let mut mode = "Normal";
    let mut opacity = 100.0_f32;
    let mut clip = true;
    form_grid(ui, "gallery-new-layer", theme, |form| {
        form.row("Name", |ui| {
            ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY));
        });
        form.row("Blending mode", |ui| {
            egui::ComboBox::from_id_salt("gallery-blend")
                .selected_text(mode)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut mode, "Normal", "Normal");
                    ui.selectable_value(&mut mode, "Multiply", "Multiply");
                });
        });
        form.row("Opacity", |ui| {
            ui.add(egui::Slider::new(&mut opacity, 0.0..=100.0).suffix("%"));
        });
        form.section("Layer");
        form.wide(|ui| ui.checkbox(&mut clip, "Clip to the layer below"));
    });
}

/// A plain window behind the components: a bar at the top, a sheet of
/// paper in the middle and a status bar at the bottom.
fn backdrop(ui: &Ui, theme: &Theme) {
    let screen = ui.ctx().content_rect();
    let painter = ui.painter();
    painter.rect_filled(screen, 0.0, theme.void);
    let bar = theme.space(Space::S8);
    let top = Rect::from_min_size(screen.min, vec2(screen.width(), bar));
    painter.rect_filled(top, 0.0, theme.surface(1));
    painter.hline(screen.x_range(), top.bottom(), theme.hairline());
    let status = Rect::from_min_max(
        Pos2::new(screen.left(), screen.bottom() - STATUS_BAR),
        screen.max,
    );
    painter.rect_filled(status, 0.0, theme.surface(1));
    painter.hline(screen.x_range(), status.top(), theme.hairline());
    let room = Rect::from_min_max(
        Pos2::new(screen.left(), top.bottom()),
        Pos2::new(screen.right(), status.top()),
    )
    .shrink(theme.space(Space::S8));
    let paper = Rect::from_center_size(
        room.center(),
        vec2(room.height() * 0.75, room.height()).min(room.size()),
    );
    painter.rect_filled(paper, 0.0, Color32::WHITE);
}

/// Runs [`FRAMES`] frames of `draw` alone in the shell's context, laid out
/// for touch when `touch`, with a letter key in the last frames so that
/// keyboard focus shows; then writes the picture `name`.
fn shoot_ui(
    tour: &mut Tour<'_>,
    name: &str,
    touch: bool,
    mut draw: impl FnMut(&mut Ui, &Theme),
) -> Result<(), String> {
    let theme = tour.runtime.theme().clone();
    let camera = &mut tour.camera;
    let screen = Rect::from_min_size(
        Pos2::ZERO,
        vec2(camera.size[0] as f32, camera.size[1] as f32),
    );
    let mut output = None;
    for frame in 0..=FRAMES {
        let events = if frame + 4 == FRAMES {
            vec![Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]
        } else {
            Vec::new()
        };
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(camera.now.duration_since(camera.start).as_secs_f64()),
            max_texture_side: Some(camera.painter.max_texture_side()),
            events,
            ..egui::RawInput::default()
        };
        raw.viewports
            .entry(raw.viewport_id)
            .or_default()
            .native_pixels_per_point = Some(camera.scale);
        let done = camera.ctx.run_ui(raw, |ui| {
            sue_wm::widgets::set_touch(ui.ctx(), touch);
            draw(ui, &theme);
        });
        camera.now += std::time::Duration::from_millis(8);
        if frame < FRAMES {
            camera.painter.apply_textures(done.textures_delta);
        } else {
            output = Some(done);
        }
    }
    let output = output.ok_or("the scene drew no frame")?;
    let size = camera.pixels();
    let pixels = camera.painter.paint_offscreen(
        &camera.ctx,
        size,
        theme.void,
        output.shapes,
        output.textures_delta,
        output.pixels_per_point,
    )?;
    let path = tour.dir.join(format!("{name}.png"));
    std::fs::write(&path, encode_png_rgba(size[0], size[1], &pixels)?)
        .map_err(|error| error.to_string())?;
    tour.written.push(path);
    Ok(())
}
