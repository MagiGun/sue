//! The Hub: a box for every studio and tool, and the lists of files.

use std::path::PathBuf;

use egui::{
    Align, Align2, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    Vec2, pos2, vec2,
};
use sue_platform::FormFactor;

use super::{Shell, ShellIo, pinned};
use crate::{AppletId, AppletRole, ShellRequest, Theme};

/// What a click on a box of the Hub asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HubClick {
    /// Open the studio or tool.
    Open,
    /// Pin it, or take its pin away.
    Pin,
}

/// What a row of the Hub's lists does when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HubRow {
    /// Open a recent file or a saved project in its studio or tool.
    Open(PathBuf),
    /// Bring back work Sue kept and open it in its studio.
    Recover(String),
    /// Throw kept work away (a first click asks, a second one does it).
    Discard(String),
}

/// A row of one of the Hub's lists, ready to draw.
struct ListRow {
    title: String,
    /// What it is, at the right of the title: `Project`, `PNG`.
    kind: String,
    detail: String,
    /// The full path, on hover.
    hover: String,
    open: HubRow,
    /// Kept work can be recovered or discarded with its own buttons.
    kept: Option<String>,
}

/// `lead` and the folder `path` is in, by its name: for a file a phone's
/// picker chose, the folder people chose it in, where the system says it.
fn in_folder(lead: String, path: &std::path::Path) -> String {
    let folder = match sue_platform::shown_place(path) {
        Some(shown) => shown.folder,
        None => path.parent().map(|parent| {
            parent.file_name().map_or_else(
                || parent.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        }),
    };
    match folder {
        Some(folder) => tr!("in-folder", lead = lead, folder = folder),
        None => lead,
    }
}

/// A pushpin in `rect`, leaning as one pressed into a board: its cap,
/// body and base plate filled when `pinned`, outlined when not, and its
/// needle.
fn pin_glyph(painter: &egui::Painter, rect: Rect, pinned: bool, color: egui::Color32) {
    let unit = rect.width().min(rect.height()) * 0.36;
    let center = rect.center();
    // Leaning 45 degrees to the right, needle down and to the left.
    let (sin, cos) = std::f32::consts::FRAC_PI_4.sin_cos();
    let at = |x: f32, y: f32| {
        let (x, y) = (x * unit, y * unit);
        center + vec2(x * cos - y * sin, x * sin + y * cos)
    };
    let stroke = Stroke::new(1.4, color);
    let part = |points: Vec<egui::Pos2>| {
        if pinned {
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        } else {
            painter.add(egui::Shape::closed_line(points, stroke));
        }
    };
    // The cap, the body narrowing to the base plate, the plate.
    part(vec![
        at(-0.42, -0.95),
        at(0.42, -0.95),
        at(0.42, -0.72),
        at(-0.42, -0.72),
    ]);
    part(vec![
        at(-0.22, -0.72),
        at(0.22, -0.72),
        at(0.3, -0.05),
        at(-0.3, -0.05),
    ]);
    part(vec![
        at(-0.55, -0.05),
        at(0.55, -0.05),
        at(0.55, 0.1),
        at(-0.55, 0.1),
    ]);
    painter.line_segment([at(0.0, 0.1), at(0.0, 0.95)], Stroke::new(1.6, color));
}

impl Shell {
    /// The Hub: a box for every studio, then for every tool, each with a
    /// pin at its bottom right.
    pub(super) fn hub_ui(
        &mut self,
        root: &mut Ui,
        area: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        root.painter().rect_filled(area, 0.0, theme.void);
        // A page on its way in takes no clicks on the Hub under it.
        let live = self.page.is_none();
        let touch = self.form_factor() != FormFactor::Desktop;
        let narrow = area.width() < 560.0;
        let margin = if narrow { 16.0 } else { 40.0 };
        let gap = if narrow { 12.0 } else { 16.0 };
        // On a phone each studio and tool is a row across the screen, its
        // icon at the left: easy to reach with a thumb and quick to scan.
        let size = if narrow {
            vec2(area.width() - margin * 2.0, 96.0)
        } else {
            vec2(256.0, 176.0)
        };
        let columns = ((area.width() - margin * 2.0 + gap) / (size.x + gap))
            .floor()
            .max(1.0) as usize;
        let pinned = pinned(&io.world.rice, io.applets);
        let mut clicked = None;
        let mut row_click = None;
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(area)
                .id_salt("sue-hub")
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(area);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut ui, |ui| {
                ui.add_space(margin * 0.75);
                for (title, role) in [
                    (tr!("studios"), AppletRole::Studio),
                    (tr!("tools"), AppletRole::Tool),
                ] {
                    let boxes: Vec<(AppletId, String, String, String)> = io
                        .applets
                        .with_role(role)
                        .map(|manifest| {
                            (
                                manifest.id.clone(),
                                manifest.title(),
                                manifest.icon.clone(),
                                manifest.blurb(),
                            )
                        })
                        .collect();
                    if boxes.is_empty() {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.label(
                            RichText::new(title)
                                .size(theme.heading_size)
                                .color(theme.text),
                        );
                    });
                    ui.add_space(gap * 0.75);
                    for row in boxes.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.spacing_mut().item_spacing.x = gap;
                            for (id, name, icon, summary) in row {
                                let on = pinned.as_ref() == Some(id);
                                let what = (name.as_str(), icon.as_str(), summary.as_str());
                                let (click, rect, pin) =
                                    Self::hub_box(ui, size, what, on, live, touch, theme);
                                self.hub_boxes.push((id.clone(), rect, pin));
                                if let Some(click) = click {
                                    clicked = Some((id.clone(), click));
                                }
                            }
                        });
                        ui.add_space(gap);
                    }
                    ui.add_space(margin * 0.5);
                }
                row_click = self.hub_lists(ui, io, theme, (margin, gap), live, touch);
                ui.add_space(margin);
            });
        match row_click {
            Some(HubRow::Open(path)) => {
                self.discard_armed = None;
                io.requests.push(ShellRequest::OpenFile(path));
            }
            Some(HubRow::Recover(id)) => {
                self.discard_armed = None;
                io.requests.push(ShellRequest::Recover(id));
            }
            Some(HubRow::Discard(id)) => {
                if self.discard_armed.as_ref() == Some(&id) {
                    self.discard_armed = None;
                    io.requests.push(ShellRequest::Discard(id));
                } else {
                    self.discard_armed = Some(id);
                }
                self.touch();
            }
            None => {}
        }
        match clicked {
            Some((id, HubClick::Open)) => self.open(id, io.applets),
            Some((id, HubClick::Pin)) => {
                let again = pinned.as_ref() == Some(&id);
                self.pin((!again).then_some(id), io);
            }
            None => {}
        }
    }

    /// The Hub's lists under the boxes: the files and projects of late,
    /// the saved projects, and the work Sue kept that nobody saved. Three
    /// columns side by side when there is room, one under the other when
    /// not. Returns the row or button clicked.
    fn hub_lists(
        &mut self,
        ui: &mut Ui,
        io: &ShellIo<'_>,
        theme: &Theme,
        (margin, gap): (f32, f32),
        live: bool,
        touch: bool,
    ) -> Option<HubRow> {
        let now = crate::unix_now();
        let mut lists = Vec::new();
        if self.config.recent_count > 0 {
            lists.push((
                tr!("recent"),
                tr!("files-projects-you-open-save-show"),
                Self::recent_rows(io, now, self.config.recent_count),
            ));
        }
        lists.push((
            tr!("saved-projects"),
            tr!("projects-you-save-sue-menu-project"),
            Self::project_rows(io, now),
        ));
        lists.push((
            tr!("unsaved-work"),
            tr!("work-sue-keeps-you-before-saved"),
            Self::unsaved_rows(io, now),
        ));
        let room = (ui.available_width() - margin * 2.0).max(120.0);
        let side_by_side = room >= 3.0 * 300.0 + 2.0 * gap;
        let width = if side_by_side {
            ((room - 2.0 * gap) / 3.0).min(440.0)
        } else {
            room.min(560.0)
        };
        let mut clicked = None;
        let mut draw = |shell: &mut Self, ui: &mut Ui, list: (String, String, Vec<ListRow>)| {
            let (title, empty, rows) = list;
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new(title)
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(gap * 0.75);
                if rows.is_empty() {
                    ui.label(
                        RichText::new(empty)
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                }
                ui.spacing_mut().item_spacing.y = 6.0;
                for row in rows {
                    if let Some(click) = shell.hub_list_row(ui, width, &row, live, touch, theme) {
                        clicked = Some(click);
                    }
                }
            });
        };
        if side_by_side {
            ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.spacing_mut().item_spacing.x = gap;
                for list in lists {
                    draw(self, ui, list);
                }
            });
        } else {
            for list in lists {
                ui.horizontal_top(|ui| {
                    ui.add_space(margin);
                    draw(self, ui, list);
                });
                ui.add_space(gap * 1.5);
            }
        }
        clicked
    }

    /// The files and projects of late, newest first.
    fn recent_rows(io: &ShellIo<'_>, now: u64, count: usize) -> Vec<ListRow> {
        io.work
            .recent()
            .take(count)
            .map(|file| {
                let verb = if file.saved >= file.opened {
                    tr!("saved-2")
                } else {
                    tr!("opened")
                };
                ListRow {
                    title: file.title(),
                    kind: if file.is_project() {
                        tr!("menu-project")
                    } else {
                        file.path
                            .extension()
                            .map(|extension| extension.to_string_lossy().to_uppercase())
                            .unwrap_or_default()
                    },
                    detail: in_folder(
                        format!("{verb} {}", crate::ago(file.last(), now)),
                        &file.path,
                    ),
                    hover: sue_platform::shown_path(&file.path),
                    open: HubRow::Open(file.path.clone()),
                    kept: None,
                }
            })
            .collect()
    }

    /// The Sue projects saved or opened, by name.
    fn project_rows(io: &ShellIo<'_>, now: u64) -> Vec<ListRow> {
        io.work
            .projects()
            .into_iter()
            .take(50)
            .map(|file| {
                let (verb, when) = if file.saved > 0 {
                    (tr!("saved-2"), file.saved)
                } else {
                    (tr!("opened"), file.opened)
                };
                ListRow {
                    title: file.title(),
                    kind: Self::studio_name(io, file.page.as_ref()),
                    detail: in_folder(format!("{verb} {}", crate::ago(when, now)), &file.path),
                    hover: sue_platform::shown_path(&file.path),
                    open: HubRow::Open(file.path.clone()),
                    kept: None,
                }
            })
            .collect()
    }

    /// The work Sue kept that nobody saved, newest first.
    fn unsaved_rows(io: &ShellIo<'_>, now: u64) -> Vec<ListRow> {
        io.work
            .unsaved()
            .map(|kept| {
                let detail = match &kept.project {
                    Some(project) => tr!(
                        "kept-newer-than",
                        value = crate::ago(kept.at, now),
                        value2 = project.file_name().map_or_else(
                            || project.display().to_string(),
                            |name| name.to_string_lossy().into_owned(),
                        )
                    ),
                    None => tr!("kept-never-saved", value = crate::ago(kept.at, now)),
                };
                ListRow {
                    title: kept.title(),
                    kind: Self::studio_name(io, kept.page.as_ref()),
                    detail,
                    hover: kept
                        .project
                        .as_ref()
                        .map(|project| project.display().to_string())
                        .unwrap_or_default(),
                    open: HubRow::Recover(kept.id.clone()),
                    kept: Some(kept.id.clone()),
                }
            })
            .collect()
    }

    /// The name of the studio or tool `page`, if it is installed.
    fn studio_name(io: &ShellIo<'_>, page: Option<&AppletId>) -> String {
        page.and_then(|page| io.applets.manifest(page))
            .map(|manifest| manifest.name.clone())
            .unwrap_or_default()
    }

    /// One row of a list: a sharp box with the title, what it is and a line
    /// of detail; kept work has Recover and Discard buttons at its right.
    fn hub_list_row(
        &mut self,
        ui: &mut Ui,
        width: f32,
        row: &ListRow,
        live: bool,
        touch: bool,
        theme: &Theme,
    ) -> Option<HubRow> {
        let height = if touch { 56.0 } else { 46.0 };
        let sense = if live { Sense::click() } else { Sense::hover() };
        let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), sense);
        if !row.hover.is_empty() {
            response = response.on_hover_text(&row.hover);
        }
        self.hub_rows.push((row.open.clone(), rect));
        // Kept work's buttons, from the right; checked after the row, so
        // they win its clicks.
        let mut buttons = Vec::new();
        if let Some(id) = &row.kept {
            let armed = self.discard_armed.as_ref() == Some(id);
            let mut right = rect.right() - 6.0;
            let discard = if armed {
                tr!("really-discard")
            } else {
                tr!("discard")
            };
            for (label, action) in [
                (discard, HubRow::Discard(id.clone())),
                (tr!("recover"), HubRow::Recover(id.clone())),
            ] {
                let text = ui.painter().layout_no_wrap(
                    label.to_owned(),
                    theme.font(theme.small_size + 1.0),
                    theme.text,
                );
                let button = Rect::from_min_max(
                    pos2(right - text.size().x - 20.0, rect.top() + 6.0),
                    pos2(right, rect.bottom() - 6.0),
                );
                right = button.left() - 6.0;
                let hit = ui.interact(
                    button,
                    ui.scope_id().with(("sue-hub-kept", label, id.as_str())),
                    sense,
                );
                self.hub_rows.push((action.clone(), button));
                let lit = armed && matches!(action, HubRow::Discard(_));
                buttons.push((button, text, hit, action, lit));
            }
        }
        let over_button = buttons.iter().any(|(_, _, hit, _, _)| hit.hovered());
        let hovered = live && (response.hovered() || over_button);
        let painter = ui.painter();
        painter.rect(
            rect,
            0.0,
            if hovered { theme.hover } else { theme.pill },
            Stroke::new(
                theme.border_width.max(1.0),
                if hovered {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
        let text_right = buttons
            .iter()
            .map(|(button, ..)| button.left())
            .fold(rect.right(), f32::min)
            - 10.0;
        let clip = Rect::from_min_max(rect.min, pos2(text_right, rect.bottom())).shrink(1.0);
        let painter = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
        let line = (height - theme.ui_size - theme.small_size - 4.0) / 2.0;
        let kind =
            painter.layout_no_wrap(row.kind.clone(), theme.font(theme.small_size), theme.muted);
        let kind_width = kind.size().x;
        painter.galley(
            pos2(text_right - kind_width, rect.top() + line + 1.0),
            kind,
            theme.muted,
        );
        let title =
            painter.layout_no_wrap(row.title.clone(), theme.font(theme.ui_size), theme.text);
        painter
            .with_clip_rect(Rect::from_min_max(
                clip.min,
                pos2(text_right - kind_width - 8.0, clip.bottom()),
            ))
            .galley(
                pos2(rect.left() + 12.0, rect.top() + line),
                title,
                theme.text,
            );
        painter.text(
            pos2(rect.left() + 12.0, rect.top() + line + theme.ui_size + 4.0),
            Align2::LEFT_TOP,
            &row.detail,
            theme.font(theme.small_size),
            theme.muted,
        );
        let mut clicked = (live && response.clicked()).then(|| row.open.clone());
        for (button, text, hit, action, lit) in buttons {
            let hot = live && hit.hovered();
            let painter = ui.painter();
            // A fill of its own, so it shows on a hovered row too.
            let fill = if lit {
                theme.active
            } else if hot {
                theme.hover
            } else {
                theme.pill
            };
            painter.rect_filled(button, 0.0, fill);
            painter.rect_stroke(
                button,
                0.0,
                if lit || hot {
                    theme.hairline_active()
                } else {
                    theme.hairline()
                },
                StrokeKind::Inside,
            );
            painter.galley(button.center() - text.size() / 2.0, text, theme.text);
            if live && hit.clicked() {
                clicked = Some(action);
            }
        }
        clicked
    }

    /// One box of the Hub, with sharp corners and a pin at its bottom
    /// right. Returns what was clicked, the box and the pin.
    fn hub_box(
        ui: &mut Ui,
        size: Vec2,
        (name, icon, summary): (&str, &str, &str),
        pinned: bool,
        live: bool,
        touch: bool,
        theme: &Theme,
    ) -> (Option<HubClick>, Rect, Rect) {
        let sense = if live { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        let side = if touch { 44.0 } else { 30.0 };
        // A row (on a phone): icon, then name and summary, pin at the end.
        let row = size.y < 120.0;
        let pin = if row {
            Rect::from_center_size(
                pos2(rect.right() - side / 2.0 - 10.0, rect.center().y),
                Vec2::splat(side),
            )
        } else {
            Rect::from_min_size(
                rect.right_bottom() - vec2(side + 8.0, side + 8.0),
                Vec2::splat(side),
            )
        };
        // The pin is checked after the box, so it wins the click over it.
        let pin_response = ui
            .interact(pin, ui.scope_id().with(("sue-hub-pin", name)), sense)
            .on_hover_text(if pinned {
                tr!("pinned-sue-opens-here-launch-click")
            } else {
                tr!("pin-sue-opens-here-launch")
            });
        // The box lights up for itself, not while the pointer is on its
        // pin: the pin lights up alone.
        let hovered = live && response.hovered() && !pin_response.hovered();
        let painter = ui.painter();
        painter.rect(
            rect,
            0.0,
            if hovered { theme.hover } else { theme.pill },
            Stroke::new(
                theme.border_width.max(1.0),
                if hovered {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
        let (icon_at, icon_align, name_at, summary_at, summary_width) = if row {
            (
                pos2(rect.left() + 20.0, rect.center().y),
                Align2::LEFT_CENTER,
                rect.left_top() + vec2(76.0, 14.0),
                rect.left_top() + vec2(76.0, 42.0),
                rect.width() - 76.0 - side - 20.0,
            )
        } else {
            (
                rect.left_top() + vec2(18.0, 16.0),
                Align2::LEFT_TOP,
                rect.left_top() + vec2(18.0, 76.0),
                rect.left_top() + vec2(18.0, 102.0),
                rect.width() - 36.0 - side,
            )
        };
        painter.text(
            icon_at,
            icon_align,
            icon,
            theme.font(theme.heading_size * if row { 1.5 } else { 1.8 }),
            theme.text,
        );
        painter.text(
            name_at,
            Align2::LEFT_TOP,
            name,
            theme.font(theme.ui_size * 1.2),
            theme.text,
        );
        let galley = painter.layout(
            summary.to_owned(),
            theme.font(theme.small_size),
            theme.muted,
            summary_width,
        );
        painter
            .with_clip_rect(rect.shrink(1.0))
            .galley(summary_at, galley, theme.muted);
        // The pin: a square button, sharp like its box.
        if pin_response.hovered() && live {
            painter.rect_filled(pin, 0.0, theme.active);
        }
        if pinned {
            painter.rect_stroke(pin, 0.0, theme.hairline_active(), StrokeKind::Inside);
        }
        pin_glyph(
            painter,
            pin,
            pinned,
            if pinned || pin_response.hovered() {
                theme.text
            } else {
                theme.muted
            },
        );
        let click = if !live {
            None
        } else if pin_response.clicked() {
            Some(HubClick::Pin)
        } else if response.clicked() {
            Some(HubClick::Open)
        } else {
            None
        };
        (click, rect, pin)
    }
}
