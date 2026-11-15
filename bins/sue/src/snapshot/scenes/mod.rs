//! The scenes of the snapshot tour, one file per area of Sue, so that two
//! pieces of work on different areas never edit the same file.
//!
//! [`run`] is the order of the tour. A scene starts where the one before
//! it left Sue, so a new scene goes into its area's file and is called
//! from here at the point where Sue is in the state it needs. A scene
//! that changes something for its picture puts it back before it returns.
//! What scenes cannot be trusted to put back is reset here: notices go
//! after every picture (`Tour::shoot`), and Sue Paint's palettes are put
//! at rest ([`paint_at_rest`]) where a run of scenes moved them.

mod gallery;
mod hub;
mod icons;
mod menus;
mod paint_3d;
mod paint_canvas;
mod paint_canvas_chrome;
mod paint_color;
mod paint_comic;
mod paint_command_bar;
mod paint_dialogs;
mod paint_dock;
mod paint_engine;
mod paint_export;
mod paint_extras;
mod paint_figure;
mod paint_fill;
mod paint_filters;
mod paint_gradient;
mod paint_handles;
mod paint_jobs;
mod paint_layers;
mod paint_modifiers;
mod paint_saving;
mod paint_start;
mod paint_timeline;
mod paint_tools;
mod paint_touch;
mod paint_vector;
mod paint_view;
mod rich_text;
mod settings;
mod shell;
mod shell_polish;
mod ui_buttons;
mod ui_dialogs;
mod ui_export;
mod ui_inputs;
mod writer_bars;
mod writer_compile;
mod writer_corkboard;
mod writer_dialogs;
mod writer_document;
mod writer_outliner;
mod writer_panes;
mod writer_start;
mod writer_touch;

use sue_rice::RiceValue;
use sue_wm::ShellRequest;

use super::Tour;

/// Runs every scene, in the order of the tour.
pub(super) fn run(tour: &mut Tour<'_>) -> Result<(), String> {
    // 00 to 03. The Hub and what the shell lays over it.
    hub::opening(tour)?;
    // 00n to 00q. Notices: folded, unfolded, under a dialog, on a phone.
    shell_polish::notices(tour)?;
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
    paint_handles::scenes(tour)?;
    paint_dialogs::galleries(tour)?;
    // Before 08o, whose own steps then replace this scene's undone ones
    // in History, so the scenes after see what they saw before it.
    paint_figure::tool(tour)?;
    paint_canvas::figure(tour)?;
    paint_fill::shrink_selection(tour)?;
    paint_gradient::scenes(tour)?;
    paint_layers::rows(tour)?;
    paint_3d::tool_property(tour)?;
    shell::sue_menu_in_paint(tour)?;
    paint_dialogs::menu_and_dialogs(tour)?;
    paint_dialogs::families(tour)?;
    paint_dialogs::pages(tour)?;
    paint_modifiers::scenes(tour)?;
    paint_comic::pages(tour)?;
    paint_comic::frames(tour)?;
    paint_filters::dialogs(tour)?;
    paint_timeline::timeline(tour)?;
    // 11j1 to 11j3. Exports in the background, and Export Timelapse.
    paint_jobs::exports(tour)?;
    paint_view::marks(tour)?;
    paint_canvas_chrome::window(tour)?;
    paint_layers::property(tour)?;
    paint_at_rest(tour);
    paint_color::scenes(tour)?;
    paint_dock::scenes(tour)?;
    // The floating Color Set palette, rolled up, goes back in the dock.
    paint_at_rest(tour);
    paint_command_bar::scenes(tour)?;
    paint_extras::scenes(tour)?;
    paint_tools::palettes(tour)?;

    // 12 and 13. Back to the Hub, then into Sue Paint again.
    shell::entering(tour)?;

    // 14 to 19a. Sue Writer on a sample novel.
    writer_document::open(tour)?;
    writer_bars::find_bar(tour)?;
    writer_bars::font_list(tour)?;
    writer_bars::text_color(tour)?;
    writer_corkboard::corkboard(tour)?;
    writer_outliner::outliner(tour)?;
    writer_panes::inspector(tour)?;
    writer_document::composition(tour)?;
    writer_compile::dialog(tour)?;
    writer_compile::export_and_tables(tour)?;
    writer_dialogs::dialogs(tour)?;
    writer_document::format_menu(tour)?;
    writer_start::desktop(tour)?;
    writer_document::comments(tour)?;
    // 19a1. What the rich text editor marks on the page.
    rich_text::marks(tour)?;

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

    // 24 to 26. Settings shaped to each screen. The desktop window's
    // size comes back after its last, small window.
    settings::screens(tour)?;
    tour.camera.size = tour.desktop;

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
    // PNG, Sue Paint's palettes at rest again after the touch layouts.
    paint_at_rest(tour);
    paint_saving::questions(tour)?;
    // 36a to 36d. Sue Paint's export settings, Batch Convert Pictures
    // and Export Animation, Audio.
    paint_export::scenes(tour)?;
    // 28n to 28r. The export settings dialog and the batch list.
    ui_export::scenes(tour)?;
    // 36a and 36b. Picked vector lines and Vector magnet, last, since
    // their steps stay in Sue Paint's history.
    paint_vector::scenes(tour)?;
    // 08s. A pen line painted by the brush engine, after everything.
    paint_engine::pen_stroke(tour)?;
    // 08x. A line of each family of brushes, after 08s.
    paint_engine::brush_strokes(tour)?;
    Ok(())
}

/// Shows one view, dialog or menu of Sue Writer over the novel's pages and
/// writes the picture `name`.
fn writer_view(tour: &mut Tour<'_>, target: &str, name: &str) -> Result<(), String> {
    tour.writer_go(&["dialog/close", "view/document", target]);
    tour.shoot(name)
}

/// Puts Sue Paint's palettes at rest: the workspace of the layout style
/// (no palette floating, rolled up or brought out by an earlier scene,
/// the Timeline closed), no dialog, menu or popover, and its palettes on
/// the tabs they open with. Sue Paint comes to the front.
fn paint_at_rest(tour: &mut Tour<'_>) {
    tour.paint_go(&["dialog/close", "bar/close", "dock/menu"]);
    tour.paint_command("window.workspace.reset");
    tour.paint_go(&["tab/tool/1", "tab/layer/0"]);
    tour.settle();
}

/// Goes back to the Hub.
fn to_the_hub(tour: &mut Tour<'_>) {
    tour.request(ShellRequest::Hub);
}

/// Opens Sue Paint.
fn open_paint(tour: &mut Tour<'_>) {
    tour.request(ShellRequest::Open(tour.paint.clone()));
}
