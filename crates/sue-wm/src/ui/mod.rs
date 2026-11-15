//! Sue's design system: the tokens every surface is drawn from, the one
//! motion helper and the focus ring.
//!
//! - [`tokens`]: radii, the spacing scale, control sizes per [`Density`],
//!   the type scale, strokes, elevation, the layer order and the color
//!   roles. All of them are read from a [`Theme`](crate::Theme), which
//!   reads them from the rice, so a settings file restyles every one.
//! - [`motion`]: timed transitions and springs that honour
//!   `motion.instant_transitions` and `motion.animation_speed`, and ask
//!   for a new frame only while something moves.
//! - [`focus`]: the focus ring, and a [`FocusScope`] that gives a group of
//!   controls a first focus, keeps Tab inside it and walks it with the
//!   arrow keys.
//!
//! The components built on these (buttons, fields, menus, dialogs) get one
//! file each beside these three.
//!
//! - [`mod@button`]: buttons in four kinds, toggle, icon and split buttons,
//!   links.
//! - [`choice`]: checkboxes, radio buttons, switches, segmented strips
//!   and chips.
//! - [`tabs`]: the tab strip (icons and names, folding to icons, dragged
//!   to reorder and torn off) and the column of side tabs.
//! - [`mod@card`]: cards, groups and sections, tiles and thumbnail cards.
//! - [`list`]: list rows, tree rows, the empty state, dividers and
//!   headings.
//! - [`progress`]: the progress bar, the spinner and the skeleton.
//! - [`scroll`]: the scroll view, with thin bars over its content and
//!   edges that fade where there is more.
//! - [`mod@splitter`]: the line between two panes.
//! - [`input`]: text, search and number fields, the dropdown and the
//!   combo box.
//! - [`mod@slider`]: the slider, the tall opacity slider, the range and
//!   the wedge.
//! - [`menu`]: the one menu of the menu bar, the Sue menu, context menus
//!   and popovers.
//! - [`mod@tooltip`]: tooltips and the note beside a menu row.
//! - [`dialog`]: the dialog host, the confirmation, the sheet and form
//!   rows.
//! - [`toast`]: the stack of notices above the status bar.

pub mod button;
pub mod card;
pub mod choice;
pub mod dialog;
pub mod focus;
pub mod input;
pub mod list;
pub mod menu;
pub mod motion;
pub mod progress;
pub mod scroll;
pub mod slider;
pub mod splitter;
pub mod tabs;
pub mod toast;
pub mod tokens;
pub mod tooltip;

pub use button::{
    Button, ButtonKind, IconButton, Link, SplitButton, SplitResponse, button, icon_button, link,
    split_button, toggle_button,
};
pub use card::{
    Card, Thumbnail, ThumbnailCard, ThumbnailOutput, Tile, card, group, section, thumbnail_card,
    tile,
};
pub use choice::{
    Check, Chip, ChipResponse, Marked, Segmented, SegmentedResponse, checkbox, chip, radio, rove,
    segmented, switch,
};
pub use dialog::{
    Answer, Dialog, DialogHost, DialogResponse, FormGrid, Sheet, SheetResponse, confirm, form_grid,
    form_row,
};
pub use focus::{FocusScope, Roving, focus_ring, focus_visible, paint_focus_ring, watch_focus};
pub use input::{
    Combo, Dropdown, NumberField, TextField, combo, dropdown, number_field, picker_box,
    search_field, text_field,
};
pub use list::{
    EmptyState, Leading, ListRow, RowAction, RowOutput, TreeOutput, TreeRow, divider, empty_state,
    heading, list_row, tree_row,
};
pub use menu::{MenuEvent, MenuRequest, MenuState, context_menu, popover};
pub use motion::{Motion, Pace};
pub use progress::{
    ProgressBar, Skeleton, Spinner, paint_skeleton, progress_bar, skeleton, spinner,
};
pub use scroll::ScrollView;
pub use slider::{
    RangeSlider, Scale, Slider, TallSlider, Wedge, range, slider, tall_slider, wedge,
};
pub use splitter::{SplitAxis, Splitter, splitter};
pub use tabs::{SideOutput, SidebarTabs, Tab, TabStrip, TabsOutput, sidebar_tabs, tab_strip};
pub use toast::{Toast, ToastEvent, ToastId, ToastRects, Toasts, Tone};
pub use tokens::{
    Density, EDGE_CONTRAST, Elevation, Layer, MIN_TEXT_SIZE, Radius, Space, TEXT_CONTRAST,
    TextRole, contrast, nested_radius,
};
pub use tooltip::{Side, Tip, tip_beside, tooltip};
