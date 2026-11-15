//! Small parts the panes share: the desk's color, drop zones, the tool
//! buttons of the bars and the icons.
//!
//! Every picture is an icon of `sue_wm::icons`, drawn by its stable name,
//! so an icon pack replaces it. The painters at the end of this file
//! (`menu_icon` and the rest) keep the shapes of the drawings Sue Writer
//! had before the shared set: each now paints its `Icon`, so the panes
//! that call them show the shared icons. They go when their last caller
//! draws the `Icon` itself.

use egui::{Color32, Pos2, Rect, Response, Ui};
use sue_core::EntityId;
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::icon_button;

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

/// The icon of an alignment.
pub(super) const fn align_icon(align: sue_type::Align) -> Icon {
    match align {
        sue_type::Align::Left => Icon::AlignLeft,
        sue_type::Align::Center => Icon::AlignCenter,
        sue_type::Align::Right => Icon::AlignRight,
        sue_type::Align::Justify => Icon::AlignJustify,
    }
}
