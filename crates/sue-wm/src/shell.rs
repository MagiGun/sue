//! The desktop shell: top bar, docks, Center Stage, Launchpad and the drop
//! overlay, composed from the workspace layout and drawn with [`Ui`].

use sue_platform::{CursorShape, FormFactor, Point, SafeAreaInsets};
use sue_rice::{RiceValue, SetRiceValue};

use crate::{
    AppletCategory, AppletContext, AppletId, AppletManifest, BoxStyle, Color, Focus, FontFace,
    Placement, Rect, Ui, WmError, Workspace, Zone, widget_id,
};

/// Per-frame facts the shell shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellFrame<'a> {
    /// The window, in logical pixels.
    pub viewport: Rect,
    /// Notches and system bars.
    pub safe_area: SafeAreaInsets,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Telemetry badge text, e.g. `Render: SLEEP (0 FPS) · balanced`.
    pub telemetry: &'a str,
    /// `true` when the next frame will not be drawn (0 FPS).
    pub sleeping: bool,
}

/// What the user asked the shell to do this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellAction {
    /// Show or hide a dock.
    ToggleDock(Zone),
    /// Open a new Hub tab.
    NewTab,
    /// Switch tabs.
    ActivateTab(usize),
    /// Close a tab.
    CloseTab(usize),
    /// Open an applet in the active tab.
    OpenApplet(AppletId),
    /// Pin or unpin an applet as the boot target (an undoable rice edit).
    TogglePin(AppletId),
    /// Move keyboard focus.
    Focus(Focus),
}

fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.y >= rect.y && point.x < rect.right() && point.y < rect.bottom()
}

fn inset(rect: Rect, amount: f32) -> Rect {
    rect.shrink(amount)
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
    ui.canvas
        .draw_box(viewport, BoxStyle::filled(theme.background, 0.0));
    let top = Rect::new(viewport.x, viewport.y, viewport.width, theme.top_bar_height);
    let content = Rect::new(
        viewport.x,
        top.bottom(),
        viewport.width,
        (viewport.height - theme.top_bar_height).max(0.0),
    );
    let layout = workspace.layout(content, frame.safe_area, frame.form_factor);
    let pressed_at = ui.input.pointer.filter(|_| ui.input.pressed);

    // Center Stage.
    ui.panel(layout.center);
    let stage = inset(layout.center, theme.padding());
    ui.canvas.push_clip(stage);
    if !workspace.draw_active(ui, stage, ctx) {
        draw_launchpad(workspace, ui, ctx, stage, &mut actions);
    }
    ui.canvas.pop_clip();
    if pressed_at.is_some_and(|point| contains(layout.center, point)) {
        actions.push(ShellAction::Focus(Focus::Center));
    }

    if let Placement::Tiled(timeline) | Placement::BottomSheet { rect: timeline, .. } =
        layout.bottom
    {
        ui.panel(timeline);
        let label = Rect::new(
            timeline.x + theme.padding(),
            timeline.y,
            timeline.width,
            ui.row_height(),
        );
        ui.label(label, "Timeline", theme.muted);
    }

    for (zone, placement) in [
        (Zone::LeftDock, layout.left),
        (Zone::RightDock, layout.right),
    ] {
        let Some(rect) = placement.rect() else {
            continue;
        };
        if let Placement::Drawer { reveal, .. } = placement {
            // Overlay drawers dim what they cover.
            ui.canvas.draw_box(
                layout.center,
                BoxStyle::filled(theme.background.fade(0.5 * reveal), 0.0),
            );
        }
        draw_dock(workspace, ui, ctx, zone, rect, pressed_at, &mut actions);
    }

    draw_top_bar(workspace, ui, ctx, top, frame, &mut actions);

    if ui.input.file_hover {
        ui.canvas
            .draw_box(viewport, BoxStyle::filled(theme.background.fade(0.75), 0.0));
        let width = (viewport.width * 0.6).min(theme.card_width * 2.5);
        let height = ui.row_height() * 3.0;
        let target = Rect::new(
            viewport.x + (viewport.width - width) / 2.0,
            viewport.y + (viewport.height - height) / 2.0,
            width,
            height,
        );
        ui.canvas.draw_box(
            target,
            BoxStyle {
                fill: theme.surface,
                border: theme.accent,
                border_width: theme.border_width.max(1.0) * 2.0,
                radius: theme.radius,
            },
        );
        let line = ui.row_height();
        ui.label_centered(
            Rect::new(target.x, target.y, target.width, line * 1.5),
            "Drop to ingest",
            theme.accent,
        );
        ui.label_centered(
            Rect::new(target.x, target.y + line * 1.4, target.width, line),
            "JPEG XL opens in the viewer · text in Text Studio · anything else in the Hex Inspector",
            theme.muted,
        );
    }
    actions
}

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
    pressed_at: Option<Point>,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    ui.canvas.push_clip(rect);
    ui.panel(rect);
    let docked: Vec<(usize, String)> = workspace
        .docked()
        .filter(|(_, dock_zone, _)| *dock_zone == zone)
        .map(|(index, _, manifest)| (index, manifest.map_or_else(String::new, |m| m.name.clone())))
        .collect();
    let count = docked.len().max(1) as f32;
    let header = ui.row_height();
    let slot_height = (rect.height - theme.padding()) / count;
    for (slot, (index, name)) in docked.into_iter().enumerate() {
        let slot_rect = Rect::new(
            rect.x + theme.padding(),
            rect.y + theme.padding() + slot_height * slot as f32,
            rect.width - theme.padding() * 2.0,
            slot_height - theme.padding(),
        );
        let focused = workspace.focus() == Focus::Dock(index);
        let title_color = if focused { theme.accent } else { theme.muted };
        ui.label(
            Rect::new(slot_rect.x, slot_rect.y, slot_rect.width, header),
            &name.to_uppercase(),
            title_color,
        );
        let body = Rect::new(
            slot_rect.x,
            slot_rect.y + header,
            slot_rect.width,
            (slot_rect.height - header).max(0.0),
        );
        ui.canvas.push_clip(body);
        workspace.draw_dock(index, ui, body, ctx);
        ui.canvas.pop_clip();
        if pressed_at.is_some_and(|point| contains(slot_rect, point)) {
            actions.push(ShellAction::Focus(Focus::Dock(index)));
        }
    }
    ui.canvas.pop_clip();
}

fn draw_top_bar(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    ctx: &AppletContext<'_>,
    top: Rect,
    frame: &ShellFrame<'_>,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    ui.canvas
        .draw_box(top, BoxStyle::filled(theme.surface, 0.0));
    ui.canvas.draw_box(
        Rect::new(
            top.x,
            top.bottom() - theme.border_width,
            top.width,
            theme.border_width,
        ),
        BoxStyle::filled(theme.raised, 0.0),
    );
    let pad = theme.padding();
    let button = (top.height - pad).max(1.0);
    let y = top.y + (top.height - button) / 2.0;
    let (left_open, right_open) = workspace.docks_open();

    let left_toggle = Rect::new(top.x + pad / 2.0, y, button, button);
    if dock_icon(
        ui,
        widget_id("shell", "toggle-left"),
        left_toggle,
        true,
        left_open,
    ) {
        actions.push(ShellAction::ToggleDock(Zone::LeftDock));
    }
    let mut x = left_toggle.right() + pad;
    let title = "Sue v1.0 — Faustian Enigma";
    let title_width = ui.fonts.measure(FontFace::Ui, theme.ui_size, title);
    ui.label(
        Rect::new(x, top.y, title_width + 1.0, top.height),
        title,
        theme.text,
    );
    x += title_width + pad * 2.0;

    let mode = workspace.tab_title(workspace.active_index(), ctx.world);
    let mode_text = format!("Mode · {mode}");
    let mode_width = ui.fonts.measure(FontFace::Ui, theme.ui_size, &mode_text) + pad * 2.0;
    let mode_rect = Rect::new(x, y, mode_width, button);
    ui.canvas.draw_box(
        mode_rect,
        BoxStyle::filled(theme.accent.fade(0.18), button / 2.0),
    );
    ui.label_centered(mode_rect, &mode_text, theme.accent);
    x = mode_rect.right() + pad;

    let right_toggle = Rect::new(top.right() - pad / 2.0 - button, y, button, button);
    if dock_icon(
        ui,
        widget_id("shell", "toggle-right"),
        right_toggle,
        false,
        right_open,
    ) {
        actions.push(ShellAction::ToggleDock(Zone::RightDock));
    }
    let badge_width = ui
        .fonts
        .measure(FontFace::Mono, theme.mono_size, frame.telemetry)
        + pad * 3.0;
    let badge = Rect::new(right_toggle.x - pad - badge_width, y, badge_width, button);
    ui.canvas.draw_box(
        badge,
        BoxStyle {
            fill: theme.background,
            border: if frame.sleeping {
                theme.raised
            } else {
                theme.accent
            },
            border_width: theme.border_width.max(1.0),
            radius: button / 2.0,
        },
    );
    let dot = button / 4.0;
    let dot_color = if frame.sleeping {
        theme.muted
    } else {
        theme.accent
    };
    ui.canvas.draw_box(
        Rect::new(
            badge.x + pad * 0.75,
            badge.y + (button - dot) / 2.0,
            dot,
            dot,
        ),
        BoxStyle::filled(dot_color, dot / 2.0),
    );
    let mono_height = ui.fonts.line_height(FontFace::Mono, theme.mono_size);
    ui.text(
        Point::new(
            badge.x + pad * 0.75 + dot + pad / 2.0,
            badge.y + (button - mono_height) / 2.0,
        ),
        frame.telemetry,
        FontFace::Mono,
        theme.mono_size,
        if frame.sleeping {
            theme.muted
        } else {
            theme.text
        },
    );

    // Tabs fill the space between the mode chip and the badge.
    let tabs_end = badge.x - pad;
    let tab_count = workspace.tabs().len();
    let plus_width = button;
    let available = (tabs_end - x - plus_width - pad).max(0.0);
    let tab_width = (available / tab_count.max(1) as f32).min(theme.card_width * 0.75);
    if tab_width > button * 2.0 {
        for index in 0..tab_count {
            let tab = Rect::new(
                x + index as f32 * (tab_width + pad / 2.0),
                y,
                tab_width,
                button,
            );
            let active = index == workspace.active_index();
            let response = ui.interact(widget_id("shell/tab", &index.to_string()), tab);
            let fill = if active {
                theme.raised
            } else if response.hovered {
                theme.hover
            } else {
                Color::TRANSPARENT
            };
            ui.canvas
                .draw_box(tab, BoxStyle::filled(fill, theme.radius / 2.0));
            let close = Rect::new(tab.right() - button, tab.y, button, button);
            let label = Rect::new(
                tab.x + pad,
                tab.y,
                (tab.width - pad - button).max(0.0),
                tab.height,
            );
            let text_color = if active { theme.text } else { theme.muted };
            ui.label(label, &workspace.tab_title(index, ctx.world), text_color);
            if response.hovered {
                ui.set_cursor(CursorShape::Pointer);
            }
            if response.clicked {
                actions.push(ShellAction::ActivateTab(index));
            }
            let close_response =
                ui.interact(widget_id("shell/tab-close", &index.to_string()), close);
            let close_color = if close_response.hovered {
                theme.alert
            } else {
                theme.muted
            };
            ui.label_centered(close, "×", close_color);
            if close_response.clicked {
                actions.push(ShellAction::CloseTab(index));
            }
        }
        let plus = Rect::new(
            x + tab_count as f32 * (tab_width + pad / 2.0),
            y,
            plus_width,
            button,
        );
        if ui.button(widget_id("shell", "new-tab"), plus, "+") {
            actions.push(ShellAction::NewTab);
        }
    }
}

/// A dock toggle drawn as a window with its left or right pane filled.
fn dock_icon(ui: &mut Ui<'_>, id: u64, rect: Rect, left: bool, open: bool) -> bool {
    let response = ui.interact(id, rect);
    let theme = ui.theme;
    if response.hovered {
        ui.canvas
            .draw_box(rect, BoxStyle::filled(theme.hover, theme.radius / 2.0));
        ui.set_cursor(CursorShape::Pointer);
    }
    let icon = rect.shrink(rect.width * 0.25);
    let color = if open { theme.accent } else { theme.muted };
    let corner = theme.radius / 4.0;
    ui.canvas.draw_box(
        icon,
        BoxStyle {
            fill: Color::TRANSPARENT,
            border: color,
            border_width: theme.border_width.max(1.0),
            radius: corner,
        },
    );
    let pane = icon.width * 0.38;
    let pane_x = if left { icon.x } else { icon.right() - pane };
    ui.canvas.draw_box(
        Rect::new(pane_x, icon.y, pane, icon.height),
        BoxStyle::filled(color, corner),
    );
    response.clicked
}

fn draw_launchpad(
    workspace: &Workspace,
    ui: &mut Ui<'_>,
    ctx: &AppletContext<'_>,
    area: Rect,
    actions: &mut Vec<ShellAction>,
) {
    let theme = ui.theme;
    let pad = theme.padding();
    let heading_size = theme.ui_size * 1.8;
    let heading = ui.fonts.line_height(FontFace::Ui, heading_size);
    ui.text(
        Point::new(area.x + pad, area.y + pad),
        "Launchpad",
        FontFace::Ui,
        heading_size,
        theme.text,
    );
    let hub = workspace.launchpad().hub();
    let pinned = ctx
        .world
        .rice
        .get_str("launchpad.boot_into")
        .unwrap_or("hub")
        .to_owned();
    let plural = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let subtitle = format!(
        "{} · {} installed · pin one to boot straight into it",
        plural(hub.studios.len(), "studio", "studios"),
        plural(hub.utilities.len(), "utility", "utilities")
    );
    let line = ui.row_height();
    ui.label(
        Rect::new(
            area.x + pad,
            area.y + pad + heading,
            area.width - pad * 2.0,
            line,
        ),
        &subtitle,
        theme.muted,
    );
    let mut y = area.y + pad * 2.0 + heading + line;

    let columns = (((area.width - pad) / (theme.card_width + pad)).floor() as usize).max(1);
    let card_width = ((area.width - pad * (columns as f32 + 1.0)) / columns as f32).max(1.0);
    let card_height = line * 3.5;
    for (title, list, empty) in [
        (
            "STUDIOS",
            &hub.studios,
            "No studios installed yet; they arrive as packages.",
        ),
        ("UTILITIES", &hub.utilities, "No utilities installed."),
    ] {
        ui.label(
            Rect::new(area.x + pad, y, area.width, line),
            title,
            theme.muted,
        );
        y += line;
        if list.is_empty() {
            ui.label(
                Rect::new(area.x + pad, y, area.width, line),
                empty,
                theme.muted.fade(0.7),
            );
            y += line + pad;
            continue;
        }
        for (index, manifest) in list.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            let card = Rect::new(
                area.x + pad + column as f32 * (card_width + pad),
                y + row as f32 * (card_height + pad),
                card_width,
                card_height,
            );
            let is_pinned = manifest.id.as_str() == pinned;
            if let Some(action) = draw_card(ui, manifest, card, is_pinned) {
                actions.push(action);
            }
        }
        let rows = list.len().div_ceil(columns);
        y += rows as f32 * (card_height + pad) + pad;
    }
}

fn draw_card(
    ui: &mut Ui<'_>,
    manifest: &AppletManifest,
    card: Rect,
    pinned: bool,
) -> Option<ShellAction> {
    let theme = ui.theme;
    let pad = theme.padding();
    let line = ui.row_height();
    let response = ui.interact(widget_id("launchpad/card", manifest.id.as_str()), card);
    let border = if pinned || response.hovered {
        theme.accent
    } else {
        theme.raised
    };
    ui.canvas.draw_box(
        card,
        BoxStyle {
            fill: if response.hovered {
                theme.hover
            } else {
                theme.raised
            },
            border,
            border_width: theme.border_width.max(1.0),
            radius: theme.radius,
        },
    );
    if response.hovered {
        ui.set_cursor(CursorShape::Pointer);
    }
    let name_size = theme.ui_size * 1.2;
    ui.text(
        Point::new(card.x + pad, card.y + pad),
        &manifest.name,
        FontFace::Ui,
        name_size,
        theme.text,
    );
    let kind = match (manifest.category, manifest.home_zone) {
        (AppletCategory::Studio, _) => "Studio",
        (AppletCategory::Utility, Zone::LeftDock) => "Utility · left dock",
        (AppletCategory::Utility, Zone::RightDock) => "Utility · right dock",
        (AppletCategory::Utility, _) => "Utility",
    };
    let name_height = ui.fonts.line_height(FontFace::Ui, name_size);
    ui.label(
        Rect::new(
            card.x + pad,
            card.y + pad + name_height,
            card.width - pad * 2.0,
            line * 0.9,
        ),
        kind,
        theme.accent,
    );
    ui.label(
        Rect::new(
            card.x + pad,
            card.y + pad + name_height + line * 0.9,
            card.width - pad * 2.0,
            line,
        ),
        &manifest.summary,
        theme.muted,
    );

    let pin = Rect::new(
        card.right() - line - pad / 2.0,
        card.y + pad / 2.0,
        line,
        line,
    );
    let pin_response = ui.interact(widget_id("launchpad/pin", manifest.id.as_str()), pin);
    if pinned || pin_response.hovered {
        let fill = if pinned { theme.accent } else { theme.hover };
        ui.canvas.draw_box(pin, BoxStyle::filled(fill, line / 2.0));
    }
    let pin_color = if pinned { theme.on_accent } else { theme.muted };
    ui.label_centered(pin, "📌", pin_color);
    if pin_response.hovered {
        ui.set_cursor(CursorShape::Pointer);
    }
    if pin_response.clicked {
        return Some(ShellAction::TogglePin(manifest.id.clone()));
    }
    response
        .clicked
        .then(|| ShellAction::OpenApplet(manifest.id.clone()))
}

/// Carries out `actions`. Pin toggles become undoable rice edits of
/// `launchpad.boot_into`, so they persist and `Ctrl+Z` reverts them.
///
/// # Errors
/// If opening an applet or recording the rice edit fails.
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
            ShellAction::NewTab => {
                workspace.new_tab();
            }
            ShellAction::ActivateTab(index) => workspace.activate_tab(index),
            ShellAction::CloseTab(index) => workspace.close_tab(index, ctx),
            ShellAction::OpenApplet(id) => workspace.open_applet(&id, ctx)?,
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
            }
            ShellAction::Focus(focus) => workspace.set_focus(focus),
        }
    }
    workspace.process_requests(ctx)
}
