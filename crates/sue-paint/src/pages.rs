//! Sue Paint's side of stories (Clip Studio Paint's Story menu): making a
//! comic of pages, stepping through them, the Page Manager with its spreads,
//! and exporting every page.

use std::path::Path;

use egui::{Rect, RichText, Sense, Ui, pos2, vec2};
use sue_core::EntityId;
use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest, Theme};

use crate::composite::{self, RenderOptions};
use crate::dialogs::{Outcome, PagesDialog};
use crate::doc::{self, NewPicture};
use crate::story;
use crate::studio::{SuePaint, texture_key};

impl SuePaint {
    /// The current picture's story, its pages and where the picture is
    /// among them.
    pub(crate) fn story_here(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(EntityId, Vec<EntityId>, usize)> {
        let picture = self.picture()?;
        let story = story::story_of(arena, picture)?;
        let pages = story::pages(arena, story);
        let index = pages.iter().position(|page| *page == picture)?;
        Some((story, pages, index))
    }

    /// Makes a comic of `count` pages like `spec` and opens its first page.
    pub(crate) fn make_story(
        &mut self,
        cx: &mut AppletContext<'_>,
        spec: &NewPicture,
        count: usize,
        right_bound: bool,
    ) {
        match story::create(spec, count, &spec.name, right_bound) {
            Ok((edit, _, pages)) => {
                // Only the first page opens; the rest wait in the Page Manager.
                self.closed.extend(pages.iter().skip(1).copied());
                if Self::apply(cx, edit)
                    && let Some(first) = pages.first()
                {
                    self.activate(&cx.world.arena, *first);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// The current picture's story, making it the first page of a new one
    /// if it has none.
    fn ensure_story(&mut self, cx: &mut AppletContext<'_>) -> Option<EntityId> {
        let picture = self.picture()?;
        if let Some(story) = story::story_of(&cx.world.arena, picture) {
            return Some(story);
        }
        let (edit, story) = story::begin(&cx.world.arena, picture);
        Self::apply(cx, edit).then_some(story)
    }

    /// Shows `page` in place of the current page's tab (other pictures keep
    /// theirs).
    pub(crate) fn go_to_page(&mut self, cx: &mut AppletContext<'_>, page: EntityId) {
        if let Some(current) = self.picture()
            && current != page
            && story::story_of(&cx.world.arena, current).is_some()
            && story::story_of(&cx.world.arena, current) == story::story_of(&cx.world.arena, page)
        {
            self.closed.insert(current);
            self.docs.retain(|doc| doc.picture != current);
        }
        self.activate(&cx.world.arena, page);
    }

    /// Runs a Story menu command; `false` if `command` is not one.
    pub(crate) fn story_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        match command {
            "story.manager" | "story.order" | "story.open_page" | "story.work_settings" => {
                let chosen = self
                    .story_here(&cx.world.arena)
                    .map_or(0, |(_, _, index)| index);
                self.dialog = Some(crate::dialogs::Dialog::Pages(PagesDialog { chosen }));
            }
            "story.first" | "story.previous" | "story.next" | "story.last" => {
                let Some((_, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.notify(tr!("canvas-one-picture-story-add-page"));
                    return true;
                };
                let to = match command {
                    "story.first" => 0,
                    "story.previous" => index.saturating_sub(1),
                    "story.next" => (index + 1).min(pages.len() - 1),
                    _ => pages.len() - 1,
                };
                if to != index {
                    self.go_to_page(cx, pages[to]);
                }
            }
            "story.add" | "story.insert" => {
                let Some(picture) = self.picture() else {
                    return true;
                };
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                let index = story::pages(&cx.world.arena, story)
                    .iter()
                    .position(|page| *page == picture)
                    .unwrap_or(0);
                let at = if command == "story.add" {
                    index + 1
                } else {
                    index
                };
                match story::add_page(&cx.world.arena, story, picture, at) {
                    Ok((edit, page)) => {
                        self.closed.insert(page);
                        if Self::apply(cx, edit) {
                            self.go_to_page(cx, page);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.duplicate_page" => {
                let Some(picture) = self.picture() else {
                    return true;
                };
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                match story::duplicate_page(&cx.world.arena, story, picture) {
                    Ok((edit, page)) => {
                        self.closed.insert(page);
                        if Self::apply(cx, edit) {
                            self.go_to_page(cx, page);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.delete" => {
                let Some((_, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("delete-page-works-storys-pages"));
                    return true;
                };
                if pages.len() < 2 {
                    cx.fail(tr!("story-keeps-least-one-page"));
                    return true;
                }
                let next = if index + 1 < pages.len() {
                    pages[index + 1]
                } else {
                    pages[index - 1]
                };
                let edit = story::delete_page(&cx.world.arena, pages[index]);
                if Self::apply(cx, edit) {
                    self.closed.remove(&next);
                    self.activate(&cx.world.arena, next);
                }
            }
            "story.combine" => {
                let Some((story, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("combine-pages-works-storys-pages"));
                    return true;
                };
                let arena = &cx.world.arena;
                let right_bound = story::bound_right(arena, story);
                // The page it faces in the spread.
                let partner = story::spreads(pages.len(), right_bound)
                    .into_iter()
                    .find(|pair| pair.contains(&Some(index)))
                    .and_then(|pair| pair.into_iter().flatten().find(|other| *other != index));
                let Some(partner) = partner else {
                    cx.fail(tr!("page-faces-no-other-page-add"));
                    return true;
                };
                let (first, second) = (index.min(partner), index.max(partner));
                // Read right to left, the later page is on the left.
                let (left, right) = if right_bound {
                    (pages[second], pages[first])
                } else {
                    (pages[first], pages[second])
                };
                match story::combine(arena, story, left, right) {
                    Ok(edit) => {
                        if Self::apply(cx, edit) {
                            self.closed.remove(&left);
                            self.go_to_page(cx, left);
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.separate" => {
                let Some((story, pages, index)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("separate-pages-works-storys-spread"));
                    return true;
                };
                let page = pages[index];
                let spread = cx
                    .world
                    .arena
                    .get_by_id(page)
                    .ok()
                    .and_then(|entity| entity.property(story::SPREAD))
                    .and_then(sue_core::Value::as_bool)
                    == Some(true);
                if !spread {
                    cx.fail(tr!("page-not-spread-combine-pages-makes"));
                    return true;
                }
                match story::separate(&cx.world.arena, story, page) {
                    Ok((edit, new_page)) => {
                        self.closed.insert(new_page);
                        Self::apply(cx, edit);
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "story.page_settings" => {
                let Some((story, _, _)) = self.story_here(&cx.world.arena) else {
                    cx.fail(tr!("change-page-settings-works-story"));
                    return true;
                };
                let Some(picture) = self.picture() else {
                    return true;
                };
                let entity = cx.world.arena.get_by_id(picture).ok();
                let number = |key: &str, default: f64| {
                    entity
                        .and_then(|entity| entity.property(key))
                        .and_then(sue_core::Value::as_f64)
                        .unwrap_or(default)
                };
                let [width, height] = entity.and_then(sue_model::picture::size).unwrap_or([1, 1]);
                self.dialog = Some(crate::dialogs::Dialog::PageSettings(
                    crate::dialogs::PageSettings {
                        story,
                        width,
                        height,
                        dpi: number(sue_model::picture::DPI, 72.0),
                        bleed: number(story::PAGE_BLEED, 3.0),
                        inner: number(story::PAGE_INNER, 10.0),
                    },
                ));
            }
            "story.info" => {
                let Some(story) = self.ensure_story(cx) else {
                    return true;
                };
                let entity = cx.world.arena.get_by_id(story).ok();
                self.dialog = Some(crate::dialogs::Dialog::StoryInfo(
                    crate::dialogs::StoryInfo {
                        story,
                        title: entity
                            .map(|entity| entity.name().to_owned())
                            .unwrap_or_default(),
                        author: entity
                            .and_then(|entity| entity.property(story::AUTHOR))
                            .and_then(sue_core::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        first: entity
                            .and_then(|entity| entity.property(story::FIRST_NUMBER))
                            .and_then(sue_core::Value::as_i64)
                            .unwrap_or(1),
                        right_bound: story::bound_right(&cx.world.arena, story),
                    },
                ));
            }
            "story.editor" => {
                let arena = &cx.world.arena;
                let pages = self.story_here(arena).map_or_else(
                    || self.picture().into_iter().collect(),
                    |(_, pages, _)| pages,
                );
                let mut texts = Vec::new();
                for (number, page) in pages.iter().enumerate() {
                    for info in doc::layers(arena, *page) {
                        if let Some(lettering) = arena
                            .get_by_id(info.id)
                            .ok()
                            .and_then(crate::lettering::Lettering::of)
                        {
                            texts.push((number + 1, *page, info.id, lettering.content));
                        }
                    }
                }
                if texts.is_empty() {
                    cx.notify(tr!("no-text-yet-text-tool-story"));
                    return true;
                }
                self.dialog = Some(crate::dialogs::Dialog::StoryEditor(texts));
            }
            "story.import_text" => {
                if self.picture().is_none() {
                    cx.fail(tr!("open-make-page-first"));
                    return true;
                }
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(sue_wm::ShellRequest::PickFile(
                    sue_wm::FilePick::open(
                        Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                        "story-text",
                        &tr!("batch-import-text"),
                    )
                    .extensions(&["txt", "md"])
                    .starting_in(start),
                ));
            }
            "story.export_page" => self.run("file.export.png", cx),
            "story.export_pages" => self.ask_export_pages(cx),
            _ => return false,
        }
        true
    }

    /// Story, Batch Import Text: each block of the text file at `path`
    /// becomes a text layer, from the page shown on, a line of `---`
    /// moving on to the next page.
    pub(crate) fn import_text(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let text = match sue_platform::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                cx.fail(tr!("cannot-read", path = path.display(), error = error));
                return;
            }
        };
        let arena = &cx.world.arena;
        let (pages, index, right_bound) = match self.story_here(arena) {
            Some((story, pages, index)) => (pages, index, story::bound_right(arena, story)),
            None => (vec![picture], 0, false),
        };
        let blocks = story::text_blocks(&text);
        let rgb = self.main;
        let mut edit = doc::Edit::new(tr!("batch-import-text"));
        let (mut placed, mut left_over) = (0, 0);
        for (offset, page_blocks) in blocks.iter().enumerate() {
            let Some(page) = pages.get(index + offset).copied() else {
                left_over += page_blocks.len();
                continue;
            };
            let Some([width, height]) = arena
                .get_by_id(page)
                .ok()
                .and_then(sue_model::picture::size)
            else {
                continue;
            };
            let size = (height as f32 / 40.0).clamp(12.0, 96.0);
            let mut order = arena.relations().next_order(page, sue_core::CHILD);
            for (row, block) in page_blocks.iter().enumerate() {
                // Down the page, in from the edge reading starts at.
                let mut lettering = crate::lettering::Lettering::new([0.0, 0.0], rgb);
                lettering.content = block.clone();
                lettering.size = size;
                let lines = block.lines().count().max(1) as f32;
                let y = size * 2.0 + row as f32 * (size * 1.4 * lines + size);
                lettering.at = if right_bound {
                    [width as f32 * 0.6, y]
                } else {
                    [size * 2.0, y]
                };
                let Some(layer) = crate::lettering::text_layer(&lettering, width, height) else {
                    continue;
                };
                let id = layer.id();
                edit = edit.then(sue_core::txn::SpawnEntity::new(layer).with_relation(
                    sue_core::Relation::new(page, sue_core::CHILD, id).ordered(order.clone()),
                ));
                order = sue_core::OrderKey::between(Some(&order), None);
                placed += 1;
            }
        }
        if placed > 0 {
            Self::apply(cx, edit);
        }
        if left_over > 0 {
            cx.notify(tr!(
                "lettered-blocks-more-need-more-pages",
                placed = placed,
                left_over = left_over
            ));
        } else {
            cx.notify(tr!("lettered-blocks", placed = placed));
        }
    }

    /// Asks where to export every page, and as what.
    fn ask_export_pages(&mut self, cx: &mut AppletContext<'_>) {
        let Some((story, _, _)) = self.story_here(&cx.world.arena) else {
            cx.fail(tr!("export-all-pages-works-story"));
            return;
        };
        let name = cx
            .world
            .arena
            .get_by_id(story)
            .map_or_else(|_| tr!("story"), |entity| entity.name().to_owned());
        let types = sue_io::PICTURE_SAVE_TYPES
            .iter()
            .map(|(what, extensions)| FileType::new(what, extensions))
            .collect();
        let start = self.last_dir.clone();
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                "story-pages",
                &tr!("export-all-pages"),
                &format!("{name}.png"),
            )
            .types(types)
            .starting_in(start),
        ));
    }

    /// Writes every page of the current story next to `path`, numbered
    /// (`Name-001.png`, `Name-002.png`), in the format of its extension.
    /// Asks where to export the story (or the picture) as a PDF or EPUB.
    pub(crate) fn ask_export_book(&mut self, cx: &mut AppletContext<'_>, epub: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let named = crate::story::story_of(arena, picture).unwrap_or(picture);
        let name = arena
            .get_by_id(named)
            .map_or_else(|_| tr!("book"), |entity| entity.name().to_owned());
        let (purpose, what, extension) = if epub {
            ("book-epub", tr!("epub-e-book"), "epub")
        } else {
            ("book-pdf", tr!("pdf-document"), "pdf")
        };
        cx.request(sue_wm::ShellRequest::PickFile(
            sue_wm::FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                purpose,
                &if epub {
                    tr!("export-as-epub")
                } else {
                    tr!("export-as-pdf")
                },
                &format!("{name}.{extension}"),
            )
            .types(vec![sue_wm::FileType::new(&what, &[extension])])
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes every page of the story (or the picture alone) to `path` as a
    /// PDF or, with `epub`, a fixed-layout EPUB.
    pub(crate) fn export_book(&mut self, cx: &mut AppletContext<'_>, path: &Path, epub: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let arena = &cx.world.arena;
        let story = crate::story::story_of(arena, picture);
        let ids = story.map_or_else(|| vec![picture], |story| crate::story::pages(arena, story));
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let pages: Vec<sue_core::RasterTile2D> = ids
            .iter()
            .filter_map(|page| crate::composite::flatten(arena, *page, options))
            .collect();
        let named = story.unwrap_or(picture);
        let entity = arena.get_by_id(named).ok();
        let dpi = arena
            .get_by_id(picture)
            .ok()
            .and_then(|entity| entity.property(sue_model::picture::DPI))
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(72.0) as f32;
        let book = sue_io::publish::PictureBook {
            title: entity.map_or_else(|| tr!("book"), |entity| entity.name().to_owned()),
            author: entity
                .and_then(|entity| entity.property("story.author"))
                .and_then(sue_core::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            language: "en".to_owned(),
            identifier: format!("urn:uuid:{}", named),
            pages,
            right_to_left: story.is_some_and(|story| crate::story::bound_right(arena, story)),
            dpi,
        };
        let count = book.pages.len();
        let bytes = if epub {
            sue_io::publish::picture_epub(&book, 90)
        } else {
            sue_io::publish::picture_pdf(&book)
        };
        match bytes.and_then(|bytes| {
            sue_platform::write_atomic(path, &bytes)
                .map_err(|error| error.to_string())
                .map(|()| bytes.len())
        }) {
            Ok(size) => cx.saved(
                path,
                tr!("pages-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!("cannot-export", error = error)),
        }
    }

    pub(crate) fn export_pages(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((_, pages, _)) = self.story_here(&cx.world.arena) else {
            return;
        };
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("page-2"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let quality = cx
            .rice()
            .get_i64("io.jpeg_quality")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(90);
        let effort = cx
            .rice()
            .get_i64("io.jxl_effort")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(7);
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let mut written = 0;
        for (index, page) in pages.iter().enumerate() {
            let Some(flat) = composite::flatten(&cx.world.arena, *page, options) else {
                continue;
            };
            let target = dir.join(format!("{stem}-{:03}.{extension}", index + 1));
            match sue_io::encode_as(&flat, &extension, quality, effort)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                Ok(()) => written += 1,
                Err(error) => {
                    cx.fail(tr!("cannot-export-page", value = index + 1, error = error));
                    return;
                }
            }
        }
        self.last_dir = Some(dir.clone());
        cx.notify(tr!(
            "exported-pages-to",
            dir = dir.display(),
            written = written
        ));
    }

    /// The Page Manager: the binding, the page buttons and the spreads.
    pub(crate) fn pages_ui(
        &mut self,
        ui: &mut Ui,
        pages_dialog: &mut PagesDialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let story = self.story_here(&cx.world.arena);
        let pages = story.as_ref().map_or_else(
            || self.picture().into_iter().collect(),
            |(_, pages, _)| pages.clone(),
        );
        pages_dialog.chosen = pages_dialog.chosen.min(pages.len().saturating_sub(1));
        let right = story
            .as_ref()
            .is_some_and(|(story, _, _)| story::bound_right(&cx.world.arena, *story));
        let mut command: Option<&'static str> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("binding-2")).color(theme.muted));
            let mut now = right;
            ui.selectable_value(&mut now, true, tr!("right-manga"));
            ui.selectable_value(&mut now, false, tr!("left"));
            if now != right {
                if let Some((story, _, _)) = &story {
                    let edit = doc::set(*story, story::BINDING, if now { "right" } else { "left" });
                    Self::apply(cx, edit);
                } else {
                    cx.notify(tr!("add-page-first-then-picture-becomes"));
                }
            }
            ui.separator();
            for (label, name) in [
                (tr!("add-2"), "story.add"),
                (tr!("insert"), "story.insert"),
                (tr!("duplicate"), "story.duplicate_page"),
                (tr!("delete"), "story.delete"),
            ] {
                if ui.button(label).clicked() {
                    command = Some(name);
                }
            }
            ui.separator();
            if ui.button(tr!("move-earlier")).clicked() {
                command = Some("earlier");
            }
            if ui.button(tr!("move-later")).clicked() {
                command = Some("later");
            }
            if ui.button(tr!("open")).clicked() {
                command = Some("open");
            }
        });
        ui.label(
            RichText::new(tr!("click-page-choose-double-click-open"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        ui.separator();
        let mut opened = None;
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                let spreads = story::spreads(pages.len(), right);
                let page_size = vec2(110.0, 150.0);
                let spread_size = vec2(page_size.x * 2.0, page_size.y + 18.0);
                let gap = vec2(28.0, 18.0);
                let columns = ((ui.available_width() + gap.x) / (spread_size.x + gap.x))
                    .floor()
                    .max(1.0) as usize;
                for row in spreads.chunks(columns) {
                    let (area, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), spread_size.y + gap.y),
                        Sense::hover(),
                    );
                    for (column, spread) in row.iter().enumerate() {
                        let left = area.left() + column as f32 * (spread_size.x + gap.x);
                        for (side, slot) in spread.iter().enumerate() {
                            let Some(index) = *slot else {
                                continue;
                            };
                            let page = pages[index];
                            let paper = Rect::from_min_size(
                                pos2(left + side as f32 * page_size.x, area.top()),
                                page_size,
                            );
                            let response = ui.interact(
                                paper,
                                ui.scope_id().with(("page", page.to_u128())),
                                Sense::click(),
                            );
                            self.page_thumbnail(ui, paper, page, cx, theme);
                            let chosen = index == pages_dialog.chosen;
                            ui.painter().rect_stroke(
                                paper,
                                0.0,
                                if chosen {
                                    egui::Stroke::new(2.0, theme.border_active)
                                } else {
                                    theme.hairline()
                                },
                                egui::StrokeKind::Inside,
                            );
                            let here = self.picture() == Some(page);
                            ui.painter().text(
                                pos2(paper.center().x, paper.bottom() + 9.0),
                                egui::Align2::CENTER_CENTER,
                                if here {
                                    tr!("open-2", value = index + 1)
                                } else {
                                    (index + 1).to_string()
                                },
                                egui::FontId::proportional(theme.small_size),
                                if chosen { theme.text } else { theme.muted },
                            );
                            if response.clicked() {
                                pages_dialog.chosen = index;
                            }
                            if response.double_clicked() {
                                opened = Some(page);
                            }
                        }
                    }
                }
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
            if ui.button(tr!("close")).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        let chosen_page = pages.get(pages_dialog.chosen).copied();
        match command {
            Some("open") => opened = chosen_page,
            Some("earlier" | "later") => {
                if let (Some((story, _, _)), Some(page)) = (&story, chosen_page) {
                    let to = if command == Some("earlier") {
                        pages_dialog.chosen.saturating_sub(1)
                    } else {
                        (pages_dialog.chosen + 1).min(pages.len() - 1)
                    };
                    if let Some(edit) = story::move_page(&cx.world.arena, *story, page, to) {
                        Self::apply(cx, edit);
                        pages_dialog.chosen = to;
                    }
                }
            }
            Some(name) => {
                // The page buttons work on the chosen page.
                if let Some(page) = chosen_page {
                    self.go_to_page(cx, page);
                }
                self.story_command(name, cx);
                if let Some((_, pages, index)) = self.story_here(&cx.world.arena) {
                    pages_dialog.chosen = index.min(pages.len().saturating_sub(1));
                }
                // story_command may have opened another dialog; this one stays.
                if matches!(self.dialog, Some(crate::dialogs::Dialog::Pages(_))) {
                    self.dialog = None;
                }
            }
            None => {}
        }
        if let Some(page) = opened {
            self.go_to_page(cx, page);
            outcome = Outcome::Cancel;
        }
        outcome
    }

    /// One page's thumbnail, from the composited page, kept until the page
    /// changes.
    fn page_thumbnail(
        &mut self,
        ui: &Ui,
        rect: Rect,
        page: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let signature = composite::signature(&cx.world.arena, page);
        let version = signature
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, (id, revision)| {
                (hash ^ (id.to_u128() as u64) ^ revision).wrapping_mul(0x0100_0000_01b3)
            });
        if self
            .page_thumbs
            .get(&page)
            .is_none_or(|(known, _, _)| *known != version)
            && let Some((pixels, size)) = composite::preview(&cx.world.arena, page, [220, 300])
        {
            self.page_thumbs.insert(page, (version, pixels, size));
        }
        ui.painter().rect_filled(rect, 0.0, theme.void);
        let Some((version, pixels, size)) = self.page_thumbs.get(&page) else {
            return;
        };
        if let Some(texture) = cx.textures.texture(
            texture_key(page, 0x5041_4745),
            *version,
            *size,
            4,
            pixels,
            false,
        ) {
            let aspect = size[0] as f32 / size[1].max(1) as f32;
            let fit = if aspect > rect.width() / rect.height() {
                vec2(rect.width(), rect.width() / aspect)
            } else {
                vec2(rect.height() * aspect, rect.height())
            };
            ui.painter().image(
                texture,
                Rect::from_center_size(rect.center(), fit),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
}
