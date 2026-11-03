//! A dock for a studio's palettes, as Clip Studio Paint's: columns at the
//! left and right of the work area and a row along its bottom, each made of
//! palette groups that share the room by weight; a group holds one palette
//! or several as tabs. Palettes can also float in windows of their own or
//! collapse to icons in a strip at the right.
//!
//! People drag a palette by its title bar, its tab or its icon onto a
//! group (it becomes a tab there), above, below or beside a group (a group
//! of its own), against a column's edge (a column of its own), onto the
//! bottom, into the strip, or anywhere else to float it; they drag the
//! lines between columns and groups to share the room differently. The
//! layout reads and writes as lines of text ([`DockLayout::to_lines`]), so
//! a studio keeps it between runs and under names (workspaces).
//!
//! The studio says what its palettes are ([`DockPanel`]) and draws them in
//! the rects [`DockLayout::layout`] gives; the dock draws the tabs, the
//! splitters and the drop targets and does the moving.

use egui::{
    Align2, Color32, CursorIcon, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};

use crate::Theme;

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

    /// The width of a palette that is always as wide (a tool strip).
    fn fixed_width(self) -> Option<f32> {
        None
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

    /// Where the palettes are until people move them, and after Reset.
    fn default_layout() -> DockLayout<Self>;
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
    /// The row along the bottom.
    Bottom,
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
    /// The bottom row's top edge.
    Bottom { from: f32, height: f32 },
    /// A palette, by its title bar, tab or icon.
    Panel { panel: P, from: Pos2 },
}

/// Where every palette is.
#[derive(Clone, Debug, PartialEq)]
pub struct DockLayout<P> {
    /// Columns from the window's left edge inward.
    pub left: Vec<Column<P>>,
    /// Columns from the window's right edge inward.
    pub right: Vec<Column<P>>,
    /// The row along the bottom of the work area, left to right.
    pub bottom: Vec<Group<P>>,
    /// The bottom row's height.
    pub bottom_height: f32,
    /// Floating palettes and their windows' rects.
    pub floating: Vec<(P, Rect)>,
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

/// The height of a group's tab strip, when it has more than one tab.
pub const TAB_STRIP: f32 = 22.0;

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
    /// Where the palette showing is drawn: below the tab strip, if any.
    pub body: Rect,
    /// Each tab's rect in the tab strip (empty for a lone palette).
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
    /// The bottom row, when it shows.
    pub bottom: Option<Rect>,
    /// What is left for the work area.
    pub center: Rect,
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

/// Equal tabs across a group's tab strip.
fn tab_rects<P: Copy>(strip: Rect, tabs: &[P]) -> Vec<(P, Rect)> {
    if tabs.len() < 2 {
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
    let (body, tab_rects) = if tabs.len() > 1 {
        let strip = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_STRIP));
        (
            Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max),
            tab_rects(strip, &tabs),
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
            bottom: Vec::new(),
            bottom_height: 160.0,
            floating: Vec::new(),
            iconized: Vec::new(),
            popped: None,
            menu: None,
            folded: [false; 2],
            hidden: [false; 2],
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
            Holder::Bottom => Some(&mut self.bottom),
        }
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
        self.bottom
            .iter()
            .position(|group| group.tabs.contains(&panel))
            .map(|group| (Holder::Bottom, group))
    }

    /// `panel` is somewhere: docked, floating or in the strip.
    #[must_use]
    pub fn has(&self, panel: P) -> bool {
        self.find(panel).is_some()
            || self.floating.iter().any(|(found, _)| *found == panel)
            || self.iconized.contains(&panel)
    }

    /// The side `panel` is docked on, if it is in a column.
    #[must_use]
    pub fn side_of(&self, panel: P) -> Option<Side> {
        match self.find(panel) {
            Some((Holder::Column(side, _), _)) => Some(side),
            _ => None,
        }
    }

    /// Takes `panel` out of wherever it is; groups and columns left empty
    /// go.
    pub fn remove(&mut self, panel: P) {
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
        take(&mut self.bottom);
        self.left.retain(|column| !column.groups.is_empty());
        self.right.retain(|column| !column.groups.is_empty());
        self.floating.retain(|(found, _)| *found != panel);
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
            Target::Bottom => {
                self.remove(panel);
                self.bottom.push(Group::one(panel, 1.0));
            }
            Target::Icon => self.iconize(panel),
            Target::Float(at) => self.float(panel, at),
            Target::Join { .. } | Target::Beside { .. } => {}
        }
    }

    /// Puts every palette the layout lacks where the default layout has
    /// it (a layout saved before the palette existed).
    pub fn fill_missing(&mut self) {
        let model = P::default_layout();
        for &panel in P::all() {
            if self.has(panel) {
                continue;
            }
            match model.find(panel) {
                Some((Holder::Column(side, _), _)) => self.dock(panel, side),
                Some((Holder::Bottom, _)) => self.bottom.push(Group::one(panel, 1.0)),
                None => match model.floating.iter().find(|(found, _)| *found == panel) {
                    Some((_, rect)) => self.floating.push((panel, *rect)),
                    None => self.iconized.push(panel),
                },
            }
        }
    }

    // ----- Lines ------------------------------------------------------------

    /// The layout as lines of text: each column with its width and groups,
    /// the bottom row, the floating palettes, the collapsed ones and the
    /// folded and closed sides. A group reads `weight:key+key@active`.
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
        let groups: Vec<String> = self.bottom.iter().map(group).collect();
        out.push(
            format!("bottom {} {}", self.bottom_height, groups.join(" "))
                .trim_end()
                .to_owned(),
        );
        for (panel, rect) in &self.floating {
            out.push(format!(
                "float {} {} {} {} {}",
                rect.min.x,
                rect.min.y,
                rect.width(),
                rect.height(),
                panel.key()
            ));
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
        out
    }

    /// A layout from [`Self::to_lines`], or from the older lines that
    /// named one palette per group (`left 236 Title=0.44,Title=0.3`, and
    /// `timeline` for the bottom's height). Lines it does not know are
    /// skipped, and palettes it lacks go where the default layout has
    /// them. `None` when no palette is placed.
    #[must_use]
    pub fn from_lines(lines: &[String]) -> Option<Self> {
        let mut dock = Self::empty();
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
                (Some("float"), Some(x), Some(rest)) => {
                    let parts: Vec<&str> = rest.splitn(4, ' ').collect();
                    if let ([y, w, h, name], Ok(x)) = (parts.as_slice(), x.parse::<f32>())
                        && let (Ok(y), Ok(w), Ok(h), Some(panel)) = (
                            y.parse::<f32>(),
                            w.parse::<f32>(),
                            h.parse::<f32>(),
                            P::named(name),
                        )
                    {
                        dock.floating
                            .push((panel, Rect::from_min_size(pos2(x, y), vec2(w, h))));
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
                _ => {}
            }
        }
        let placed = !dock.left.is_empty()
            || !dock.right.is_empty()
            || !dock.bottom.is_empty()
            || !dock.floating.is_empty()
            || !dock.iconized.is_empty();
        dock.fill_missing();
        placed.then_some(dock)
    }

    // ----- Layout -----------------------------------------------------------

    /// Lays everything out in `body`; `shown` says which palettes may show
    /// and `hide` (0 to 1) slides the columns out to their edges and the
    /// bottom row down.
    pub fn layout(&self, body: Rect, shown: &dyn Fn(P) -> bool, hide: f32) -> Laid<P> {
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
        let width_of = |column: &Column<P>| -> f32 {
            visible_tabs(column)
                .iter()
                .filter_map(|panel| panel.fixed_width())
                .fold(None, |most: Option<f32>, width| {
                    Some(most.map_or(width, |most| most.max(width)))
                })
                .unwrap_or(column.width)
        };
        // Columns that show anything, and their widths. A closed side shows
        // nothing; a folded side keeps its anchored columns and folds the
        // rest into icons.
        let mut sides: Vec<(Side, usize, f32, bool)> = Vec::new();
        let mut left_icons: Vec<P> = Vec::new();
        let mut folded_right: Vec<P> = Vec::new();
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
                        &mut left_icons
                    } else {
                        &mut folded_right
                    };
                    into.extend(visible_tabs(column));
                } else {
                    sides.push((side, index, width_of(column), true));
                }
            }
        }
        let mut icons: Vec<P> = if self.hidden[1] {
            Vec::new()
        } else {
            self.iconized
                .iter()
                .copied()
                .filter(|panel| shown(*panel))
                .collect()
        };
        icons.extend(folded_right);
        // Too narrow: the widest columns collapse to icons (never the
        // anchored ones).
        loop {
            let used: f32 = sides
                .iter()
                .filter(|(_, _, _, open)| *open)
                .map(|(_, _, width, _)| width)
                .sum();
            let strip = if icons.is_empty() { 0.0 } else { STRIP }
                + if left_icons.is_empty() { 0.0 } else { STRIP };
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
            icons.extend(visible_tabs(&self.columns(widest.0)[widest.1]));
        }
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
            bottom: None,
            center: body,
        };
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
                        + if tabs.len() > 1 { TAB_STRIP } else { 0.0 };
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
            let resizable = column.groups.iter().all(|group| !group.fixed());
            if resizable {
                let x = match side {
                    Side::Left => rect.right(),
                    Side::Right => rect.left(),
                };
                laid.edges.push((
                    side,
                    index,
                    Rect::from_center_size(pos2(x, rect.center().y), vec2(6.0, rect.height())),
                ));
            }
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
        if !bottom.is_empty() && center.height() > 360.0 && center.width() > 320.0 {
            let height = self.bottom_height.min(center.height() - 200.0).max(90.0);
            let top = center.bottom() - height * (1.0 - hide);
            let row =
                Rect::from_min_max(pos2(center.left(), top), pos2(center.right(), top + height));
            let entries: Vec<(f32, f32)> = bottom
                .iter()
                .map(|at| (self.bottom[*at].weight, 120.0))
                .collect();
            let widths = share(row.width(), &entries);
            let mut left = row.left();
            for (at, (group, width)) in bottom.iter().zip(&widths).enumerate() {
                let piece = Rect::from_min_size(pos2(left, row.top()), vec2(*width, row.height()));
                if let Some(found) =
                    lay_group(Holder::Bottom, *group, &self.bottom[*group], piece, shown)
                {
                    laid.groups.push(found);
                }
                left = piece.right();
                if let Some(next) = bottom.get(at + 1) {
                    laid.splits.push(Split {
                        holder: Holder::Bottom,
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
            laid.bottom = Some(row);
            laid.center.max.y = top;
        }
        laid
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
        for group in &laid.groups {
            if !group.rect.contains(at) {
                continue;
            }
            let anchor = group.active;
            let joins = !fixed && group.tabs.iter().all(|tab| tab.fixed_width().is_none());
            if joins
                && let Some((tab, rect)) =
                    group.tab_rects.iter().find(|(_, rect)| rect.contains(at))
            {
                return (
                    Target::Join {
                        anchor: *tab,
                        before: true,
                    },
                    Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                );
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
                Holder::Bottom if !fixed => {
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
                Holder::Bottom => {}
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
            if !fixed && at.y > center.bottom() - band {
                return (
                    Target::Bottom,
                    Rect::from_min_max(pos2(center.left(), center.bottom() - band), center.max),
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
    /// It senses drags only, so buttons in `rect` keep their clicks.
    pub fn grip(&mut self, ui: &Ui, rect: Rect, panel: P, salt: &str) {
        let response = ui.interact(
            rect,
            Id::unique(("sue-dock-grip", salt, panel.key())),
            Sense::drag(),
        );
        if response.drag_started()
            && self.drag.is_none()
            && let Some(from) = response.interact_pointer_pos()
        {
            self.drag = Some(Drag::Panel { panel, from });
        }
    }

    /// A group's tab strip, when it has more than one tab: a click shows a
    /// tab, a drag moves it (to another place in the strip, too).
    pub fn tabs_ui(
        &mut self,
        ui: &mut Ui,
        group: &LaidGroup<P>,
        title: &dyn Fn(P) -> String,
        theme: &Theme,
    ) {
        if group.tab_rects.is_empty() {
            return;
        }
        let strip = Rect::from_min_size(group.rect.min, vec2(group.rect.width(), TAB_STRIP));
        let painter = ui.painter().with_clip_rect(strip);
        painter.rect_filled(strip, 0.0, theme.void);
        for (tab, rect) in &group.tab_rects {
            let response = ui.interact(
                *rect,
                Id::unique(("sue-dock-tab", tab.key())),
                Sense::click(),
            );
            let chosen = *tab == group.active;
            if chosen {
                painter.rect_filled(*rect, 0.0, theme.panel);
            } else if response.hovered() {
                painter.rect_filled(*rect, 0.0, theme.hover);
            }
            painter.vline(rect.right(), rect.y_range(), theme.hairline());
            let text = title(*tab);
            let galley = painter.layout_no_wrap(
                text.clone(),
                theme.font(theme.small_size),
                if chosen { theme.text } else { theme.muted },
            );
            let at = pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0);
            painter
                .with_clip_rect(rect.shrink2(vec2(2.0, 0.0)).intersect(strip))
                .galley(at, galley, theme.text);
            if response.on_hover_text(text).clicked() {
                self.choose(*tab);
            }
            self.grip(ui, *rect, *tab, "tab");
        }
        painter.hline(strip.x_range(), strip.bottom(), theme.hairline());
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
            let across = split.holder == Holder::Bottom;
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
                self.drag = Some(Drag::Bottom {
                    from: point.y,
                    height: self.bottom_height,
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
                if let Some(found) = self.columns_mut(side).get_mut(column) {
                    found.width = (width + delta).clamp(170.0, 560.0);
                }
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
                let now = if holder == Holder::Bottom {
                    point.x
                } else {
                    point.y
                };
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
            (Some(Drag::Bottom { from, height }), Some(point)) if down => {
                self.bottom_height = (height + from - point.y).clamp(90.0, 520.0);
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
        let Some(Drag::Panel { panel, from }) = self.drag else {
            return;
        };
        let (at, down) =
            ui.input(|input| (input.pointer.latest_pos(), input.pointer.primary_down()));
        let Some(at) = at else {
            if !down {
                self.drag = None;
            }
            return;
        };
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

        fn default_layout() -> DockLayout<Self> {
            let mut dock = DockLayout::empty();
            dock.left = vec![
                Column::of(&[(Pane::Tools, 1.0)], 46.0),
                Column::of(&[(Pane::Brush, 0.5), (Pane::Color, 0.5)], 240.0),
            ];
            dock.right = vec![Column::of(&[(Pane::Nav, 0.4), (Pane::Layers, 0.6)], 300.0)];
            dock.bottom = vec![Group::one(Pane::Timeline, 1.0)];
            dock.iconized = vec![Pane::History];
            dock
        }
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
        assert_eq!(laid.groups.len(), 6);
        assert_eq!(laid.icons, [Pane::History]);
        assert_eq!(laid.center.left(), 46.0 + 240.0);
        assert_eq!(laid.center.right(), 1600.0 - 300.0 - STRIP);
        let timeline = laid.bottom.unwrap();
        assert_eq!(timeline.height(), 160.0);
        assert_eq!(laid.center.bottom(), timeline.top());
        assert_eq!(laid.edges.len(), 2, "the two wide columns resize");
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
}
