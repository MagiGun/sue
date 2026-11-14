//! Scenes of the gallery (`crate::gallery`): Sue's design system on one
//! page, drawn live with the real context and painter of the tour. It is
//! no part of Sue's interface; these are the pictures the tokens and the
//! components are judged on.
//!
//! Every picture shows the focus ring: the first control is reached with
//! Tab, as a keyboard would.

use std::time::Duration;

use egui::{Pos2, Rect, vec2};
use sue_wm::Theme;
use sue_wm::ui::Density;

use sue_io::encode_png_rgba;

use super::super::{Camera2, Tour, pixel_size};
use crate::gallery::{self, Demo};

/// The size of the page, in points.
const SIZE: [u32; 2] = [1440, 900];

/// Frames drawn after the Tab, so that the focus ring has faded in.
const SETTLE: usize = 60;

/// Scenes 29: the gallery in each built-in preset at the scale of the run
/// (`29-gallery-preset-<id>`), at 1.5 and 2.0 pixels per point
/// (`29-gallery-scale-150`, `-200`) and in each density
/// (`29-gallery-density-<density>`), the last five in the theme of the
/// run.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.settle();
    let before = (tour.camera.size, tour.camera.scale);
    let ctx = tour.camera.ctx.clone();
    let touch = sue_wm::widgets::is_touch(&ctx);
    sue_wm::widgets::set_touch(&ctx, false);
    let run = tour.runtime.theme().clone();
    let caret = tour.runtime.engine.config.caret_blink;
    tour.camera.size = SIZE;

    let mut applied = run.clone();
    let mut pictures: Vec<(String, Theme, Density, Option<f32>)> = Vec::new();
    for preset in sue_rice::builtin_presets() {
        let name = format!("29-gallery-preset-{}", preset.id);
        pictures.push((name, run.with_preset(&preset), Density::Regular, None));
    }
    for (scale, label) in [(1.5, "150"), (2.0, "200")] {
        let name = format!("29-gallery-scale-{label}");
        pictures.push((name, run.clone(), Density::Regular, Some(scale)));
    }
    for density in Density::ALL {
        let name = format!(
            "29-gallery-density-{}",
            format!("{density:?}").to_lowercase()
        );
        pictures.push((name, run.clone(), density, None));
    }
    let mut result = Ok(());
    for (name, theme, density, scale) in pictures {
        theme.apply(&ctx, caret, Some(&applied));
        applied = theme.clone();
        tour.camera.scale = scale.unwrap_or(before.1);
        let path = tour.dir.join(format!("{name}.png"));
        result = shoot(&mut tour.camera, &path, &theme, density);
        if result.is_err() {
            break;
        }
        tour.written.push(path);
    }

    // Leave the tour as it was: no focus, no keyboard focus showing, the
    // run's theme, size, scale and touch.
    if let Some(id) = ctx.memory(egui::Memory::focused) {
        ctx.memory_mut(|memory| memory.surrender_focus(id));
    }
    let press = |pressed| egui::Event::PointerButton {
        pos: Pos2::ZERO,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut demo = Demo::default();
    for events in [vec![press(true)], vec![press(false)]] {
        let output = frame(&mut tour.camera, events, &run, Density::Regular, &mut demo);
        tour.camera.painter.apply_textures(output.textures_delta);
    }
    run.apply(&ctx, caret, Some(&applied));
    sue_wm::widgets::set_touch(&ctx, touch);
    (tour.camera.size, tour.camera.scale) = before;
    result
}

/// Draws one frame of the gallery with `events`.
fn frame(
    camera: &mut Camera2,
    events: Vec<egui::Event>,
    theme: &Theme,
    density: Density,
    demo: &mut Demo,
) -> egui::FullOutput {
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        vec2(camera.size[0] as f32, camera.size[1] as f32),
    );
    let mut raw = egui::RawInput {
        screen_rect: Some(rect),
        time: Some(camera.now.duration_since(camera.start).as_secs_f64()),
        max_texture_side: Some(camera.painter.max_texture_side()),
        events,
        ..egui::RawInput::default()
    };
    raw.viewports
        .entry(raw.viewport_id)
        .or_default()
        .native_pixels_per_point = Some(camera.scale);
    let output = camera.ctx.run_ui(raw, |ui| {
        egui::Frame::new()
            .fill(theme.void)
            .inner_margin(egui::Margin::same(
                theme.dialog_margin().clamp(0.0, 127.0) as i8
            ))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                gallery::show(ui, theme, density, demo);
            });
    });
    camera.now += Duration::from_millis(8);
    output
}

/// Draws the gallery: once to lay it out, once with a Tab, then until the
/// focus ring has faded in; and writes the last frame to `path`.
fn shoot(
    camera: &mut Camera2,
    path: &std::path::Path,
    theme: &Theme,
    density: Density,
) -> Result<(), String> {
    let mut demo = Demo::default();
    if let Some(id) = camera.ctx.memory(egui::Memory::focused) {
        camera.ctx.memory_mut(|memory| memory.surrender_focus(id));
    }
    let tab = egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let mut events = vec![Vec::new(), vec![tab]];
    events.extend(std::iter::repeat_with(Vec::new).take(SETTLE));
    for events in events {
        let output = frame(camera, events, theme, density, &mut demo);
        camera.painter.apply_textures(output.textures_delta);
    }
    let output = frame(camera, Vec::new(), theme, density, &mut demo);
    let size = pixel_size(camera.size, camera.scale);
    let pixels = camera.painter.paint_offscreen(
        &camera.ctx,
        size,
        theme.void,
        output.shapes,
        output.textures_delta,
        output.pixels_per_point,
    )?;
    std::fs::write(path, encode_png_rgba(size[0], size[1], &pixels)?)
        .map_err(|error| error.to_string())
}
