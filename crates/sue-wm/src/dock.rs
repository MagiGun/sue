//! A dock for a studio's palettes, as Clip Studio Paint's: columns at the
//! left and right of the work area, a row across the top of the window (a
//! command bar's) and a row along the work area's bottom, each made of
//! palette groups that share the room by weight; a group holds one palette
//! or several as tabs. Palettes can also float in windows inside the
//! studio, pop out into windows of their own (outside the main window
//! too), or collapse to icons in a strip at the right.
//!
//! People drag a palette by its title bar, its tab or its icon onto a
//! group (it becomes a tab there), above, below or beside a group (a group
//! of its own), against a column's edge (a column of its own), onto the
//! top or the bottom, into the strip, or anywhere else to float it; they
//! drag the lines between columns, rows and groups to share the room
//! differently. Every column resizes, the narrow ones (a tool strip) too.
//! A studio may also have optional palettes that are nowhere until people
//! tear them off another palette's tab ([`DockPanel::optional`],
//! [`DockLayout::begin_drag`]). The layout reads and writes as lines of
//! text ([`DockLayout::to_lines`]), so a studio keeps it between runs and
//! under names (workspaces).
//!
//! The studio says what its palettes are ([`DockPanel`]) and draws them in
//! the rects [`DockLayout::layout`] gives; the dock draws the tabs, the
//! splitters and the drop targets and does the moving.

use egui::{
    Align2, Color32, CursorIcon, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};

use crate::Theme;
use crate::icons::Icon;
use crate::ui::{Density, Tab, TabStrip};

/// How a palette's tab in a dock header looks: its name, its icon and the
/// keys that show it (empty for none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabFace {
    /// What the palette is called.
    pub name: String,
    /// Its icon; a tab with one shows only it when names do not fit.
    pub icon: Option<Icon>,
    /// The keys that show it, for its tooltip.
    pub keys: String,
}

/// A palette a studio's dock holds.
pub trait DockPanel: Copy + Eq + std::fmt::Debug + 'static {
    /// Every palette the studio has.
    fn all() -> &'static [Self];

    /// Its name in saved layouts; never translated, never changed.
    fn key(self) -> &'static str;

    /// The palette a saved layout names: its key, or a name older layouts
    /// used for it.
    fn named(text: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|panel| panel.key() == text)
    }

    /// The width a narrow palette (a tool strip) starts at in a column of
    /// its own; such a palette never becomes a tab.
    fn fixed_width(self) -> Option<f32> {
        None
    }

    /// The least width it is usable at, when people drag its column's edge.
    fn min_width(self) -> f32 {
        self.fixed_width().map_or(170.0, |width| width.min(170.0))
    }

    /// A palette that is nowhere until people tear it off another
    /// palette's tab (a missing one is not put back where the default
    /// layout has it).
    fn optional(self) -> bool {
        false
    }

    /// The least height it is usable at.
    fn min_height(self) -> f32 {
        90.0
    }

    /// The size of its window when it floats.
    fn float_size(self) -> Vec2 {
        vec2(260.0, 320.0)
    }

    /// It stays open when its side folds into icons or the window runs
    /// out of room (a tool strip).
    fn anchored(self) -> bool {
        false
    }

    /// Whether the dock draws its name over it in a header, also when it
    /// is alone in its group. A group with more than one palette always
    /// has one, its tabs. A headed palette draws no title of its own: the
    /// header is its one title, and dragging it moves the palette.
    fn headed(self) -> bool {
        false
    }

    /// The room its header leaves at its left, for a button the studio
    /// draws there (a palette's menu).
    fn header_inset(self) -> f32 {
        0.0
    }

    /// Every layout style the studio has, the default one first. A studio
    /// with a single design may leave it empty and give its layout in
    /// [`Self::default_layout`] alone.
    fn styles() -> &'static [LayoutStyle<Self>] {
        &[]
    }

    /// Where the palettes are until people move them, and after Reset,
    /// for callers without a style at hand: the first style's layout
    /// (the first built one, so a grayed style is never the default).
    fn default_layout() -> DockLayout<Self> {
        Self::styles()
            .iter()
            .find(|style| style.available)
            .map_or_else(DockLayout::empty, |style| (style.layout)())
    }

    /// The style `id` names when it can be chosen, else the first style
    /// that can (a settings file may name a style that is not built yet,
    /// or one a later version dropped). `None` only without styles.
    fn style(id: &str) -> Option<&'static LayoutStyle<Self>> {
        let styles = Self::styles();
        styles
            .iter()
            .find(|style| style.available && style.id == id)
            .or_else(|| styles.iter().find(|style| style.available))
    }

    /// The layout of the style `id` names ([`Self::style`]), or
    /// [`Self::default_layout`] without styles.
    fn style_layout(id: &str) -> DockLayout<Self> {
        Self::style(id).map_or_else(Self::default_layout, |style| (style.layout)())
    }
}

/// One way a studio's workspace is designed: which palettes there are and
/// where they sit until people move them. A studio lists its styles in
/// [`DockPanel::styles`]; people pick one, and Reset goes back to that
/// style's layout ([`DockLayout::reset`]).
#[derive(Clone, Copy, Debug)]
pub struct LayoutStyle<P: DockPanel> {
    /// Its name in settings files; never translated, never changed.
    pub id: &'static str,
    /// The language database id of the name people read.
    pub label: &'static str,
    /// `false` lists it grayed out: it is not built yet and can never be
    /// the one in use.
    pub available: bool,
    /// Where the palettes are until people move them, and after Reset.
    pub layout: fn() -> DockLayout<P>,
}

/// A side of the work area with columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// Its place in [`DockLayout::folded`] and [`DockLayout::hidden`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// A row of the dock: across the top of the window, or along the work
/// area's bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Top,
    Bottom,
}

impl Row {
    /// Its place in [`DockLayout::row_folded`] and
    /// [`DockLayout::row_hidden`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Row::Top => 0,
            Row::Bottom => 1,
        }
    }

    /// What holds its groups.
    #[must_use]
    pub fn holder(self) -> Holder {
        match self {
            Row::Top => Holder::Top,
            Row::Bottom => Holder::Bottom,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Row::Top => "top",
            Row::Bottom => "bottom",
        }
    }
}

/// Palettes stacked as tabs, one showing, with a share of their column's
/// height (or of the bottom row's width).
#[derive(Clone, Debug, PartialEq)]
pub struct Group<P> {
    /// The tabs, in order.
    pub tabs: Vec<P>,
    /// The tab showing.
    pub active: usize,
    /// Its share of the room.
    pub weight: f32,
}

impl<P: DockPanel> Group<P> {
    /// A group of one palette.
    #[must_use]
    pub fn one(panel: P, weight: f32) -> Self {
        Self {
            tabs: vec![panel],
            active: 0,
            weight,
        }
    }

    /// The tabs `shown` lets show.
    fn visible(&self, shown: &dyn Fn(P) -> bool) -> Vec<P> {
        self.tabs
            .iter()
            .copied()
            .filter(|tab| shown(*tab))
            .collect()
    }

    /// The tab showing: the chosen one, or the first that may show.
    fn current(&self, shown: &dyn Fn(P) -> bool) -> Option<P> {
        self.tabs
            .get(self.active)
            .copied()
            .filter(|tab| shown(*tab))
            .or_else(|| self.visible(shown).first().copied())
    }

    fn fixed(&self) -> bool {
        self.tabs.iter().any(|tab| tab.fixed_width().is_some())
    }
}

/// A column of palette groups sharing its height by weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Column<P> {
    /// The groups, top to bottom.
    pub groups: Vec<Group<P>>,
    /// Its width in points (palettes with a fixed width decide it).
    pub width: f32,
}

impl<P: DockPanel> Column<P> {
    /// A column of one palette per group, each with its weight.
    #[must_use]
    pub fn of(panels: &[(P, f32)], width: f32) -> Self {
        Self {
            groups: panels
                .iter()
                .map(|(panel, weight)| Group::one(*panel, *weight))
                .collect(),
            width,
        }
    }

    fn anchored(&self) -> bool {
        self.groups
            .iter()
            .any(|group| group.tabs.iter().any(|tab| tab.anchored()))
    }
}

/// What holds a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Holder {
    /// Column `usize` on a side, counted from the window's edge inward.
    Column(Side, usize),
    /// The row across the top.
    Top,
    /// The row along the bottom.
    Bottom,
}

impl Holder {
    /// A row: its groups sit side by side.
    #[must_use]
    pub fn is_row(self) -> bool {
        matches!(self, Holder::Top | Holder::Bottom)
    }
}

/// Where a dragged palette goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target<P> {
    /// A tab in `anchor`'s group: just before `anchor` when `before`, else
    /// the last.
    Join { anchor: P, before: bool },
    /// A group of its own just before (above, or left of in the bottom
    /// row) or after `anchor`'s group.
    Beside { anchor: P, after: bool },
    /// A column of its own on `side`: next to `anchor`'s column, inward
    /// when the flag is `true`, or the innermost.
    Column {
        side: Side,
        beside: Option<(P, bool)>,
    },
    /// A group of its own at the end of the top row.
    Top,
    /// A group of its own at the end of the bottom row.
    Bottom,
    /// An icon in the strip.
    Icon,
    /// A window of its own at this place.
    Float(Pos2),
}

/// What is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag<P> {
    /// A column's inner edge.
    Edge {
        side: Side,
        column: usize,
        from: f32,
        width: f32,
    },
    /// The line between two groups.
    Split {
        holder: Holder,
        upper: usize,
        lower: usize,
        from: f32,
        weights: (f32, f32),
        per_point: f32,
    },
    /// The bottom row's top edge: where the drag began, the row's height
    /// as it was drawn then, and the most it may grow to (the room the
    /// layout leaves it).
    Bottom { from: f32, height: f32, most: f32 },
    /// The top row's bottom edge.
    Top { from: f32, height: f32 },
    /// A palette, by its title bar, tab or icon: where the drag began and
    /// where the pointer was last seen (a pen or a finger goes away in the
    /// same moment it lifts, so the drop happens there).
    Panel { panel: P, from: Pos2, at: Pos2 },
}

/// Where every palette is.
#[derive(Clone, Debug, PartialEq)]
pub struct DockLayout<P> {
    /// Columns from the window's left edge inward.
    pub left: Vec<Column<P>>,
    /// Columns from the window's right edge inward.
    pub right: Vec<Column<P>>,
    /// The row across the top of the window, left to right.
    pub top: Vec<Group<P>>,
    /// The top row's height.
    pub top_height: f32,
    /// The row along the bottom of the work area, left to right.
    pub bottom: Vec<Group<P>>,
    /// The bottom row's height.
    pub bottom_height: f32,
    /// Floating palettes and their windows' rects, inside the studio.
    pub floating: Vec<(P, Rect)>,
    /// Palettes popped out into windows of their own and those windows'
    /// rects, relative to the main window (they may lie outside it).
    pub windows: Vec<(P, Rect)>,
    /// Palettes collapsed to icons in the strip.
    pub iconized: Vec<P>,
    /// The collapsed palette showing beside the strip.
    pub popped: Option<P>,
    /// The palette whose menu is open.
    pub menu: Option<P>,
    /// Each side's columns folded into icons (Clip Studio Paint's single
    /// arrow), left then right. Anchored palettes stay.
    pub folded: [bool; 2],
    /// Each side closed whole (Clip Studio Paint's double arrow).
    pub hidden: [bool; 2],
    /// Each row folded into a strip of icons, top then bottom (a Timeline
    /// that is only an icon under the work area).
    pub row_folded: [bool; 2],
    /// Each row closed whole, leaving a thin bar that opens it again.
    pub row_hidden: [bool; 2],
    /// Collapsed palettes that share an icon group with the one before
    /// them in [`Self::iconized`] (they were tabs of one group).
    pub icon_joined: Vec<P>,
    /// Palettes people closed for good: [`Self::fill_missing`] leaves them
    /// closed, and saved layouts remember it.
    pub closed: Vec<P>,
    /// Floating palettes rolled up to their header (Clip Studio Paint's
    /// Minimize). Not kept in the layout's lines: a palette opens whole.
    pub rolled: Vec<P>,
    drag: Option<Drag<P>>,
}

impl<P: DockPanel> Default for DockLayout<P> {
    fn default() -> Self {
        P::default_layout()
    }
}

/// The width of the icon strip.
pub const STRIP: f32 = 34.0;

/// The bar at the top of every column and strip.
pub const COLUMN_BAR: f32 = 12.0;

/// How wide a closed side's bar is.
pub const HANDLE: f32 = 14.0;

/// The least height of the bottom row (the timeline's).
pub const BOTTOM_LEAST: f32 = 90.0;

/// How much of the work area the bottom row always leaves above it.
pub const BOTTOM_LEAVES: f32 = 200.0;

/// The height of a group's header: its tab strip, when it has more than
/// one tab or its palette is [headed](DockPanel::headed). The height of a
/// compact tab strip, which it is drawn with.
pub const TAB_STRIP: f32 = crate::ui::Density::Compact.tab_strip();

/// The work area never gets narrower than this while columns can
/// collapse.
pub const MIN_CENTER: f32 = 300.0;

/// How far from a column's edge a drop makes a column of its own.
const EDGE_DROP: f32 = 14.0;

/// How far a palette moves before it is being dragged.
const DRAG_START: f32 = 6.0;

/// A group where it goes this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LaidGroup<P> {
    /// What holds it.
    pub holder: Holder,
    /// Its place in its holder.
    pub index: usize,
    /// Its whole rect.
    pub rect: Rect,
    /// The tabs that show.
    pub tabs: Vec<P>,
    /// The tab showing.
    pub active: P,
    /// Where the palette showing is drawn: below the header, if any.
    pub body: Rect,
    /// Each tab's rect in the header (empty without one): shared out
    /// evenly by [`DockLayout::layout`], where the strip laid them once
    /// it is drawn ([`DockLayout::tabs_ui`]).
    pub tab_rects: Vec<(P, Rect)>,
}

/// A line people drag between two groups.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    /// What holds both groups.
    pub holder: Holder,
    /// The group above (or left).
    pub upper: usize,
    /// The group below (or right).
    pub lower: usize,
    /// The line's rect.
    pub rect: Rect,
    /// The holder's length along the split: a column's height, the
    /// bottom row's width.
    pub span: f32,
}

/// Where everything goes this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid<P> {
    /// The groups that show.
    pub groups: Vec<LaidGroup<P>>,
    /// Columns: side, index, rect.
    pub columns: Vec<(Side, usize, Rect)>,
    /// Lines between groups.
    pub splits: Vec<Split>,
    /// Columns' inner edges people drag: side, column, rect.
    pub edges: Vec<(Side, usize, Rect)>,
    /// The icon strip at the right, when it shows.
    pub strip: Option<Rect>,
    /// Palettes in the right strip: the collapsed ones, the right side's
    /// folded columns, then those the window had no room for.
    pub icons: Vec<P>,
    /// The icon strip at the left, while the left columns are folded.
    pub left_strip: Option<Rect>,
    /// Palettes in the left strip.
    pub left_icons: Vec<P>,
    /// Closed sides' bars.
    pub handles: Vec<(Side, Rect)>,
    /// The top row, when it shows.
    pub top: Option<Rect>,
    /// The bottom row, when it shows.
    pub bottom: Option<Rect>,
    /// What is left for the work area.
    pub center: Rect,
    /// [`Self::icons`] in groups: palettes that were tabs of one group
    /// share an icon group.
    pub icon_groups: Vec<Vec<P>>,
    /// [`Self::left_icons`] in groups.
    pub left_icon_groups: Vec<Vec<P>>,
    /// The top row folded into icons, across the window.
    pub top_strip: Option<IconStrip<P>>,
    /// The bottom row folded into icons, under the work area.
    pub bottom_strip: Option<IconStrip<P>>,
    /// Closed rows' bars.
    pub row_handles: Vec<(Row, Rect)>,
}

/// A row folded into icons: where it is and its icon groups, left to
/// right.
#[derive(Clone, Debug, PartialEq)]
pub struct IconStrip<P> {
    /// The strip.
    pub rect: Rect,
    /// The icons, a group per palette group of the row.
    pub groups: Vec<Vec<P>>,
}

impl<P: DockPanel> Laid<P> {
    /// Each palette showing and the rect it is drawn in.
    pub fn docked(&self) -> impl Iterator<Item = (P, Rect)> + '_ {
        self.groups.iter().map(|group| (group.active, group.body))
    }
}

/// Heights (or widths) sharing `length` by weight, each at least its
/// least, the excess taken from the largest.
fn share(length: f32, entries: &[(f32, f32)]) -> Vec<f32> {
    let total: f32 = entries.iter().map(|(weight, _)| weight.max(0.01)).sum();
    let mut sizes: Vec<f32> = entries
        .iter()
        .map(|(weight, least)| (length * weight.max(0.01) / total).max(least.min(length)))
        .collect();
    let excess: f32 = sizes.iter().sum::<f32>() - length;
    if excess > 0.0
        && let Some(largest) = sizes.iter_mut().max_by(|a, b| a.total_cmp(b))
    {
        *largest = (*largest - excess).max(24.0);
    }
    sizes
}

/// Equal tabs across a group's tab strip: where the tabs are until the
/// strip is drawn and says where it laid them ([`DockLayout::tabs_ui`]).
fn tab_rects<P: Copy>(strip: Rect, tabs: &[P]) -> Vec<(P, Rect)> {
    if tabs.is_empty() {
        return Vec::new();
    }
    let width = (strip.width() / tabs.len() as f32).min(140.0);
    tabs.iter()
        .enumerate()
        .map(|(at, tab)| {
            let left = strip.left() + at as f32 * width;
            (
                *tab,
                Rect::from_min_size(pos2(left, strip.top()), vec2(width, strip.height())),
            )
        })
        .collect()
}

/// Where the tab of the palette whose key is `key` is in a dock header,
/// as the last frame drew it (none when no header drew it then): for what
/// points at a palette (a tour, a test), whether its name shows or only
/// its icon.
pub fn tab_place(ctx: &egui::Context, key: &str) -> Option<Rect> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(u64, Rect)>(tab_place_id(key)))
        .filter(|(pass, _)| pass + 1 >= now)
        .map(|(_, place)| place)
}

/// Where the tab named `name` (its palette's name, as its tooltip says
/// it) is in a dock header, as the last frame drew it: for a test or a
/// tour that points at a palette by the name it reads, while a folded tab
/// shows only its icon.
pub fn tab_place_named(ctx: &egui::Context, name: &str) -> Option<Rect> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(u64, Rect)>(tab_name_id(name)))
        .filter(|(pass, _)| pass + 1 >= now)
        .map(|(_, place)| place)
}

/// Where the bottom row (the timeline's, under the work area) was laid
/// in the last frame, while it is open: for a test or a tour that drags
/// its edge or looks at how tall it is.
pub fn bottom_row(ctx: &egui::Context) -> Option<Rect> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<(u64, Rect)>(Id::unique("sue-dock-bottom-row")))
        .filter(|(pass, _)| pass + 1 >= now)
        .map(|(_, place)| place)
}

/// Where [`tab_place_named`] keeps a tab's place.
fn tab_name_id(name: &str) -> Id {
    Id::unique(("sue-dock-tab-named", name))
}

/// Where [`tab_place`] keeps a tab's place.
fn tab_place_id(key: &str) -> Id {
    Id::unique(("sue-dock-tab-place", key))
}

/// Whether a group of `tabs` has a header: more than one tab, or one
/// that is [headed](DockPanel::headed).
fn has_header<P: DockPanel>(tabs: &[P]) -> bool {
    tabs.len() > 1 || tabs.iter().any(|tab| tab.headed())
}

/// A drop at `at` on `group`'s header: between the tabs, before the
/// first whose middle lies right of `at`, else after the last. The line
/// shows where the tab goes.
fn header_drop<P: Copy>(group: &LaidGroup<P>, at: Pos2) -> Option<(Target<P>, Rect)> {
    let (first, last) = (group.tab_rects.first()?, group.tab_rects.last()?);
    let strip = Rect::from_min_max(
        pos2(group.rect.left(), first.1.top()),
        pos2(group.rect.right(), first.1.bottom()),
    );
    if !strip.contains(at) {
        return None;
    }
    let line = |x: f32| Rect::from_min_size(pos2(x - 1.5, strip.top()), vec2(3.0, strip.height()));
    Some(
        match group
            .tab_rects
            .iter()
            .find(|(_, rect)| at.x < rect.center().x)
        {
            Some((tab, rect)) => (
                Target::Join {
                    anchor: *tab,
                    before: true,
                },
                line(rect.left()),
            ),
            None => (
                Target::Join {
                    anchor: last.0,
                    before: false,
                },
                line(last.1.right()),
            ),
        },
    )
}

/// A group's place in `rect`: its tab strip and body.
fn lay_group<P: DockPanel>(
    holder: Holder,
    index: usize,
    group: &Group<P>,
    rect: Rect,
    shown: &dyn Fn(P) -> bool,
) -> Option<LaidGroup<P>> {
    let tabs = group.visible(shown);
    let active = group.current(shown)?;
    let (body, tab_rects) = if has_header(&tabs) {
        let strip = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_STRIP));
        let inset = active.header_inset().min(strip.width());
        (
            Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max),
            tab_rects(
                Rect::from_min_max(pos2(strip.left() + inset, strip.top()), strip.max),
                &tabs,
            ),
        )
    } else {
        (rect, Vec::new())
    };
    Some(LaidGroup {
        holder,
        index,
        rect,
        tabs,
        active,
        body,
        tab_rects,
    })
}

impl<P: DockPanel> DockLayout<P> {
    /// No palette anywhere; a studio fills it in for its default layout.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
            top: Vec::new(),
            top_height: 38.0,
            bottom: Vec::new(),
            bottom_height: 160.0,
            floating: Vec::new(),
            windows: Vec::new(),
            iconized: Vec::new(),
            popped: None,
            menu: None,
            folded: [false; 2],
            hidden: [false; 2],
            row_folded: [false; 2],
            row_hidden: [false; 2],
            icon_joined: Vec::new(),
            closed: Vec::new(),
            rolled: Vec::new(),
            drag: None,
        }
    }

    fn columns(&self, side: Side) -> &Vec<Column<P>> {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn columns_mut(&mut self, side: Side) -> &mut Vec<Column<P>> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn groups_mut(&mut self, holder: Holder) -> Option<&mut Vec<Group<P>>> {
        match holder {
            Holder::Column(side, column) => self
                .columns_mut(side)
                .get_mut(column)
                .map(|column| &mut column.groups),
            Holder::Top => Some(&mut self.top),
            Holder::Bottom => Some(&mut self.bottom),
        }
    }

    /// Starts dragging `panel` from `from`, as its title bar, tab or icon
    /// does; a studio calls it to tear an optional palette off another
    /// palette's tab. Nothing moves until the pointer does.
    pub fn begin_drag(&mut self, panel: P, from: Pos2) {
        if self.drag.is_none() {
            self.drag = Some(Drag::Panel {
                panel,
                from,
                at: from,
            });
        }
    }

    /// The palette being dragged and how far the pointer went since the
    /// drag began: what a floating palette's window moves by while its
    /// header is dragged.
    pub fn panel_drag(&self) -> Option<(P, Vec2)> {
        match self.drag {
            Some(Drag::Panel { panel, from, at }) => Some((panel, at - from)),
            _ => None,
        }
    }

    /// Makes column `index` on `side` `width` wide, within what its
    /// palettes allow (as dragging its edge does).
    pub fn set_column_width(&mut self, side: Side, index: usize, width: f32) {
        if let Some(column) = self.columns_mut(side).get_mut(index) {
            let least = column
                .groups
                .iter()
                .flat_map(|group| group.tabs.iter())
                .map(|tab| tab.min_width())
                .fold(24.0, f32::max);
            column.width = width.clamp(least, 560.0_f32.max(least));
        }
    }

    /// Puts `panel` in a window of its own at `rect` (relative to the main
    /// window, which it may lie outside of).
    pub fn pop_out(&mut self, panel: P, rect: Rect) {
        self.remove(panel);
        self.windows.push((panel, rect));
    }

    /// Where the layout goes back to: `saved` (a default people chose)
    /// unless the layout already is it, then `model`, the layout of the
    /// style in use ([`DockPanel::style_layout`]). `true` when it went to
    /// `saved`.
    pub fn reset(&mut self, saved: Option<&Self>, model: &Self) -> bool {
        let calm = |dock: &Self| {
            let mut dock = dock.clone();
            dock.menu = None;
            dock.popped = None;
            dock.drag = None;
            dock
        };
        let back = saved.map(calm).filter(|saved| *saved != calm(self));
        let to_saved = back.is_some();
        *self = back.unwrap_or_else(|| calm(model));
        to_saved
    }

    /// A palette is being dragged, or a line or edge.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The palette being dragged.
    #[must_use]
    pub fn dragged_panel(&self) -> Option<P> {
        match self.drag {
            Some(Drag::Panel { panel, .. }) => Some(panel),
            _ => None,
        }
    }

    /// Where `panel` is docked: its holder and its group's place there.
    #[must_use]
    pub fn find(&self, panel: P) -> Option<(Holder, usize)> {
        for side in [Side::Left, Side::Right] {
            for (index, column) in self.columns(side).iter().enumerate() {
                if let Some(group) = column
                    .groups
                    .iter()
                    .position(|group| group.tabs.contains(&panel))
                {
                    return Some((Holder::Column(side, index), group));
                }
            }
        }
        for (holder, groups) in [(Holder::Top, &self.top), (Holder::Bottom, &self.bottom)] {
            if let Some(group) = groups.iter().position(|group| group.tabs.contains(&panel)) {
                return Some((holder, group));
            }
        }
        None
    }

    /// `panel` is somewhere: docked, floating, in a window or in the strip.
    #[must_use]
    pub fn has(&self, panel: P) -> bool {
        self.is_open(panel) || self.iconized.contains(&panel)
    }

    /// `panel` shows by itself: docked, floating or in a window (not just
    /// an icon in the strip).
    #[must_use]
    pub fn is_open(&self, panel: P) -> bool {
        self.find(panel).is_some()
            || self.floating.iter().any(|(found, _)| *found == panel)
            || self.windows.iter().any(|(found, _)| *found == panel)
    }

    /// The side `panel` is docked on, if it is in a column.
    #[must_use]
    pub fn side_of(&self, panel: P) -> Option<Side> {
        match self.find(panel) {
            Some((Holder::Column(side, _), _)) => Some(side),
            _ => None,
        }
    }

    /// Takes `panel` out of wherever it is (closed for good too, so it can
    /// be placed again); groups and columns left empty go. To take it
    /// away for good, [`Self::close`] it.
    pub fn remove(&mut self, panel: P) {
        if let Some(at) = self.iconized.iter().position(|found| *found == panel)
            && !self.icon_joined.contains(&panel)
            && let Some(next) = self.iconized.get(at + 1)
        {
            // The next icon of its group leads the group now.
            let next = *next;
            self.icon_joined.retain(|found| *found != next);
        }
        self.icon_joined.retain(|found| *found != panel);
        self.closed.retain(|found| *found != panel);
        let take = |groups: &mut Vec<Group<P>>| {
            for group in groups.iter_mut() {
                if let Some(at) = group.tabs.iter().position(|tab| *tab == panel) {
                    group.tabs.remove(at);
                    if at < group.active {
                        group.active -= 1;
                    }
                    group.active = group.active.min(group.tabs.len().saturating_sub(1));
                }
            }
            groups.retain(|group| !group.tabs.is_empty());
        };
        for column in self.left.iter_mut().chain(self.right.iter_mut()) {
            take(&mut column.groups);
        }
        take(&mut self.top);
        take(&mut self.bottom);
        self.left.retain(|column| !column.groups.is_empty());
        self.right.retain(|column| !column.groups.is_empty());
        self.floating.retain(|(found, _)| *found != panel);
        self.windows.retain(|(found, _)| *found != panel);
        self.iconized.retain(|found| *found != panel);
        if self.popped == Some(panel) {
            self.popped = None;
        }
    }

    /// Docks `panel` on `side`: in a column of its own for a palette of
    /// fixed width, else as a group at the bottom of the innermost wide
    /// column.
    pub fn dock(&mut self, panel: P, side: Side) {
        self.remove(panel);
        self.hidden[side.index()] = false;
        let columns = self.columns_mut(side);
        if let Some(width) = panel.fixed_width() {
            columns.push(Column::of(&[(panel, 1.0)], width));
            return;
        }
        let wide = columns
            .iter_mut()
            .rev()
            .find(|column| column.groups.iter().all(|group| !group.fixed()));
        match wide {
            Some(column) => {
                let share = 1.0 / (column.groups.len() as f32 + 1.0);
                column.groups.push(Group::one(panel, share.max(0.2)));
            }
            None => columns.push(Column::of(&[(panel, 1.0)], 260.0)),
        }
    }

    /// Floats `panel` in a window at `at`.
    pub fn float(&mut self, panel: P, at: Pos2) {
        self.remove(panel);
        self.floating
            .push((panel, Rect::from_min_size(at, panel.float_size())));
    }

    /// Collapses `panel` to an icon in the strip.
    pub fn iconize(&mut self, panel: P) {
        self.remove(panel);
        self.iconized.push(panel);
    }

    /// Collapses `panel` and the other tabs of its group into one icon
    /// group in the strip, in tab order (a lone palette, or one that is
    /// not docked, becomes an icon of its own).
    pub fn iconize_group(&mut self, panel: P) {
        let tabs = match self.find(panel) {
            Some((holder, index)) => self
                .groups_mut(holder)
                .and_then(|groups| groups.get(index))
                .map_or_else(|| vec![panel], |group| group.tabs.clone()),
            None => vec![panel],
        };
        for (at, tab) in tabs.iter().enumerate() {
            self.iconize(*tab);
            if at > 0 {
                self.icon_joined.push(*tab);
            }
        }
        self.tidy_icons();
    }

    /// Keeps [`Self::icon_joined`] in strip order, so equal layouts
    /// compare equal however they were made.
    fn tidy_icons(&mut self) {
        let joined = std::mem::take(&mut self.icon_joined);
        self.icon_joined = self
            .iconized
            .iter()
            .copied()
            .filter(|panel| joined.contains(panel))
            .collect();
    }

    /// Puts collapsed `panel` into `anchor`'s icon group, as its last icon
    /// (as dropping one collapsed icon on another does). Nothing happens
    /// unless `anchor` is collapsed and is not `panel`.
    pub fn join_icon(&mut self, panel: P, anchor: P) {
        if panel == anchor || !self.iconized.contains(&anchor) {
            return;
        }
        self.remove(panel);
        let groups = self.icon_groups();
        let Some(group) = groups.iter().find(|group| group.contains(&anchor)) else {
            return;
        };
        let last = group[group.len() - 1];
        let at = self
            .iconized
            .iter()
            .position(|found| *found == last)
            .map_or(self.iconized.len(), |at| at + 1);
        self.iconized.insert(at, panel);
        self.icon_joined.push(panel);
        self.tidy_icons();
    }

    /// The collapsed palettes in their icon groups, in strip order.
    #[must_use]
    pub fn icon_groups(&self) -> Vec<Vec<P>> {
        let mut groups: Vec<Vec<P>> = Vec::new();
        for panel in &self.iconized {
            match groups.last_mut() {
                Some(group) if self.icon_joined.contains(panel) => group.push(*panel),
                _ => groups.push(vec![*panel]),
            }
        }
        groups
    }

    /// Closes `panel` for good: it leaves the layout, and reading the
    /// layout again or filling in missing palettes does not bring it back
    /// until it is placed again.
    pub fn close(&mut self, panel: P) {
        self.remove(panel);
        // Closed from its own menu: the menu goes with it.
        if self.menu == Some(panel) {
            self.menu = None;
        }
        self.closed.push(panel);
    }

    /// `panel` was closed for good.
    #[must_use]
    pub fn is_closed(&self, panel: P) -> bool {
        self.closed.contains(&panel)
    }

    /// Shows `panel`'s tab in its group.
    pub fn choose(&mut self, panel: P) {
        if let Some((holder, index)) = self.find(panel)
            && let Some(group) = self
                .groups_mut(holder)
                .and_then(|groups| groups.get_mut(index))
            && let Some(at) = group.tabs.iter().position(|tab| *tab == panel)
        {
            group.active = at;
        }
    }

    /// Puts `panel` where `drop` says.
    pub fn put(&mut self, panel: P, drop: Target<P>) {
        match drop {
            Target::Join { anchor, before } if anchor != panel => {
                if self.find(anchor).is_none() {
                    return;
                }
                self.remove(panel);
                let Some((holder, index)) = self.find(anchor) else {
                    return;
                };
                if let Some(group) = self
                    .groups_mut(holder)
                    .and_then(|groups| groups.get_mut(index))
                {
                    let at = if before {
                        group
                            .tabs
                            .iter()
                            .position(|tab| *tab == anchor)
                            .unwrap_or(group.tabs.len())
                    } else {
                        group.tabs.len()
                    };
                    group.tabs.insert(at, panel);
                    group.active = at;
                }
            }
            Target::Beside { anchor, after } if anchor != panel => {
                if self.find(anchor).is_none() {
                    return;
                }
                self.remove(panel);
                let Some((holder, index)) = self.find(anchor) else {
                    return;
                };
                if let Some(groups) = self.groups_mut(holder) {
                    let weight = groups[index].weight.max(0.02) / 2.0;
                    groups[index].weight = weight;
                    groups.insert(index + usize::from(after), Group::one(panel, weight));
                }
            }
            Target::Column { side, beside } => {
                if beside.is_some_and(|(anchor, _)| anchor == panel) {
                    return;
                }
                self.remove(panel);
                self.hidden[side.index()] = false;
                let at = beside.and_then(|(anchor, inward)| match self.find(anchor) {
                    Some((Holder::Column(found, index), _)) if found == side => {
                        Some(index + usize::from(inward))
                    }
                    _ => None,
                });
                let width = panel.fixed_width().unwrap_or(260.0);
                let columns = self.columns_mut(side);
                let at = at.unwrap_or(columns.len()).min(columns.len());
                columns.insert(at, Column::of(&[(panel, 1.0)], width));
            }
            Target::Top => {
                self.remove(panel);
                self.row_hidden[Row::Top.index()] = false;
                self.top_height = self.top_height.max(panel.min_height());
                self.top.push(Group::one(panel, 1.0));
            }
            Target::Bottom => {
                self.remove(panel);
                self.row_hidden[Row::Bottom.index()] = false;
                self.bottom.push(Group::one(panel, 1.0));
            }
            Target::Icon => self.iconize(panel),
            Target::Float(at) => self.float(panel, at),
            Target::Join { .. } | Target::Beside { .. } => {}
        }
    }

    /// Puts every palette the layout lacks where the default layout has
    /// it (a layout saved before the palette existed); see
    /// [`Self::fill_missing_from`].
    pub fn fill_missing(&mut self) {
        self.fill_missing_from(&P::default_layout());
    }

    /// Puts every palette the layout lacks where `model` (the layout of
    /// the style in use) has it: a layout saved before the palette
    /// existed. Palettes closed for good (here or in `model`) and optional
    /// ones stay away; one `model` lacks becomes an icon.
    pub fn fill_missing_from(&mut self, model: &Self) {
        // A side people closed stays closed: the palette waits in it.
        let hidden = self.hidden;
        for &panel in P::all() {
            if self.has(panel)
                || self.is_closed(panel)
                || model.is_closed(panel)
                || panel.optional()
            {
                continue;
            }
            match model.find(panel) {
                Some((Holder::Column(side, _), _)) => self.dock(panel, side),
                Some((Holder::Top, _)) => {
                    self.top_height = self.top_height.max(panel.min_height());
                    self.top.push(Group::one(panel, 1.0));
                }
                Some((Holder::Bottom, _)) => self.bottom.push(Group::one(panel, 1.0)),
                None => match model.floating.iter().find(|(found, _)| *found == panel) {
                    Some((_, rect)) => self.floating.push((panel, *rect)),
                    None => self.iconized.push(panel),
                },
            }
        }
        self.hidden = hidden;
    }

    // ----- Lines ------------------------------------------------------------

    /// The layout as lines of text: each column with its width and groups,
    /// the top and bottom rows, the floating palettes, those in windows of
    /// their own, the collapsed ones, the folded and closed sides and rows,
    /// which collapsed palettes share an icon group and which palettes
    /// were closed for good. A group reads `weight:key+key@active`.
    #[must_use]
    pub fn to_lines(&self) -> Vec<String> {
        let group = |group: &Group<P>| {
            let keys: Vec<&str> = group.tabs.iter().map(|tab| tab.key()).collect();
            let active = if group.active > 0 {
                format!("@{}", group.active)
            } else {
                String::new()
            };
            format!("{}:{}{active}", group.weight, keys.join("+"))
        };
        let mut out = Vec::new();
        for side in [Side::Left, Side::Right] {
            for column in self.columns(side) {
                let groups: Vec<String> = column.groups.iter().map(group).collect();
                out.push(format!(
                    "column {} {} {}",
                    side.name(),
                    column.width,
                    groups.join(" ")
                ));
            }
        }
        for (name, height, row) in [
            ("top", self.top_height, &self.top),
            ("bottom", self.bottom_height, &self.bottom),
        ] {
            let groups: Vec<String> = row.iter().map(group).collect();
            out.push(
                format!("{name} {height} {}", groups.join(" "))
                    .trim_end()
                    .to_owned(),
            );
        }
        for (name, list) in [("float", &self.floating), ("window", &self.windows)] {
            for (panel, rect) in list {
                out.push(format!(
                    "{name} {} {} {} {} {}",
                    rect.min.x,
                    rect.min.y,
                    rect.width(),
                    rect.height(),
                    panel.key()
                ));
            }
        }
        for panel in &self.iconized {
            out.push(format!("icon {}", panel.key()));
        }
        for side in [Side::Left, Side::Right] {
            if self.folded[side.index()] {
                out.push(format!("fold {}", side.name()));
            }
            if self.hidden[side.index()] {
                out.push(format!("hide {}", side.name()));
            }
        }
        // Lines older readers skip: their palettes still read as above.
        for row in [Row::Top, Row::Bottom] {
            if self.row_folded[row.index()] {
                out.push(format!("fold {}", row.name()));
            }
            if self.row_hidden[row.index()] {
                out.push(format!("hide {}", row.name()));
            }
        }
        for panel in &self.iconized {
            if self.icon_joined.contains(panel) {
                out.push(format!("icon-join {}", panel.key()));
            }
        }
        for panel in &self.closed {
            out.push(format!("close {}", panel.key()));
        }
        out
    }

    /// A layout from [`Self::to_lines`], or from the older lines that
    /// named one palette per group (`left 236 Title=0.44,Title=0.3`, and
    /// `timeline` for the bottom's height). Lines it does not know are
    /// skipped, and palettes it lacks go where the default layout has
    /// them. `None` when no palette is placed or closed.
    #[must_use]
    pub fn from_lines(lines: &[String]) -> Option<Self> {
        Self::from_lines_with(lines, &P::default_layout())
    }

    /// [`Self::from_lines`], with the palettes the lines lack put where
    /// `model` (the layout of the style in use) has them.
    #[must_use]
    pub fn from_lines_with(lines: &[String], model: &Self) -> Option<Self> {
        let mut dock = Self::empty();
        let mut joined: Vec<P> = Vec::new();
        let group = |text: &str| -> Option<Group<P>> {
            let (weight, rest) = text.split_once(':')?;
            let (keys, active) = match rest.split_once('@') {
                Some((keys, active)) => (keys, active.parse().unwrap_or(0)),
                None => (rest, 0),
            };
            let tabs: Vec<P> = keys.split('+').filter_map(P::named).collect();
            (!tabs.is_empty()).then(|| Group {
                active: active.min(tabs.len() - 1),
                tabs,
                weight: weight.parse().unwrap_or(1.0),
            })
        };
        for line in lines {
            let mut words = line.splitn(3, ' ');
            match (words.next(), words.next(), words.next()) {
                (Some("column"), Some(side @ ("left" | "right")), Some(rest)) => {
                    let mut parts = rest.split(' ');
                    let width = parts.next().and_then(|width| width.parse().ok());
                    let groups: Vec<Group<P>> = parts.filter_map(group).collect();
                    if groups.is_empty() {
                        continue;
                    }
                    let column = Column {
                        groups,
                        width: width.unwrap_or(260.0),
                    };
                    if side == "left" {
                        dock.left.push(column);
                    } else {
                        dock.right.push(column);
                    }
                }
                (Some("bottom"), Some(height), rest) => {
                    if let Ok(height) = height.parse() {
                        dock.bottom_height = height;
                    }
                    dock.bottom
                        .extend(rest.unwrap_or("").split(' ').filter_map(group));
                }
                (Some("top"), Some(height), rest) => {
                    if let Ok(height) = height.parse() {
                        dock.top_height = height;
                    }
                    dock.top
                        .extend(rest.unwrap_or("").split(' ').filter_map(group));
                }
                // Before tabs: one palette per group, by title.
                (Some(side @ ("left" | "right")), Some(width), Some(palettes)) => {
                    let groups: Vec<Group<P>> = palettes
                        .split(',')
                        .filter_map(|entry| {
                            let (title, weight) = entry.split_once('=')?;
                            Some(Group::one(P::named(title)?, weight.parse().ok()?))
                        })
                        .collect();
                    if groups.is_empty() {
                        continue;
                    }
                    let column = Column {
                        groups,
                        width: width.parse().unwrap_or(260.0),
                    };
                    if side == "left" {
                        dock.left.push(column);
                    } else {
                        dock.right.push(column);
                    }
                }
                (Some(what @ ("float" | "window")), Some(x), Some(rest)) => {
                    let parts: Vec<&str> = rest.splitn(4, ' ').collect();
                    if let ([y, w, h, name], Ok(x)) = (parts.as_slice(), x.parse::<f32>())
                        && let (Ok(y), Ok(w), Ok(h), Some(panel)) = (
                            y.parse::<f32>(),
                            w.parse::<f32>(),
                            h.parse::<f32>(),
                            P::named(name),
                        )
                    {
                        let rect = Rect::from_min_size(pos2(x, y), vec2(w, h));
                        if what == "float" {
                            dock.floating.push((panel, rect));
                        } else {
                            dock.windows.push((panel, rect));
                        }
                    }
                }
                (Some("icon"), Some(first), rest) => {
                    let name =
                        rest.map_or_else(|| first.to_owned(), |rest| format!("{first} {rest}"));
                    if let Some(panel) = P::named(&name) {
                        dock.iconized.push(panel);
                    }
                }
                (Some("timeline"), Some(height), None) => {
                    if let Ok(height) = height.parse() {
                        dock.bottom_height = height;
                    }
                }
                (Some(what @ ("fold" | "hide")), Some(side @ ("left" | "right")), None) => {
                    let at = usize::from(side == "right");
                    if what == "fold" {
                        dock.folded[at] = true;
                    } else {
                        dock.hidden[at] = true;
                    }
                }
                (Some(what @ ("fold" | "hide")), Some(row @ ("top" | "bottom")), None) => {
                    let at = usize::from(row == "bottom");
                    if what == "fold" {
                        dock.row_folded[at] = true;
                    } else {
                        dock.row_hidden[at] = true;
                    }
                }
                (Some("icon-join"), Some(name), None) => joined.extend(P::named(name)),
                (Some("close"), Some(name), None) => {
                    if let Some(panel) = P::named(name)
                        && !dock.closed.contains(&panel)
                    {
                        dock.closed.push(panel);
                    }
                }
                _ => {}
            }
        }
        let placed = !dock.left.is_empty()
            || !dock.right.is_empty()
            || !dock.top.is_empty()
            || !dock.bottom.is_empty()
            || !dock.floating.is_empty()
            || !dock.windows.is_empty()
            || !dock.iconized.is_empty()
            || !dock.closed.is_empty();
        // A palette both placed and closed (a hand-edited file) is placed.
        let placed_too: Vec<P> = dock
            .closed
            .iter()
            .copied()
            .filter(|panel| dock.has(*panel))
            .collect();
        dock.closed.retain(|panel| !placed_too.contains(panel));
        dock.icon_joined = joined;
        dock.tidy_icons();
        dock.fill_missing_from(model);
        placed.then_some(dock)
    }

    // ----- Layout -----------------------------------------------------------

    /// Lays everything out in `body`; `shown` says which palettes may show
    /// and `hide` (0 to 1) slides the columns out to their edges and the
    /// bottom row down.
    pub fn layout(&self, body: Rect, shown: &dyn Fn(P) -> bool, hide: f32) -> Laid<P> {
        // The top row, across the whole window, while there is room.
        let open_row = |row: &[Group<P>]| -> Vec<usize> {
            row.iter()
                .enumerate()
                .filter(|(_, group)| group.current(shown).is_some())
                .map(|(index, _)| index)
                .collect()
        };
        // A row's open groups as icon groups, while it is folded.
        let row_icons = |row: &[Group<P>], open: &[usize]| -> Vec<Vec<P>> {
            open.iter().map(|at| row[*at].visible(shown)).collect()
        };
        let top_groups = open_row(&self.top);
        let mut top_row = None;
        let mut top_strip = None;
        let mut row_handles = Vec::new();
        let body = if !top_groups.is_empty() && body.height() > 240.0 {
            let closed = self.row_hidden[Row::Top.index()];
            let folded = self.row_folded[Row::Top.index()];
            let height = if closed {
                HANDLE
            } else if folded {
                STRIP
            } else {
                self.top_height.min(body.height() - 200.0).max(24.0)
            };
            let top = body.top() - hide * height;
            let row = Rect::from_min_max(pos2(body.left(), top), pos2(body.right(), top + height));
            if closed {
                row_handles.push((Row::Top, row));
            } else if folded {
                top_strip = Some(IconStrip {
                    rect: row,
                    groups: row_icons(&self.top, &top_groups),
                });
            } else {
                top_row = Some(row);
            }
            Rect::from_min_max(pos2(body.left(), row.bottom()), body.max)
        } else {
            body
        };
        let open_groups = |column: &Column<P>| -> Vec<usize> {
            column
                .groups
                .iter()
                .enumerate()
                .filter(|(_, group)| group.current(shown).is_some())
                .map(|(index, _)| index)
                .collect()
        };
        let visible_tabs = |column: &Column<P>| -> Vec<P> {
            column
                .groups
                .iter()
                .flat_map(|group| group.visible(shown))
                .collect()
        };
        // Every column is as wide as people made it, at least as wide as
        // its palettes need.
        let width_of = |column: &Column<P>| -> f32 {
            visible_tabs(column)
                .iter()
                .map(|panel| panel.min_width())
                .fold(column.width, f32::max)
        };
        // Columns that show anything, and their widths. A closed side shows
        // nothing; a folded side keeps its anchored columns and folds the
        // rest into icons.
        let visible_groups = |column: &Column<P>| -> Vec<Vec<P>> {
            column
                .groups
                .iter()
                .map(|group| group.visible(shown))
                .filter(|tabs| !tabs.is_empty())
                .collect()
        };
        let mut sides: Vec<(Side, usize, f32, bool)> = Vec::new();
        let mut left_icon_groups: Vec<Vec<P>> = Vec::new();
        let mut folded_right: Vec<Vec<P>> = Vec::new();
        for side in [Side::Left, Side::Right] {
            if self.hidden[side.index()] {
                continue;
            }
            for (index, column) in self.columns(side).iter().enumerate() {
                if open_groups(column).is_empty() {
                    continue;
                }
                if self.folded[side.index()] && !column.anchored() {
                    let into = if side == Side::Left {
                        &mut left_icon_groups
                    } else {
                        &mut folded_right
                    };
                    into.extend(visible_groups(column));
                } else {
                    sides.push((side, index, width_of(column), true));
                }
            }
        }
        let mut icon_groups: Vec<Vec<P>> = if self.hidden[1] {
            Vec::new()
        } else {
            self.icon_groups()
                .into_iter()
                .map(|group| group.into_iter().filter(|panel| shown(*panel)).collect())
                .filter(|group: &Vec<P>| !group.is_empty())
                .collect()
        };
        icon_groups.extend(folded_right);
        // Too narrow: the widest columns collapse to icons (never the
        // anchored ones).
        loop {
            let used: f32 = sides
                .iter()
                .filter(|(_, _, _, open)| *open)
                .map(|(_, _, width, _)| width)
                .sum();
            let strip = if icon_groups.is_empty() { 0.0 } else { STRIP }
                + if left_icon_groups.is_empty() {
                    0.0
                } else {
                    STRIP
                };
            if used + strip + MIN_CENTER <= body.width() {
                break;
            }
            let widest = sides
                .iter_mut()
                .filter(|(side, index, _, open)| *open && !self.columns(*side)[*index].anchored())
                .max_by(|a, b| a.2.total_cmp(&b.2));
            let Some(widest) = widest else {
                break;
            };
            widest.3 = false;
            icon_groups.extend(visible_groups(&self.columns(widest.0)[widest.1]));
        }
        let icons: Vec<P> = icon_groups.iter().flatten().copied().collect();
        let left_icons: Vec<P> = left_icon_groups.iter().flatten().copied().collect();
        let open = |side: Side| -> Vec<(usize, f32)> {
            sides
                .iter()
                .filter(|(found, _, _, open)| *found == side && *open)
                .map(|(_, index, width, _)| (*index, *width))
                .collect()
        };
        let (left, right) = (open(Side::Left), open(Side::Right));
        // A folded side keeps its strip, even empty, for its arrows.
        let strip_width = if icons.is_empty() && !(self.folded[1] && !self.hidden[1]) {
            0.0
        } else {
            STRIP
        };
        let left_strip_width = if self.folded[0] && !self.hidden[0] {
            STRIP
        } else {
            0.0
        };
        let left_handle = if self.hidden[0] { HANDLE } else { 0.0 };
        let right_handle = if self.hidden[1] { HANDLE } else { 0.0 };
        let left_total: f32 = left.iter().map(|(_, width)| width).sum::<f32>() + left_strip_width;
        let right_total: f32 = right.iter().map(|(_, width)| width).sum::<f32>() + strip_width;
        let mut laid = Laid {
            groups: Vec::new(),
            columns: Vec::new(),
            splits: Vec::new(),
            edges: Vec::new(),
            strip: None,
            icons,
            left_strip: None,
            left_icons,
            handles: Vec::new(),
            top: top_row,
            bottom: None,
            center: body,
            icon_groups,
            left_icon_groups,
            top_strip,
            bottom_strip: None,
            row_handles,
        };
        if let Some(row) = top_row {
            self.lay_row(&mut laid, Holder::Top, &top_groups, row, shown);
        }
        if self.hidden[0] {
            laid.handles.push((
                Side::Left,
                Rect::from_min_max(body.min, pos2(body.left() + HANDLE, body.bottom())),
            ));
        }
        if self.hidden[1] {
            laid.handles.push((
                Side::Right,
                Rect::from_min_max(pos2(body.right() - HANDLE, body.top()), body.max),
            ));
        }
        let place_column = |laid: &mut Laid<P>, side: Side, index: usize, rect: Rect| {
            let column = &self.columns(side)[index];
            laid.columns.push((side, index, rect));
            let rect = Rect::from_min_max(pos2(rect.left(), rect.top() + COLUMN_BAR), rect.max);
            let groups = open_groups(column);
            let entries: Vec<(f32, f32)> = groups
                .iter()
                .map(|at| {
                    let group = &column.groups[*at];
                    let tabs = group.visible(shown);
                    let least = tabs.iter().map(|tab| tab.min_height()).fold(0.0, f32::max)
                        + if has_header(&tabs) { TAB_STRIP } else { 0.0 };
                    (group.weight, least)
                })
                .collect();
            let heights = share(rect.height(), &entries);
            let holder = Holder::Column(side, index);
            let mut top = rect.top();
            for (at, (group, height)) in groups.iter().zip(&heights).enumerate() {
                let piece =
                    Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), *height));
                if let Some(found) = lay_group(holder, *group, &column.groups[*group], piece, shown)
                {
                    laid.groups.push(found);
                }
                top = piece.bottom();
                if let Some(lower) = groups.get(at + 1) {
                    laid.splits.push(Split {
                        holder,
                        upper: *group,
                        lower: *lower,
                        rect: Rect::from_center_size(
                            pos2(rect.center().x, top),
                            vec2(rect.width(), 6.0),
                        ),
                        span: rect.height(),
                    });
                }
            }
            // Every column resizes; a tool strip grows into more columns.
            let x = match side {
                Side::Left => rect.right(),
                Side::Right => rect.left(),
            };
            laid.edges.push((
                side,
                index,
                Rect::from_center_size(pos2(x, rect.center().y), vec2(6.0, rect.height())),
            ));
        };
        let mut x = body.left() + left_handle - hide * left_total;
        if left_strip_width > 0.0 {
            laid.left_strip = Some(Rect::from_min_max(
                pos2(x, body.top()),
                pos2(x + STRIP, body.bottom()),
            ));
            x += STRIP;
        }
        for (index, width) in &left {
            let rect = Rect::from_min_max(pos2(x, body.top()), pos2(x + width, body.bottom()));
            place_column(&mut laid, Side::Left, *index, rect);
            x = rect.right();
        }
        let center_left = x;
        let mut x = body.right() - right_handle + hide * right_total;
        if strip_width > 0.0 {
            laid.strip = Some(Rect::from_min_max(
                pos2(x - STRIP, body.top()),
                pos2(x, body.bottom()),
            ));
            x -= STRIP;
        }
        for (index, width) in &right {
            let rect = Rect::from_min_max(pos2(x - width, body.top()), pos2(x, body.bottom()));
            place_column(&mut laid, Side::Right, *index, rect);
            x = rect.left();
        }
        laid.center = Rect::from_min_max(
            pos2(center_left, body.top()),
            pos2(x.max(center_left), body.bottom()),
        );
        // The bottom row, under the work area, while there is room.
        let center = laid.center;
        let bottom: Vec<usize> = self
            .bottom
            .iter()
            .enumerate()
            .filter(|(_, group)| group.current(shown).is_some())
            .map(|(index, _)| index)
            .collect();
        let at_bottom = |height: f32| {
            let top = center.bottom() - height * (1.0 - hide);
            Rect::from_min_max(pos2(center.left(), top), pos2(center.right(), top + height))
        };
        let closed = self.row_hidden[Row::Bottom.index()];
        let folded = self.row_folded[Row::Bottom.index()];
        if bottom.is_empty() || center.width() <= 320.0 {
            // No row.
        } else if closed || folded {
            // A thin bar that opens the row again, or its icons.
            if center.height() > 200.0 {
                let bar = at_bottom(if closed { HANDLE } else { STRIP });
                if closed {
                    laid.row_handles.push((Row::Bottom, bar));
                } else {
                    laid.bottom_strip = Some(IconStrip {
                        rect: bar,
                        groups: row_icons(&self.bottom, &bottom),
                    });
                }
                laid.center.max.y = bar.top();
            }
        } else if center.height() > 360.0 {
            let height = self
                .bottom_height
                .min(center.height() - BOTTOM_LEAVES)
                .max(BOTTOM_LEAST);
            let row = at_bottom(height);
            self.lay_row(&mut laid, Holder::Bottom, &bottom, row, shown);
            laid.bottom = Some(row);
            laid.center.max.y = row.top();
        }
        laid
    }

    /// The groups `open` of a row (top or bottom) side by side in `row`,
    /// sharing its width by weight, and the lines between them.
    fn lay_row(
        &self,
        laid: &mut Laid<P>,
        holder: Holder,
        open: &[usize],
        row: Rect,
        shown: &dyn Fn(P) -> bool,
    ) {
        let groups = match holder {
            Holder::Top => &self.top,
            _ => &self.bottom,
        };
        let entries: Vec<(f32, f32)> = open
            .iter()
            .map(|at| {
                let least = groups[*at]
                    .tabs
                    .iter()
                    .map(|tab| tab.min_width())
                    .fold(46.0, f32::max)
                    .min(120.0);
                (groups[*at].weight, least)
            })
            .collect();
        let widths = share(row.width(), &entries);
        let mut left = row.left();
        for (at, (group, width)) in open.iter().zip(&widths).enumerate() {
            let piece = Rect::from_min_size(pos2(left, row.top()), vec2(*width, row.height()));
            if let Some(found) = lay_group(holder, *group, &groups[*group], piece, shown) {
                laid.groups.push(found);
            }
            left = piece.right();
            if let Some(next) = open.get(at + 1) {
                laid.splits.push(Split {
                    holder,
                    upper: *group,
                    lower: *next,
                    rect: Rect::from_center_size(
                        pos2(left, row.center().y),
                        vec2(6.0, row.height()),
                    ),
                    span: row.width(),
                });
            }
        }
    }

    /// Where `moving` would go if dropped at `at`, and the rect to light
    /// up for it.
    #[must_use]
    pub fn drop_at(&self, laid: &Laid<P>, at: Pos2, moving: P) -> (Target<P>, Rect) {
        let fixed = moving.fixed_width().is_some();
        for strip in [laid.strip, laid.left_strip].into_iter().flatten() {
            if strip.contains(at) {
                return (Target::Icon, strip);
            }
        }
        // A folded or closed row takes the palette into the row.
        let rows = [
            (Row::Top, laid.top_strip.as_ref().map(|strip| strip.rect)),
            (
                Row::Bottom,
                laid.bottom_strip.as_ref().map(|strip| strip.rect),
            ),
        ]
        .into_iter()
        .filter_map(|(row, rect)| Some((row, rect?)))
        .chain(laid.row_handles.iter().copied());
        for (row, rect) in rows {
            if rect.contains(at) {
                let target = match row {
                    Row::Top => Target::Top,
                    Row::Bottom => Target::Bottom,
                };
                return (target, rect);
            }
        }
        for group in &laid.groups {
            if !group.rect.contains(at) {
                continue;
            }
            let anchor = group.active;
            let joins = !fixed && group.tabs.iter().all(|tab| tab.fixed_width().is_none());
            if joins && let Some(found) = header_drop(group, at) {
                return found;
            }
            let rect = group.rect;
            match group.holder {
                Holder::Column(side, column) => {
                    let whole = laid
                        .columns
                        .iter()
                        .find(|(found, index, _)| *found == side && *index == column)
                        .map_or(rect, |(_, _, whole)| *whole);
                    let band = vec2(EDGE_DROP * 2.0, rect.height());
                    if at.x - whole.left() < EDGE_DROP {
                        return (
                            Target::Column {
                                side,
                                beside: Some((anchor, side == Side::Right)),
                            },
                            Rect::from_min_size(pos2(whole.left(), rect.top()), band),
                        );
                    }
                    if whole.right() - at.x < EDGE_DROP {
                        return (
                            Target::Column {
                                side,
                                beside: Some((anchor, side == Side::Left)),
                            },
                            Rect::from_min_size(pos2(whole.right() - band.x, rect.top()), band),
                        );
                    }
                    let t = (at.y - rect.top()) / rect.height().max(1.0);
                    let quarter = rect.height() * 0.3;
                    if t < 0.25 || (!joins && t < 0.5) {
                        return (
                            Target::Beside {
                                anchor,
                                after: false,
                            },
                            Rect::from_min_size(rect.min, vec2(rect.width(), quarter)),
                        );
                    }
                    if t > 0.75 || !joins {
                        return (
                            Target::Beside {
                                anchor,
                                after: true,
                            },
                            Rect::from_min_max(
                                pos2(rect.left(), rect.bottom() - quarter),
                                rect.max,
                            ),
                        );
                    }
                    return (
                        Target::Join {
                            anchor,
                            before: false,
                        },
                        rect,
                    );
                }
                Holder::Top | Holder::Bottom => {
                    let t = (at.x - rect.left()) / rect.width().max(1.0);
                    let quarter = rect.width() * 0.3;
                    if t < 0.25 || (!joins && t < 0.5) {
                        return (
                            Target::Beside {
                                anchor,
                                after: false,
                            },
                            Rect::from_min_size(rect.min, vec2(quarter, rect.height())),
                        );
                    }
                    if t > 0.75 || !joins {
                        return (
                            Target::Beside {
                                anchor,
                                after: true,
                            },
                            Rect::from_min_max(pos2(rect.right() - quarter, rect.top()), rect.max),
                        );
                    }
                    return (
                        Target::Join {
                            anchor,
                            before: false,
                        },
                        rect,
                    );
                }
            }
        }
        let center = laid.center;
        if center.contains(at) {
            let band = 48.0;
            if at.x < center.left() + band {
                return (
                    Target::Column {
                        side: Side::Left,
                        beside: None,
                    },
                    Rect::from_min_size(center.min, vec2(band, center.height())),
                );
            }
            if at.x > center.right() - band {
                return (
                    Target::Column {
                        side: Side::Right,
                        beside: None,
                    },
                    Rect::from_min_max(pos2(center.right() - band, center.top()), center.max),
                );
            }
            if at.y > center.bottom() - band {
                return (
                    Target::Bottom,
                    Rect::from_min_max(pos2(center.left(), center.bottom() - band), center.max),
                );
            }
            if at.y < center.top() + band {
                return (
                    Target::Top,
                    Rect::from_min_size(center.min, vec2(center.width(), band)),
                );
            }
        }
        let min = at - vec2(30.0, 10.0);
        (
            Target::Float(min),
            Rect::from_min_size(min, moving.float_size()),
        )
    }

    // ----- Interaction --------------------------------------------------------

    /// Lets people drag `panel` from `rect` (its title bar, tab or icon).
    /// It senses drags only: call it before drawing what sits in `rect`,
    /// so buttons there stay on top and keep their clicks.
    pub fn grip(&mut self, ui: &Ui, rect: Rect, panel: P, salt: &str) {
        let response = ui.interact(
            rect,
            Id::unique(("sue-dock-grip", salt, panel.key())),
            Sense::drag(),
        );
        if response.drag_started()
            && let Some(from) = response.interact_pointer_pos()
        {
            self.begin_drag(panel, from);
        }
    }

    /// A group's header, when it has one: its tabs through the shared
    /// tab strip ([`crate::ui::TabStrip`]), each its palette's icon and
    /// name, the one showing underlined. When the names do not all fit,
    /// the tabs give up their names for their icons (the one showing
    /// last), and a tab without its name shows it on hover: no name is
    /// ever cut short. A click shows a tab, the arrow keys walk them, a
    /// drag moves one to another place in the strip and out of it onto
    /// the dock. A lone palette's tab only clicks, so a drag on it moves
    /// the palette at once, by the grip under the header
    /// ([`Self::grip`]).
    ///
    /// Returns where each tab was laid, for [`Self::drop_at`]: the caller
    /// puts them in the group's [`LaidGroup::tab_rects`].
    pub fn tabs_ui(
        &mut self,
        ui: &mut Ui,
        group: &LaidGroup<P>,
        face: &dyn Fn(P) -> TabFace,
        theme: &Theme,
    ) -> Vec<(P, Rect)> {
        if group.tab_rects.is_empty() {
            return Vec::new();
        }
        let strip = Rect::from_min_size(group.rect.min, vec2(group.rect.width(), TAB_STRIP));
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        let inset = group.active.header_inset().min(strip.width());
        let area = Rect::from_min_max(pos2(strip.left() + inset, strip.top()), strip.max);
        let faces: Vec<TabFace> = group.tabs.iter().map(|tab| face(*tab)).collect();
        let tabs: Vec<Tab<'_>> = group
            .tabs
            .iter()
            .zip(&faces)
            .map(|(tab, face)| {
                let made = Tab::new(("sue-dock-tab", tab.key()), &face.name).shortcut(&face.keys);
                match face.icon {
                    Some(icon) => made.icon(icon),
                    None => made,
                }
            })
            .collect();
        let chosen = group
            .tabs
            .iter()
            .position(|tab| *tab == group.active)
            .unwrap_or(0);
        let many = group.tabs.len() > 1;
        let label = faces[chosen].name.clone();
        let output = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("sue-dock-header", group.holder, group.index))
                    .max_rect(area)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.set_clip_rect(area.intersect(ui.clip_rect()));
                    Density::Compact.scope(ui, |ui| {
                        TabStrip::new(("sue-dock-tabs", group.holder, group.index), &tabs, chosen)
                            .reorder(many)
                            .tear(many)
                            .label(&label)
                            .show(ui, theme)
                    })
                },
            )
            .inner;
        // The hairline under the whole header, the menu's room too.
        ui.painter().hline(
            strip.x_range(),
            strip.bottom() - theme.border_width / 2.0,
            theme.hairline(),
        );
        if let Some(index) = output.chosen {
            self.choose(group.tabs[index]);
        }
        if let Some((from, to)) = output.moved {
            let moving = group.tabs[from];
            let drop = match group.tabs.get(to + usize::from(to > from)) {
                Some(anchor) => Target::Join {
                    anchor: *anchor,
                    before: true,
                },
                None => Target::Join {
                    anchor: group.tabs[to],
                    before: false,
                },
            };
            self.put(moving, drop);
            // The palette showing stays: a tab moved is not chosen.
            self.choose(group.active);
        }
        if let Some((index, at)) = output.torn {
            self.begin_drag(group.tabs[index], at);
        }
        let places: Vec<(P, Rect)> = group.tabs.iter().copied().zip(output.places).collect();
        let pass = ui.ctx().cumulative_pass_nr();
        ui.data_mut(|data| {
            for ((tab, place), face) in places.iter().zip(&faces) {
                data.insert_temp(tab_place_id(tab.key()), (pass, *place));
                data.insert_temp(tab_name_id(&face.name), (pass, *place));
            }
        });
        places
    }

    /// Dragging columns' edges, the lines between groups and the bottom
    /// row's top edge.
    pub fn splitters(&mut self, ui: &mut Ui, laid: &Laid<P>, theme: &Theme) {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let lit = Stroke::new(2.0, theme.border_active);
        for (side, column, rect) in &laid.edges {
            let response = ui.interact(
                *rect,
                Id::unique(("sue-dock-edge", *side, *column)),
                Sense::drag(),
            );
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                ui.painter().vline(rect.center().x, rect.y_range(), lit);
            }
            if response.drag_started()
                && let (Some(point), Some(found)) = (pointer, self.columns(*side).get(*column))
            {
                self.drag = Some(Drag::Edge {
                    side: *side,
                    column: *column,
                    from: point.x,
                    width: found.width,
                });
            }
        }
        for split in &laid.splits {
            let response = ui.interact(
                split.rect,
                Id::unique(("sue-dock-split", split.holder, split.upper)),
                Sense::drag(),
            );
            let across = split.holder.is_row();
            if response.hovered() || response.dragged() {
                if across {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                    ui.painter()
                        .vline(split.rect.center().x, split.rect.y_range(), lit);
                } else {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                    ui.painter()
                        .hline(split.rect.x_range(), split.rect.center().y, lit);
                }
            }
            if response.drag_started()
                && let Some(point) = pointer
                && let Some(groups) = self.groups_mut(split.holder)
            {
                let weight = |at: usize| groups.get(at).map_or(0.5, |group| group.weight);
                let total: f32 = groups.iter().map(|group| group.weight.max(0.01)).sum();
                self.drag = Some(Drag::Split {
                    holder: split.holder,
                    upper: split.upper,
                    lower: split.lower,
                    from: if across { point.x } else { point.y },
                    weights: (weight(split.upper), weight(split.lower)),
                    per_point: total / split.span.max(1.0),
                });
            }
        }
        if let Some(row) = laid.bottom {
            let pass = ui.ctx().cumulative_pass_nr();
            ui.ctx().data_mut(|data| {
                data.insert_temp(Id::unique("sue-dock-bottom-row"), (pass, row));
            });
            let top = row.top();
            let rect =
                Rect::from_x_y_ranges(row.x_range(), egui::Rangef::new(top - 3.0, top + 3.0));
            let response = ui.interact(rect, Id::unique("sue-dock-bottom-edge"), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                ui.painter().hline(row.x_range(), top, lit);
            }
            if response.drag_started()
                && let Some(point) = pointer
            {
                // From the height drawn (a stored height taller than the
                // window allows is drawn shorter), up to the room the
                // layout gives the row.
                let room = row.bottom() - laid.center.top() - BOTTOM_LEAVES;
                self.drag = Some(Drag::Bottom {
                    from: point.y,
                    height: row.height(),
                    most: room.max(BOTTOM_LEAST),
                });
            }
        }
        if let Some(row) = laid.top {
            let edge = row.bottom();
            let rect =
                Rect::from_x_y_ranges(row.x_range(), egui::Rangef::new(edge - 3.0, edge + 3.0));
            let response = ui.interact(rect, Id::unique("sue-dock-top-edge"), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                ui.painter().hline(row.x_range(), edge, lit);
            }
            if response.drag_started()
                && let Some(point) = pointer
            {
                self.drag = Some(Drag::Top {
                    from: point.y,
                    height: self.top_height,
                });
            }
        }
        let down = ui.input(|input| input.pointer.primary_down());
        match (self.drag, pointer) {
            (
                Some(Drag::Edge {
                    side,
                    column,
                    from,
                    width,
                }),
                Some(point),
            ) if down => {
                let delta = match side {
                    Side::Left => point.x - from,
                    Side::Right => from - point.x,
                };
                self.set_column_width(side, column, width + delta);
            }
            (
                Some(Drag::Split {
                    holder,
                    upper,
                    lower,
                    from,
                    weights,
                    per_point,
                }),
                Some(point),
            ) if down => {
                let now = if holder.is_row() { point.x } else { point.y };
                let moved = (now - from) * per_point;
                let least = 0.08;
                let up = (weights.0 + moved).max(least);
                let low = (weights.0 + weights.1 - up).max(least);
                if let Some(groups) = self.groups_mut(holder) {
                    if let Some(group) = groups.get_mut(upper) {
                        group.weight = weights.0 + weights.1 - low;
                    }
                    if let Some(group) = groups.get_mut(lower) {
                        group.weight = low;
                    }
                }
            }
            (Some(Drag::Bottom { from, height, most }), Some(point)) if down => {
                self.bottom_height = (height + from - point.y).clamp(BOTTOM_LEAST, most);
            }
            (Some(Drag::Top { from, height }), Some(point)) if down => {
                let least = self
                    .top
                    .iter()
                    .flat_map(|group| group.tabs.iter())
                    .map(|tab| tab.min_height())
                    .fold(24.0, f32::max)
                    .min(90.0);
                self.top_height = (height + point.y - from).clamp(least, 520.0);
            }
            (Some(Drag::Panel { .. }), _) => {}
            (Some(_), _) if !down => self.drag = None,
            _ => {}
        }
    }

    /// While a palette is dragged: lights up where it would go and, when
    /// it is let go, puts it there. Call it after the palettes, the
    /// splitters and the floating windows.
    pub fn drop_ui(&mut self, ui: &Ui, laid: &Laid<P>, title: &dyn Fn(P) -> String, theme: &Theme) {
        let Some(Drag::Panel { panel, from, at }) = self.drag else {
            return;
        };
        let (seen, down) =
            ui.input(|input| (input.pointer.latest_pos(), input.pointer.primary_down()));
        let at = seen.unwrap_or(at);
        self.drag = Some(Drag::Panel { panel, from, at });
        if (at - from).length() < DRAG_START {
            if !down {
                self.drag = None;
            }
            return;
        }
        let (drop, target) = self.drop_at(laid, at, panel);
        if !down {
            self.drag = None;
            let floating = self.floating.iter().any(|(found, _)| *found == panel);
            // A floating window moves by its own title bar.
            if !(floating && matches!(drop, Target::Float(_))) {
                self.put(panel, drop);
            }
            return;
        }
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        // A floating palette dragged by its header moves its own window
        // ([`Self::panel_drag`]): nothing more to show until it is over
        // a place it docks in.
        if matches!(drop, Target::Float(_))
            && self.floating.iter().any(|(found, _)| *found == panel)
        {
            return;
        }
        let painter = ui
            .ctx()
            .layer_painter(LayerId::new(Order::Tooltip, Id::unique("sue-dock-drop")));
        painter.rect_filled(target, 0.0, theme.border_active.gamma_multiply(0.25));
        painter.rect_stroke(
            target,
            0.0,
            Stroke::new(2.0, theme.border_active),
            StrokeKind::Inside,
        );
        let label = painter.layout_no_wrap(title(panel), theme.font(theme.small_size), theme.text);
        let ghost = Rect::from_min_size(at + vec2(14.0, 14.0), label.size() + vec2(16.0, 8.0));
        painter.rect_filled(ghost, theme.control_radius, theme.panel);
        painter.rect_stroke(
            ghost,
            theme.control_radius,
            theme.hairline(),
            StrokeKind::Inside,
        );
        painter.galley(
            Align2::LEFT_CENTER
                .align_size_within_rect(label.size(), ghost.shrink2(vec2(8.0, 0.0)))
                .min,
            label,
            Color32::PLACEHOLDER,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Pane {
        Tools,
        Brush,
        Color,
        Layers,
        Nav,
        History,
        Timeline,
        Bar,
        Slider,
    }

    impl DockPanel for Pane {
        fn all() -> &'static [Self] {
            &[
                Pane::Tools,
                Pane::Brush,
                Pane::Color,
                Pane::Layers,
                Pane::Nav,
                Pane::History,
                Pane::Timeline,
                Pane::Bar,
                Pane::Slider,
            ]
        }

        fn key(self) -> &'static str {
            match self {
                Pane::Tools => "tools",
                Pane::Brush => "brush",
                Pane::Color => "color",
                Pane::Layers => "layers",
                Pane::Nav => "nav",
                Pane::History => "history",
                Pane::Timeline => "timeline",
                Pane::Bar => "bar",
                Pane::Slider => "slider",
            }
        }

        fn named(text: &str) -> Option<Self> {
            Self::all()
                .iter()
                .copied()
                .find(|pane| pane.key() == text || format!("Old {}", pane.key()) == text)
        }

        fn fixed_width(self) -> Option<f32> {
            (self == Pane::Tools).then_some(46.0)
        }

        fn anchored(self) -> bool {
            self == Pane::Tools
        }

        fn optional(self) -> bool {
            self == Pane::Slider
        }

        fn min_height(self) -> f32 {
            if self == Pane::Bar { 34.0 } else { 90.0 }
        }

        fn styles() -> &'static [LayoutStyle<Self>] {
            &STYLES
        }
    }

    /// The test panel's styles: the default, a second one and one that is
    /// not built yet.
    static STYLES: [LayoutStyle<Pane>; 3] = [
        LayoutStyle {
            id: "classic",
            label: "style-classic",
            available: true,
            layout: classic,
        },
        LayoutStyle {
            id: "wide",
            label: "style-wide",
            available: true,
            layout: wide,
        },
        LayoutStyle {
            id: "later",
            label: "style-later",
            available: false,
            layout: wide,
        },
    ];

    fn classic() -> DockLayout<Pane> {
        let mut dock = DockLayout::empty();
        dock.top = vec![Group::one(Pane::Bar, 1.0)];
        dock.left = vec![
            Column::of(&[(Pane::Tools, 1.0)], 46.0),
            Column::of(&[(Pane::Brush, 0.5), (Pane::Color, 0.5)], 240.0),
        ];
        dock.right = vec![Column::of(&[(Pane::Nav, 0.4), (Pane::Layers, 0.6)], 300.0)];
        dock.bottom = vec![Group::one(Pane::Timeline, 1.0)];
        dock.iconized = vec![Pane::History];
        dock
    }

    /// Everything at the right, the timeline folded under the work area
    /// and no history at all.
    fn wide() -> DockLayout<Pane> {
        let mut dock = DockLayout::empty();
        dock.top = vec![Group::one(Pane::Bar, 1.0)];
        dock.right = vec![
            Column::of(&[(Pane::Tools, 1.0)], 46.0),
            Column::of(
                &[(Pane::Brush, 0.3), (Pane::Color, 0.3), (Pane::Nav, 0.4)],
                280.0,
            ),
            Column::of(&[(Pane::Layers, 1.0)], 260.0),
        ];
        dock.bottom = vec![Group::one(Pane::Timeline, 1.0)];
        dock.row_folded[Row::Bottom.index()] = true;
        dock.closed = vec![Pane::History];
        dock
    }

    fn all(_: Pane) -> bool {
        true
    }

    fn body() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))
    }

    #[test]
    fn the_default_layout_places_columns_bottom_and_strip() {
        let dock = DockLayout::<Pane>::default();
        let laid = dock.layout(body(), &all, 0.0);
        assert_eq!(laid.groups.len(), 7);
        assert_eq!(laid.icons, [Pane::History]);
        assert_eq!(laid.center.left(), 46.0 + 240.0);
        assert_eq!(laid.center.right(), 1600.0 - 300.0 - STRIP);
        let timeline = laid.bottom.unwrap();
        assert_eq!(timeline.height(), 160.0);
        assert_eq!(laid.center.bottom(), timeline.top());
        // The top row spans the window over the columns.
        let top = laid.top.unwrap();
        assert_eq!((top.width(), top.height()), (1600.0, 38.0));
        assert_eq!(laid.center.top(), 38.0);
        assert!(laid.columns.iter().all(|(_, _, rect)| rect.top() == 38.0));
        assert_eq!(laid.edges.len(), 3, "every column resizes, the tools too");
        assert_eq!(laid.splits.len(), 2);
        // Without its palette, no bottom row.
        let laid = dock.layout(body(), &|pane| pane != Pane::Timeline, 0.0);
        assert!(laid.bottom.is_none());
        assert_eq!(laid.center.bottom(), 900.0);
    }

    #[test]
    fn palettes_dock_as_tabs_and_tabs_reorder() {
        let mut dock = DockLayout::<Pane>::default();
        dock.put(
            Pane::Color,
            Target::Join {
                anchor: Pane::Layers,
                before: false,
            },
        );
        let (holder, index) = dock.find(Pane::Color).unwrap();
        assert_eq!(holder, Holder::Column(Side::Right, 0));
        assert_eq!(
            dock.right[0].groups[index].tabs,
            [Pane::Layers, Pane::Color]
        );
        assert_eq!(dock.right[0].groups[index].active, 1, "the new tab shows");
        assert_eq!(dock.left[1].groups.len(), 1, "it left its old group");
        // The tab strip shows, and the palette is drawn below it.
        let laid = dock.layout(body(), &all, 0.0);
        let group = laid
            .groups
            .iter()
            .find(|group| group.tabs.contains(&Pane::Color))
            .unwrap();
        assert_eq!(group.tab_rects.len(), 2);
        assert_eq!(group.body.top(), group.rect.top() + TAB_STRIP);
        assert_eq!(group.active, Pane::Color);
        // Dropped before the first tab, it comes first.
        dock.put(
            Pane::Color,
            Target::Join {
                anchor: Pane::Layers,
                before: true,
            },
        );
        assert_eq!(
            dock.right[0].groups[index].tabs,
            [Pane::Color, Pane::Layers]
        );
        dock.choose(Pane::Layers);
        assert_eq!(dock.right[0].groups[index].active, 1);
        // A hidden tab gives way to the next.
        let laid = dock.layout(body(), &|pane| pane != Pane::Layers, 0.0);
        let group = laid
            .groups
            .iter()
            .find(|group| group.tabs.contains(&Pane::Color))
            .unwrap();
        assert_eq!(group.active, Pane::Color);
        assert!(group.tab_rects.is_empty(), "one tab left: no strip");
    }

    #[test]
    fn palettes_undock_float_go_beside_into_columns_and_the_bottom() {
        let mut dock = DockLayout::<Pane>::default();
        dock.put(Pane::Layers, Target::Float(pos2(100.0, 120.0)));
        assert_eq!(dock.find(Pane::Layers), None);
        assert_eq!(dock.floating[0].0, Pane::Layers);
        dock.put(
            Pane::Layers,
            Target::Beside {
                anchor: Pane::Brush,
                after: false,
            },
        );
        assert_eq!(
            dock.find(Pane::Layers),
            Some((Holder::Column(Side::Left, 1), 0))
        );
        assert!(dock.floating.is_empty());
        dock.put(
            Pane::Nav,
            Target::Column {
                side: Side::Left,
                beside: Some((Pane::Tools, true)),
            },
        );
        assert_eq!(
            dock.find(Pane::Nav),
            Some((Holder::Column(Side::Left, 1), 0))
        );
        assert!(dock.right.is_empty(), "the emptied column goes");
        dock.put(Pane::Color, Target::Bottom);
        assert_eq!(dock.find(Pane::Color), Some((Holder::Bottom, 1)));
        dock.put(Pane::Color, Target::Icon);
        assert!(dock.iconized.contains(&Pane::Color));
        // Dropped onto itself, nothing moves.
        let before = dock.clone();
        dock.put(
            Pane::Nav,
            Target::Join {
                anchor: Pane::Nav,
                before: false,
            },
        );
        assert_eq!(dock, before);
        assert!(
            dock.left
                .iter()
                .chain(&dock.right)
                .all(|column| column.groups.iter().all(|group| !group.tabs.is_empty()))
        );
    }

    #[test]
    fn drops_follow_the_pointer() {
        let dock = DockLayout::<Pane>::default();
        let laid = dock.layout(body(), &all, 0.0);
        let layers = laid
            .groups
            .iter()
            .find(|group| group.active == Pane::Layers)
            .unwrap()
            .rect;
        let (drop, _) = dock.drop_at(&laid, layers.center(), Pane::Color);
        assert_eq!(
            drop,
            Target::Join {
                anchor: Pane::Layers,
                before: false
            }
        );
        let (drop, _) = dock.drop_at(&laid, layers.center_top() + vec2(0.0, 5.0), Pane::Color);
        assert_eq!(
            drop,
            Target::Beside {
                anchor: Pane::Layers,
                after: false
            }
        );
        let (drop, _) = dock.drop_at(&laid, layers.left_center() + vec2(3.0, 0.0), Pane::Color);
        assert!(matches!(
            drop,
            Target::Column {
                side: Side::Right,
                beside: Some((_, true))
            }
        ));
        let (drop, _) = dock.drop_at(&laid, laid.strip.unwrap().center(), Pane::Color);
        assert_eq!(drop, Target::Icon);
        let center = laid.center;
        let (drop, _) = dock.drop_at(&laid, center.center(), Pane::Color);
        assert!(matches!(drop, Target::Float(_)));
        let (drop, _) = dock.drop_at(&laid, center.left_center() + vec2(5.0, 0.0), Pane::Color);
        assert_eq!(
            drop,
            Target::Column {
                side: Side::Left,
                beside: None
            }
        );
        // A palette of fixed width never becomes a tab.
        let (drop, _) = dock.drop_at(&laid, layers.center(), Pane::Tools);
        assert!(matches!(drop, Target::Beside { .. }));
    }

    #[test]
    fn a_layout_round_trips_as_lines_and_reads_older_lines() {
        let mut dock = DockLayout::<Pane>::default();
        dock.put(
            Pane::Color,
            Target::Join {
                anchor: Pane::Layers,
                before: false,
            },
        );
        dock.choose(Pane::Layers);
        dock.float(Pane::Nav, pos2(40.0, 60.0));
        dock.bottom_height = 210.0;
        dock.folded = [true, false];
        let back = DockLayout::<Pane>::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock);
        assert!(DockLayout::<Pane>::from_lines(&["nonsense".to_owned()]).is_none());
        // The lines before tabs, with palettes named by title.
        let old: Vec<String> = [
            "left 46 Old tools=1",
            "left 240 Old brush=0.5,Old color=0.5",
            "right 300 Old nav=0.4,Old layers=0.6",
            "icon Old history",
            "timeline 200",
        ]
        .map(String::from)
        .to_vec();
        let read = DockLayout::<Pane>::from_lines(&old).unwrap();
        let expected = DockLayout::<Pane> {
            bottom_height: 200.0,
            ..DockLayout::default()
        };
        assert_eq!(read, expected, "the timeline comes back to the bottom");
    }

    #[test]
    fn a_reset_gives_the_default_and_missing_palettes_come_back() {
        let mut dock = DockLayout::<Pane>::default();
        dock.iconize(Pane::Brush);
        dock.remove(Pane::Timeline);
        dock.fill_missing();
        assert_eq!(dock.find(Pane::Timeline), Some((Holder::Bottom, 0)));
        assert_ne!(dock, DockLayout::default());
        dock = DockLayout::default();
        assert_eq!(dock, Pane::default_layout());
    }

    #[test]
    fn a_narrow_window_collapses_the_widest_columns_but_never_the_anchored() {
        let dock = DockLayout::<Pane>::default();
        let narrow = Rect::from_min_size(Pos2::ZERO, vec2(700.0, 700.0));
        let laid = dock.layout(narrow, &all, 0.0);
        assert!(laid.icons.contains(&Pane::Layers));
        assert!(laid.center.width() >= MIN_CENTER - 0.5);
        assert!(laid.docked().any(|(pane, _)| pane == Pane::Tools));
    }

    #[test]
    fn palettes_dock_at_the_top_and_its_edge_drops_there() {
        let mut dock = DockLayout::<Pane>::default();
        let laid = dock.layout(body(), &all, 0.0);
        // The band along the work area's top drops into the top row.
        let (drop, _) = dock.drop_at(
            &laid,
            laid.center.center_top() + vec2(0.0, 10.0),
            Pane::Layers,
        );
        assert_eq!(drop, Target::Top);
        dock.put(Pane::Layers, drop);
        assert_eq!(dock.find(Pane::Layers), Some((Holder::Top, 1)));
        assert_eq!(dock.top_height, 90.0, "the row grows to what it holds");
        let laid = dock.layout(body(), &all, 0.0);
        let row = laid.top.unwrap();
        let layers = laid
            .groups
            .iter()
            .find(|group| group.active == Pane::Layers)
            .unwrap();
        assert_eq!(layers.holder, Holder::Top);
        assert_eq!(layers.rect.top(), row.top());
        assert!(
            laid.splits.iter().any(|split| split.holder == Holder::Top),
            "the line between the bar and the layers drags"
        );
        // Dropped on the bar's right end: a group of its own after it.
        let bar = laid
            .groups
            .iter()
            .find(|group| group.active == Pane::Bar)
            .unwrap()
            .rect;
        let (drop, _) = dock.drop_at(&laid, bar.right_center() - vec2(4.0, 0.0), Pane::Nav);
        assert_eq!(
            drop,
            Target::Beside {
                anchor: Pane::Bar,
                after: true
            }
        );
        // Hidden, the bar leaves no row; sliding out lifts the row away.
        let laid = dock.layout(
            body(),
            &|pane| !matches!(pane, Pane::Bar | Pane::Layers),
            0.0,
        );
        assert!(laid.top.is_none());
        assert_eq!(laid.center.top(), 0.0);
        let laid = dock.layout(body(), &all, 1.0);
        assert_eq!(laid.top.unwrap().bottom(), 0.0);
    }

    #[test]
    fn new_columns_pop_outs_and_narrow_columns_resize() {
        let mut dock = DockLayout::<Pane>::default();
        // New Column on the Right: the innermost right column.
        dock.put(
            Pane::Color,
            Target::Column {
                side: Side::Right,
                beside: None,
            },
        );
        assert_eq!(
            dock.find(Pane::Color),
            Some((Holder::Column(Side::Right, 1), 0))
        );
        // A tool strip's column widens into a palette of several columns
        // and narrows no further than the strip.
        dock.set_column_width(Side::Left, 0, 120.0);
        assert_eq!(dock.left[0].width, 120.0);
        dock.set_column_width(Side::Left, 0, 10.0);
        assert_eq!(dock.left[0].width, 46.0);
        dock.set_column_width(Side::Right, 0, 9000.0);
        assert_eq!(dock.right[0].width, 560.0);
        let laid = dock.layout(body(), &all, 0.0);
        assert!(
            laid.edges
                .iter()
                .any(|(side, index, _)| *side == Side::Left && *index == 0)
        );
        // Popped out into a window of its own: open, not docked.
        let rect = Rect::from_min_size(pos2(-200.0, 50.0), vec2(300.0, 400.0));
        dock.pop_out(Pane::Layers, rect);
        assert!(dock.is_open(Pane::Layers));
        assert_eq!(dock.find(Pane::Layers), None);
        assert_eq!(dock.windows, [(Pane::Layers, rect)]);
        // Iconized is somewhere but not open.
        assert!(dock.has(Pane::History) && !dock.is_open(Pane::History));
        // Docked again, it leaves its window.
        dock.dock(Pane::Layers, Side::Left);
        assert!(dock.windows.is_empty());
        let back = DockLayout::<Pane>::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn optional_palettes_tear_off_and_stay_away_until_then() {
        let mut dock = DockLayout::<Pane>::default();
        assert!(!dock.has(Pane::Slider));
        dock.fill_missing();
        assert!(!dock.has(Pane::Slider), "not put back by itself");
        // Torn off a tab: dragged from where the pointer pressed.
        dock.begin_drag(Pane::Slider, pos2(400.0, 300.0));
        assert_eq!(dock.dragged_panel(), Some(Pane::Slider));
        dock.drag = None;
        dock.put(
            Pane::Slider,
            Target::Beside {
                anchor: Pane::Nav,
                after: true,
            },
        );
        assert_eq!(
            dock.find(Pane::Slider),
            Some((Holder::Column(Side::Right, 0), 1))
        );
        dock.pop_out(
            Pane::Slider,
            Rect::from_min_size(pos2(10.0, 10.0), vec2(200.0, 200.0)),
        );
        let lines = dock.to_lines();
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("window 10 10 200 200 slider"))
        );
        let back = DockLayout::<Pane>::from_lines(&lines).unwrap();
        assert_eq!(back, dock);
        dock.remove(Pane::Slider);
        assert!(!dock.has(Pane::Slider));
    }

    #[test]
    fn layouts_kept_before_the_top_row_open_with_the_bar_on_top() {
        // Lines as the dock wrote them before it had a top row.
        let old: Vec<String> = [
            "column left 46 1:tools",
            "column left 240 0.5:brush 0.5:color",
            "column right 300 0.4:nav 0.6:layers",
            "bottom 180 1:timeline",
            "float 40 60 260 320 history",
        ]
        .map(String::from)
        .to_vec();
        let read = DockLayout::<Pane>::from_lines(&old).unwrap();
        assert_eq!(read.top, [Group::one(Pane::Bar, 1.0)]);
        assert_eq!(read.top_height, 38.0);
        assert_eq!(read.bottom_height, 180.0);
        assert_eq!(read.floating[0].0, Pane::History);
        assert!(read.windows.is_empty());
    }

    #[test]
    fn reset_goes_to_the_saved_default_then_the_studios_own() {
        let mut dock = DockLayout::<Pane>::default();
        dock.put(Pane::Layers, Target::Top);
        let saved = dock.clone();
        dock.float(Pane::Brush, pos2(300.0, 300.0));
        let model = Pane::default_layout();
        assert!(
            dock.reset(Some(&saved), &model),
            "back to the saved default"
        );
        assert_eq!(dock, saved);
        assert!(
            !dock.reset(Some(&saved), &model),
            "already there: the studio's own"
        );
        assert_eq!(dock, Pane::default_layout());
        assert!(!dock.reset(None, &model));
        assert_eq!(dock, Pane::default_layout());
    }

    #[test]
    fn styles_are_a_registry_and_an_unbuilt_one_is_never_chosen() {
        assert_eq!(Pane::default_layout(), classic(), "the first style's");
        assert_eq!(DockLayout::<Pane>::default(), classic());
        assert_eq!(Pane::style("wide").map(|style| style.id), Some("wide"));
        // Grayed out, unknown or missing: the default.
        for id in ["later", "nonsense", ""] {
            assert_eq!(Pane::style(id).map(|style| style.id), Some("classic"));
            assert_eq!(Pane::style_layout(id), classic());
        }
        assert_eq!(Pane::style_layout("wide"), wide());
        assert!(STYLES.iter().all(|style| !style.label.is_empty()));
    }

    #[test]
    fn a_second_style_resets_to_its_own_layout() {
        let model = Pane::style_layout("wide");
        let mut dock = DockLayout::<Pane>::default();
        dock.float(Pane::Layers, pos2(500.0, 200.0));
        dock.menu = Some(Pane::Nav);
        assert!(!dock.reset(None, &model));
        assert_eq!(dock, wide());
        assert!(!dock.has(Pane::History) && dock.is_closed(Pane::History));
        // A saved default first, then the style's own again.
        let mut saved = wide();
        saved.put(Pane::Nav, Target::Top);
        assert!(dock.reset(Some(&saved), &model));
        assert_eq!(dock, saved);
        assert!(!dock.reset(Some(&saved), &model));
        assert_eq!(dock, wide());
        // The style's layout lays out as it says: no columns at the left,
        // the timeline an icon under the work area.
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.columns.iter().all(|(side, _, _)| *side == Side::Right));
        assert_eq!(laid.center.left(), 0.0);
        assert!(laid.bottom.is_none());
        let strip = laid.bottom_strip.unwrap();
        assert_eq!(strip.groups, [vec![Pane::Timeline]]);
        assert_eq!(strip.rect.height(), STRIP);
        assert_eq!(laid.center.bottom(), strip.rect.top());
        assert_eq!(strip.rect.bottom(), 900.0);
        // Palettes a layout lacks go where the style has them.
        let mut lacking = wide();
        lacking.remove(Pane::Nav);
        lacking.fill_missing_from(&model);
        assert_eq!(lacking.side_of(Pane::Nav), Some(Side::Right));
        assert!(!lacking.has(Pane::History), "closed in the style");
        let read =
            DockLayout::<Pane>::from_lines_with(&["column right 300 1:nav".to_owned()], &model)
                .unwrap();
        assert_eq!(read.side_of(Pane::Brush), Some(Side::Right));
        assert!(!read.has(Pane::History));
    }

    #[test]
    fn an_emptied_layout_stays_empty_after_save_and_load() {
        let mut dock = DockLayout::<Pane>::default();
        for &pane in Pane::all() {
            dock.close(pane);
        }
        assert!(Pane::all().iter().all(|pane| !dock.has(*pane)));
        dock.fill_missing();
        assert!(Pane::all().iter().all(|pane| !dock.has(*pane)));
        let lines = dock.to_lines();
        assert!(lines.contains(&"close history".to_owned()));
        let back = DockLayout::<Pane>::from_lines(&lines).expect("an empty layout is a layout");
        assert_eq!(back, dock);
        assert!(Pane::all().iter().all(|pane| !back.has(*pane)));
        let laid = back.layout(body(), &all, 0.0);
        assert!(laid.groups.is_empty() && laid.icons.is_empty());
        assert_eq!(laid.center, body(), "the work area has the window");
    }

    #[test]
    fn a_closed_palette_stays_closed_until_it_is_placed_again() {
        let mut dock = Pane::default_layout();
        dock.menu = Some(Pane::Layers);
        dock.close(Pane::Layers);
        assert_eq!(dock.menu, None, "its menu closes with it");
        dock.close(Pane::History);
        dock.close(Pane::History);
        assert_eq!(dock.closed, [Pane::Layers, Pane::History]);
        dock.fill_missing();
        assert!(!dock.has(Pane::Layers) && !dock.has(Pane::History));
        let back = DockLayout::<Pane>::from_lines(&dock.to_lines()).unwrap();
        assert_eq!(back, dock);
        // Placed again (from the Window menu, say), it is no longer
        // closed; nor after a drop.
        dock.dock(Pane::Layers, Side::Right);
        assert!(!dock.is_closed(Pane::Layers));
        dock.put(Pane::History, Target::Bottom);
        assert!(!dock.is_closed(Pane::History));
        assert!(dock.closed.is_empty());
        // A palette a file both places and closes is placed.
        let read = DockLayout::<Pane>::from_lines(
            &["column left 46 1:tools", "close tools"].map(String::from),
        )
        .unwrap();
        assert!(read.has(Pane::Tools) && !read.is_closed(Pane::Tools));
        // Older lines without close lines refill as before.
        let old = DockLayout::<Pane>::from_lines(&["column left 46 1:tools".to_owned()]).unwrap();
        assert!(old.has(Pane::History) && old.has(Pane::Layers));
        // Refilling a closed side puts the palette there and keeps the
        // side closed.
        let read = DockLayout::<Pane>::from_lines(
            &["column left 46 1:tools", "hide left"].map(String::from),
        )
        .unwrap();
        assert_eq!(read.side_of(Pane::Brush), Some(Side::Left));
        assert!(read.hidden[Side::Left.index()], "the side stays closed");
    }

    #[test]
    fn the_top_and_bottom_rows_fold_and_close_like_the_sides() {
        let mut dock = DockLayout::<Pane>::default();
        dock.row_folded[Row::Bottom.index()] = true;
        dock.put(
            Pane::Brush,
            Target::Join {
                anchor: Pane::Timeline,
                before: false,
            },
        );
        dock.put(
            Pane::Color,
            Target::Beside {
                anchor: Pane::Timeline,
                after: true,
            },
        );
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.bottom.is_none(), "no row, only its icons");
        let strip = laid.bottom_strip.clone().unwrap();
        assert_eq!(
            strip.groups,
            [vec![Pane::Timeline, Pane::Brush], vec![Pane::Color]],
            "a group of tabs is one icon group"
        );
        assert_eq!(
            (strip.rect.left(), strip.rect.right()),
            (laid.center.left(), laid.center.right()),
            "under the work area, between the columns"
        );
        assert_eq!(laid.center.bottom(), 900.0 - STRIP);
        assert!(
            laid.splits
                .iter()
                .all(|split| split.holder != Holder::Bottom),
            "nothing to drag in a folded row"
        );
        // Dropped on the strip, a palette joins the row.
        let (drop, _) = dock.drop_at(&laid, strip.rect.center(), Pane::Nav);
        assert_eq!(drop, Target::Bottom);
        // Hidden palettes leave no icon; a row with none shows nothing.
        let laid = dock.layout(
            body(),
            &|pane| !matches!(pane, Pane::Timeline | Pane::Brush | Pane::Color),
            0.0,
        );
        assert!(laid.bottom_strip.is_none());
        assert_eq!(laid.center.bottom(), 900.0);
        // Closed: a thin bar that takes drops and opens the row again.
        dock.row_hidden[Row::Bottom.index()] = true;
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.bottom_strip.is_none() && laid.bottom.is_none());
        let (row, bar) = laid.row_handles[0];
        assert_eq!((row, bar.height()), (Row::Bottom, HANDLE));
        assert_eq!(laid.center.bottom(), bar.top());
        let (drop, _) = dock.drop_at(&laid, bar.center(), Pane::Nav);
        assert_eq!(drop, Target::Bottom);
        dock.put(Pane::Nav, drop);
        assert!(!dock.row_hidden[Row::Bottom.index()], "a drop opens it");
        // The top row: folded across the window, closed to a bar.
        dock.row_folded[Row::Top.index()] = true;
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.top.is_none());
        let top = laid.top_strip.clone().unwrap();
        assert_eq!(
            top.rect,
            Rect::from_min_size(Pos2::ZERO, vec2(1600.0, STRIP))
        );
        assert_eq!(top.groups, [vec![Pane::Bar]]);
        assert!(laid.columns.iter().all(|(_, _, rect)| rect.top() == STRIP));
        dock.row_hidden[Row::Top.index()] = true;
        let laid = dock.layout(body(), &all, 0.0);
        assert!(laid.top_strip.is_none());
        assert!(laid.row_handles.contains(&(
            Row::Top,
            Rect::from_min_size(Pos2::ZERO, vec2(1600.0, HANDLE))
        )));
        // Sliding out (Tab) moves the bar away with the rest.
        let laid = dock.layout(body(), &all, 1.0);
        assert!(
            laid.row_handles
                .iter()
                .all(|(_, rect)| rect.bottom() <= 0.0 || rect.top() >= 900.0)
        );
        // The states are kept, and older readers skip their lines.
        dock.row_hidden[Row::Bottom.index()] = true;
        let lines = dock.to_lines();
        for line in ["fold top", "hide top", "fold bottom", "hide bottom"] {
            assert!(lines.contains(&line.to_owned()), "{line}");
        }
        let back = DockLayout::<Pane>::from_lines(&lines).unwrap();
        assert_eq!(back, dock);
    }

    #[test]
    fn collapsed_icons_group_as_their_tabs_did() {
        let mut dock = DockLayout::<Pane>::default();
        dock.put(
            Pane::Color,
            Target::Join {
                anchor: Pane::Brush,
                before: false,
            },
        );
        dock.iconize_group(Pane::Color);
        assert_eq!(
            dock.icon_groups(),
            [vec![Pane::History], vec![Pane::Brush, Pane::Color]]
        );
        assert_eq!(dock.find(Pane::Brush), None);
        // Dropping one icon on another joins its group.
        dock.join_icon(Pane::History, Pane::Color);
        assert_eq!(
            dock.icon_groups(),
            [vec![Pane::Brush, Pane::Color, Pane::History]]
        );
        dock.join_icon(Pane::Nav, Pane::Tools);
        assert!(
            dock.is_open(Pane::Nav),
            "Tools is not an icon: nothing moves"
        );
        // The strip shows the groups; the flat list stays as it was.
        let laid = dock.layout(body(), &all, 0.0);
        assert_eq!(
            laid.icon_groups,
            [vec![Pane::Brush, Pane::Color, Pane::History]]
        );
        assert_eq!(laid.icons, [Pane::Brush, Pane::Color, Pane::History]);
        let laid = dock.layout(body(), &|pane| pane != Pane::Color, 0.0);
        assert_eq!(laid.icon_groups, [vec![Pane::Brush, Pane::History]]);
        // Groups survive save and load, and an older reader sees icons.
        let lines = dock.to_lines();
        assert!(lines.contains(&"icon-join color".to_owned()));
        let back = DockLayout::<Pane>::from_lines(&lines).unwrap();
        assert_eq!(back, dock);
        // Taking the group's first icon out leaves the rest together.
        dock.dock(Pane::Brush, Side::Left);
        assert_eq!(dock.icon_groups(), [vec![Pane::Color, Pane::History]]);
        dock.remove(Pane::History);
        assert_eq!(dock.icon_groups(), [vec![Pane::Color]]);
        dock.iconize(Pane::History);
        assert_eq!(
            dock.icon_groups(),
            [vec![Pane::Color], vec![Pane::History]],
            "a plain collapse is an icon of its own"
        );
        // A folded side's groups stay together in its strip.
        let mut dock = DockLayout::<Pane>::default();
        dock.put(
            Pane::Layers,
            Target::Join {
                anchor: Pane::Nav,
                before: false,
            },
        );
        dock.folded = [true, true];
        let laid = dock.layout(body(), &all, 0.0);
        assert_eq!(
            laid.left_icon_groups,
            [vec![Pane::Brush], vec![Pane::Color]]
        );
        assert_eq!(
            laid.icon_groups,
            [vec![Pane::History], vec![Pane::Nav, Pane::Layers]]
        );
        assert_eq!(laid.left_icons, [Pane::Brush, Pane::Color]);
    }
}
