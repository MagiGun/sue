//! The files browser: labels on the left; search, type chips (and a chip
//! per studio when it shows the work of more than one), the toolbar and
//! the cards on the right; the popup that says what a project is, and the
//! Data Transfers popup. Built from the start page kit
//! ([`crate::start`]); the studio or the Hub that shows it does what the
//! [`Event`]s it returns ask for.

use egui::{Id, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use super::model::{
    Item, ProjectKey, ProjectsState, Shelf, Studio, Type, View, forget, items, shown, texture_key,
};
use super::thumbs::Thumbs;
use crate::icons::Icon;
use crate::start::{
    self as page, Card, CardClick, Category, Chosen, Glyph, PopupOutcome, Row, Trailing,
};
use crate::ui::{Density, Leading, ListRow, RowAction, Space, Thumbnail, ThumbnailCard};
use crate::{
    AppletContext, LabelEdit, ShellRequest, Theme, TransferKind, TransferState, ago, unix_now,
};

/// How wide the column of labels is.
pub const COLUMN: f32 = 210.0;

/// What the browser asks its host for, after drawing.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Open the project.
    Open(Item),
    /// Say what the project is, in [`project_popup`].
    Details(ProjectKey),
}

/// What a card or the popup calls an item's kind: the studio's name for
/// a file, when the browser shows more than one studio's work.
fn kind_label(item: &Item, studios: &[Studio<'_>]) -> String {
    if item.kind == Type::Picture
        && studios.len() > 1
        && let Some(studio) = studios
            .iter()
            .find(|studio| item.studio.as_ref() == Some(&studio.id))
    {
        return studio.name.clone();
    }
    item.kind.label()
}

/// What a type chip reads. With more than one studio's work shown, a file
/// that is not a project is not always a picture (a Sue Writer text is
/// one too), so that chip reads "File" there.
fn type_label(kind: Type, studios: &[Studio<'_>]) -> String {
    if kind == Type::Picture && studios.len() > 1 {
        tr!("start-type-file")
    } else {
        kind.label()
    }
}

/// What the browser draws besides what [`ProjectsState`] keeps: its shape
/// for the room it has, and where it drew each card, for the host's tests
/// and its keyboard.
#[derive(Clone, Debug, Default)]
pub struct Browse {
    /// Narrow (a phone): no column of labels; the shelves are chips over
    /// the cards, and the toolbar keeps only the sort and the view.
    pub compact: bool,
    /// Where the last frame drew each card and each of kept work's two
    /// buttons.
    pub drawn: Vec<(Spot, Rect)>,
    /// The kept work whose Discard was clicked once and asks again.
    pub discard_armed: Option<String>,
}

/// A place on a card that [`Browse::drawn`] reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spot {
    /// A whole card (or row): a click opens it, or recovers kept work.
    Card(ProjectKey),
    /// Kept work's Recover button.
    Recover(String),
    /// Kept work's Discard button (a first click asks, a second does it).
    Discard(String),
}

/// The projects of `studios` in `body`: labels on the left; search, type
/// chips, the toolbar and the cards on the right.
pub fn projects_ui(
    ui: &mut Ui,
    body: Rect,
    state: &mut ProjectsState,
    thumbs: &mut Thumbs,
    studios: &[Studio<'_>],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Vec<Event> {
    browse(
        ui,
        body,
        (state, &mut Browse::default()),
        thumbs,
        studios,
        cx,
        theme,
    )
}

/// The projects of `studios` in `body`, shaped as `options` says: the
/// labels on the left (chips over the cards when compact); search, type
/// chips, the toolbar and the cards on the right. Kept work's cards carry
/// Recover and a Discard that asks once more.
#[allow(clippy::too_many_lines)]
pub fn browse(
    ui: &mut Ui,
    body: Rect,
    (state, options): (&mut ProjectsState, &mut Browse),
    thumbs: &mut Thumbs,
    studios: &[Studio<'_>],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Vec<Event> {
    let mut events = Vec::new();
    options.drawn.clear();
    let compact = options.compact;
    let all = items(cx.work, studios);
    // A label just added is renamed once it shows.
    if let Some(name) = state.adding.take() {
        if cx.work.labels().contains(&name) {
            state.shelf = Shelf::Label(name.clone());
            state.renaming = Some((name.clone(), name));
        } else {
            state.adding = Some(name);
        }
    }
    if let Shelf::Label(label) = &state.shelf
        && !cx.work.labels().contains(label)
        && state.adding.is_none()
    {
        state.shelf = Shelf::All;
    }

    // The label column.
    let inset = theme.space(Space::S5);
    let column = if compact {
        Rect::from_min_max(body.min, pos2(body.left(), body.bottom()))
    } else {
        Rect::from_min_size(
            body.min + Vec2::splat(inset),
            vec2(COLUMN, body.height() - inset * 2.0),
        )
    };
    let mut col = ui.new_child(
        UiBuilder::new()
            .max_rect(column)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let label = match &state.shelf {
        Shelf::Label(label) => Some(label.clone()),
        _ => None,
    };
    if !compact {
        col.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let size = Vec2::splat(28.0);
            let (add, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button(ui, add, Glyph::Add, &tr!("start-add-label"), false, theme)
                .clicked()
            {
                let mut name = tr!("start-new-label");
                let mut n = 2;
                while cx.work.labels().contains(&name) {
                    name = format!("{} {n}", tr!("start-new-label"));
                    n += 1;
                }
                cx.request(ShellRequest::Labels(LabelEdit::Add(name.clone())));
                state.adding = Some(name);
            }
            let (delete, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button_enabled(
                ui,
                delete,
                Glyph::Delete,
                &tr!("start-delete-label"),
                false,
                label.is_some(),
                theme,
            )
            .clicked()
                && let Some(label) = &label
            {
                cx.request(ShellRequest::Labels(LabelEdit::Remove(label.clone())));
                state.shelf = Shelf::All;
            }
            let (rename, _) = ui.allocate_exact_size(size, Sense::hover());
            if page::icon_button_enabled(
                ui,
                rename,
                Glyph::Rename,
                &tr!("start-rename-label"),
                false,
                label.is_some(),
                theme,
            )
            .clicked()
                && let Some(label) = &label
            {
                state.renaming = Some((label.clone(), label.clone()));
            }
        });
        col.add_space(6.0);
        col.spacing_mut().item_spacing.y = 3.0;
        let mut shelves = vec![
            (Shelf::All, Glyph::All, tr!("start-all")),
            (Shelf::Favorite, Glyph::Heart, tr!("start-favorite")),
        ];
        shelves.extend(
            cx.work
                .labels()
                .iter()
                .map(|label| (Shelf::Label(label.clone()), Glyph::Tag, label.clone())),
        );
        let width = column.width();
        for (shelf, glyph, name) in shelves {
            if let (Shelf::Label(label), Some((from, text))) = (&shelf, state.renaming.as_mut())
                && *label == *from
            {
                let response = col.add_sized(
                    vec2(width, 26.0),
                    egui::TextEdit::singleline(text).font(theme.font(theme.ui_size)),
                );
                response.request_focus();
                let enter = col.input(|input| input.key_pressed(egui::Key::Enter));
                let escape = col.input(|input| input.key_pressed(egui::Key::Escape));
                if enter || (response.lost_focus() && !escape) {
                    let to = text.trim().to_owned();
                    if !to.is_empty() && to != *from {
                        cx.request(ShellRequest::Labels(LabelEdit::Rename {
                            from: from.clone(),
                            to: to.clone(),
                        }));
                        state.shelf = Shelf::Label(to);
                    }
                    state.renaming = None;
                } else if escape {
                    state.renaming = None;
                }
                continue;
            }
            let row = Category {
                glyph: Some(glyph),
                label: name,
                depth: 0,
                open: None,
            };
            let (click, response) =
                page::category(&mut col, width, &row, state.shelf == shelf, theme);
            if click.is_some() {
                state.shelf = shelf.clone();
                state.chosen.clear();
                state.armed = false;
            }
            if let Shelf::Label(label) = &shelf {
                // Chosen projects drop onto a label with a right click.
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            !state.chosen.is_empty(),
                            egui::Button::new(tr!("start-move-chosen-here")),
                        )
                        .clicked()
                    {
                        for key in &state.chosen {
                            if let Some(item) = all.iter().find(|item| &item.key == key) {
                                cx.request(ShellRequest::Labels(LabelEdit::Mark {
                                    path: item.path.clone(),
                                    label: label.clone(),
                                    on: true,
                                }));
                            }
                        }
                        ui.close();
                    }
                    if ui.button(tr!("start-rename-label")).clicked() {
                        state.renaming = Some((label.clone(), label.clone()));
                        ui.close();
                    }
                    if ui.button(tr!("start-delete-label")).clicked() {
                        cx.request(ShellRequest::Labels(LabelEdit::Remove(label.clone())));
                        ui.close();
                    }
                });
            }
        }
    }

    // Search, chips and the toolbar.
    let right = Rect::from_min_max(
        pos2(
            column.right() + if compact { 0.0 } else { inset },
            body.top() + if compact { 0.0 } else { inset },
        ),
        pos2(
            body.right() - if compact { 0.0 } else { inset },
            body.bottom(),
        ),
    );
    let mut area = ui.new_child(
        UiBuilder::new()
            .max_rect(right)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    area.set_clip_rect(right.intersect(ui.clip_rect()));
    page::search_box(
        &mut area,
        &mut state.search,
        &tr!("start-search-projects"),
        right.width(),
        theme,
    );
    area.add_space(8.0);
    if compact {
        let mut shelves = vec![
            (Shelf::All, tr!("start-all")),
            (Shelf::Favorite, tr!("start-favorite")),
        ];
        shelves.extend(
            cx.work
                .labels()
                .iter()
                .map(|label| (Shelf::Label(label.clone()), label.clone())),
        );
        area.horizontal_wrapped(|ui| {
            for (shelf, name) in shelves {
                if page::chip(ui, &name, state.shelf == shelf, theme).clicked() {
                    state.shelf = shelf;
                    state.chosen.clear();
                    state.armed = false;
                }
            }
        });
        area.add_space(8.0);
    }
    // With two rows of chips, both rows start where the wider name ends.
    let names = (studios.len() > 1).then(|| {
        let font = theme.font(theme.small_size + 1.0);
        let width = [tr!("start-studio"), tr!("start-type")]
            .into_iter()
            .map(|name| {
                area.painter()
                    .layout_no_wrap(name, font.clone(), theme.muted)
                    .size()
                    .x
            })
            .fold(0.0, f32::max);
        // As far from the chips as a lone name is: its gap and a spacing.
        width + 6.0 + area.spacing().item_spacing.x
    });
    let name = |ui: &mut Ui, text: String| {
        if let Some(width) = names {
            let galley =
                ui.painter()
                    .layout_no_wrap(text, theme.font(theme.small_size + 1.0), theme.muted);
            let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
            ui.painter().galley(rect.min, galley, theme.muted);
        } else {
            ui.label(
                egui::RichText::new(text)
                    .color(theme.muted)
                    .size(theme.small_size + 1.0),
            );
            ui.add_space(6.0);
        }
    };
    if studios.len() > 1 {
        area.horizontal(|ui| {
            name(ui, tr!("start-studio"));
            for studio in studios {
                let on = state.studios.contains(&studio.id);
                if page::chip(ui, &studio.name, on, theme).clicked() {
                    if on {
                        state.studios.remove(&studio.id);
                    } else {
                        state.studios.insert(studio.id.clone());
                    }
                }
            }
        });
        area.add_space(8.0);
    }
    area.horizontal(|ui| {
        name(ui, tr!("start-type"));
        for kind in Type::ALL {
            let on = state.types.contains(&(kind as u8));
            if page::chip(ui, &type_label(kind, studios), on, theme).clicked() {
                if on {
                    state.types.remove(&(kind as u8));
                } else {
                    state.types.insert(kind as u8);
                }
            }
        }
    });
    area.add_space(8.0);
    let shown = shown(&all, state, cx.work);
    state
        .chosen
        .retain(|key| all.iter().any(|item| &item.key == key));
    let chosen: Vec<Item> = shown
        .iter()
        .filter(|item| state.chosen.contains(&item.key))
        .cloned()
        .collect();
    toolbar_frame(&mut area, right.width(), theme, |ui| {
        if compact {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                view_buttons(ui, &mut state.view, false, theme);
                ui.add_space(6.0);
                sort_buttons(
                    ui,
                    "sue-paint-start-project-sort",
                    &mut state.sort,
                    &mut state.reversed,
                    &[tr!("start-date-modified"), tr!("start-name")],
                    theme,
                );
            });
            return;
        }
        let mark = match chosen.len() {
            0 => Chosen::None,
            n if n == shown.len() => Chosen::All,
            _ => Chosen::Some,
        };
        if page::check(ui, &tr!("start-select-all"), mark, theme).clicked() {
            if mark == Chosen::All {
                state.chosen.clear();
            } else {
                state
                    .chosen
                    .extend(shown.iter().map(|item| item.key.clone()));
            }
        }
        ui.add_space(8.0);
        let some = !chosen.is_empty();
        let move_to = page::action(ui, Glyph::Move, &tr!("start-move-to"), some, false, theme);
        egui::Popup::menu(&move_to).show(|ui| {
            for label in cx.work.labels() {
                if ui.button(label).clicked() {
                    for item in &chosen {
                        cx.request(ShellRequest::Labels(LabelEdit::Mark {
                            path: item.path.clone(),
                            label: label.clone(),
                            on: true,
                        }));
                    }
                }
            }
            if ui.button(tr!("start-favorite")).clicked() {
                for item in &chosen {
                    cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                        path: item.path.clone(),
                        on: true,
                    }));
                }
            }
        });
        if let Shelf::Label(label) = &state.shelf
            && page::action(
                ui,
                Glyph::Tag,
                &tr!("start-take-off-label"),
                some,
                false,
                theme,
            )
            .clicked()
        {
            for item in &chosen {
                cx.request(ShellRequest::Labels(LabelEdit::Mark {
                    path: item.path.clone(),
                    label: label.clone(),
                    on: false,
                }));
            }
        }
        let text = if state.armed {
            tr!("start-really-remove")
        } else {
            tr!("start-remove-from-list")
        };
        let remove = page::action(ui, Glyph::Delete, &text, some, state.armed, theme)
            .on_hover_text(tr!("start-remove-hint"));
        if remove.clicked() {
            if state.armed {
                for item in &chosen {
                    forget(item, cx);
                }
                state.chosen.clear();
                state.armed = false;
            } else {
                state.armed = true;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            view_buttons(ui, &mut state.view, false, theme);
            ui.add_space(6.0);
            sort_buttons(
                ui,
                "sue-paint-start-project-sort",
                &mut state.sort,
                &mut state.reversed,
                &[tr!("start-date-modified"), tr!("start-name")],
                theme,
            );
        });
    });
    area.add_space(10.0);

    // The cards.
    if shown.is_empty() {
        let message = if all.is_empty() {
            tr!("start-no-projects")
        } else {
            tr!("start-none-match")
        };
        area.add_space(40.0);
        area.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(message)
                    .color(theme.muted)
                    .size(theme.ui_size),
            );
        });
        return events;
    }
    let pictures: Vec<Option<(egui::TextureId, [u32; 2])>> = shown
        .iter()
        .map(|item| {
            let studio = item.studio.as_ref();
            thumbs.get_for(&item.path, studio).and_then(|(at, thumb)| {
                let key = texture_key(&item.path.display().to_string());
                cx.textures
                    .texture(key, at.max(1), thumb.size, 4, &thumb.pixels, false)
                    .map(|texture| (texture, thumb.size))
            })
        })
        .collect();
    let mut clicks = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt("sue-paint-start-projects")
        .auto_shrink([false, false])
        .show(&mut area, |ui| {
            let shape = Shape {
                view: state.view,
                tick: !compact,
                armed: options.discard_armed.clone(),
            };
            let drawn = cards(ui, &shown, &pictures, studios, &shape, state, theme);
            for (index, click, response) in drawn {
                let item = &shown[index];
                options
                    .drawn
                    .push((Spot::Card(item.key.clone()), response.rect));
                if let ProjectKey::Kept(id) = &item.key {
                    let [recover, discard] = kept_buttons(ui, &response, state.view, theme);
                    options.drawn.push((Spot::Recover(id.clone()), recover));
                    options.drawn.push((Spot::Discard(id.clone()), discard));
                }
                if let Some(click) = click {
                    clicks.push((index, click));
                }
                response.context_menu(|ui| {
                    if ui.button(tr!("start-open")).clicked() {
                        clicks.push((index, Click::Open));
                        ui.close();
                    }
                    if ui.button(tr!("start-show-in-folder")).clicked() {
                        cx.request(ShellRequest::Command(format!(
                            "reveal-file:{}",
                            item.path.display()
                        )));
                        ui.close();
                    }
                    if ui.button(tr!("start-details")).clicked() {
                        clicks.push((index, Click::Details));
                        ui.close();
                    }
                });
            }
        });
    for (index, click) in clicks {
        let Some(item) = shown.get(index) else {
            continue;
        };
        match click {
            Click::Open | Click::Recover => {
                options.discard_armed = None;
                events.push(Event::Open(item.clone()));
            }
            Click::Choose => {
                if !state.chosen.remove(&item.key) {
                    state.chosen.insert(item.key.clone());
                }
                state.armed = false;
            }
            Click::Favorite => cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                path: item.path.clone(),
                on: !item.favorite,
            })),
            Click::Details => events.push(Event::Details(item.key.clone())),
            Click::Discard => {
                if let ProjectKey::Kept(id) = &item.key {
                    if options.discard_armed.as_deref() == Some(id.as_str()) {
                        options.discard_armed = None;
                        forget(item, cx);
                    } else {
                        options.discard_armed = Some(id.clone());
                    }
                }
            }
        }
    }
    events
}

/// What a click on a card of [`cards`] asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    /// Open it (a click on the card).
    Open,
    /// Tick it, or take its tick away.
    Choose,
    /// Make it a favorite, or not.
    Favorite,
    /// Say what it is.
    Details,
    /// Bring kept work back.
    Recover,
    /// Throw kept work away (asks once more first).
    Discard,
}

/// How [`cards`] draws.
struct Shape {
    view: View,
    /// Cards carry a tick box for the toolbar's actions.
    tick: bool,
    /// The kept work whose Discard asks again.
    armed: Option<String>,
}

/// Lays the cards of `shown` out as `shape` says, in the width there is:
/// thumbnail cards that fill the rows evenly, or list rows. Each card's
/// text ends before its buttons. Returns every card's click, if any, and
/// its response.
fn cards(
    ui: &mut Ui,
    shown: &[Item],
    pictures: &[Option<(egui::TextureId, [u32; 2])>],
    studios: &[Studio<'_>],
    shape: &Shape,
    state: &ProjectsState,
    theme: &Theme,
) -> Vec<(usize, Option<Click>, egui::Response)> {
    let now = unix_now();
    let density = Density::of(ui);
    let width = ui.available_width();
    let gap = theme.space(Space::S5);
    let (favorite, details, select) = (
        tr!("start-favorite"),
        tr!("start-details"),
        tr!("start-select-all"),
    );
    let (recover, discard, really) = (
        tr!("recover"),
        tr!("start-discard"),
        tr!("start-really-discard"),
    );
    let mut drawn = Vec::new();
    let mut one = |ui: &mut Ui, index: usize, size: Option<Vec2>| {
        let item = &shown[index];
        let detail = format!("{}  {}", kind_label(item, studios), ago(item.when, now));
        let chosen = state.chosen.contains(&item.key);
        let actions = match &item.key {
            ProjectKey::Kept(id) => {
                let armed = shape.armed.as_deref() == Some(id.as_str());
                vec![
                    RowAction::new(Icon::Recover, &recover),
                    RowAction::new(Icon::Delete, if armed { &really } else { &discard }).on(armed),
                ]
            }
            ProjectKey::File(_) => vec![
                RowAction::new(Icon::Heart, &favorite).on(item.favorite),
                RowAction::new(Icon::Details, &details),
            ],
        };
        let picture = pictures.get(index).copied().flatten();
        let (response, ticked, action) = match size {
            None => {
                let leading = picture
                    .map_or(Leading::Icon(item.kind.glyph().icon()), |(texture, _)| {
                        Leading::Picture(texture)
                    });
                let row = ListRow::new(("sue-files-row", &item.key), &item.title)
                    .detail(&detail)
                    .leading(leading)
                    .actions(&actions)
                    .selected(chosen)
                    .width(width)
                    .show(ui, theme);
                (row.response, false, row.action)
            }
            Some(size) => {
                let thumbnail = picture.map_or(
                    Thumbnail::Placeholder(item.kind.glyph().icon()),
                    |(texture, size)| Thumbnail::Picture(texture, size),
                );
                let mut card = ThumbnailCard::new(("sue-files-card", &item.key), &item.title)
                    .detail(&detail)
                    .thumbnail(thumbnail)
                    .actions(&actions)
                    .selected(chosen)
                    .size(size);
                if shape.tick {
                    card = card.tick(chosen, &select);
                }
                let out = card.show(ui, theme);
                (out.response, out.ticked, out.action)
            }
        };
        let hover = item.path.display().to_string();
        let kept = matches!(item.key, ProjectKey::Kept(_));
        let click = if ticked {
            Some(Click::Choose)
        } else if let Some(action) = action {
            Some(match (kept, action) {
                (true, 0) => Click::Recover,
                (true, _) => Click::Discard,
                (false, 0) => Click::Favorite,
                (false, _) => Click::Details,
            })
        } else if response.clicked() {
            Some(
                if response
                    .ctx
                    .input(|input| input.modifiers.command || input.modifiers.shift)
                {
                    Click::Choose
                } else {
                    Click::Open
                },
            )
        } else {
            None
        };
        let response = response.on_hover_text(hover);
        drawn.push((index, click, response));
    };
    if shape.view == View::List {
        ui.spacing_mut().item_spacing.y = theme.space(Space::S1);
        for index in 0..shown.len() {
            one(ui, index, None);
        }
        return drawn;
    }
    // As many columns as cards of the view's smallest width fit, each
    // stretched so that the rows end flush.
    let least = if shape.view == View::Large {
        theme.space(Space::S9) * 4.5
    } else {
        theme.space(Space::S9) * 3.0
    };
    let columns = ((width + gap) / (least + gap)).floor().max(1.0) as usize;
    let card_width = ((width - gap * (columns - 1) as f32) / columns as f32).floor();
    let footer = ListRow::height(theme, density, true);
    let size = vec2(card_width, (card_width * 0.75).round() + footer);
    let mut index = 0;
    while index < shown.len() {
        let end = (index + columns).min(shown.len());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for at in index..end {
                one(ui, at, Some(size));
            }
        });
        ui.add_space(gap);
        index = end;
    }
    drawn
}

/// Where a kept card's two buttons are, Recover then Discard: the
/// actions of a thumbnail card's footer, or of a list row's end, which
/// lie from the right edge leftward in squares of the density's least
/// hit.
fn kept_buttons(ui: &Ui, response: &egui::Response, view: View, theme: &Theme) -> [Rect; 2] {
    let density = Density::of(ui);
    let rect = response.rect;
    let band = ListRow::height(theme, density, true).min(rect.height());
    let middle = if view == View::List {
        rect.center().y
    } else {
        rect.bottom() - band / 2.0
    };
    let side = density.min_hit().min(band);
    let right = rect.right() - theme.space(Space::S2);
    let at = |from_right: f32| {
        Rect::from_center_size(
            pos2(right - side * from_right - side / 2.0, middle),
            Vec2::splat(side),
        )
    };
    [at(1.0), at(0.0)]
}

/// What a project is, with what can be done to it, as a popup (or a
/// sheet over the whole screen when `full`). `armed` is `true` once
/// Remove from List (Discard, for kept work) was clicked and asks again.
/// Returns how the popup ended, and `true` when Open was chosen.
#[allow(clippy::too_many_arguments)]
pub fn project_popup(
    ctx: &egui::Context,
    full: bool,
    item: &Item,
    thumbs: &mut Thumbs,
    armed: &mut bool,
    studios: &[Studio<'_>],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> (PopupOutcome, bool) {
    let now = unix_now();
    let mut close = false;
    let mut open = false;
    let studio = item.studio.as_ref();
    let picture = thumbs.get_for(&item.path, studio).and_then(|(at, thumb)| {
        cx.textures
            .texture(
                texture_key(&item.path.display().to_string()),
                at.max(1),
                thumb.size,
                4,
                &thumb.pixels,
                false,
            )
            .map(|texture| (texture, thumb.size))
    });
    let was_armed = *armed;
    let mut arm = None;
    let outcome = show(
        full,
        ctx,
        "project",
        &item.title,
        vec2(480.0, 620.0),
        theme,
        |ui| {
            preview(ui, picture, item.kind.glyph(), theme);
            page::gap(ui);
            let rows = [
                Row::new(Some(Glyph::Draw), tr!("start-open"), Trailing::More),
                Row::new(
                    Some(Glyph::Folder),
                    tr!("start-show-in-folder"),
                    Trailing::External,
                ),
            ];
            match page::group(ui, &rows, theme) {
                Some(0) => {
                    open = true;
                    close = true;
                }
                Some(1) => cx.request(ShellRequest::Command(format!(
                    "reveal-file:{}",
                    item.path.display()
                ))),
                _ => {}
            }
            page::gap(ui);
            let mut rows = vec![Row::new(
                Some(Glyph::Heart),
                tr!("start-favorite"),
                Trailing::Switch(item.favorite),
            )];
            let labels = cx.work.labels().to_vec();
            rows.extend(labels.iter().map(|label| {
                Row::new(
                    Some(Glyph::Tag),
                    label.clone(),
                    Trailing::Switch(cx.work.has_label(&item.path, label)),
                )
            }));
            match page::group(ui, &rows, theme) {
                Some(0) => cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                    path: item.path.clone(),
                    on: !item.favorite,
                })),
                Some(index) => {
                    if let Some(label) = labels.get(index - 1) {
                        cx.request(ShellRequest::Labels(LabelEdit::Mark {
                            path: item.path.clone(),
                            label: label.clone(),
                            on: !cx.work.has_label(&item.path, label),
                        }));
                    }
                }
                None => {}
            }
            page::gap(ui);
            let rows = [
                Row::new(
                    None,
                    tr!("start-kind"),
                    Trailing::Text(kind_label(item, studios)),
                ),
                Row::new(
                    None,
                    tr!("start-last-worked-on"),
                    Trailing::Text(ago(item.when, now)),
                ),
                Row::new(None, tr!("start-where"), Trailing::None)
                    .detail(item.path.display().to_string()),
            ];
            page::group(ui, &rows, theme);
            page::gap(ui);
            let text = match (item.kind, was_armed) {
                (Type::Unsaved, false) => tr!("start-discard"),
                (Type::Unsaved, true) => tr!("start-really-discard"),
                (_, false) => tr!("start-remove-from-list"),
                (_, true) => tr!("start-really-remove"),
            };
            let rows = [Row::new(Some(Glyph::Delete), text, Trailing::None)
                .detail(tr!("start-remove-hint"))];
            if page::group(ui, &rows, theme).is_some() {
                if was_armed {
                    forget(item, cx);
                    close = true;
                } else {
                    arm = Some(true);
                }
            }
        },
    );
    if let Some(on) = arm {
        *armed = on;
    }
    let outcome = if close { PopupOutcome::Closed } else { outcome };
    (outcome, open)
}

/// The Data Transfers popup: what Sue brought in, wrote out and copied in
/// this run, still going or done.
pub fn transfers_popup(
    full: bool,
    ctx: &egui::Context,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> PopupOutcome {
    let now = unix_now();
    show(
        full,
        ctx,
        "transfers",
        &tr!("files-transfers"),
        vec2(760.0, 560.0),
        theme,
        |ui| {
            let finished = cx
                .transfers
                .list()
                .iter()
                .filter(|transfer| transfer.finished())
                .count();
            ui.horizontal(|ui| {
                if page::action(
                    ui,
                    Glyph::Delete,
                    &tr!("start-clear-completed"),
                    finished > 0,
                    false,
                    theme,
                )
                .clicked()
                {
                    cx.transfers.clear_completed();
                }
            });
            ui.add_space(12.0);
            if cx.transfers.list().is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("start-no-transfers"))
                            .color(theme.muted)
                            .size(theme.ui_size),
                    );
                });
                return;
            }
            let width = ui.available_width();
            let mut cancel = None;
            for transfer in cx.transfers.list() {
                let (rect, _) = ui.allocate_exact_size(vec2(width, 46.0), Sense::hover());
                let painter = ui.painter();
                painter.rect(
                    rect,
                    0.0,
                    theme.pill,
                    theme.hairline(),
                    egui::StrokeKind::Inside,
                );
                let glyph = match transfer.kind {
                    TransferKind::Import => Glyph::Folder,
                    TransferKind::Export => Glyph::External,
                    TransferKind::Copy => Glyph::Transfers,
                };
                page::glyph(
                    painter,
                    Rect::from_center_size(
                        pos2(rect.left() + 20.0, rect.center().y),
                        Vec2::splat(18.0),
                    ),
                    glyph,
                    theme.text,
                );
                painter.with_clip_rect(rect.shrink(1.0)).text(
                    pos2(rect.left() + 40.0, rect.top() + 15.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{}  {}", transfer.kind.label(), transfer.label),
                    theme.font(theme.ui_size),
                    theme.text,
                );
                let place = transfer
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                painter
                    .with_clip_rect(Rect::from_min_max(
                        rect.min,
                        pos2(rect.right() - 200.0, rect.bottom()),
                    ))
                    .text(
                        pos2(rect.left() + 40.0, rect.top() + 32.0),
                        egui::Align2::LEFT_CENTER,
                        place,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                let state = match &transfer.state {
                    TransferState::Running(_) => tr!("start-transfer-running"),
                    TransferState::Done => {
                        tr!("start-transfer-done", when = ago(transfer.started, now))
                    }
                    TransferState::Failed(why) => tr!("start-transfer-failed", why = why.clone()),
                    TransferState::Cancelled => tr!("start-transfer-cancelled"),
                };
                painter.text(
                    pos2(rect.right() - 14.0, rect.top() + 15.0),
                    egui::Align2::RIGHT_CENTER,
                    state,
                    theme.font(theme.small_size),
                    theme.muted,
                );
                if let TransferState::Running(fraction) = transfer.state {
                    let mut bar = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                        pos2(rect.right() - 190.0, rect.top() + 29.0),
                        vec2(140.0, 8.0),
                    )));
                    page::progress(&mut bar, 140.0, fraction, theme);
                    let stop = Rect::from_center_size(
                        pos2(rect.right() - 24.0, rect.top() + 32.0),
                        Vec2::splat(18.0),
                    );
                    if page::icon_button(ui, stop, Glyph::Close, &tr!("start-cancel"), false, theme)
                        .clicked()
                    {
                        cancel = Some(transfer.id);
                    }
                }
                ui.add_space(4.0);
            }
            if let Some(id) = cancel {
                cx.transfers.cancel(id);
            }
        },
    )
}

/// A toolbar's band, `width` wide.
pub fn toolbar_frame(ui: &mut Ui, width: f32, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    let height = Density::of(ui).min_hit() + theme.space(Space::S4);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, theme.radius(crate::ui::Radius::Lg), theme.panel);
    let mut band = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 5.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    band.spacing_mut().item_spacing.x = 4.0;
    add(&mut band);
}

/// The view toggles (`small` adds small cards), right to left.
pub fn view_buttons(ui: &mut Ui, view: &mut View, small: bool, theme: &Theme) {
    let mut views = vec![(View::List, Glyph::List, tr!("start-view-list"))];
    if small {
        views.push((View::Small, Glyph::GridSmall, tr!("start-view-small")));
    }
    views.push((View::Large, Glyph::Grid, tr!("start-view-large")));
    for (which, glyph, hint) in views {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
        if page::icon_button(ui, rect, glyph, &hint, *view == which, theme).clicked() {
            *view = which;
        }
    }
}

/// The sort dropdown and its direction, right to left.
pub fn sort_buttons(
    ui: &mut Ui,
    id: &str,
    sort: &mut usize,
    reversed: &mut bool,
    options: &[String],
    theme: &Theme,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
    let glyph = if *reversed {
        Glyph::SortUp
    } else {
        Glyph::SortDown
    };
    if page::icon_button(ui, rect, glyph, &tr!("start-sort-direction"), false, theme).clicked() {
        *reversed = !*reversed;
    }
    page::dropdown(ui, Id::unique(id), sort, options, 150.0, theme);
}

/// Lays `cards` out as `view` says in `width`, calling `each` with every
/// card's click, if any, and its response (for a context menu).
pub fn lay_out(
    ui: &mut Ui,
    width: f32,
    view: View,
    cards: &[Card],
    theme: &Theme,
    mut each: impl FnMut(usize, Option<CardClick>, &egui::Response),
) {
    let mut report = |index: usize, (click, response): (Option<CardClick>, egui::Response)| {
        each(index, click, &response);
    };
    match view {
        View::List => {
            ui.spacing_mut().item_spacing.y = 4.0;
            for (index, card) in cards.iter().enumerate() {
                let shown = page::list_row(ui, width - 4.0, card, theme);
                report(index, shown);
            }
        }
        View::Large | View::Small => {
            let size = if view == View::Large {
                vec2(150.0, 196.0)
            } else {
                vec2(100.0, 122.0)
            };
            let gap = 10.0;
            let columns = ((width + gap) / (size.x + gap)).floor().max(1.0) as usize;
            for (row, chunk) in cards.chunks(columns).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (column, card) in chunk.iter().enumerate() {
                        let shown = page::card(ui, size, card, theme);
                        report(row * columns + column, shown);
                    }
                });
                ui.add_space(gap);
            }
        }
    }
}

/// A popup in the middle of the page, or a sheet over the whole screen
/// (`full`, on phones and tablets).
pub fn show(
    full: bool,
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: Vec2,
    theme: &Theme,
    body: impl FnOnce(&mut Ui),
) -> PopupOutcome {
    if full {
        page::sheet(ctx, id, title, theme, body)
    } else {
        page::popup(ctx, id, title, size, theme, body)
    }
}

/// A large picture at the top of a popup.
pub fn preview(
    ui: &mut Ui,
    picture: Option<(egui::TextureId, [u32; 2])>,
    placeholder: Glyph,
    theme: &Theme,
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 180.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme.void);
    match picture {
        Some((texture, [w, h])) if w > 0 && h > 0 => {
            let inner = rect.shrink(8.0);
            let scale = (inner.width() / w as f32).min(inner.height() / h as f32);
            painter.image(
                texture,
                Rect::from_center_size(inner.center(), vec2(w as f32 * scale, h as f32 * scale)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        _ => page::glyph(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(56.0)),
            placeholder,
            theme.muted,
        ),
    }
    painter.rect_stroke(rect, 0.0, theme.hairline(), egui::StrokeKind::Inside);
}
