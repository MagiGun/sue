//! Scenes of the menus and tooltips of `sue_wm::ui`: Sue Paint's menu bar
//! walked with the keyboard, a menu taller than the window, the note of a
//! row that cannot be used, and a tooltip.

use egui::{Event, Key, Modifiers, Pos2, Rect, vec2};
use sue_io::encode_png_rgba;
use sue_wm::ui::{Density, MenuRequest, MenuState, Side, Tip, tip_beside, tooltip};
use sue_wm::{ItemState, MenuItem, Slot};

use super::super::Tour;

/// The window of these scenes, in points.
const SIZE: [u32; 2] = [1440, 900];
/// A window too short for Sue Paint's longest menu.
const SHORT: [u32; 2] = [1200, 420];

fn key(key: Key) -> Vec<Event> {
    vec![Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

fn typed(text: &str) -> Vec<Event> {
    vec![Event::Text(text.to_owned())]
}

/// Scenes 29a to 29e, and 33a to 33e.
pub(super) fn scenes(tour: &mut Tour<'_>) -> Result<(), String> {
    let before = tour.camera.size;
    tour.camera.size = SIZE;
    super::open_paint(tour);
    tour.settle();
    keyboard(tour)?;
    tall(tour)?;
    tour.camera.size = SIZE;
    note(tour)?;
    tip(tour)?;
    clip_studio_order(tour)?;
    tour.camera.size = before;
    Ok(())
}

/// Scene 29a. Sue Paint's File menu walked with the keys: typing finds
/// Save Duplicate, Right opens it with the keyboard on its first row.
fn keyboard(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.paint_go(&["menu/File"]);
    for letter in ["s", "a", "v", "e", " ", "d"] {
        tour.camera.input.push(typed(letter));
    }
    tour.camera.input.push(key(Key::ArrowRight));
    tour.camera.input.push(key(Key::ArrowDown));
    tour.shoot("29a-menu-keyboard")?;
    tour.camera.input.push(key(Key::Escape));
    tour.camera.input.push(key(Key::Escape));
    tour.settle();
    Ok(())
}

/// Scenes 29b and 29c. Sue Paint's longest menu in a short window: it
/// stays under its title and scrolls, at its top and at its end.
fn tall(tour: &mut Tour<'_>) -> Result<(), String> {
    tour.camera.size = SHORT;
    tour.settle();
    let menus = sue_paint::menus();
    let title = menus
        .iter()
        .max_by_key(|menu| menu.items.len())
        .map(|menu| menu.title.clone())
        .ok_or("Sue Paint has no menus")?;
    tour.paint_go(&[&format!("menu/{title}")]);
    tour.camera.input.push(key(Key::ArrowDown));
    tour.shoot("29b-menu-scrolls-top")?;
    tour.camera.input.push(key(Key::End));
    tour.shoot("29c-menu-scrolls-end")?;
    tour.camera.input.push(key(Key::Escape));
    tour.settle();
    Ok(())
}

/// Scene 29d. An Edit menu whose keyboard row cannot be used now: its
/// note says why, beside the menu.
fn note(tour: &mut Tour<'_>) -> Result<(), String> {
    let needs = sue_wm::note_needs_selection();
    let action = |slot: Slot, key: &str| {
        MenuItem::action(slot.label(""), slot.command().unwrap_or_default()).key(key)
    };
    let items = vec![
        action(Slot::Undo, "Ctrl+Z"),
        action(Slot::Redo, "Ctrl+Y"),
        MenuItem::Separator,
        action(Slot::Cut, "Ctrl+X").note(needs.clone()),
        action(Slot::Copy, "Ctrl+C").note(needs),
        action(Slot::Paste, "Ctrl+V"),
        MenuItem::Separator,
        action(Slot::SelectAll, "Ctrl+A"),
        MenuItem::sub(
            Slot::Recent.label(""),
            vec![action(Slot::Save, "Ctrl+S"), action(Slot::SaveAs, "")],
        ),
    ];
    let state = |command: &str| match command {
        "edit.cut" | "edit.copy" | "edit.redo" => ItemState::off(),
        _ => ItemState::on(),
    };
    let mut menu = MenuState::new();
    menu.open_at_once();
    // Down three times: Undo, Redo, then over the line to Cut.
    let mut events = vec![Vec::new()];
    events.extend(std::iter::repeat_n(key(Key::ArrowDown), 3));
    // The note shows once the keyboard has rested on the row.
    events.extend(std::iter::repeat_n(Vec::new(), 90));
    shoot_ui(tour, "29d-menu-note", events, |ui, theme| {
        menu.show(
            ui.ctx(),
            &MenuRequest {
                id: egui::Id::unique("snapshot-menu-note"),
                anchor: Pos2::new(120.0, 100.0),
                owner: Rect::NOTHING,
                items: &items,
                state: &state,
                density: Density::Compact,
            },
            theme,
        );
    })
}

/// Scene 29e. Tooltips: the Sue button's (its name and a line of help),
/// shown by the pointer resting on it, and one with a name and a key.
fn tip(tour: &mut Tour<'_>) -> Result<(), String> {
    let button = Rect::from_min_size(Pos2::new(120.0, 100.0), vec2(64.0, 30.0));
    let save = Rect::from_min_size(Pos2::new(120.0, 400.0), vec2(64.0, 30.0));
    let help = sue_lang::tr!("wm", "sue-hub-every-studio-tool-files");
    let save_label = Slot::Save.label("");
    let mut events = vec![Vec::new(), vec![Event::PointerMoved(button.center())]];
    events.extend(std::iter::repeat_n(Vec::new(), 90));
    shoot_ui(tour, "29e-tooltip", events, |ui, theme| {
        let response = ui.put(button, egui::Button::new("Sue"));
        tooltip(response, &Tip::new("Sue").help(&help), theme);
        ui.put(save, egui::Button::new(save_label.as_str()));
        tip_beside(
            ui.ctx(),
            egui::Id::unique("snapshot-tip-save"),
            save,
            Side::Below,
            &Tip::new(&save_label).shortcut("Ctrl+S"),
            theme,
        );
    })
}

/// Runs one frame of `draw` for each list of `events`, alone and without
/// the shell, in the theme of the run, and writes the last frame as the
/// picture `name`: a sheet for review that is no part of Sue's interface.
fn shoot_ui(
    tour: &mut Tour<'_>,
    name: &str,
    events: Vec<Vec<Event>>,
    mut draw: impl FnMut(&mut egui::Ui, &sue_wm::Theme),
) -> Result<(), String> {
    tour.settle();
    let theme = tour.runtime.theme().clone();
    let camera = &mut tour.camera;
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        vec2(camera.size[0] as f32, camera.size[1] as f32),
    );
    let count = events.len();
    let mut output = None;
    for (index, events) in events.into_iter().enumerate() {
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
        let frame = camera.ctx.run_ui(raw, |ui| {
            ui.painter().rect_filled(rect, 0.0, theme.panel);
            draw(ui, &theme);
        });
        camera.now += std::time::Duration::from_millis(8);
        if index + 1 == count {
            output = Some(frame);
        } else {
            camera.painter.apply_textures(frame.textures_delta);
        }
    }
    let output = output.ok_or("a picture needs a frame")?;
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

/// Scenes 33a to 33e. Sue Paint's File, Animation, Window and Help menus
/// in Clip Studio Paint's order, Title Case, with the keys that run them.
fn clip_studio_order(tour: &mut Tour<'_>) -> Result<(), String> {
    for (menu, name) in [
        ("File", "33a-menu-file"),
        ("Animation", "33b-menu-animation"),
        ("Window", "33c-menu-window"),
        ("Help", "33d-menu-help"),
    ] {
        tour.paint_go(&[&format!("menu/{menu}")]);
        tour.shoot(name)?;
        tour.camera.input.push(key(Key::Escape));
        tour.settle();
    }
    // Scene 33e. Window, Canvas: the commands, then the open canvases,
    // numbered, the current one checked.
    tour.paint_go(&["menu/Window"]);
    tour.camera.input.push(key(Key::ArrowDown));
    tour.camera.input.push(key(Key::ArrowRight));
    tour.shoot("33e-menu-window-canvas")?;
    tour.camera.input.push(key(Key::Escape));
    tour.camera.input.push(key(Key::Escape));
    tour.settle();
    Ok(())
}
