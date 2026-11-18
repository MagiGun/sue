//! Drawing Sue VE: the menu bar, the tool row, the pages along the bottom
//! (as DaVinci Resolve has them), and on the Edit page Sue's dock with the
//! media pool, the effects browser, the inspector, the timeline and the
//! audio meters around the source and program monitors.

use egui::{Rect, Ui, UiBuilder, pos2, vec2};
use sue_core::Time;
use sue_wm::dock::{Column, Group, TabFace};
use sue_wm::dock::{DockLayout, DockPanel};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, AppletId, Facts, Theme};

use crate::studio::{Dialog, ID, Page, RATES, SueVe, Tool};

pub mod inspector;
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
    /// Transitions and, from stage 2, effects.
    Effects,
    /// The selected clip's settings.
    Inspector,
    /// The timeline.
    Timeline,
    /// The audio meters.
    Meters,
}

impl Panel {
    /// Every palette.
    pub const ALL: [Self; 5] = [
        Self::Pool,
        Self::Effects,
        Self::Inspector,
        Self::Timeline,
        Self::Meters,
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

/// The Edit page as it opens: the pool (with the effects behind it) at
/// the left, the inspector at the right, the timeline and the meters
/// along the bottom, the monitors in the middle.
#[must_use]
pub fn default_dock() -> DockLayout<Panel> {
    let mut dock = DockLayout::empty();
    dock.left = vec![Column {
        groups: vec![Group {
            tabs: vec![Panel::Pool, Panel::Effects],
            active: 0,
            weight: 1.0,
        }],
        width: COLUMN_WIDTH,
    }];
    dock.right = vec![Column::of(&[(Panel::Inspector, 1.0)], COLUMN_WIDTH)];
    dock.bottom = vec![
        Group::one(Panel::Timeline, 1.0),
        Group::one(Panel::Meters, 0.04),
    ];
    dock.bottom_height = TIMELINE_HEIGHT;
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
        self.tool_row(ui, tools, cx, &theme);
        self.page_bar(ui, pages, &theme);
        if self.project.is_none() {
            pages::start_ui(self, ui, body, cx, &theme);
        } else {
            match self.page {
                Page::Edit => self.edit_page(ui, body, cx, &theme),
                Page::Media => pages::media_page(self, ui, body, cx, &theme),
                Page::Deliver => pages::deliver_page(self, ui, body, cx, &theme),
                Page::Effects | Page::Color | Page::Audio => {
                    pages::later_page(self, ui, body, &theme)
                }
            }
        }
        self.dialogs(ui.ctx(), cx, &theme);
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
            Panel::Effects => panels::effects_ui(self, ui, cx, theme),
            Panel::Inspector => inspector::inspector_ui(self, ui, cx, theme),
            Panel::Timeline => timeline::timeline_ui(self, ui, cx, theme),
            Panel::Meters => panels::meters_ui(self, ui, theme),
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
