//! The workstation shell: a flush header (menus, tabs, dock toggles), the
//! activity rails, the docks, Center Stage with Home, and the status bar,
//! with menus, tooltips, dialogs and the drop overlay on top.

use sue_platform::{CursorShape, FormFactor, Point, SafeAreaInsets};
use sue_rice::{HardwareProfile, RiceValue, SetRiceValue};

use crate::canvas::Layer;
use crate::{
    AppletContext, AppletId, AppletManifest, BoxStyle, Focus, FontFace, MenuEntry, Placement, Rect,
    ShellRequest, Tab, Ui, WmError, Workspace, Zone, widget_id,
};

/// A question the shell asks over everything else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    /// Title line.
    pub title: String,
    /// Explanation.
    pub body: String,
    /// The confirming button.
    pub confirm: String,
    /// The cancelling button.
    pub cancel: String,
}

/// Per-frame facts the shell shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellFrame<'a> {
    /// The window, in logical pixels.
    pub viewport: Rect,
    /// Notches and system bars.
    pub safe_area: SafeAreaInsets,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// The animation frame rate, or `None` while the scene sleeps at 0 FPS.
    pub fps: Option<u32>,
    /// A status message, and whether it reports an error.
    pub message: Option<(&'a str, bool)>,
    /// A modal question, if one is open.
    pub dialog: Option<&'a Dialog>,
}

/// What the user asked the shell to do this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellAction {
    /// Show or hide a dock.
    ToggleDock(Zone),
    /// A rail click on dock applet `index`.
    SelectDock(usize),
    /// Open a new tab.
    NewTab,
    /// Switch tabs.
    ActivateTab(usize),
    /// Close a tab.
    CloseTab(usize),
    /// Open an applet.
    OpenApplet(AppletId),
    /// Pin or unpin an applet as the boot target (an undoable rice edit).
    TogglePin(AppletId),
    /// Move keyboard focus.
    Focus(Focus),
    /// Open Settings, optionally at a page.
    OpenSettings(Option<String>),
    /// Undo the last change.
    Undo,
    /// Redo.
    Redo,
    /// Hand a request to the engine.
    Post(ShellRequest),
    /// The open dialog was answered.
    DialogAnswer(bool),
}

/// The icon drawn for the Settings gear.
const GEAR: &str = "\u{2699}";
/// The tab close mark.
const CLOSE: &str = "\u{00D7}";

/// `high-refresh` becomes `High Refresh`.
#[must_use]
pub fn profile_label(profile: HardwareProfile) -> String {
    profile
        .rice_name()
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Draws the whole shell and returns the actions the user triggered. Applets
/// draw themselves into their zones.
pub fn draw_shell(
    workspace: &mut Workspace,
    ui: &mut Ui<'_>,
    ctx: &mut AppletContext<'_>,
    frame: &ShellFrame<'_>,
) -> Vec<ShellAction> {
    let mut actions = Vec::new();
    let theme = ui.theme;
    let viewport = frame.viewport;
    ui.fill(viewport, theme.base);
    let layout = workspace.layout(viewport, frame.safe_area, frame.form_factor);
    let pressed_at = ui.input.pointer.filter(|_| ui.input.pressed);
    ctx.inspection = workspace.inspect_active(ctx.world);

    // Center Stage.
    ui.canvas.push_clip(layout.center);
    if !workspace.draw_active(ui, layout.center, ctx) {
        draw_home(workspace, ui, ctx, layout.center, &mut actions);
    }
    ui.canvas.pop_clip();
    if pressed_at.is_some_and(|point| layout.center.contains(point.x, point.y)) {
        actions.push(ShellAction::Focus(Focus::Center));
    }

    if let Placement::Tiled(timeline) | Placement::BottomSheet { rect: timeline, .. } =
        layout.bottom
    {
        ui.panel(timeline);
        ui.hline(timeline.x, timeline.y, timeline.width);
        let pad = theme.padding;
        ui.label_sized(
            Rect::new(
                timeline.x + pad,
                timeline.y,
                timeline.width,
                ui.row_height(),
            ),
            "TIMELINE",
            theme.small_size,
            theme.muted,
        );
    }

    let rails = layout.left_rail.is_some();
    for (zone, placement) in [
        (Zone::LeftDock, layout.left),
        (Zone::RightDock, layout.right),
    ] {
        match placement {
            Placement::Tiled(rect) => {
                draw_dock(
                    workspace,
                    ui,
                    ctx,
                    zone,
                    rect,
                    rails,
                    pressed_at,
                    &mut actions,
                );
            }
            Placement::Drawer {
                rect,
                bounds,
                reveal,
                ..
            } => {
                ui.canvas.push_clip(bounds);
                ui.fill(bounds, theme.base.fade(0.5 * reveal));
                draw_dock(
                    workspace,
                    ui,
                    ctx,
                    zone,
                    rect,
                    rails,
                    pressed_at,
                    &mut actions,
                );
                ui.canvas.pop_clip();
            }
            Placement::Hidden | Placement::BottomSheet { .. } => {}
        }
    }

    for (zone, rail) in [
        (Zone::LeftDock, layout.left_rail),
        (Zone::RightDock, layout.right_rail),
    ] {
        if let Some(rail) = rail {
            draw_rail(workspace, ui, zone, rail, &mut actions);
        }
    }

    draw_header(
        workspace,
        ui,
        ctx,
        layout.header,
        frame,
        &layout,
        &mut actions,
    );
    if let Some(status) = layout.status {
        draw_status(workspace, ui, ctx, status, frame);
    }

    if let Some(dialog) = frame.dialog {
        draw_dialog(ui, viewport, dialog, &mut actions);
    }
    if ui.input.file_hover {
        draw_drop_overlay(ui, viewport);
    }
    ui.draw_tooltip();
    actions
}

/// The dock title row, then the dock's active applet.
#[allow(
    clippy::too_many_arguments,
    reason = "one dock needs the shell, the zone and the frame's input"
)]
fn draw_dock(
    workspace: &mut Workspace,
    ui: &mut Ui<'_>,
    ctx: &mut AppletContext<'_>,
    zone: Zone,
    rect: Rect,
    rails: bool,
    pressed_at: Option<Point>,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    let pad = theme.padding;
    ui.canvas.push_clip(rect);
    ui.panel(rect);
    match zone {
        Zone::LeftDock => ui.vline(rect.right() - theme.hairline(), rect.y, rect.height),
        _ => ui.vline(rect.x, rect.y, rect.height),
    }
    let Some(index) = workspace.active_dock(zone) else {
        ui.canvas.pop_clip();
        return;
    };
    let row = ui.row_height();
    let mut top = rect.y;
    let applets = workspace.dock_applets(zone);
    if rails {
        let name = applets
            .iter()
            .find(|(candidate, _)| *candidate == index)
            .map_or_else(String::new, |(_, manifest)| manifest.name.to_uppercase());
        let color = if workspace.focus() == Focus::Dock(index) {
            theme.text
        } else {
            theme.muted
        };
        ui.label_sized(
            Rect::new(rect.x + pad, top, rect.width - pad * 2.0, row),
            &name,
            theme.small_size,
            color,
        );
    } else {
        // Phones have no rails: the drawer switches its applets itself.
        let mut x = rect.x + pad / 2.0;
        for (candidate, manifest) in &applets {
            let button = Rect::new(x, top, row, row);
            let id = widget_id("shell/drawer", manifest.id.as_str());
            if ui.icon_button(
                id,
                button,
                &manifest.icon,
                *candidate == index,
                &manifest.name,
            ) {
                actions.push(ShellAction::SelectDock(*candidate));
            }
            x += row;
        }
    }
    top += row;
    let body = Rect::new(
        rect.x + pad,
        top,
        (rect.width - pad * 2.0).max(0.0),
        (rect.bottom() - top - pad / 2.0).max(0.0),
    );
    ui.canvas.push_clip(body);
    workspace.draw_dock(index, ui, body, ctx);
    ui.canvas.pop_clip();
    if pressed_at.is_some_and(|point| rect.contains(point.x, point.y)) {
        actions.push(ShellAction::Focus(Focus::Dock(index)));
    }
    ui.canvas.pop_clip();
}

/// An activity rail: one icon per dock applet. The active one is lit and
/// marked with an accent bar on the outer edge.
fn draw_rail(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    zone: Zone,
    rail: Rect,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    ui.fill(rail, theme.chrome);
    match zone {
        Zone::LeftDock => ui.vline(rail.right() - theme.hairline(), rail.y, rail.height),
        _ => ui.vline(rail.x, rail.y, rail.height),
    }
    let (left_open, right_open) = workspace.docks_open();
    let open = if zone == Zone::LeftDock {
        left_open
    } else {
        right_open
    };
    let active = workspace.active_dock(zone);
    let size = rail.width;
    for (slot, (index, manifest)) in workspace.dock_applets(zone).into_iter().enumerate() {
        let button = Rect::new(rail.x, rail.y + slot as f32 * size, size, size);
        let lit = open && active == Some(index);
        if lit {
            let bar = theme.tab_accent.max(theme.hairline());
            let x = if zone == Zone::LeftDock {
                button.x
            } else {
                button.right() - bar
            };
            ui.fill(Rect::new(x, button.y, bar, button.height), theme.accent);
        }
        let id = widget_id("shell/rail", manifest.id.as_str());
        if ui.icon_button(id, button, &manifest.icon, lit, &manifest.name) {
            actions.push(ShellAction::SelectDock(index));
        }
    }
}

/// A dock toggle drawn as a window with its left or right pane filled.
fn dock_icon(ui: &mut Ui<'_>, id: u64, rect: Rect, left: bool, open: bool, tooltip: &str) -> bool {
    let response = ui.interact(id, rect);
    let theme = ui.theme;
    if response.hovered {
        ui.canvas.draw_box(
            rect.shrink(3.0),
            BoxStyle::filled(theme.hover, theme.radius),
        );
        ui.set_cursor(CursorShape::Pointer);
    }
    let side = (rect.height * 0.5).round();
    let icon = Rect::new(
        (rect.x + (rect.width - side * 1.2) / 2.0).round(),
        (rect.y + (rect.height - side) / 2.0).round(),
        (side * 1.2).round(),
        side,
    );
    let color = if open || response.hovered {
        theme.text
    } else {
        theme.muted
    };
    let line = theme.hairline().max(1.0);
    ui.canvas.draw_box(
        icon,
        BoxStyle {
            fill: crate::Color::TRANSPARENT,
            border: color,
            border_width: line,
            radius: theme.radius.min(2.0),
        },
    );
    let pane = (icon.width * 0.38).round();
    let pane_x = if left { icon.x } else { icon.right() - pane };
    let fill = if open { color } else { color.fade(0.35) };
    ui.canvas.draw_box(
        Rect::new(pane_x, icon.y, pane, icon.height),
        BoxStyle::filled(fill, theme.radius.min(2.0)),
    );
    ui.tooltip(response, rect, tooltip);
    response.clicked
}

/// One header menu: its title and its entries, each with what it does.
type Menu = (&'static str, Vec<(MenuEntry, Option<ShellAction>)>);

/// The menus of the header, with what each entry does.
fn menus(workspace: &Workspace, ctx: &AppletContext<'_>) -> Vec<Menu> {
    let keys = workspace.config().keymap;
    let key = |chord: sue_rice::Keybind| Some(chord.to_string());
    let item = |label: &str, chord: Option<String>, action: ShellAction| {
        (MenuEntry::item(label, chord), Some(action))
    };
    let separator = || (MenuEntry::Separator, None);
    let has_applet = workspace.active_tab().applet_id().is_some();
    let post = |command: &str| ShellAction::Post(ShellRequest::Command(command.to_owned()));

    let file = vec![
        item("Open File...", key(keys.open_file), post("file.open")),
        {
            let (entry, action) = item("Save", key(keys.save), post("file.save"));
            (entry.enabled(has_applet), action)
        },
        separator(),
        {
            let (entry, action) = item(
                "Close Tab",
                key(keys.close_tab),
                ShellAction::CloseTab(workspace.active_index()),
            );
            (entry.enabled(workspace.active_index() > 0), action)
        },
        separator(),
        item("Quit", key(keys.quit), post("app.quit")),
    ];
    let edit = vec![
        {
            let (entry, action) = item("Undo", key(keys.undo), ShellAction::Undo);
            (entry.enabled(ctx.history.can_undo()), action)
        },
        {
            let (entry, action) = item("Redo", key(keys.redo), ShellAction::Redo);
            (entry.enabled(ctx.history.can_redo()), action)
        },
    ];
    let (left_open, right_open) = workspace.docks_open();
    let dock_label =
        |name: &str, open: bool| format!("{} {name} Dock", if open { "Hide" } else { "Show" });
    let workspace_menu = vec![
        item("New Tab", key(keys.new_tab), ShellAction::NewTab),
        item("Home", key(keys.home), ShellAction::ActivateTab(0)),
        separator(),
        item(
            &dock_label("Left", left_open),
            key(keys.toggle_left_dock),
            ShellAction::ToggleDock(Zone::LeftDock),
        ),
        item(
            &dock_label("Right", right_open),
            key(keys.toggle_right_dock),
            ShellAction::ToggleDock(Zone::RightDock),
        ),
    ];
    let hub = workspace.launchpad().hub();
    let mut tools: Vec<(MenuEntry, Option<ShellAction>)> = hub
        .studios
        .iter()
        .chain(&hub.inspectors)
        .map(|manifest| {
            item(
                &manifest.name,
                None,
                ShellAction::OpenApplet(manifest.id.clone()),
            )
        })
        .collect();
    let docked: Vec<(MenuEntry, Option<ShellAction>)> = [Zone::LeftDock, Zone::RightDock]
        .into_iter()
        .flat_map(|zone| workspace.dock_applets(zone))
        .map(|(index, manifest)| item(&manifest.name, None, ShellAction::SelectDock(index)))
        .collect();
    if !tools.is_empty() && !docked.is_empty() {
        tools.push(separator());
    }
    tools.extend(docked);
    let mut settings = vec![
        item(
            "Open Settings",
            key(keys.open_settings),
            ShellAction::OpenSettings(None),
        ),
        separator(),
    ];
    settings.extend(ctx.catalog.pages.iter().map(|page| {
        item(
            &page.title,
            None,
            ShellAction::OpenSettings(Some(page.id.clone())),
        )
    }));
    settings.extend([
        separator(),
        item("Reload default.suerice", None, post("rice.reload")),
        item("Export Rice...", None, post("rice.export")),
        item("Open Config Folder", None, post("rice.reveal")),
    ]);
    vec![
        ("File", file),
        ("Edit", edit),
        ("Workspace", workspace_menu),
        ("Tools", tools),
        ("Settings", settings),
    ]
}

#[allow(
    clippy::too_many_arguments,
    reason = "the header needs the layout and the frame facts"
)]
fn draw_header(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    ctx: &AppletContext<'_>,
    header: Rect,
    frame: &ShellFrame<'_>,
    layout: &crate::LayoutSolution,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    let pad = theme.padding;
    ui.fill(header, theme.chrome);
    ui.hline(header.x, header.bottom() - theme.hairline(), header.width);
    let (left_open, right_open) = workspace.docks_open();
    let toggle_width = layout.left_rail.map_or(header.height, |rail| rail.width);

    let left_toggle = Rect::new(header.x, header.y, toggle_width, header.height);
    if dock_icon(
        ui,
        widget_id("shell", "toggle-left"),
        left_toggle,
        true,
        left_open,
        "Left dock",
    ) {
        actions.push(ShellAction::ToggleDock(Zone::LeftDock));
    }
    let right_toggle = Rect::new(
        header.right() - toggle_width,
        header.y,
        toggle_width,
        header.height,
    );
    if dock_icon(
        ui,
        widget_id("shell", "toggle-right"),
        right_toggle,
        false,
        right_open,
        "Right dock",
    ) {
        actions.push(ShellAction::ToggleDock(Zone::RightDock));
    }
    let gear = Rect::new(
        right_toggle.x - header.height,
        header.y,
        header.height,
        header.height,
    );
    if ui.icon_button(widget_id("shell", "gear"), gear, GEAR, false, "Settings") {
        actions.push(ShellAction::OpenSettings(None));
    }

    let mut x = left_toggle.right();
    if frame.form_factor != FormFactor::Mobile {
        for (name, entries) in menus(workspace, ctx) {
            let width = ui.fonts.measure(FontFace::Ui, theme.ui_size, name) + pad * 2.0;
            let button = Rect::new(x, header.y + 3.0, width, header.height - 6.0);
            let button_id = widget_id("shell/menu-button", name);
            let menu_id = widget_id("shell/menu", name);
            let open = ui.popup_open(menu_id);
            if ui.flat_button(button_id, button, name, open).clicked {
                if open {
                    ui.close_popup();
                } else {
                    ui.open_popup(menu_id, button_id);
                }
            }
            if ui.popup_open(menu_id) {
                let shown: Vec<MenuEntry> =
                    entries.iter().map(|(entry, _)| entry.clone()).collect();
                if let Some(picked) =
                    ui.menu(menu_id, Point::new(button.x, header.bottom()), &shown)
                    && let Some(Some(action)) =
                        entries.get(picked).map(|(_, action)| action.clone())
                {
                    actions.push(action);
                }
            }
            x = button.right();
        }
        x += pad;
        ui.vline(x, header.y + 6.0, header.height - 12.0);
    }

    // Tabs fill the rest of the header, flush, then "+".
    let tabs_start = x;
    let plus_width = header.height;
    let available = (gear.x - tabs_start - plus_width).max(0.0);
    let count = workspace.tabs().len();
    let titles: Vec<String> = (0..count)
        .map(|index| {
            let title = workspace.tab_title(index, ctx.world);
            let icon = workspace
                .tabs()
                .get(index)
                .and_then(Tab::applet_id)
                .and_then(|id| workspace.launchpad().manifest(id))
                .map(|manifest| manifest.icon.clone());
            icon.map_or(title.clone(), |icon| format!("{icon}  {title}"))
        })
        .collect();
    let close_width = header.height * 0.8;
    let natural: Vec<f32> = titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            ui.fonts.measure(FontFace::Ui, theme.ui_size, title)
                + pad * 2.0
                + if index == 0 { 0.0 } else { close_width }
        })
        .collect();
    let total: f32 = natural.iter().sum();
    let squeeze = if total > available && total > 0.0 {
        available / total
    } else {
        1.0
    };
    let mut tab_x = tabs_start;
    for (index, title) in titles.iter().enumerate() {
        let width = natural[index] * squeeze;
        let tab = Rect::new(tab_x, header.y, width, header.height - theme.hairline());
        tab_x = tab.right();
        let active = index == workspace.active_index();
        let response = ui.interact(widget_id("shell/tab", &index.to_string()), tab);
        if active {
            ui.fill(tab, theme.surface);
            ui.fill(
                Rect::new(tab.x, tab.y, tab.width, theme.tab_accent),
                theme.accent,
            );
        } else if response.hovered {
            ui.fill(tab, theme.hover);
        }
        ui.vline(tab.right() - theme.hairline(), tab.y, tab.height);
        let closable = index > 0;
        let label = Rect::new(
            tab.x + pad,
            tab.y,
            (tab.width - pad * 2.0 - if closable { close_width } else { 0.0 }).max(0.0),
            tab.height,
        );
        ui.canvas.push_clip(label);
        ui.text(
            Point::new(label.x, tab.y + (tab.height - ui.line_height()) / 2.0),
            title,
            FontFace::Ui,
            theme.ui_size,
            if active { theme.text } else { theme.muted },
        );
        ui.canvas.pop_clip();
        if response.hovered {
            ui.set_cursor(CursorShape::Pointer);
        }
        if response.clicked {
            actions.push(ShellAction::ActivateTab(index));
        }
        if closable {
            let close = Rect::new(
                tab.right() - close_width - pad / 2.0,
                tab.y + (tab.height - close_width) / 2.0,
                close_width,
                close_width,
            );
            let close_response =
                ui.interact(widget_id("shell/tab-close", &index.to_string()), close);
            if close_response.hovered {
                ui.canvas
                    .draw_box(close, BoxStyle::filled(theme.hover, theme.radius));
                ui.set_cursor(CursorShape::Pointer);
            }
            if active || response.hovered || close_response.hovered {
                ui.label_centered(close, CLOSE, theme.muted);
            }
            ui.tooltip(close_response, close, "Close tab");
            if close_response.clicked {
                actions.push(ShellAction::CloseTab(index));
            }
        }
    }
    let plus = Rect::new(
        tab_x,
        header.y,
        plus_width,
        header.height - theme.hairline(),
    );
    let response = ui.flat_button(widget_id("shell", "new-tab"), plus, "+", false);
    ui.tooltip(response, plus, "New tab");
    if response.clicked {
        actions.push(ShellAction::NewTab);
    }
}

fn draw_status(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    ctx: &AppletContext<'_>,
    status: Rect,
    frame: &ShellFrame<'_>,
) {
    let theme = ui.theme;
    let pad = theme.padding;
    let size = theme.small_size;
    ui.fill(status, theme.chrome);
    ui.hline(status.x, status.y, status.width);
    let (left_text, left_color) = match frame.message {
        Some((message, true)) => (message.to_owned(), theme.alert),
        Some((message, false)) => (message.to_owned(), theme.text),
        None => (
            workspace.tab_title(workspace.active_index(), ctx.world),
            theme.muted,
        ),
    };
    let entities = ctx.world.arena.len();
    let items = [
        format!(
            "{entities} {}",
            if entities == 1 { "entity" } else { "entities" }
        ),
        format!("Undo {}", ctx.history.depth()),
        profile_label(workspace.profile()),
        frame
            .fps
            .map_or_else(|| "Idle".to_owned(), |fps| format!("{fps} FPS")),
    ];
    let mut x = status.right() - pad;
    for (slot, item) in items.iter().enumerate().rev() {
        let width = ui.fonts.measure(FontFace::Ui, size, item);
        x -= width;
        let color = if slot == items.len() - 1 && frame.fps.is_some() {
            theme.accent
        } else {
            theme.muted
        };
        ui.label_sized(
            Rect::new(x, status.y, width + 1.0, status.height),
            item,
            size,
            color,
        );
        x -= pad;
        if slot > 0 {
            ui.vline(x, status.y + 5.0, status.height - 10.0);
            x -= pad;
        }
    }
    ui.label_sized(
        Rect::new(
            status.x + pad,
            status.y,
            (x - status.x - pad * 2.0).max(0.0),
            status.height,
        ),
        &left_text,
        size,
        left_color,
    );
}

/// Home: Center Stage applets grouped by category in a grid of wide cards
/// whose descriptions wrap instead of being cut.
fn draw_home(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    ctx: &AppletContext<'_>,
    area: Rect,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    let pad = theme.padding;
    let margin = pad * 3.0;
    let hub = workspace.launchpad().hub();
    let pinned = ctx
        .world
        .rice
        .get_str("launchpad.boot_into")
        .unwrap_or("hub")
        .to_owned();
    let inner = Rect::new(
        area.x + margin,
        area.y + margin,
        (area.width - margin * 2.0).max(1.0),
        area.height,
    );
    let columns = (((inner.width + pad) / (theme.card_width + pad)).floor() as usize).max(1);
    let card_width = ((inner.width - pad * (columns as f32 - 1.0)) / columns as f32).max(1.0);
    let mut y = inner.y;
    for (category, list) in [
        (crate::AppletCategory::Studio, &hub.studios),
        (crate::AppletCategory::Inspector, &hub.inspectors),
    ] {
        ui.label_sized(
            Rect::new(inner.x, y, inner.width, ui.row_height()),
            &category.plural().to_uppercase(),
            theme.small_size,
            theme.muted,
        );
        y += ui.row_height();
        if list.is_empty() {
            let note = format!(
                "No {} are installed. They arrive as packages.",
                category.plural().to_lowercase()
            );
            y += ui.wrapped(inner.x, y, inner.width, &note, theme.ui_size, theme.muted) + margin;
            continue;
        }
        for row in list.chunks(columns) {
            let height = row
                .iter()
                .map(|manifest| card_height(ui, manifest, card_width))
                .fold(0.0_f32, f32::max);
            for (column, manifest) in row.iter().enumerate() {
                let card = Rect::new(
                    inner.x + column as f32 * (card_width + pad),
                    y,
                    card_width,
                    height,
                );
                if let Some(action) = draw_card(ui, manifest, card, manifest.id.as_str() == pinned)
                {
                    actions.push(action);
                }
            }
            y += height + pad;
        }
        y += margin - pad;
    }
}

fn name_size(ui: &Ui<'_>) -> f32 {
    (ui.theme.ui_size + ui.theme.heading_size) / 2.0
}

fn card_height(ui: &Ui<'_>, manifest: &AppletManifest, width: f32) -> f32 {
    let pad = ui.theme.padding;
    let text_width = width - pad * 3.0 - name_size(ui) * 1.6;
    pad * 1.5
        + ui.fonts.line_height(FontFace::Ui, name_size(ui))
        + pad / 2.0
        + ui.wrapped_height(&manifest.summary, text_width, ui.theme.ui_size)
        + pad
        + ui.row_height()
        + pad
}

fn draw_card(
    ui: &mut Ui<'_>,
    manifest: &AppletManifest,
    card: Rect,
    pinned: bool,
) -> Option<ShellAction> {
    let theme = ui.theme;
    let pad = theme.padding;
    let response = ui.interact(widget_id("launchpad/card", manifest.id.as_str()), card);
    ui.canvas.draw_box(
        card,
        BoxStyle {
            fill: if response.hovered {
                theme.hover
            } else {
                theme.surface
            },
            border: if response.hovered || pinned {
                theme.accent
            } else {
                theme.border
            },
            border_width: theme.hairline(),
            radius: theme.radius,
        },
    );
    if response.hovered {
        ui.set_cursor(CursorShape::Pointer);
    }
    let name_size = name_size(ui);
    let icon_size = name_size * 1.6;
    ui.label_centered_sized(
        Rect::new(card.x + pad, card.y + pad * 1.5, icon_size, icon_size),
        &manifest.icon,
        name_size * 1.2,
        theme.accent,
    );
    let text_x = card.x + pad * 2.0 + icon_size;
    let text_width = card.right() - pad - text_x;
    ui.text(
        Point::new(text_x, card.y + pad * 1.5),
        &manifest.name,
        FontFace::Ui,
        name_size,
        theme.text,
    );
    let summary_y = card.y + pad * 2.0 + ui.fonts.line_height(FontFace::Ui, name_size);
    ui.wrapped(
        text_x,
        summary_y,
        text_width,
        &manifest.summary,
        theme.ui_size,
        theme.muted,
    );
    let row = ui.row_height();
    let toggle_row = Rect::new(text_x, card.bottom() - pad - row, text_width, row);
    let pin_id = widget_id("launchpad/pin", manifest.id.as_str());
    let switch = Rect::new(toggle_row.x, toggle_row.y, row * 1.8, row);
    let clicked = ui.toggle(pin_id, switch, pinned);
    ui.label_sized(
        Rect::new(
            switch.right() + pad / 2.0,
            toggle_row.y,
            toggle_row.width - switch.width - pad / 2.0,
            row,
        ),
        if pinned {
            "Opens at launch"
        } else {
            "Pin as boot"
        },
        theme.small_size,
        if pinned { theme.accent } else { theme.muted },
    );
    if clicked {
        return Some(ShellAction::TogglePin(manifest.id.clone()));
    }
    response
        .clicked
        .then(|| ShellAction::OpenApplet(manifest.id.clone()))
}

fn draw_dialog(ui: &mut Ui<'_>, viewport: Rect, dialog: &Dialog, actions: &mut Vec<ShellAction>) {
    let theme = ui.theme;
    let pad = theme.padding;
    ui.canvas.set_layer(Layer::Overlay);
    ui.fill(viewport, theme.base.fade(0.6));
    // The backdrop takes every click outside the dialog.
    let _ = ui.interact(widget_id("shell/dialog", "backdrop"), viewport);
    let width = (theme.card_width * 1.4).min(viewport.width - pad * 4.0);
    let body_height = ui.wrapped_height(&dialog.body, width - pad * 4.0, theme.ui_size);
    let height = pad * 6.0 + ui.line_height() + body_height + ui.row_height();
    let panel = Rect::new(
        viewport.x + (viewport.width - width) / 2.0,
        viewport.y + (viewport.height - height) / 2.0,
        width,
        height,
    );
    ui.canvas.draw_box(
        panel,
        BoxStyle {
            fill: theme.panel,
            border: theme.border,
            border_width: theme.hairline(),
            radius: theme.radius,
        },
    );
    let x = panel.x + pad * 2.0;
    let mut y = panel.y + pad * 2.0;
    ui.text(
        Point::new(x, y),
        &dialog.title,
        FontFace::Ui,
        theme.ui_size,
        theme.text,
    );
    y += ui.line_height() + pad;
    ui.wrapped(
        x,
        y,
        width - pad * 4.0,
        &dialog.body,
        theme.ui_size,
        theme.muted,
    );
    let row = ui.row_height();
    let button_width =
        |ui: &Ui<'_>, text: &str| ui.fonts.measure(FontFace::Ui, theme.ui_size, text) + pad * 4.0;
    let confirm_width = button_width(ui, &dialog.confirm);
    let cancel_width = button_width(ui, &dialog.cancel);
    let confirm = Rect::new(
        panel.right() - pad * 2.0 - confirm_width,
        panel.bottom() - pad * 2.0 - row,
        confirm_width,
        row,
    );
    let cancel = Rect::new(confirm.x - pad - cancel_width, confirm.y, cancel_width, row);
    if ui.button(widget_id("shell/dialog", "cancel"), cancel, &dialog.cancel) {
        actions.push(ShellAction::DialogAnswer(false));
    }
    if ui.primary_button(
        widget_id("shell/dialog", "confirm"),
        confirm,
        &dialog.confirm,
        true,
    ) {
        actions.push(ShellAction::DialogAnswer(true));
    }
    ui.canvas.set_layer(Layer::Base);
}

fn draw_drop_overlay(ui: &mut Ui<'_>, viewport: Rect) {
    let theme = ui.theme;
    let pad = theme.padding;
    ui.canvas.set_layer(Layer::Overlay);
    ui.fill(viewport, theme.base.fade(0.75));
    let width = (theme.card_width * 1.6).min(viewport.width - pad * 4.0);
    let line = ui.row_height();
    let target = Rect::new(
        viewport.x + (viewport.width - width) / 2.0,
        viewport.y + (viewport.height - line * 3.0) / 2.0,
        width,
        line * 3.0,
    );
    ui.canvas.draw_box(
        target,
        BoxStyle {
            fill: theme.panel,
            border: theme.accent,
            border_width: theme.hairline() * 2.0,
            radius: theme.radius,
        },
    );
    ui.label_centered(
        Rect::new(target.x, target.y + pad, target.width, line),
        "Drop to open",
        theme.text,
    );
    ui.label_centered_sized(
        Rect::new(target.x, target.y + pad + line, target.width, line),
        "Sue reads the file's content and opens it in the applet that fits",
        theme.small_size,
        theme.muted,
    );
    ui.canvas.set_layer(Layer::Base);
}

/// Carries out `actions`. Pin toggles become undoable rice edits of
/// `launchpad.boot_into`, so they persist and `Ctrl+Z` reverts them.
///
/// # Errors
/// If opening an applet or recording an edit fails.
pub fn apply_actions(
    workspace: &mut Workspace,
    ctx: &mut AppletContext<'_>,
    actions: Vec<ShellAction>,
) -> Result<(), WmError> {
    for action in actions {
        match action {
            ShellAction::ToggleDock(Zone::LeftDock) => {
                let (left, _) = workspace.docks_open();
                workspace.set_left_dock(!left);
            }
            ShellAction::ToggleDock(_) => {
                let (_, right) = workspace.docks_open();
                workspace.set_right_dock(!right);
            }
            ShellAction::SelectDock(index) => workspace.select_dock(index),
            ShellAction::NewTab => {
                workspace.new_tab();
            }
            ShellAction::ActivateTab(index) => workspace.activate_tab(index),
            ShellAction::CloseTab(index) => workspace.close_tab(index, ctx),
            ShellAction::OpenApplet(id) => workspace.open_applet(&id, ctx)?,
            ShellAction::OpenSettings(page) => workspace.open_settings(page.as_deref(), ctx)?,
            ShellAction::TogglePin(id) => {
                let current = ctx
                    .world
                    .rice
                    .get_str("launchpad.boot_into")
                    .unwrap_or("hub");
                let next = if current == id.as_str() {
                    "hub".to_owned()
                } else {
                    id.to_string()
                };
                ctx.edit_rice(SetRiceValue::new(
                    "launchpad.boot_into",
                    RiceValue::Text(next),
                ))?;
                workspace.invalidate();
            }
            ShellAction::Focus(focus) => workspace.set_focus(focus),
            ShellAction::Undo => {
                ctx.history.undo(ctx.world)?;
                workspace.invalidate();
            }
            ShellAction::Redo => {
                ctx.history.redo(ctx.world)?;
                workspace.invalidate();
            }
            ShellAction::Post(request) => workspace.post(request),
            ShellAction::DialogAnswer(yes) => workspace.post(ShellRequest::Command(
                if yes {
                    "dialog.confirm"
                } else {
                    "dialog.cancel"
                }
                .to_owned(),
            )),
        }
    }
    workspace.process_requests(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_read_as_words() {
        assert_eq!(profile_label(HardwareProfile::HighRefresh), "High Refresh");
        assert_eq!(
            profile_label(HardwareProfile::LegacyLowSpec),
            "Legacy Low Spec"
        );
    }
}
