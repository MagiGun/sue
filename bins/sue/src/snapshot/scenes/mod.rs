//! The scenes of the snapshot tour, one file per area of Sue, so that two
//! pieces of work on different areas never edit the same file.
//!
//! [`run`] is the order of the tour. A scene starts where the one before
//! it left Sue, so a new scene goes into its area's file and is called
//! from here at the point where Sue is in the state it needs. A scene
//! that changes something for its picture puts it back before it returns.

mod gallery;
mod hub;
mod icons;
mod menus;
mod paint_canvas;
mod paint_color;
mod paint_dialogs;
mod paint_dock;
mod paint_layers;
mod paint_saving;
mod paint_start;
mod paint_timeline;
mod paint_tools;
mod paint_touch;
mod paint_view;
mod settings;
mod shell;
mod ui_buttons;
mod ui_dialogs;
mod ui_export;
mod ui_inputs;
mod writer_bars;
mod writer_compile;
mod writer_corkboard;
mod writer_document;
mod writer_outliner;
mod writer_start;
mod writer_touch;

use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::Tour;

/// Runs every scene, in the order of the tour.
pub(super) fn run(tour: &mut Tour<'_>) -> Result<(), String> {
    // 00 to 03. The Hub and what the shell lays over it.
    hub::opening(tour)?;
    shell::sue_menu(tour)?;
    shell::command_palette(tour)?;

    // 04 and 05. Settings in a computer's window.
    settings::desktop(tour)?;

    // 08a to 08z. Sue Paint with no canvas open, and its Materials place.
    paint_start::desktop(tour)?;

    // 08 to 11. Sue Paint with a sample illustration.
    paint_canvas::sample(tour)?;
    paint_tools::tabs(tour)?;
    paint_layers::history(tour)?;
    paint_tools::first_tab(tour);
    paint_layers::first_tab(tour);
    paint_canvas::launchers(tour)?;
    paint_dialogs::galleries(tour)?;
    paint_canvas::figure(tour)?;
    shell::sue_menu_in_paint(tour)?;
    paint_dialogs::menu_and_dialogs(tour)?;
    paint_dialogs::pages(tour)?;
    paint_timeline::timeline(tour)?;
    paint_view::marks(tour)?;
    paint_layers::property(tour)?;
    paint_color::scenes(tour);
    paint_dock::scenes(tour)?;

    // 12 and 13. Back to the Hub, then into Sue Paint again.
    shell::entering(tour)?;

    // 14 to 19a. Sue Writer on a sample novel.
    writer_document::open(tour)?;
    writer_bars::find_bar(tour)?;
    writer_corkboard::corkboard(tour)?;
    writer_outliner::outliner(tour)?;
    writer_document::composition(tour)?;
    writer_compile::dialog(tour)?;
    writer_compile::export_and_tables(tour)?;
    writer_document::format_menu(tour)?;
    writer_start::desktop(tour)?;
    writer_document::comments(tour)?;

    // 19b to 19k. The Hub with work in it (and 31a to 31f: Home and
    // Files in the other presets), languages and the Layout menu.
    hub::lists(tour)?;
    shell::languages(tour)?;
    shell::layouts(tour)?;
    // 19l and 19m. Notices, and the question when quitting.
    shell::overlays(tour)?;

    // 20 to 23. The touch layouts: the Hub, Sue Paint and Sue Writer on a
    // phone and on a tablet.
    for (interface, size, prefix) in [
        ("phone", [430_u32, 932_u32], "20-phone"),
        ("tablet", [1180, 820], "22-tablet"),
    ] {
        tour.set("display.interface", RiceValue::Text(interface.to_owned()));
        tour.camera.size = size;
        hub::touch(tour, prefix)?;
        paint_touch::touch(tour, prefix)?;
        writer_touch::touch(tour, prefix, interface == "phone")?;
    }

    // 24 to 26. Settings shaped to each screen.
    settings::screens(tour)?;

    // 27. The icons: the reference sheet, then every icon.
    icons::contact_sheets(tour)?;

    // 28a and 28b. The buttons and the choices of the design system.
    ui_buttons::sheets(tour)?;
    // 28c to 28g. The inputs and sliders of the component library.
    ui_inputs::sheets(tour)?;
    // 29a to 29e. Menus walked with the keyboard, a menu taller than the
    // window, the note of a row and tooltips.
    menus::scenes(tour)?;
    // 28h to 28m. The dialog host, the sheet and the toast stack.
    ui_dialogs::gallery(tour)?;
    // 30. The gallery of the design system: per preset, at 1.5 and 2.0
    // pixels per point and per density, with the focus ring showing.
    gallery::scenes(tour)?;
    // 35a and 35b. Save's question on a canvas from a .clip and from a
    // PNG.
    paint_saving::questions(tour)?;
    // 28n to 28r. The export settings dialog and the batch list.
    ui_export::scenes(tour)?;
    Ok(())
}

/// Shows one view, dialog or menu of Sue Writer over the novel's pages and
/// writes the picture `name`.
fn writer_view(tour: &mut Tour<'_>, target: &str, name: &str) -> Result<(), String> {
    tour.writer_go(&["dialog/close", "view/document", target]);
    tour.shoot(name)
}

/// Goes back to the Hub.
fn to_the_hub(tour: &mut Tour<'_>) {
    tour.request(ShellRequest::Hub);
}

/// Opens Sue Paint.
fn open_paint(tour: &mut Tour<'_>) {
    tour.request(ShellRequest::Open(tour.paint.clone()));
}
