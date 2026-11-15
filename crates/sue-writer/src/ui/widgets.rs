//! Small parts the panes share: the desk's color, drop zones, the tool
//! buttons of the bars and the icons.
//!
//! Every picture is an icon of `sue_wm::icons`, drawn by its stable name,
//! so an icon pack replaces it. The painters at the end of this file
//! (`menu_icon` and the rest) keep the shapes of the drawings Sue Writer
//! had before the shared set: each now paints its `Icon`, so the panes
//! that call them show the shared icons. They go when their last caller
//! draws the `Icon` itself.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Ui, Vec2};
use sue_core::EntityId;
use sue_wm::Theme;
use sue_wm::icons::{self, Icon, IconStyle};
use sue_wm::ui::{Density, Radius, icon_button};

use super::TOOL;

/// The desk around the pages: the page's color, a step lighter.
pub(super) fn desk(theme: &Theme) -> Color32 {
    theme.void.lerp_to_gamma(theme.surface(1), DESK_LIFT)
}

/// How far the desk moves from the page's color toward a panel's.
const DESK_LIFT: f32 = 0.5;

pub(super) fn texture_key(id: EntityId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-writer-figure", id).hash(&mut hasher);
    hasher.finish()
}

/// Where a drop lands on a binder row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zone {
    Before,
    Into,
    After,
}

pub(super) fn drop_zone(rect: Rect, pointer: Pos2, folder: bool) -> Zone {
    let t = (pointer.y - rect.top()) / rect.height().max(1.0);
    if folder && (0.25..0.75).contains(&t) {
        Zone::Into
    } else if t < 0.5 {
        Zone::Before
    } else {
        Zone::After
    }
}

/// A button of a bar that shows `icon`: `name` is read out and shown in
/// its tooltip with `keys` (empty for none). It takes the keyboard's
/// focus only by Tab, so a click leaves the caret in the text; its size
/// is the density's of `ui`.
pub(super) fn tool(
    ui: &mut Ui,
    theme: &Theme,
    salt: &str,
    icon: Icon,
    name: &str,
    keys: &str,
) -> Response {
    icon_button(salt, icon, name)
        .quiet()
        .shortcut(keys)
        .show(ui, theme)
}

/// As [`tool`], lit while `on` (Bold while the text is bold).
pub(super) fn tool_toggle(
    ui: &mut Ui,
    theme: &Theme,
    salt: &str,
    icon: Icon,
    (name, keys): (&str, &str),
    on: bool,
) -> Response {
    icon_button(salt, icon, name)
        .quiet()
        .selected(on)
        .shortcut(keys)
        .show(ui, theme)
}

/// A thumb-sized button with a drawn icon at `rect` (a phone's top bar).
pub(super) fn thumb(
    ui: &mut Ui,
    rect: Rect,
    on: bool,
    enabled: bool,
    theme: &Theme,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    let sense = if enabled { TOOL } else { Sense::hover() };
    let id = ui
        .scope_id()
        .with(("writer-thumb", rect.min.x as i32, rect.min.y as i32));
    let response = ui.interact(rect, id, sense);
    let radius = theme.radius(Radius::Md);
    if on {
        ui.painter()
            .rect_filled(rect, radius, theme.fill_selected());
    } else if response.hovered() && enabled {
        ui.painter().rect_filled(rect, radius, theme.fill_hover());
    }
    let color = if !enabled {
        theme.text_disabled(theme.panel)
    } else if on || response.hovered() {
        theme.text
    } else {
        theme.text_muted()
    };
    let side = Density::Touch.icon().min(rect.width().min(rect.height()));
    draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        color,
    );
    response
}

/// Paints `icon` in `color` over `rect`, solid when `solid`.
fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32, solid: bool) {
    let style = if solid {
        IconStyle::Solid
    } else {
        IconStyle::Line
    };
    icons::paint_styled(painter, rect, icon, style, color);
}

/// The menus.
pub(super) fn menu_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Menu, color, false);
}

/// Undo, or Redo.
pub(super) fn undo_icon(painter: &Painter, rect: Rect, color: Color32, redo: bool) {
    let icon = if redo { Icon::Redo } else { Icon::Undo };
    paint(painter, rect, icon, color, false);
}

/// Find.
pub(super) fn find_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Search, color, false);
}

/// The comments.
pub(super) fn comment_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Comment, color, false);
}

/// Save as PDF.
pub(super) fn export_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::ExportPdf, color, false);
}

/// A table.
pub(super) fn table_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Table, color, false);
}

/// A document, or one with no words yet.
pub(super) fn page_icon(painter: &Painter, rect: Rect, color: Color32, empty: bool) {
    let icon = if empty { Icon::PageEmpty } else { Icon::Page };
    paint(painter, rect, icon, color, false);
}

/// A folder of the binder.
pub(super) fn folder_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Folder, color, false);
}

/// The Draft folder, and the binder that holds it.
pub(super) fn book_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Binder, color, false);
}

/// The Trash folder.
pub(super) fn trash_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Delete, color, false);
}

/// A picture.
pub(super) fn picture_icon(painter: &Painter, rect: Rect, color: Color32) {
    paint(painter, rect, Icon::Picture, color, false);
}

/// The icon of an alignment.
pub(super) const fn align_icon(align: sue_type::Align) -> Icon {
    match align {
        sue_type::Align::Left => Icon::AlignLeft,
        sue_type::Align::Center => Icon::AlignCenter,
        sue_type::Align::Right => Icon::AlignRight,
        sue_type::Align::Justify => Icon::AlignJustify,
    }
}
