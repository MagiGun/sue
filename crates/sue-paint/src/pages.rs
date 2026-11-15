//! Sue Paint's side of stories (Clip Studio Paint's Story menu): making a
//! comic of pages, stepping through them, the Page Manager (a view of the
//! canvas area, filling it or docked beside the page, with spreads,
//! webtoon columns, drag to reorder and its right-click menu), the 3D
//! Preview for Binding, and exporting every page.

use std::path::Path;

use egui::{Id, Key, Rect, Sense, Ui, pos2, vec2};
use sue_core::EntityId;
use sue_platform::write_atomic;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, Tip, context_menu, focus_ring, icon_button, tooltip};
use sue_wm::{AppletContext, FilePick, FileType, ItemState, MenuItem, ShellRequest, Theme};

use crate::commands::WEBTOON_READING;
use crate::composite::{self, RenderOptions};
use crate::dialogs::{Outcome, PagesDialog};
use crate::doc::{self, NewPicture};
use crate::panels::child;
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
    pub(crate) fn ensure_story(&mut self, cx: &mut AppletContext<'_>) -> Option<EntityId> {
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
    pub(crate) fn ask_export_pages(&mut self, cx: &mut AppletContext<'_>) {
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
            .filter_map(|page| {
                let mut flat = crate::composite::flatten(arena, *page, options)?;
                crate::story::draw_folio(arena, *page, &mut flat);
                Some(flat)
            })
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
            let Some(mut flat) = composite::flatten(&cx.world.arena, *page, options) else {
                continue;
            };
            story::draw_folio(&cx.world.arena, *page, &mut flat);
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

    /// Story, Page Manager: the pages fill the canvas area (or, when it
    /// is already docked beside the page, it fills it again).
    pub(crate) fn open_page_manager(&mut self, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let Some((story, _, index)) = self.story_here(arena) else {
            cx.notify(tr!("canvas-one-picture-story-add-page"));
            return;
        };
        let webtoon = arena
            .get_by_id(story)
            .is_ok_and(|entity| entity.property(WEBTOON_READING).is_some());
        let view = self.page_view.get_or_insert_with(|| PageView {
            webtoon,
            ..PageView::default()
        });
        view.docked = false;
        view.chosen = index;
        view.dragging = None;
    }

    /// The Page Manager's share of the canvas area `area`, drawn: all of
    /// it (`None` is left for the page) or a column at its left (the rest
    /// is returned). While it is closed, or the picture shown is in no
    /// story, the page has all of `area`.
    pub(crate) fn page_manager_area(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<Rect> {
        let Some(mut view) = self.page_view.take() else {
            return Some(area);
        };
        let Some((story, pages, here)) = self.story_here(&cx.world.arena) else {
            // It shows again with the next page of a story.
            self.page_view = Some(view);
            return Some(area);
        };
        view.chosen = view.chosen.min(pages.len().saturating_sub(1));
        let (panel, rest) = if view.docked {
            let width = PAGE_COLUMN.min(area.width() / 2.0);
            let split = area.left() + width;
            (
                Rect::from_min_max(area.min, pos2(split, area.bottom())),
                Some(Rect::from_min_max(pos2(split, area.top()), area.max)),
            )
        } else {
            (area, None)
        };
        let shown = PageShown {
            story,
            pages: &pages,
            here,
        };
        let act = self.page_panel(ui, panel, &mut view, &shown, cx, theme);
        let keep = act.is_none_or(|act| self.page_act(act, &mut view, &shown, cx));
        if !keep {
            return Some(area);
        }
        // A command may have opened the Page Manager anew; that one wins.
        if self.page_view.is_none() {
            self.page_view = Some(view);
        }
        rest
    }

    /// The Page Manager drawn in `panel`: a header with its buttons, then
    /// the pages. Returns what was asked of it this frame.
    fn page_panel(
        &mut self,
        ui: &mut Ui,
        panel: Rect,
        view: &mut PageView,
        shown: &PageShown<'_>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<PageAct> {
        let pad = theme.space(Space::S4);
        ui.painter().rect_filled(
            panel,
            0.0,
            if view.docked { theme.panel } else { theme.void },
        );
        if view.docked {
            ui.painter()
                .vline(panel.right(), panel.y_range(), theme.hairline());
        }
        let density = Density::of(ui);
        let button = icon_button("", Icon::Close, "").size(density);
        let header =
            Rect::from_min_size(panel.min, egui::vec2(panel.width(), button.y + pad * 2.0));
        ui.painter()
            .hline(header.x_range(), header.bottom(), theme.hairline());
        let mut act = None;
        // The buttons, from the right edge in.
        let mut right = header.right() - pad;
        let mut next = |name: &str, icon: Icon, on: Option<bool>, ui: &mut Ui| {
            let rect = Rect::from_min_size(
                pos2(right - button.x, header.center().y - button.y / 2.0),
                button,
            );
            right = rect.left() - theme.space(Space::S2);
            let mut control = icon_button(("sue-paint-page-manager", name), icon, name);
            if let Some(on) = on {
                control = control.selected(on);
            }
            control.show_at(ui, rect, theme).clicked()
        };
        if next(&tr!("close-page-manager"), Icon::Close, None, ui) {
            act = Some(PageAct::Close);
        }
        let (dock_name, dock_icon) = if view.docked {
            (tr!("page-manager-fill-canvas-area"), Icon::Fullscreen)
        } else {
            (tr!("page-manager-dock-beside-page"), Icon::PaletteSubView)
        };
        if next(&dock_name, dock_icon, None, ui) {
            act = Some(PageAct::Command("pages.dock".to_owned()));
        }
        if !view.docked {
            if next(
                &tr!("page-manager-webtoon-columns"),
                Icon::DocWebtoon,
                Some(view.webtoon),
                ui,
            ) {
                act = Some(PageAct::Command(
                    if view.webtoon {
                        "pages.spreads"
                    } else {
                        "pages.webtoon"
                    }
                    .to_owned(),
                ));
            }
            let delete = crate::commands::registry()
                .label("story.delete")
                .unwrap_or_default();
            if next(&delete, Icon::Delete, None, ui) {
                act = Some(PageAct::Command("story.delete".to_owned()));
            }
            let add = crate::commands::registry()
                .label("story.add")
                .unwrap_or_default();
            if next(&add, Icon::FileNew, None, ui) {
                act = Some(PageAct::Command("story.add".to_owned()));
            }
        }
        // The title, in what room the buttons leave.
        let title = ui.painter().layout_no_wrap(
            tr!("page-manager"),
            theme.font(theme.small_size + 1.0),
            theme.text,
        );
        let title_at = pos2(
            header.left() + pad,
            header.center().y - title.size().y / 2.0,
        );
        let title_room = Rect::from_min_max(header.min, pos2(right, header.bottom()));
        let count = tr!("page-count", count = shown.pages.len());
        let painter = ui.painter().with_clip_rect(title_room);
        let title_end = title_at.x + title.size().x;
        painter.galley(title_at, title, theme.text);
        if !view.docked {
            painter.text(
                pos2(title_end + pad, header.center().y),
                egui::Align2::LEFT_CENTER,
                count,
                theme.font(theme.small_size),
                theme.muted,
            );
        }
        let body = Rect::from_min_max(pos2(panel.left(), header.bottom()), panel.max);
        let mut inner = child(ui, body, "page-manager");
        let pages_act = egui::ScrollArea::both()
            .id_salt("sue-paint-page-manager-pages")
            .auto_shrink([false, false])
            .show(&mut inner, |ui| self.page_cells(ui, view, shown, cx, theme))
            .inner;
        act.or(pages_act)
    }

    /// The pages as cells: spreads, webtoon columns or (docked) one
    /// column. A click picks a page, a double-click (or Enter) opens it,
    /// a drag moves it, a right-click shows the Page Manager's menu.
    fn page_cells(
        &mut self,
        ui: &mut Ui,
        view: &mut PageView,
        shown: &PageShown<'_>,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<PageAct> {
        let arena = &cx.world.arena;
        let count = shown.pages.len();
        let aspect = arena
            .get_by_id(shown.pages[0])
            .ok()
            .and_then(sue_model::picture::size)
            .map_or(0.7, |[w, h]| (w as f32 / h.max(1) as f32).clamp(0.2, 5.0));
        let right = story::bound_right(arena, shown.story);
        let layout = PageLayout {
            width: ui.clip_rect().width(),
            height: ui.clip_rect().height(),
            pad: theme.space(Space::S5),
            label: theme.small_size + theme.space(Space::S4),
            aspect,
        };
        let slots = if view.docked {
            layout.column(count)
        } else if view.webtoon {
            layout.webtoon(
                count,
                crate::commands::webtoon_right_to_left(arena, shown.story),
            )
        } else {
            layout.spreads(count, right, story::starts_paired(arena, shown.story))
        };
        let extent = slots
            .iter()
            .fold(egui::Vec2::ZERO, |most, rect| most.max(rect.max.to_vec2()));
        let (area, _) = ui.allocate_exact_size(
            egui::vec2(
                layout.width.max(extent.x + layout.pad),
                extent.y + layout.pad,
            ),
            Sense::hover(),
        );
        let slots: Vec<Rect> = slots
            .into_iter()
            .map(|rect| rect.translate(area.min.to_vec2()))
            .collect();
        let items = page_menu();
        let mut act = None;
        let pointer = ui.ctx().pointer_latest_pos();
        for (index, paper) in slots.iter().copied().enumerate() {
            let page = shown.pages[index];
            let response = ui.interact(
                paper,
                Id::unique(("sue-paint-page-cell", page.to_u128())),
                Sense::click_and_drag(),
            );
            self.page_thumbnail(ui, paper, page, cx, theme);
            let chosen = index == view.chosen;
            let moving = view.dragging == Some(index);
            if moving {
                ui.painter()
                    .rect_filled(paper, 0.0, theme.void.gamma_multiply(0.6));
            }
            let target = view.dragging.is_some_and(|from| from != index)
                && pointer.is_some_and(|at| paper.contains(at));
            ui.painter().rect_stroke(
                paper,
                0.0,
                if chosen || target {
                    egui::Stroke::new(2.0, theme.border_active)
                } else {
                    theme.hairline()
                },
                egui::StrokeKind::Inside,
            );
            let open = index == shown.here;
            let number = if open {
                tr!("open-2", value = index + 1)
            } else {
                (index + 1).to_string()
            };
            ui.painter().text(
                pos2(paper.center().x, paper.bottom() + layout.label / 2.0),
                egui::Align2::CENTER_CENTER,
                number,
                theme.font(theme.small_size),
                if chosen || open {
                    theme.text
                } else {
                    theme.muted
                },
            );
            focus_ring(ui, &response, 0.0, theme);
            let name = tr!("page", number = index + 1);
            let help = tr!("page-manager-cell-help");
            let response = tooltip(response, &Tip::new(&name).help(&help), theme);
            if response.clicked() || response.secondary_clicked() || response.drag_started() {
                view.chosen = index;
            }
            if response.double_clicked()
                || (response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter)))
            {
                act = Some(PageAct::Open(index));
            }
            if response.drag_started() {
                view.dragging = Some(index);
            }
            if moving && response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if moving && response.drag_stopped() {
                view.dragging = None;
                let to = pointer.and_then(|at| slots.iter().position(|slot| slot.contains(at)));
                if let Some(to) = to.filter(|to| *to != index) {
                    act = Some(PageAct::Move(index, to));
                }
            }
            let facts_cx: &AppletContext<'_> = cx;
            let flags = (*view, count);
            if let Some(command) = context_menu(
                ui,
                &response,
                &items,
                &|name| self.page_item_state(name, flags, &sue_wm::Facts::of(facts_cx)),
                theme,
            ) {
                act = Some(PageAct::Command(command));
            }
        }
        act
    }

    /// How an item of the Page Manager's menu looks: its own items by the
    /// view and the page picked, the rest as their commands say.
    fn page_item_state(
        &self,
        name: &str,
        (view, count): (PageView, usize),
        facts: &sue_wm::Facts<'_>,
    ) -> ItemState {
        let when = |on: bool| {
            if on {
                ItemState::on()
            } else {
                ItemState::off()
            }
        };
        match name {
            "pages.open" => ItemState::on(),
            "pages.earlier" => when(view.chosen > 0),
            "pages.later" => when(view.chosen + 1 < count),
            "pages.spreads" => ItemState::check(!view.webtoon),
            "pages.webtoon" => ItemState::check(view.webtoon),
            "pages.dock" => ItemState::check(view.docked),
            _ => self.state(name, facts),
        }
    }

    /// Does what the Page Manager was asked. `false` when it closes.
    fn page_act(
        &mut self,
        act: PageAct,
        view: &mut PageView,
        shown: &PageShown<'_>,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let last = shown.pages.len().saturating_sub(1);
        match act {
            PageAct::Close => return false,
            PageAct::Open(index) => {
                if let Some(page) = shown.pages.get(index) {
                    self.go_to_page(cx, *page);
                    // Opened, the page takes the canvas area and the pages
                    // stay beside it, as in Clip Studio Paint.
                    view.docked = true;
                    view.chosen = index;
                }
            }
            PageAct::Move(from, to) => {
                if let Some(page) = shown.pages.get(from)
                    && let Some(edit) =
                        story::move_page(&cx.world.arena, shown.story, *page, to.min(last))
                {
                    Self::apply(cx, edit);
                    view.chosen = to.min(last);
                }
            }
            PageAct::Command(command) => match command.as_str() {
                "pages.open" => return self.page_act(PageAct::Open(view.chosen), view, shown, cx),
                "pages.earlier" | "pages.later" => {
                    let to = if command == "pages.earlier" {
                        view.chosen.saturating_sub(1)
                    } else {
                        (view.chosen + 1).min(last)
                    };
                    return self.page_act(PageAct::Move(view.chosen, to), view, shown, cx);
                }
                "pages.spreads" => view.webtoon = false,
                "pages.webtoon" => view.webtoon = true,
                "pages.dock" => view.docked = !view.docked,
                name => {
                    // The page commands act on the page picked, so it is
                    // the one shown first.
                    if ON_PAGE_PICKED.contains(&name)
                        && let Some(page) = shown.pages.get(view.chosen)
                    {
                        self.go_to_page(cx, *page);
                    }
                    self.run(name, cx);
                    if name.starts_with("story.webtoon.") {
                        view.webtoon = true;
                    }
                    if let Some((_, _, index)) = self.story_here(&cx.world.arena) {
                        view.chosen = index;
                    }
                }
            },
        }
        true
    }

    /// 3D Preview for Binding: the story bound as a book, one spread open,
    /// its bleed cut off as the printer will; a bar under it zooms and
    /// turns the pages.
    pub(crate) fn binding_ui(
        &mut self,
        ui: &mut Ui,
        preview: &mut PagesDialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        let arena = &cx.world.arena;
        let Some((story, pages, _)) = self.story_here(arena) else {
            return Outcome::Cancel;
        };
        let right = story::bound_right(arena, story);
        let spreads = story::spreads_from(pages.len(), right, story::starts_paired(arena, story));
        let last = spreads.len().saturating_sub(1);
        preview.spread = preview.spread.min(last);
        let density = Density::of(ui);
        let button = icon_button("", Icon::Close, "").size(density);
        let bar_height = button.y + theme.space(Space::S4) * 2.0;
        let room = ui.available_rect_before_wrap();
        let stage = Rect::from_min_max(room.min, pos2(room.right(), room.bottom() - bar_height));
        let bar = Rect::from_min_max(pos2(room.left(), stage.bottom()), room.max);
        let response = ui.interact(stage, Id::unique("sue-paint-binding-stage"), Sense::click());
        // The wheel zooms; Left and Right turn the pages the way they lie.
        let (scroll, left_key, right_key, plus, minus) = ui.input(|input| {
            (
                if response.hovered() {
                    input.smooth_scroll_delta.y
                } else {
                    0.0
                },
                input.key_pressed(Key::ArrowLeft),
                input.key_pressed(Key::ArrowRight),
                input.key_pressed(Key::Plus) || input.key_pressed(Key::Equals),
                input.key_pressed(Key::Minus),
            )
        });
        if scroll != 0.0 {
            preview.zoom = (preview.zoom * (scroll / 400.0).exp()).clamp(ZOOM_LEAST, ZOOM_MOST);
        }
        // Turning toward the left goes on in a book bound on the right.
        let (mut turn_left, mut turn_right) = (left_key, right_key);
        let painter = ui.painter_at(stage);
        painter.rect_filled(stage, 0.0, theme.void);
        let spread = spreads.get(preview.spread).copied().unwrap_or([None, None]);
        let mut sides: [Option<(egui::TextureId, Rect, f32)>; 2] = [None, None];
        for (side, slot) in spread.iter().enumerate() {
            let Some(page) = slot.and_then(|index| pages.get(index)).copied() else {
                continue;
            };
            let Some((texture, size)) = self.page_texture(page, cx) else {
                continue;
            };
            let arena = &cx.world.arena;
            let full = arena
                .get_by_id(page)
                .ok()
                .and_then(sue_model::picture::size)
                .unwrap_or(size);
            let (w, h) = (full[0].max(1) as f32, full[1].max(1) as f32);
            let trim = story::borders(arena, page).map_or([0.0, 0.0, w, h], |borders| borders.trim);
            let uv = Rect::from_min_max(
                pos2(trim[0] / w, trim[1] / h),
                pos2(trim[2] / w, trim[3] / h),
            );
            let aspect = (trim[2] - trim[0]) / (trim[3] - trim[1]).max(1.0);
            sides[side] = Some((texture, uv, aspect));
        }
        draw_book(&painter, stage, sides, preview.zoom, theme);
        // The bar: zoom, turn, which pages, Close.
        ui.painter()
            .hline(bar.x_range(), bar.top(), theme.hairline());
        let mut x = bar.left() + theme.space(Space::S4);
        let y = bar.center().y - button.y / 2.0;
        let mut place = |name: &str, icon: Icon, enabled: bool, ui: &mut Ui| {
            let rect = Rect::from_min_size(pos2(x, y), button);
            x = rect.right() + theme.space(Space::S2);
            icon_button(("sue-paint-binding", name), icon, name)
                .enabled(enabled)
                .show_at(ui, rect, theme)
                .clicked()
        };
        if place(&tr!("zoom-out"), Icon::ZoomOut, true, ui) || minus {
            preview.zoom = (preview.zoom / ZOOM_STEP).max(ZOOM_LEAST);
        }
        if place(&tr!("zoom-in"), Icon::ZoomIn, true, ui) || plus {
            preview.zoom = (preview.zoom * ZOOM_STEP).min(ZOOM_MOST);
        }
        if place(&tr!("fit-to-window"), Icon::ZoomFit, true, ui) {
            preview.zoom = 1.0;
        }
        let (left_on, right_on) = if right {
            (preview.spread < last, preview.spread > 0)
        } else {
            (preview.spread > 0, preview.spread < last)
        };
        turn_left |= place(&tr!("turn-page-left"), Icon::ChevronLeft, left_on, ui);
        turn_right |= place(&tr!("turn-page-right"), Icon::ChevronRight, right_on, ui);
        let forward = |spread: usize| (spread + 1).min(last);
        let back = |spread: usize| spread.saturating_sub(1);
        if turn_left {
            preview.spread = if right {
                forward(preview.spread)
            } else {
                back(preview.spread)
            };
        }
        if turn_right {
            preview.spread = if right {
                back(preview.spread)
            } else {
                forward(preview.spread)
            };
        }
        let mut numbers: Vec<usize> = spread.iter().flatten().map(|index| index + 1).collect();
        numbers.sort_unstable();
        let shown = numbers
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" - ");
        ui.painter().text(
            pos2(x + theme.space(Space::S4), bar.center().y),
            egui::Align2::LEFT_CENTER,
            tr!("binding-spread-of", pages = shown, count = pages.len()),
            theme.font(theme.small_size),
            theme.text,
        );
        let close = tr!("close");
        let close_button = sue_wm::ui::button("sue-paint-binding-close", &close);
        let size = close_button.size(ui.painter(), theme, density);
        let close_rect = Rect::from_min_size(
            pos2(
                bar.right() - theme.space(Space::S4) - size.x,
                bar.center().y - size.y / 2.0,
            ),
            size,
        );
        ui.allocate_rect(room, Sense::hover());
        if close_button.show_at(ui, close_rect, theme).clicked() {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
    }

    /// `page`'s picture as a texture, from the composited page, made again
    /// when the page changes (but not while a stroke is under way, so
    /// drawing beside the Page Manager stays quick).
    fn page_texture(
        &mut self,
        page: EntityId,
        cx: &mut AppletContext<'_>,
    ) -> Option<(egui::TextureId, [u32; 2])> {
        let signature = composite::signature(&cx.world.arena, page);
        let version = signature
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, (id, revision)| {
                (hash ^ (id.to_u128() as u64) ^ revision).wrapping_mul(0x0100_0000_01b3)
            });
        let stale = self
            .page_thumbs
            .get(&page)
            .is_none_or(|(known, _, _)| *known != version);
        let busy = self.gesture.is_some() && self.page_thumbs.contains_key(&page);
        if stale
            && !busy
            && let Some((pixels, size)) = composite::preview(&cx.world.arena, page, PAGE_TEXTURE)
        {
            self.page_thumbs.insert(page, (version, pixels, size));
        }
        let (version, pixels, size) = self.page_thumbs.get(&page)?;
        let texture = cx.textures.texture(
            texture_key(page, 0x5041_4745),
            *version,
            *size,
            4,
            pixels,
            false,
        )?;
        Some((texture, *size))
    }

    /// One page's thumbnail, fitted into `rect`.
    fn page_thumbnail(
        &mut self,
        ui: &Ui,
        rect: Rect,
        page: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.void);
        let Some((texture, size)) = self.page_texture(page, cx) else {
            return;
        };
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

/// How wide the Page Manager is while docked beside the page.
const PAGE_COLUMN: f32 = 176.0;
/// How tall a page is in the Page Manager's spreads and webtoon columns.
const PAGE_HEIGHT: f32 = 150.0;
/// The most a page's texture holds, for the Page Manager and the 3D
/// preview alike.
const PAGE_TEXTURE: [u32; 2] = [420, 600];
/// The 3D preview's zoom range and step.
const ZOOM_LEAST: f32 = 0.4;
const ZOOM_MOST: f32 = 3.0;
const ZOOM_STEP: f32 = 1.25;

/// The Page Manager while it shows in the canvas area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PageView {
    /// It sits in a column beside the page instead of filling the canvas
    /// area.
    pub docked: bool,
    /// The page picked, by index: where its page commands act.
    pub chosen: usize,
    /// The pages stand in webtoon columns, not in spreads.
    pub webtoon: bool,
    /// The page being dragged to another place, by index.
    pub dragging: Option<usize>,
}

/// The story whose pages the Page Manager shows.
struct PageShown<'a> {
    story: EntityId,
    pages: &'a [EntityId],
    /// The page shown on the canvas, by index.
    here: usize,
}

/// What the Page Manager was asked this frame.
enum PageAct {
    /// Close it.
    Close,
    /// Open the page at this index.
    Open(usize),
    /// Move a page from one index to another.
    Move(usize, usize),
    /// An item of its menu or a button: a command, or one of its own
    /// (`pages.*`).
    Command(String),
}

/// The Story commands that act on the page shown, so the Page Manager
/// shows the page picked before it runs them.
const ON_PAGE_PICKED: [&str; 7] = [
    "story.add",
    "story.insert",
    "story.duplicate_page",
    "story.delete",
    "story.combine",
    "story.separate",
    "story.export_page",
];

/// Where the Page Manager's pages go, in its own coordinates.
struct PageLayout {
    width: f32,
    /// How tall the room is: webtoon columns are as long as it.
    height: f32,
    pad: f32,
    /// Room under a page for its number.
    label: f32,
    /// A page's width over its height.
    aspect: f32,
}

impl PageLayout {
    /// The pages as the spreads of the bound book, the spreads in rows
    /// read from the bound side.
    fn spreads(&self, count: usize, right: bool, paired: bool) -> Vec<Rect> {
        let page = vec2(PAGE_HEIGHT * self.aspect, PAGE_HEIGHT);
        let spread = vec2(page.x * 2.0, page.y + self.label);
        let gap = vec2(self.pad * 2.0, self.pad);
        let across = ((self.width - self.pad * 2.0 + gap.x) / (spread.x + gap.x))
            .floor()
            .max(1.0) as usize;
        let mut out = vec![Rect::NOTHING; count];
        for (number, pair) in story::spreads_from(count, right, paired).iter().enumerate() {
            let (row, column) = ((number / across) as f32, (number % across) as f32);
            let x = if right {
                self.width - self.pad - (column + 1.0) * spread.x - column * gap.x
            } else {
                self.pad + column * (spread.x + gap.x)
            };
            let y = self.pad + row * (spread.y + gap.y);
            for (side, slot) in pair.iter().enumerate() {
                if let Some(index) = slot {
                    out[*index] = Rect::from_min_size(pos2(x + side as f32 * page.x, y), page);
                }
            }
        }
        out
    }

    /// The pages in webtoon columns as long as the room is tall: down
    /// each column, the columns from the right when the webtoon reads
    /// right to left (wider than the room, they scroll).
    fn webtoon(&self, count: usize, right_to_left: bool) -> Vec<Rect> {
        let page = vec2(PAGE_HEIGHT * self.aspect, PAGE_HEIGHT);
        let cell = vec2(page.x + self.pad * 2.0, page.y + self.label + self.pad);
        let rows = ((self.height - self.pad) / cell.y).floor().max(1.0) as usize;
        let columns = count.div_ceil(rows) as f32;
        let wide = self.width.max(columns * cell.x + self.pad * 2.0);
        story::webtoon_slots(count, rows)
            .into_iter()
            .map(|(column, row)| {
                let column = column as f32;
                let x = if right_to_left {
                    wide - self.pad * 2.0 - (column + 1.0) * cell.x
                } else {
                    self.pad + column * cell.x
                };
                Rect::from_min_size(pos2(x + self.pad, self.pad + row as f32 * cell.y), page)
            })
            .collect()
    }

    /// The pages one under another, as wide as the docked column lets
    /// them be.
    fn column(&self, count: usize) -> Vec<Rect> {
        let width = (self.width - self.pad * 2.0).max(1.0);
        let page = vec2(width, width / self.aspect);
        (0..count)
            .map(|index| {
                Rect::from_min_size(
                    pos2(
                        self.pad,
                        self.pad + index as f32 * (page.y + self.label + self.pad),
                    ),
                    page,
                )
            })
            .collect()
    }
}

/// The Page Manager's right-click menu, in Clip Studio Paint's order.
/// Its own items are `pages.*`; the rest are Story commands. Team work
/// and the Clip Studio file commands it has have no place in Sue.
fn page_menu() -> Vec<MenuItem> {
    let registry = crate::commands::registry();
    let item = |command: &str| {
        MenuItem::action(
            registry
                .label(command)
                .unwrap_or_else(|| command.to_owned()),
            command,
        )
    };
    vec![
        MenuItem::action(tr!("open-page"), "pages.open"),
        item("story.first"),
        item("story.previous"),
        item("story.next"),
        item("story.last"),
        MenuItem::Separator,
        item("story.add"),
        item("story.insert"),
        item("story.duplicate_page"),
        item("story.delete"),
        MenuItem::Separator,
        item("story.combine"),
        item("story.separate"),
        MenuItem::Separator,
        item("story.page_settings"),
        item("story.info"),
        MenuItem::Separator,
        MenuItem::sub(
            tr!("page-reordering"),
            vec![
                MenuItem::action(tr!("move-earlier"), "pages.earlier"),
                MenuItem::action(tr!("move-later"), "pages.later"),
            ],
        ),
        MenuItem::sub(
            tr!("view"),
            vec![
                MenuItem::action(tr!("page-manager-spreads"), "pages.spreads"),
                MenuItem::action(tr!("page-manager-webtoon-columns"), "pages.webtoon"),
                MenuItem::Separator,
                MenuItem::action(tr!("page-manager-dock-beside-page"), "pages.dock"),
            ],
        ),
        MenuItem::sub(
            tr!("webtoon-reading-direction"),
            vec![
                MenuItem::action(tr!("reading-right-to-left"), "story.webtoon.right_to_left"),
                MenuItem::action(tr!("reading-left-to-right"), "story.webtoon.left_to_right"),
            ],
        ),
        MenuItem::sub(
            tr!("batch-process"),
            vec![item("story.export_pages"), item("story.export_page")],
        ),
        MenuItem::sub(
            tr!("edit-text"),
            vec![item("story.editor"), item("story.import_text")],
        ),
        MenuItem::Separator,
        item("story.preview_binding"),
    ]
}

/// The book seen from above its lower edge: `sides` are the left and the
/// right page (a texture, the part of it that is the cut page, and that
/// part's width over its height), each rising from the spine and falling
/// flat, darker into the gutter.
fn draw_book(
    painter: &egui::Painter,
    stage: Rect,
    sides: [Option<(egui::TextureId, Rect, f32)>; 2],
    zoom: f32,
    theme: &Theme,
) {
    let aspect = sides
        .iter()
        .flatten()
        .map(|(_, _, aspect)| *aspect)
        .next()
        .unwrap_or(0.7);
    // In page heights: a page is `aspect` wide and 1 tall.
    let scale = (stage.width() * 0.8 / (aspect * 2.0)).min(stage.height() * 0.8) * zoom;
    let center = stage.center();
    let (tilt_cos, tilt_sin) = (BOOK_TILT.cos(), BOOK_TILT.sin());
    let project = |x: f32, y: f32, z: f32| {
        let depth = BOOK_DISTANCE - y * tilt_sin - z * tilt_cos;
        let near = BOOK_DISTANCE / depth.max(0.1);
        let up = y * tilt_cos - z * tilt_sin;
        center + vec2(x * near, up * near) * scale
    };
    // A page rises from the spine and lies flat at its edge.
    let lift = |from_spine: f32| BOOK_LIFT * 4.0 * (from_spine.sqrt() - from_spine);
    // The shadow the book casts on the ground.
    let reach = aspect * 1.01;
    let shadow = vec![
        project(-reach, -0.49, 0.0),
        project(reach, -0.49, 0.0),
        project(reach, 0.53, 0.0),
        project(-reach, 0.53, 0.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        shadow,
        theme.shadow,
        egui::Stroke::NONE,
    ));
    for (side, page) in sides.iter().enumerate() {
        let Some((texture, uv, _)) = page else {
            continue;
        };
        let away = if side == 0 { -1.0 } else { 1.0 };
        let mut mesh = egui::Mesh::with_texture(*texture);
        for step in 0..=BOOK_STRIPS {
            let from_spine = step as f32 / BOOK_STRIPS as f32;
            let x = away * from_spine * aspect;
            let z = lift(from_spine);
            // The left page's spine is its right edge.
            let u = if side == 0 {
                uv.max.x - from_spine * uv.width()
            } else {
                uv.min.x + from_spine * uv.width()
            };
            let gray = (255.0 * (1.0 - BOOK_GUTTER * (1.0 - from_spine).powi(6))) as u8;
            let color = egui::Color32::from_gray(gray);
            for (y, v) in [(-0.5, uv.min.y), (0.5, uv.max.y)] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: project(x, y, z),
                    uv: pos2(u, v),
                    color,
                });
            }
        }
        for step in 0..BOOK_STRIPS as u32 {
            let at = step * 2;
            mesh.add_triangle(at, at + 1, at + 2);
            mesh.add_triangle(at + 1, at + 3, at + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// How far the book is tipped away from the eye, in radians.
const BOOK_TILT: f32 = 0.55;
/// How far the eye is from the book, in page heights.
const BOOK_DISTANCE: f32 = 3.2;
/// How high a page rises off the ground at most, in page heights.
const BOOK_LIFT: f32 = 0.05;
/// How dark the gutter gets, 0 to 1.
const BOOK_GUTTER: f32 = 0.38;
/// How many strips a page is bent through.
const BOOK_STRIPS: usize = 32;
