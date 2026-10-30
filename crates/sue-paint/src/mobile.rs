//! Sue Paint's start page on phones and tablets: a header with Help and
//! Settings in Sue's top bar, the Projects or Manage Materials page, and a
//! bottom bar with a round Draw button between them; and the New canvas
//! page, whose template cards come from the
//! [`crate::templates::CanvasTemplates`] registry. Popups come up as
//! sheets over the whole screen. The data is the desktop start page's:
//! the shared work list and the material library.

use egui::{Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_wm::start::{self as page, Card, CardClick, Glyph, Preview, RailItem};
use sue_wm::{AppletContext, AppletId, FilePick, LabelEdit, ShellRequest, Theme, ago, unix_now};

use crate::start::{Act, Item, Popup, ProjectsState, Shelf, StartPage, Tab, Type};
use crate::studio::SuePaint;
use crate::templates::{Make, Work};

/// The bottom bar's height.
const BAR: f32 = 68.0;
/// The movies an animation can start from.
const MOVIES: &[&str] = &["gif", "png", "apng", "webp", "avi"];

fn me() -> AppletId {
    AppletId::new(crate::ID).expect("a valid id")
}

impl SuePaint {
    /// Sue Paint's header in Sue's top bar while the start page shows on a
    /// phone or tablet: its name, and Help and Settings at the right.
    pub(crate) fn mobile_header(&mut self, ui: &mut Ui, bar: Rect, theme: &Theme) {
        let painter = ui.painter();
        page::glyph(
            painter,
            Rect::from_center_size(pos2(bar.left() + 20.0, bar.center().y), Vec2::splat(20.0)),
            Glyph::Draw,
            theme.text,
        );
        painter.text(
            pos2(bar.left() + 38.0, bar.center().y),
            egui::Align2::LEFT_CENTER,
            tr!("applet-name"),
            theme.font(theme.ui_size * 1.25),
            theme.text,
        );
        let side = 44.0_f32.min(bar.height());
        for (index, (glyph, hint, popup)) in [
            (Glyph::Settings, tr!("start-settings"), Popup::Settings),
            (Glyph::Help, tr!("start-help"), Popup::Help),
        ]
        .into_iter()
        .enumerate()
        {
            let right = bar.right() - 4.0 - index as f32 * (side + 4.0);
            let rect = Rect::from_min_size(
                pos2(right - side, bar.center().y - side / 2.0),
                Vec2::splat(side),
            );
            let on = self.start.popup.as_ref() == Some(&popup);
            if page::icon_button(ui, rect, glyph, &hint, on, theme).clicked() {
                self.start.popup = Some(popup);
            }
        }
    }

    /// The start page on a phone or tablet, over `rect`.
    pub(crate) fn mobile_start_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut start = std::mem::take(&mut self.start);
        start.full = true;
        let pixels = ui.ctx().content_rect().size() * ui.ctx().pixels_per_point();
        start.screen = [pixels.x.round() as u32, pixels.y.round() as u32];
        let mut acts = Vec::new();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt("sue-paint-mobile-start")
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect);
        child.multiply_opacity(self.cover.max(0.05));
        if !self.ready {
            child.disable();
        }
        child.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(category) = start.new_canvas {
            self.new_canvas_ui(&mut child, rect, &mut start, &mut acts, category, cx, theme);
        } else {
            let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - BAR), rect.max);
            let body = Rect::from_min_max(rect.min, pos2(rect.right(), bar.top()));
            // Phones show Projects for Browse Assets (it comes to them
            // later).
            match start.tab {
                Tab::Browse | Tab::Projects => {
                    self.mobile_projects(&mut child, body, &mut start, &mut acts, cx, theme)
                }
                Tab::Materials => {
                    self.mobile_materials(&mut child, body, &mut start, &mut acts, cx, theme)
                }
            }
            let items = [
                RailItem {
                    glyph: Glyph::Projects,
                    label: tr!("start-projects"),
                    round: false,
                },
                RailItem {
                    glyph: Glyph::Draw,
                    label: tr!("start-draw"),
                    round: true,
                },
                RailItem {
                    glyph: Glyph::Materials,
                    label: tr!("start-manage-materials"),
                    round: false,
                },
            ];
            let selected = Some(match start.tab {
                Tab::Browse | Tab::Projects => 0,
                Tab::Materials => 2,
            });
            match page::tab_bar(&mut child, bar, &items, selected, theme) {
                Some(0) => start.tab = Tab::Projects,
                Some(1) => acts.push(Act::Draw),
                Some(2) => start.tab = Tab::Materials,
                _ => {}
            }
        }
        if start.thumbs.poll() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.ready {
            self.popup_ui(ui.ctx(), &mut start, &mut acts, cx, theme);
        }
        self.start = start;
        for act in acts {
            self.start_act(act, cx);
        }
    }

    /// The projects page: Create new file, New from device storage and
    /// Open file, then the projects, sorted by label.
    fn mobile_projects(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let all = crate::start::items(cx.work);
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(body.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(body);
        let mut clicks = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-mobile-projects")
            .auto_shrink([false, false])
            .show(&mut area, |ui| {
                let width = ui.available_width();
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                let starts = [
                    (Glyph::Add, tr!("start-create-new-file"), 0),
                    (Glyph::Picture, tr!("start-new-from-device"), 1),
                    (Glyph::Folder, tr!("start-open-file"), 2),
                ];
                // Side by side on a tablet, one under the other on a phone.
                let wide = width > 640.0;
                let size = if wide {
                    vec2((width - 20.0) / 3.0, 110.0)
                } else {
                    vec2(width, 84.0)
                };
                let mut chosen = None;
                let mut row = |ui: &mut Ui| {
                    for (glyph, label, index) in &starts {
                        if page::big_row(ui, size, *glyph, label, theme).clicked() {
                            chosen = Some(*index);
                        }
                    }
                };
                if wide {
                    ui.horizontal(|ui| row(ui));
                } else {
                    row(ui);
                }
                match chosen {
                    Some(0) => start.new_canvas = Some(0),
                    Some(1) => acts.push(Act::ImportImage),
                    Some(2) => acts.push(Act::OpenFile),
                    _ => {}
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(tr!("start-your-projects"))
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                // The labels as chips that scroll sideways.
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
                egui::ScrollArea::horizontal()
                    .id_salt("sue-paint-mobile-shelves")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (shelf, name) in shelves {
                                let on = start.projects.shelf == shelf;
                                if page::chip(ui, &name, on, theme).clicked() {
                                    start.projects.shelf = shelf;
                                }
                            }
                        });
                    });
                let state = ProjectsState::on_shelf(start.projects.shelf.clone());
                let shown = crate::start::shown(&all, &state, cx.work);
                if shown.is_empty() {
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new(tr!("start-no-projects"))
                            .color(theme.muted)
                            .size(theme.ui_size),
                    );
                    return;
                }
                let columns = ((width + 10.0) / 170.0).floor().max(2.0) as usize;
                let side = (width - 10.0 * (columns as f32 - 1.0)) / columns as f32;
                let now = unix_now();
                for (row_index, chunk) in shown.chunks(columns).enumerate() {
                    ui.horizontal(|ui| {
                        for (column, item) in chunk.iter().enumerate() {
                            let card = project_card(start, item, now, cx);
                            let (click, response) =
                                page::card(ui, vec2(side, side * 1.2), &card, theme);
                            if let Some(click) = click {
                                clicks.push((row_index * columns + column, click));
                            }
                            let _ = response;
                        }
                    });
                }
                start.projects.chosen.clear();
                clicks.retain(|(index, _)| *index < shown.len());
                for (index, click) in std::mem::take(&mut clicks) {
                    let item = shown[index].clone();
                    match click {
                        CardClick::Open => acts.push(Act::Open(item)),
                        CardClick::Favorite => {
                            cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                                path: item.path.clone(),
                                on: !item.favorite,
                            }))
                        }
                        CardClick::Details | CardClick::Choose => {
                            start.popup = Some(Popup::Project(item.key.clone()));
                        }
                    }
                }
            });
    }

    /// The materials page: kinds as chips, search, and the materials.
    fn mobile_materials(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let kinds = self.material_kinds.clone();
        let entries = start.library(&kinds, folder).entries();
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(body.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(body);
        let width = body.width() - 24.0;
        let state = &mut start.materials;
        page::search_box(
            &mut area,
            &mut state.search,
            &tr!("start-search-materials"),
            width,
            theme,
        );
        area.add_space(8.0);
        let mut nodes = vec![(crate::start::Node::All, tr!("start-all"))];
        nodes.extend(kinds.iter().map(|kind| {
            (
                crate::start::Node::Kind(kind.id.clone()),
                crate::localize(&kind.name),
            )
        }));
        nodes.push((crate::start::Node::Favorite, tr!("start-favorite")));
        nodes.push((crate::start::Node::Recent, tr!("start-recently-used")));
        egui::ScrollArea::horizontal()
            .id_salt("sue-paint-mobile-kinds")
            .show(&mut area, |ui| {
                ui.horizontal(|ui| {
                    for (node, name) in nodes {
                        let on = state.node == node;
                        if page::chip(ui, &name, on, theme).clicked() {
                            state.node = node;
                        }
                    }
                });
            });
        area.add_space(8.0);
        let needle = state.search.trim().to_lowercase();
        let shown: Vec<&crate::materials::Entry> = entries
            .iter()
            .filter(|entry| match &state.node {
                crate::start::Node::All => true,
                crate::start::Node::Kind(kind) => entry.material.kind == *kind,
                crate::start::Node::Favorite => entry.favorite,
                crate::start::Node::Recent => entry.used > 0,
            })
            .filter(|entry| {
                needle.is_empty()
                    || crate::localize(&entry.material.name)
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect();
        if shown.is_empty() {
            area.label(
                egui::RichText::new(tr!("start-no-materials"))
                    .color(theme.muted)
                    .size(theme.ui_size),
            );
            return;
        }
        let canvas = !self.docs.is_empty();
        let columns = ((width + 8.0) / 116.0).floor().max(3.0) as usize;
        let side = (width - 8.0 * (columns as f32 - 1.0)) / columns as f32;
        let mut tapped = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-mobile-materials")
            .auto_shrink([false, false])
            .show(&mut area, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for chunk in shown.chunks(columns) {
                    ui.horizontal(|ui| {
                        for entry in chunk {
                            let thumb_side = 128_u32;
                            let key = crate::start::texture_key(&entry.material.key());
                            let thumb = self.material_thumbs.entry(key).or_insert_with(|| {
                                crate::panels::material_thumb(&entry.material, thumb_side)
                            });
                            let picture = cx
                                .textures
                                .texture(key, 1, [thumb_side; 2], 4, thumb, false)
                                .map(|texture| (texture, [thumb_side; 2]));
                            let card = Card {
                                title: crate::localize(&entry.material.name),
                                detail: String::new(),
                                picture,
                                placeholder: Glyph::Materials,
                                favorite: Some(entry.favorite),
                                chosen: false,
                                hover: String::new(),
                            };
                            let (click, _) = page::card(ui, vec2(side, side + 22.0), &card, theme);
                            if click.is_some() {
                                tapped = Some(entry.id);
                            }
                        }
                    });
                }
            });
        if let Some(id) = tapped {
            if canvas {
                acts.push(Act::Paste(id));
            } else {
                start.popup = Some(Popup::Material(id));
            }
        }
    }

    /// The New canvas page: the categories as tabs, their templates in a
    /// row that scrolls sideways, then the projects to continue.
    #[allow(clippy::too_many_arguments, reason = "the page and what it reads")]
    fn new_canvas_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        start: &mut StartPage,
        acts: &mut Vec<Act>,
        category: usize,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let side = ((rect.width() - 24.0) / 3.3).clamp(96.0, 150.0);
        let mut area = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(rect);
        let width = rect.width() - 24.0;
        area.spacing_mut().item_spacing.y = 10.0;
        // The back arrow, the title and Advanced Settings.
        area.horizontal(|ui| {
            let (back, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
            if page::icon_button(ui, back, Glyph::Back, &tr!("start-back"), false, theme).clicked()
            {
                start.new_canvas = None;
            }
        });
        area.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("start-new-canvas"))
                    .size(theme.heading_size * 1.2)
                    .color(theme.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page::outline_button(ui, None, &tr!("start-advanced-settings"), theme).clicked()
                {
                    acts.push(Act::Advanced(category));
                }
            });
        });
        let categories: Vec<(String, String)> = self.canvas_templates.categories().to_vec();
        let labels: Vec<String> = categories
            .iter()
            .map(|(_, name)| crate::localize(name))
            .collect();
        if let Some(tab) = page::tabs(&mut area, width, &labels, category, theme) {
            start.new_canvas = Some(tab);
        }
        let screen = start.screen;
        if let Some((id, _)) = categories.get(category) {
            let templates: Vec<crate::templates::CanvasTemplate> =
                self.canvas_templates.of(id).cloned().collect();
            egui::ScrollArea::horizontal()
                .id_salt(("sue-paint-templates", category))
                .show(&mut area, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for template in &templates {
                            let preview = match template.make {
                                Make::FromImage => Preview::Glyph(Glyph::Picture),
                                Make::FromMovie => Preview::Glyph(Glyph::Film),
                                Make::Custom => Preview::Glyph(Glyph::Add),
                                Make::Size { .. } | Make::Screen => {
                                    let picture = template.picture(screen);
                                    Preview::Paper(picture.map_or(1.0, |picture| {
                                        picture.width as f32 / picture.height.max(1) as f32
                                    }))
                                }
                            };
                            let name = if template.name.is_empty() {
                                template.size_text(screen)
                            } else {
                                crate::localize(&template.name)
                            };
                            let detail = if template.name.is_empty() {
                                String::new()
                            } else {
                                template.size_text(screen)
                            };
                            if page::tile(ui, side, preview, &name, &detail, theme).clicked() {
                                acts.push(Act::Template(template.id.clone()));
                            }
                        }
                    });
                });
        }
        area.add_space(10.0);
        // Continue drawing: the projects of late, and Import.
        area.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("start-continue-drawing"))
                    .size(theme.heading_size)
                    .color(theme.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page::outline_button(ui, None, &tr!("start-import"), theme).clicked() {
                    acts.push(Act::ImportImage);
                }
            });
        });
        let mut recent = crate::start::items(cx.work);
        recent.sort_by_key(|item| std::cmp::Reverse(item.when));
        let now = unix_now();
        if recent.is_empty() {
            area.label(
                egui::RichText::new(tr!("start-nothing-to-continue"))
                    .color(theme.muted)
                    .size(theme.ui_size),
            );
        } else {
            egui::ScrollArea::horizontal()
                .id_salt("sue-paint-continue")
                .show(&mut area, |ui| {
                    ui.horizontal(|ui| {
                        for item in recent.iter().take(20) {
                            let card = project_card(start, item, now, cx);
                            let (click, _) = page::card(ui, vec2(side, side * 1.25), &card, theme);
                            match click {
                                Some(CardClick::Open) => acts.push(Act::Open(item.clone())),
                                Some(CardClick::Favorite) => {
                                    cx.request(ShellRequest::Labels(LabelEdit::Favorite {
                                        path: item.path.clone(),
                                        on: !item.favorite,
                                    }));
                                }
                                Some(_) => start.popup = Some(Popup::Project(item.key.clone())),
                                None => {}
                            }
                        }
                    });
                });
        }
        area.add_space(16.0);
        area.vertical_centered(|ui| {
            if page::outline_button(
                ui,
                Some(Glyph::Projects),
                &tr!("start-see-list-of-projects"),
                theme,
            )
            .clicked()
            {
                start.new_canvas = None;
                start.tab = Tab::Projects;
            }
        });
    }

    /// Makes a canvas from the template `id`, or asks for what it needs.
    pub(crate) fn make_from_template(&mut self, id: &str, cx: &mut AppletContext<'_>) {
        let Some(template) = self.canvas_templates.get(id).cloned() else {
            return;
        };
        match template.make {
            Make::FromImage => self.pick_image(cx, false),
            Make::FromMovie => cx.request(ShellRequest::PickFile(
                FilePick::open(Some(me()), "new-from-movie", &tr!("import-movie-2"))
                    .extensions(MOVIES),
            )),
            Make::Custom => {
                let category = self
                    .canvas_templates
                    .categories()
                    .iter()
                    .position(|(id, _)| *id == template.category)
                    .unwrap_or(0);
                self.advanced_new(category, cx);
            }
            Make::Size { .. } | Make::Screen => {
                let Some(spec) = template.picture(self.start.screen) else {
                    return;
                };
                let defaults = crate::dialogs::NewDialog::new(&spec);
                match template.work {
                    Work::Illustration => self.make_picture(cx, &spec),
                    Work::Comic => {
                        self.make_story(cx, &spec, defaults.pages as usize, defaults.right_bound)
                    }
                    Work::Animation => {
                        self.make_animation(cx, &spec, defaults.frames, defaults.fps);
                    }
                }
                self.start.new_canvas = None;
                self.start.forced = false;
            }
        }
    }

    /// The full New dialog, set for the work of `category`.
    pub(crate) fn advanced_new(&mut self, category: usize, cx: &mut AppletContext<'_>) {
        let mut dialog = Self::new_dialog(cx);
        let work = self
            .canvas_templates
            .categories()
            .get(category)
            .and_then(|(id, _)| self.canvas_templates.of(id).next())
            .map_or(Work::Illustration, |template| template.work);
        dialog.project = match work {
            Work::Illustration => crate::dialogs::Project::Illustration,
            Work::Comic => crate::dialogs::Project::Comic,
            Work::Animation => crate::dialogs::Project::Animation,
        };
        self.dialog = Some(crate::dialogs::Dialog::New(dialog));
    }

    /// A movie picked on the New canvas page becomes a new animation.
    pub(crate) fn new_from_movie(&mut self, path: &std::path::Path, cx: &mut AppletContext<'_>) {
        let spec = crate::doc::NewPicture {
            name: path.file_stem().map_or_else(
                || tr!("animation"),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            width: 1920,
            height: 1080,
            ..crate::doc::NewPicture::default()
        };
        let defaults = crate::dialogs::NewDialog::new(&spec);
        self.make_animation(cx, &spec, defaults.frames, defaults.fps);
        self.import_file(cx, "import-movie", path);
        self.start.new_canvas = None;
        self.start.forced = false;
    }
}

/// A project's card, with its thumbnail once made.
fn project_card(start: &mut StartPage, item: &Item, now: u64, cx: &mut AppletContext<'_>) -> Card {
    let picture = start.thumbs.get(&item.path).and_then(|(at, thumb)| {
        let key = crate::start::texture_key(&item.path.display().to_string());
        cx.textures
            .texture(key, at.max(1), thumb.size, 4, &thumb.pixels, false)
            .map(|texture| (texture, thumb.size))
    });
    Card {
        title: item.title.clone(),
        detail: ago(item.when, now),
        picture,
        placeholder: match item.kind {
            Type::Picture => Glyph::Picture,
            Type::Project => Glyph::Projects,
            Type::Unsaved => Glyph::Clock,
        },
        favorite: Some(item.favorite),
        chosen: false,
        hover: String::new(),
    }
}
