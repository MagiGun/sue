//! Drawing Sue VE: the menu bar, the tool row, the pages along the bottom
//! (as DaVinci Resolve has them), and on the Edit page Sue's dock with the
//! media pool, the effects browser, the inspector, Effect Controls, the
//! timeline, the graph editor, the scopes, the mixer and the audio meters
//! around the source and program monitors, arranged by workspaces as
//! Premiere Pro's are. On phones and tablets the studio takes CapCut's
//! single column instead (`mobile`).

use egui::{Rect, Ui, UiBuilder, pos2, vec2};
use sue_core::Time;
use sue_wm::dock::{Column, Group, TabFace};
use sue_wm::dock::{DockLayout, DockPanel};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, AppletId, Facts, Theme};

use crate::studio::{Dialog, ID, Page, RATES, SueVe, Tool};

pub mod audio_page;
pub mod browser;
pub mod color_page;
pub mod controls;
pub mod effects_page;
pub mod graph;
pub mod inspector;
pub mod mobile;
pub mod monitor;
pub mod pages;
pub mod panels;
pub mod pool;
pub mod timeline;

/// The palettes of the Edit page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The media pool.
    Pool,
    /// The effects browser: effects, transitions, presets.
    Effects,
    /// The selected clip's settings.
    Inspector,
    /// The timeline.
    Timeline,
    /// The audio meters.
    Meters,
    /// Effect Controls: the selected clip's effects stack.
    Controls,
    /// The graph editor.
    Graph,
    /// The video scopes.
    Scopes,
    /// The mixer.
    Mixer,
}

impl Panel {
    /// Every palette.
    pub const ALL: [Self; 9] = [
        Self::Pool,
        Self::Effects,
        Self::Inspector,
        Self::Timeline,
        Self::Meters,
        Self::Controls,
        Self::Graph,
        Self::Scopes,
        Self::Mixer,
    ];

    /// Its name.
    #[must_use]
    pub fn title(self) -> String {
        match self {
            Self::Pool => tr!("panel-pool"),
            Self::Effects => tr!("panel-effects"),
            Self::Inspector => tr!("panel-inspector"),
            Self::Timeline => tr!("panel-timeline"),
            Self::Meters => tr!("panel-meters"),
            Self::Controls => tr!("panel-controls"),
            Self::Graph => tr!("panel-graph"),
            Self::Scopes => tr!("panel-scopes"),
            Self::Mixer => tr!("panel-mixer"),
        }
    }

    /// Its icon.
    #[must_use]
    pub fn icon(self) -> Icon {
        match self {
            Self::Pool => Icon::Files,
            Self::Effects => Icon::Effects,
            Self::Inspector => Icon::Details,
            Self::Timeline => Icon::PaletteTimeline,
            Self::Meters => Icon::Audio,
            Self::Controls => Icon::Effects,
            Self::Graph => Icon::Graph,
            Self::Scopes => Icon::Scope,
            Self::Mixer => Icon::Mixer,
        }
    }
}

/// Square corners, for the panels that tile the page.
pub(crate) const SQUARE: egui::CornerRadius = egui::CornerRadius::ZERO;

/// The meters' width: room for two bars and their scale.
const METERS_WIDTH: f32 = 64.0;

impl DockPanel for Panel {
    fn all() -> &'static [Self] {
        &Self::ALL
    }

    fn key(self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::Effects => "effects",
            Self::Inspector => "inspector",
            Self::Timeline => "timeline",
            Self::Meters => "meters",
            Self::Controls => "controls",
            Self::Graph => "graph",
            Self::Scopes => "scopes",
            Self::Mixer => "mixer",
        }
    }

    fn fixed_width(self) -> Option<f32> {
        (self == Self::Meters).then_some(METERS_WIDTH)
    }

    fn headed(self) -> bool {
        true
    }

    fn default_layout() -> DockLayout<Self> {
        default_dock()
    }
}

/// The side columns' width at first.
const COLUMN_WIDTH: f32 = 300.0;
/// The timeline's height at first.
const TIMELINE_HEIGHT: f32 = 380.0;

/// A group of `tabs`, the first showing.
fn tabs(tabs: &[Panel], weight: f32) -> Group<Panel> {
    Group {
        tabs: tabs.to_vec(),
        active: 0,
        weight,
    }
}

/// The Edit page as it opens (the Editing workspace): the pool (with the
/// effects behind it) at the left, the inspector (Effect Controls behind
/// it) at the right, the timeline (the graph editor behind it) and the
/// meters along the bottom, the monitors in the middle.
#[must_use]
pub fn default_dock() -> DockLayout<Panel> {
    let mut dock = DockLayout::empty();
    dock.left = vec![Column {
        groups: vec![tabs(&[Panel::Pool, Panel::Effects], 1.0)],
        width: COLUMN_WIDTH,
    }];
    dock.right = vec![Column {
        groups: vec![tabs(
            &[
                Panel::Inspector,
                Panel::Controls,
                Panel::Scopes,
                Panel::Mixer,
            ],
            1.0,
        )],
        width: COLUMN_WIDTH,
    }];
    dock.bottom = vec![
        tabs(&[Panel::Timeline, Panel::Graph], 1.0),
        Group::one(Panel::Meters, 0.04),
    ];
    dock.bottom_height = TIMELINE_HEIGHT;
    dock
}

/// The Edit page's palettes arranged for a kind of work, as Premiere Pro's
/// workspaces are: Editing, Effects (the browser and Effect Controls
/// forward, the graph editor beside the timeline), Color (the scopes
/// beside the inspector) and Audio (the mixer at the right).
#[must_use]
pub fn workspace(name: &str) -> DockLayout<Panel> {
    let mut dock = default_dock();
    match name {
        "view.workspace_effects" => {
            dock.left = vec![Column {
                groups: vec![tabs(&[Panel::Effects, Panel::Pool], 1.0)],
                width: COLUMN_WIDTH,
            }];
            dock.right = vec![Column {
                groups: vec![tabs(&[Panel::Controls, Panel::Inspector], 1.0)],
                width: COLUMN_WIDTH,
            }];
            dock.bottom = vec![
                Group::one(Panel::Timeline, 0.6),
                Group::one(Panel::Graph, 0.4),
                Group::one(Panel::Meters, 0.04),
            ];
        }
        "view.workspace_color" => {
            dock.right = vec![Column {
                groups: vec![
                    Group::one(Panel::Scopes, 0.55),
                    tabs(&[Panel::Inspector, Panel::Controls], 0.45),
                ],
                width: COLUMN_WIDTH * 1.2,
            }];
        }
        "view.workspace_audio" => {
            dock.right = vec![Column {
                groups: vec![tabs(&[Panel::Mixer, Panel::Inspector], 1.0)],
                width: COLUMN_WIDTH * 1.3,
            }];
            dock.bottom = vec![
                Group::one(Panel::Timeline, 1.0),
                Group::one(Panel::Meters, 0.06),
            ];
        }
        _ => {}
    }
    dock
}

/// The height of the tool row and the page bar.
fn bar_height() -> f32 {
    Density::Compact.icon_button()
}

/// Text of `role` in `theme`.
pub(crate) fn font(theme: &Theme, role: TextRole) -> egui::FontId {
    theme.text_font(role)
}

/// `time` as timecode at `rate`: `01:02:03:04`.
#[must_use]
pub fn timecode(time: Time, rate: sue_core::Rate) -> String {
    let frame = time.frame(rate).max(0);
    sue_core::Timecode::from_frame(frame, rate, rate.is_ntsc()).to_string()
}

impl SueVe {
    /// Builds the menus again when the language changed, and keeps their
    /// keys in step with the settings.
    pub(crate) fn follow_language(&mut self, cx: &AppletContext<'_>) {
        if self.language.changed() {
            self.menus = crate::menus::menus();
            self.bound.invalidate();
        }
        self.bound.refresh(
            &mut self.menus,
            cx.rice(),
            crate::studio::KEYS_SECTION,
            crate::commands::shortcut_table(),
        );
    }

    /// The menu bar, in Sue's top bar.
    pub(crate) fn menu_bar(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        if cx.view.form_factor != sue_platform::FormFactor::Desktop {
            // A phone's bar is short: the project's name, as CapCut's.
            let name = self
                .project
                .and_then(|id| cx.arena().get_by_id(id).ok())
                .map_or_else(|| tr!("applet-name"), |entity| entity.name().to_owned());
            ui.painter().text(
                pos2(rect.left() + theme.space(Space::S4), rect.center().y),
                egui::Align2::LEFT_CENTER,
                name,
                font(&theme, TextRole::Body),
                theme.text,
            );
            return;
        }
        let picked = {
            let mut menubar = std::mem::take(&mut self.menubar);
            let facts = Facts::of(cx);
            let state = |command: &str| crate::commands::registry().state(self, &facts, command);
            let picked = menubar.ui(ui, rect, &self.menus, &state, &theme);
            self.menubar = menubar;
            picked
        };
        if let Some(command) = picked {
            self.run(&command, cx);
        }
    }

    /// The studio's page.
    pub(crate) fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        let present = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        self.ready = present && cx.view.reveal >= 0.999;
        // What was open may have been undone away.
        if self
            .project
            .is_some_and(|id| cx.arena().get_by_id(id).is_err())
        {
            self.project = None;
            self.sequence = None;
        }
        if self
            .sequence
            .is_some_and(|id| crate::model::sequence(cx.arena(), id).is_none())
        {
            self.sequence = self.project.and_then(|project| {
                crate::model::sequences(cx.arena(), project)
                    .first()
                    .copied()
            });
        }
        if self
            .source
            .item
            .is_some_and(|id| cx.arena().get_by_id(id).is_err())
        {
            self.source = crate::studio::SourceView::default();
        }
        self.follow_pending(cx);
        // What finished in the background, whenever the ticks run.
        if self.hub.poll() {
            ui.ctx().request_repaint();
        }
        self.poll_jobs(cx);
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, SQUARE, theme.void);
        if self.ready {
            self.keys(ui.ctx(), cx);
        }
        let bar = bar_height() + theme.space(Space::S2) * 2.0;
        let tools = Rect::from_min_size(full.min, vec2(full.width(), bar));
        let pages = Rect::from_min_max(pos2(full.left(), full.bottom() - bar), full.max);
        let body = Rect::from_min_max(
            pos2(full.left(), tools.bottom()),
            pos2(full.right(), pages.top()),
        );
        // Phones and tablets: CapCut's single column.
        if cx.view.form_factor != sue_platform::FormFactor::Desktop {
            mobile::mobile_ui(self, ui, full, cx, &theme);
            self.dialogs(ui.ctx(), cx, &theme);
            return;
        }
        self.ease_zoom(ui);
        self.tool_row(ui, tools, cx, &theme);
        self.page_bar(ui, pages, &theme);
        if self.project.is_none() {
            pages::start_ui(self, ui, body, cx, &theme);
        } else {
            match self.page {
                Page::Edit => self.edit_page(ui, body, cx, &theme),
                Page::Media => pages::media_page(self, ui, body, cx, &theme),
                Page::Deliver => pages::deliver_page(self, ui, body, cx, &theme),
                Page::Effects => effects_page::effects_page(self, ui, body, cx, &theme),
                Page::Color => color_page::color_page(self, ui, body, cx, &theme),
                Page::Audio => audio_page::audio_page(self, ui, body, cx, &theme),
            }
        }
        self.dialogs(ui.ctx(), cx, &theme);
    }

    /// Moves the timeline's zoom a step toward where it is going, keeping
    /// the time under the playhead where it is, so zooming glides.
    fn ease_zoom(&mut self, ui: &Ui) {
        let Some(target) = self.zoom_target else {
            return;
        };
        let at = self.clock.position.seconds() as f32;
        let place = (at - self.scroll) * self.zoom;
        let next = self.zoom + (target - self.zoom) * 0.35;
        self.zoom = if (next - target).abs() < target * 0.005 {
            self.zoom_target = None;
            target
        } else {
            next
        };
        self.scroll = (at - place / self.zoom).max(0.0);
        ui.ctx().request_repaint();
    }

    /// The keys: the studio's shortcuts, unless a text field has them.
    fn keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        if ctx.text_edit_focused() || self.dialog.is_some() {
            return;
        }
        let taken = ctx.input_mut(|input| {
            self.bound.take(
                input,
                cx.rice(),
                crate::studio::KEYS_SECTION,
                crate::commands::shortcut_table(),
            )
        });
        for name in taken {
            self.run(name, cx);
        }
    }

    /// The row over the page: the tools, snapping, linked selection, the
    /// sequence's name and the playhead's timecode.
    fn tool_row(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, SQUARE, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let size = bar_height();
        let gap = theme.space(Space::S2);
        let y = rect.center().y - size / 2.0;
        // Groups of buttons, a wider gap between groups.
        let mut groups: Vec<Vec<(Icon, &'static str, bool)>> = vec![
            Tool::ALL
                .iter()
                .map(|tool| (tool.icon(), tool.command(), self.tool == *tool))
                .collect(),
        ];
        groups.push(vec![
            (Icon::Magnet, "sequence.snap", self.snapping),
            (Icon::Link, "sequence.linked", self.linked),
            (Icon::Loop, "playback.loop", self.clock.looping),
        ]);
        groups.push(vec![
            (Icon::Insert, "clip.insert", false),
            (Icon::Overwrite, "clip.overwrite", false),
            (Icon::ToolRazor, "sequence.razor", false),
            (Icon::Marker, "marker.add", false),
        ]);
        let mut x = rect.left() + theme.space(Space::S4);
        let mut toggles = Vec::new();
        for group in groups {
            for (icon, command, on) in group {
                let place = Rect::from_min_size(pos2(x, y), vec2(size, size));
                x += size + gap;
                let (name, keys) = self.tip(command);
                if sue_wm::ui::icon_button(("sve-tool-row", command), icon, &name)
                    .selected(on)
                    .shortcut(&keys)
                    .show_at(ui, place, theme)
                    .clicked()
                {
                    toggles.push(command);
                }
            }
            x += gap * 2.0;
        }
        // The open sequence and where the playhead is, at the right.
        if let Some(sequence) = self.current(cx) {
            let name = self
                .sequence
                .and_then(|id| cx.arena().get_by_id(id).ok())
                .map(|entity| entity.name().to_owned())
                .unwrap_or_default();
            let (width, height) = crate::model::frame_size(sequence);
            let text = format!(
                "{name}   {width} x {height}   {:.3} fps   {}",
                sequence.rate.as_f64(),
                timecode(self.clock.position, sequence.rate)
            )
            .replace(".000 fps", " fps");
            ui.painter().text(
                pos2(rect.right() - theme.space(Space::S5), rect.center().y),
                egui::Align2::RIGHT_CENTER,
                text,
                font(theme, TextRole::Mono),
                theme.text,
            );
        }
        for command in toggles {
            self.run(command, cx);
        }
    }

    /// The command's name and keys, for tooltips.
    pub(crate) fn tip(&self, command: &str) -> (String, String) {
        let name = crate::commands::registry()
            .label(command)
            .unwrap_or_else(|| command.to_owned());
        let keys = self
            .bound
            .first_chord(crate::commands::shortcut_table(), command);
        (name, keys)
    }

    /// The pages along the bottom.
    fn page_bar(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        ui.painter().rect_filled(rect, SQUARE, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let count = Page::ALL.len() as f32;
        let width = (theme.space(Space::S9) * 3.0).min(rect.width() / count);
        let left = rect.center().x - width * count / 2.0;
        for (index, page) in Page::ALL.into_iter().enumerate() {
            let place = Rect::from_min_size(
                pos2(left + width * index as f32, rect.top()),
                vec2(width, rect.height()),
            );
            let label = page.label();
            let response =
                sue_wm::ui::toggle_button(("sve-page", page.id()), &label, self.page == page)
                    .icon(page.icon())
                    .show_at(ui, place.shrink(theme.space(Space::S1)), theme);
            if response.clicked() {
                self.page = page;
            }
        }
    }

    /// The Edit page: the dock around the monitors.
    fn edit_page(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let shown = |_: Panel| true;
        let mut laid = self.dock.layout(body, &shown, 0.0);
        for (_, _, rect) in &laid.columns {
            ui.painter().rect_filled(*rect, SQUARE, theme.panel);
        }
        let mut places = Vec::new();
        for group in &laid.groups {
            ui.painter().rect_filled(group.rect, SQUARE, theme.panel);
            let face = |panel: Panel| TabFace {
                name: panel.title(),
                icon: Some(panel.icon()),
                keys: String::new(),
            };
            let tabs = self.dock.tabs_ui(ui, group, &face, theme);
            places.push(tabs);
            let inner = group.body;
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(inner)
                    .id_salt(("sve-panel", group.active.key())),
            );
            child.set_clip_rect(inner);
            self.panel_ui(group.active, &mut child, cx, theme);
            ui.painter().rect_stroke(
                group.rect,
                SQUARE,
                theme.hairline(),
                egui::StrokeKind::Inside,
            );
        }
        for (group, tabs) in laid.groups.iter_mut().zip(places) {
            if !tabs.is_empty() {
                group.tab_rects = tabs;
            }
        }
        if self.ready {
            self.dock.splitters(ui, &laid, theme);
            self.dock
                .drop_ui(ui, &laid, &|panel: Panel| panel.title(), theme);
        }
        monitor::monitors(self, ui, laid.center, cx, theme);
        // A palette dropped off the dock floats in a window of its own
        // over the page, as Sue Paint's do.
        self.floating_ui(ui, body, cx, theme);
    }

    /// The palettes floating over the Edit page: each in a window that
    /// moves and resizes, with its name and Dock Again in a header. The
    /// header drags the window, and over the dock docks it again.
    fn floating_ui(&mut self, ui: &mut Ui, body: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let floating: Vec<(Panel, Rect)> = self.dock.floating.clone();
        let header = Density::Compact.icon_button() + theme.space(Space::S2);
        for (panel, rect) in floating {
            let start_id = egui::Id::unique(("sve-float-from", panel.key()));
            let moved = match self.dock.panel_drag() {
                Some((dragged, by)) if dragged == panel => {
                    let from = ui
                        .data_mut(|data| *data.get_temp_mut_or_insert_with(start_id, || rect.min));
                    Some(from + by)
                }
                _ => {
                    ui.data_mut(|data| data.remove::<egui::Pos2>(start_id));
                    None
                }
            };
            let mut window = egui::Window::new(panel.title())
                .id(egui::Id::unique(("sve-float", panel.key())))
                .default_rect(rect)
                .constrain_to(body)
                .title_bar(false)
                .collapsible(false)
                .resizable(true)
                .min_size(vec2(header * 4.0, header * 3.0));
            if let Some(at) = moved {
                window = window.current_pos(at);
            }
            let mut redock = false;
            let shown = window.show(ui.ctx(), |ui| {
                let area = ui.available_rect_before_wrap();
                ui.allocate_rect(area, egui::Sense::hover());
                let top = Rect::from_min_size(area.min, vec2(area.width(), header));
                let button = Density::Compact.icon_button();
                let dock_at = Rect::from_min_size(
                    pos2(top.right() - button, top.center().y - button / 2.0),
                    vec2(button, button),
                );
                if self.ready {
                    let grab = Rect::from_min_max(top.min, pos2(dock_at.left(), top.bottom()));
                    self.dock.grip(ui, grab, panel, "sve-floating");
                }
                ui.painter().rect_filled(top, SQUARE, theme.pill);
                ui.painter().text(
                    pos2(top.left() + theme.space(Space::S4), top.center().y),
                    egui::Align2::LEFT_CENTER,
                    panel.title(),
                    font(theme, TextRole::Caption),
                    theme.text,
                );
                if sue_wm::ui::icon_button(
                    ("sve-float-dock", panel.key()),
                    Icon::ArrowUp,
                    &tr!("dock-again"),
                )
                .quiet()
                .show_at(ui, dock_at, theme)
                .clicked()
                {
                    redock = true;
                }
                let inner = Rect::from_min_max(pos2(area.left(), top.bottom()), area.max);
                let mut child = ui.new_child(
                    UiBuilder::new()
                        .max_rect(inner)
                        .id_salt(("sve-float-panel", panel.key())),
                );
                child.set_clip_rect(inner.intersect(ui.clip_rect()));
                self.panel_ui(panel, &mut child, cx, theme);
            });
            if let Some(inner) = shown
                && let Some(entry) = self
                    .dock
                    .floating
                    .iter_mut()
                    .find(|(found, _)| *found == panel)
            {
                entry.1 = inner.response.rect;
            }
            if redock {
                self.redock(panel, rect, body);
            }
        }
    }

    /// Docks a floating `panel` again where it belongs: the timeline and
    /// the meters along the bottom, the inspector at the right, the rest
    /// on the side its window is nearer.
    pub(crate) fn redock(&mut self, panel: Panel, rect: Rect, body: Rect) {
        use sue_wm::dock::{Side, Target};
        self.dock.remove(panel);
        match panel {
            Panel::Timeline | Panel::Meters => self.dock.put(panel, Target::Bottom),
            Panel::Inspector => self.dock.dock(panel, Side::Right),
            _ if rect.center().x > body.center().x => self.dock.dock(panel, Side::Right),
            _ => self.dock.dock(panel, Side::Left),
        }
    }

    /// Draws palette `panel` in `ui`.
    fn panel_ui(&mut self, panel: Panel, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        match panel {
            Panel::Pool => pool::pool_ui(self, ui, cx, theme),
            Panel::Effects => browser::browser_ui(self, ui, cx, theme),
            Panel::Inspector => inspector::inspector_ui(self, ui, cx, theme),
            Panel::Timeline => timeline::timeline_ui(self, ui, cx, theme),
            Panel::Meters => panels::meters_ui(self, ui, theme),
            Panel::Controls => controls::controls_ui(self, ui, cx, theme),
            Panel::Graph => graph::graph_ui(self, ui, cx, theme),
            Panel::Scopes => panels::scopes_ui(self, ui, cx, theme),
            Panel::Mixer => panels::mixer_ui(self, ui, cx, theme),
        }
    }

    /// The dialogs.
    fn dialogs(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>, theme: &Theme) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        match dialog {
            Dialog::NewSequence {
                mut name,
                mut width,
                mut height,
                mut rate,
            } => {
                let title = tr!("new-sequence-title");
                let shown = sue_wm::ui::Dialog::new("sve-new-sequence", &title, theme)
                    .ok_cancel()
                    .show(ctx, |ui| {
                        sue_wm::ui::form_grid(ui, "sve-new-sequence-form", theme, |form| {
                            form.row(&tr!("field-name"), |ui| {
                                sue_wm::ui::text_field("sve-sequence-name", &mut name)
                                    .show(ui, theme);
                            });
                            form.row(&tr!("field-width"), |ui| {
                                sue_wm::ui::number_field("sve-sequence-width", &mut width)
                                    .range(16.0..=8192.0)
                                    .decimals(0)
                                    .suffix(" px")
                                    .show(ui, theme);
                            });
                            form.row(&tr!("field-height"), |ui| {
                                sue_wm::ui::number_field("sve-sequence-height", &mut height)
                                    .range(16.0..=8192.0)
                                    .decimals(0)
                                    .suffix(" px")
                                    .show(ui, theme);
                            });
                            form.row(&tr!("field-rate"), |ui| {
                                let names: Vec<String> = RATES
                                    .iter()
                                    .map(|rate| format!("{:.3}", rate.as_f64()).replace(".000", ""))
                                    .collect();
                                sue_wm::ui::dropdown("sve-sequence-rate", &mut rate, &names)
                                    .show(ui, theme);
                            });
                        });
                    });
                match shown.answer {
                    Some(sue_wm::ui::Answer::Accept) => {
                        self.dialog = None;
                        let rate = RATES.get(rate).copied().unwrap_or(sue_core::Rate::FPS_30);
                        let sequence =
                            crate::model::empty_sequence(width as u32, height as u32, rate, 3, 3);
                        let entity = sue_core::Entity::empty(name, sue_model::class::SEQUENCE)
                            .with_ext(sequence);
                        let id = entity.id();
                        if let Some(project) = self.project {
                            let spawn = crate::model::spawn_under(cx.arena(), project, entity);
                            if self.apply(cx, spawn) {
                                self.sequence = Some(id);
                                self.selection.clear();
                                self.clock = crate::playback::Clock::default();
                            }
                        }
                    }
                    Some(_) => self.dialog = None,
                    None => {
                        self.dialog = Some(Dialog::NewSequence {
                            name,
                            width,
                            height,
                            rate,
                        });
                    }
                }
            }
            Dialog::Shortcuts => {
                let title = tr!("cmd-keyboard-shortcuts");
                let rows = crate::commands::shortcut_rows(cx.rice());
                let shown = sue_wm::ui::Dialog::new("sve-shortcuts", &title, theme)
                    .ok()
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            sue_wm::shortcuts::grid_ui(ui, "sve-shortcut-grid", &rows, theme);
                        });
                    });
                if shown.answer.is_some() {
                    self.dialog = None;
                }
            }
            Dialog::SavePreset { mut name } => {
                let title = tr!("cmd-save-effect-preset");
                let shown = sue_wm::ui::Dialog::new("sve-save-preset", &title, theme)
                    .ok_cancel()
                    .show(ctx, |ui| {
                        sue_wm::ui::form_grid(ui, "sve-save-preset-form", theme, |form| {
                            form.row(&tr!("field-name"), |ui| {
                                sue_wm::ui::text_field("sve-preset-name", &mut name)
                                    .show(ui, theme);
                            });
                        });
                    });
                match shown.answer {
                    Some(sue_wm::ui::Answer::Accept) => {
                        self.dialog = None;
                        if let Some((_, clip)) = self.selected_clip(cx) {
                            let preset = crate::fx::preset::capture(&clip, name.trim());
                            match crate::fx::preset::save(&preset) {
                                Ok(path) => cx.saved(&path, tr!("preset-saved")),
                                Err(why) => cx.fail(why),
                            }
                        }
                    }
                    Some(_) => self.dialog = None,
                    None => self.dialog = Some(Dialog::SavePreset { name }),
                }
            }
            Dialog::About => {
                let title = tr!("cmd-about");
                let shown = sue_wm::ui::Dialog::new("sve-about", &title, theme)
                    .ok()
                    .show(ctx, |ui| {
                        ui.label(tr!("about-text"));
                    });
                if shown.answer.is_some() {
                    self.dialog = None;
                }
            }
        }
    }
}
