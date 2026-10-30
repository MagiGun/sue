//! Sue Writer: what it holds and what its commands do. Drawing is in
//! `ui.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, Rect, Ui, Vec2};
use sue_core::{Entity, EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::{class, mark_kind as kind, picture};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_type::editor::TableEdit;
use sue_type::find::{Finder, Query};
use sue_type::{Editor, Fonts, PageSetup, StyleSheet};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaEdit, FilePick, Menu, MenuBar,
    PaletteEntry, ShellRequest, SueAction, SueApplet, sue_menu,
};

use crate::compile::{self, FORMATS, Separator, Titles};
use crate::project;

/// The applet id.
pub const ID: &str = "sue-writer";

/// The studio's applet id.
/// The next font size up (or down) the usual ladder: 8, 9, 10, 10.5, 11,
/// 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72, then by tens.
pub(crate) fn font_step(now: f32, up: bool) -> f32 {
    const SIZES: [f32; 17] = [
        8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0,
        48.0, 72.0,
    ];
    if up {
        SIZES
            .iter()
            .copied()
            .find(|size| *size > now + 0.01)
            .unwrap_or(((now / 10.0).floor() + 1.0) * 10.0)
            .min(1638.0)
    } else if now > 72.0 + 0.01 {
        (((now / 10.0).ceil() - 1.0) * 10.0).max(72.0)
    } else {
        SIZES
            .iter()
            .rev()
            .copied()
            .find(|size| *size < now - 0.01)
            .unwrap_or(1.0)
    }
}

/// The rice key that turns spelling as you type on and off.
pub(crate) const SPELL_CHECK: &str = "sue_writer.spell_check";

pub(crate) fn me() -> AppletId {
    AppletId::new(ID).expect("a valid id")
}

/// What the middle of the studio shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    /// The text: one document, or a folder's documents one after another.
    Document,
    /// Index cards of a folder's items.
    Corkboard,
    /// A table of a folder's items.
    Outliner,
}

/// The compile dialog's choices.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompileChoices {
    pub format: usize,
    pub titles: Titles,
    pub separator: Separator,
    pub title_page: bool,
    pub chapter_pages: bool,
    pub whole: bool,
}

/// The project settings dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub title: String,
    pub author: String,
    pub sheet: String,
    pub page: String,
    pub margin: f32,
    pub font: String,
    pub heading_font: String,
    pub size: f32,
    pub target: i64,
    pub session: i64,
    pub font_filter: String,
}

/// The header and footer dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Furniture {
    pub header: sue_model::Running,
    pub footer: sue_model::Running,
    pub different_first: bool,
}

/// A dialog over the studio.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Dialog {
    /// Choose a template for a new project.
    NewProject { template: usize, title: String },
    /// Compile the project.
    Compile(CompileChoices),
    /// The project's settings.
    Settings(Settings),
    /// A link on the selection.
    Link(String),
    /// A footnote on the selection.
    Footnote(String),
    /// The running header and footer.
    Furniture(Furniture),
    /// A new table of contents.
    Contents(sue_model::contents::Contents),
    /// A new comment on the selection.
    Comment(String),
    /// A new table: rows, columns, and whether the first row is headings.
    Table {
        rows: u32,
        columns: u32,
        heading: bool,
    },
    /// Words written, by day.
    Statistics,
    /// The keyboard shortcuts.
    Shortcuts,
}

/// The find bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Find {
    pub query: String,
    pub replace: String,
    pub case: bool,
    pub words: bool,
    pub regex: bool,
    pub project: bool,
    pub replacing: bool,
    pub focus: bool,
    pub message: String,
}

/// A texture for an embedded picture.
pub(crate) struct Figure {
    pub texture: Option<egui::TextureId>,
    pub size: Vec2,
    pub seen: u64,
    /// Its pixels and their size, asked for again every frame: the painter
    /// lets go of a texture a frame did not use.
    pub pixels: Vec<u8>,
    pub pixel_size: [u32; 2],
}

/// The writing studio.
pub struct SueWriter {
    pub(crate) project: Option<EntityId>,
    pub(crate) open: Option<EntityId>,
    pub(crate) selected: Option<EntityId>,
    pub(crate) expanded: HashSet<EntityId>,
    pub(crate) view: View,
    pub(crate) paged: bool,
    pub(crate) composition: bool,
    pub(crate) typewriter: bool,
    pub(crate) zoom: f32,
    pub(crate) editor: Editor,
    pub(crate) menubar: MenuBar,
    pub(crate) menus: Vec<Menu>,
    /// The shortcuts as Settings has them, for the menus and the shell.
    pub(crate) bound: sue_wm::shortcuts::Bound,
    /// When to build the menus again, in another language.
    pub(crate) language: sue_lang::Watch,
    pub(crate) binder: bool,
    pub(crate) inspector: bool,
    /// The comments show where the inspector does.
    pub(crate) comments: bool,
    /// Resolved comments show too.
    pub(crate) show_resolved: bool,
    pub(crate) binder_width: f32,
    pub(crate) inspector_width: f32,
    pub(crate) find: Option<Find>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) renaming: Option<(EntityId, String)>,
    /// Words in the project when this sitting began, and today.
    pub(crate) session: Option<(EntityId, usize, String)>,
    pub(crate) words: (u64, usize, usize),
    pub(crate) figures: HashMap<EntityId, Figure>,
    pub(crate) present: bool,
    pub(crate) ready: bool,
    pub(crate) shown: bool,
    pub(crate) reveal: f32,
    /// What the inspector is editing, before it is committed.
    pub(crate) drafts: HashMap<(EntityId, &'static str), String>,
    /// The last compile's choices.
    pub(crate) last_compile: Option<CompileChoices>,
    /// The caret's page and the page count, from the last frame.
    pub(crate) page_info: (usize, usize),
    /// Fit the page to the space (until people zoom themselves).
    pub(crate) fit: bool,
    /// The zoom the text was shown at last frame.
    pub(crate) shown_zoom: f32,
    /// The toolbar's height: one row, or two when the studio is narrow.
    pub(crate) toolbar_height: f32,
    /// Laid out for a phone this frame.
    pub(crate) phone: bool,
    /// Pages on a phone, which reflows the text to the screen unless
    /// asked for them (View, Page View).
    pub(crate) phone_pages: bool,
    /// The binder over a phone's page (it starts closed: the text first).
    pub(crate) phone_binder: bool,
    /// The Writer's own bars last frame: a press there keeps the keyboard
    /// with the text.
    pub(crate) bars: Vec<Rect>,
    /// The Writer's row in Sue's top bar on a phone.
    pub(crate) top_bar: Option<Rect>,
}

impl Default for SueWriter {
    fn default() -> Self {
        Self {
            project: None,
            open: None,
            selected: None,
            expanded: HashSet::new(),
            view: View::Document,
            paged: true,
            composition: false,
            typewriter: false,
            zoom: 1.0,
            editor: Editor::new("sue-writer"),
            menubar: MenuBar::new(),
            menus: crate::menus::menus(),
            bound: sue_wm::shortcuts::Bound::default(),
            language: sue_lang::Watch::default(),
            binder: true,
            // Shown from View, Inspector: the page and the binder are what
            // a word processor opens with.
            inspector: false,
            comments: false,
            show_resolved: false,
            binder_width: 230.0,
            inspector_width: 260.0,
            find: None,
            dialog: None,
            renaming: None,
            session: None,
            words: (u64::MAX, 0, 0),
            figures: HashMap::new(),
            present: false,
            ready: false,
            shown: false,
            reveal: 0.0,
            drafts: HashMap::new(),
            last_compile: None,
            page_info: (0, 1),
            fit: true,
            shown_zoom: 1.0,
            toolbar_height: 36.0,
            phone: false,
            phone_pages: false,
            phone_binder: false,
            bars: Vec::new(),
            top_bar: None,
        }
    }
}

impl std::fmt::Debug for SueWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SueWriter")
            .field("project", &self.project)
            .field("open", &self.open)
            .field("view", &self.view)
            .finish_non_exhaustive()
    }
}

/// The page sizes the settings offer.
pub(crate) const PAGES: &[&str] = &["Letter", "A4", "A5", "B5", "Legal", "Trade paperback"];

impl SueWriter {
    // ----- What is open -----------------------------------------------------

    /// The project's entity.
    pub(crate) fn project_entity<'a>(&self, cx: &'a AppletContext<'_>) -> Option<&'a Entity> {
        cx.arena().get_by_id(self.project?).ok()
    }

    /// A text property of the project (or its default).
    pub(crate) fn project_text(&self, cx: &AppletContext<'_>, key: &str, default: &str) -> String {
        self.project_entity(cx)
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_owned()
    }

    fn project_number(&self, cx: &AppletContext<'_>, key: &str, default: f64) -> f64 {
        self.project_entity(cx)
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    /// The fonts the project is set in.
    pub(crate) fn fonts(&self, cx: &AppletContext<'_>) -> Fonts {
        // A project names its fonts once someone chooses them; until then it
        // is set in the best installed ones.
        let installed = Fonts::installed(12.0);
        Fonts {
            body: self.project_text(cx, project::FONT, &installed.body),
            heading: self.project_text(cx, project::HEADING_FONT, &installed.heading),
            mono: installed.mono,
            size: self.project_number(cx, project::SIZE, 12.0) as f32,
        }
    }

    /// The project's style sheet.
    pub(crate) fn sheet(&self, cx: &AppletContext<'_>) -> StyleSheet {
        let key = self.project_text(cx, project::SHEET, "standard");
        StyleSheet::named(&key, &self.fonts(cx))
    }

    /// The project's page.
    pub(crate) fn page(&self, cx: &AppletContext<'_>) -> PageSetup {
        let name = self.project_text(cx, project::PAGE, "Letter");
        let size = sue_model::page_size(&name).unwrap_or([612.0, 792.0]);
        let margin = self.project_number(cx, project::MARGIN, 72.0) as f32;
        PageSetup {
            size: [size[0] as f32, size[1] as f32],
            margins: [margin; 4],
            header: sue_model::Running::parse(&self.project_text(cx, project::HEADER, "")),
            footer: sue_model::Running::parse(&self.project_text(cx, project::FOOTER, "{page}")),
            different_first: self
                .project_entity(cx)
                .and_then(|entity| entity.property(project::DIFFERENT_FIRST))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            title: self.title(cx),
            author: self.project_text(cx, manuscript::AUTHOR, ""),
            date: project::today(),
        }
    }

    /// `true` for a single document shown like a word processor's file.
    /// Whether the binder shows: a phone's covers the page when asked for.
    pub(crate) fn binder_shown(&self) -> bool {
        if self.phone {
            self.phone_binder
        } else {
            self.binder
        }
    }

    /// Whether the text shows as pages: on a phone it reflows to the
    /// screen unless asked for pages.
    pub(crate) fn pages_shown(&self) -> bool {
        if self.phone {
            self.phone_pages
        } else {
            self.paged
        }
    }

    pub(crate) fn simple(&self, cx: &AppletContext<'_>) -> bool {
        self.project.is_none()
            || self
                .project_entity(cx)
                .and_then(|entity| entity.property(project::SIMPLE))
                .and_then(Value::as_bool)
                .unwrap_or(false)
    }

    /// The documents the text view shows: the open document, or every
    /// document under the open folder.
    pub(crate) fn shown_docs(&self, cx: &AppletContext<'_>) -> Vec<EntityId> {
        let Some(open) = self.open else {
            return Vec::new();
        };
        let arena = cx.arena();
        match arena.get_by_id(open) {
            Ok(entity) if manuscript::is_document(entity) => vec![open],
            Ok(entity) if entity.string_buffer().is_some() => vec![open],
            Ok(_) => manuscript::walk(arena, open)
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| arena.get_by_id(*id).is_ok_and(manuscript::is_document))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The folder the corkboard and outliner show.
    pub(crate) fn board_folder(&self, cx: &AppletContext<'_>) -> Option<EntityId> {
        let open = self.open?;
        let entity = cx.arena().get_by_id(open).ok()?;
        if manuscript::is_document(entity) {
            manuscript::parent(cx.arena(), open)
        } else {
            Some(open)
        }
    }

    /// Opens project `root`, showing `open` (else its first document).
    pub(crate) fn open_project(
        &mut self,
        cx: &mut AppletContext<'_>,
        root: EntityId,
        open: Option<EntityId>,
    ) {
        self.end_session(cx);
        self.project = Some(root);
        let first = open.or_else(|| {
            let draft = manuscript::folder(cx.arena(), root, role::DRAFT)?;
            manuscript::walk(cx.arena(), draft)
                .into_iter()
                .map(|(id, _)| id)
                .find(|id| cx.arena().get_by_id(*id).is_ok_and(manuscript::is_document))
        });
        self.expanded.insert(root);
        for child in manuscript::children(cx.arena(), root) {
            self.expanded.insert(child);
        }
        let mut at = first;
        while let Some(here) = at {
            self.expanded.insert(here);
            at = manuscript::parent(cx.arena(), here);
        }
        self.open = first;
        self.selected = first;
        self.view = View::Document;
        let words = manuscript::total_words(cx.arena(), root);
        self.session = Some((root, words, project::today()));
        if let Some(doc) = first {
            self.editor.select(doc, 0..0);
        }
    }

    /// Opens a document that is in no project, on its own.
    pub(crate) fn open_loose(&mut self, cx: &mut AppletContext<'_>, doc: EntityId) {
        self.end_session(cx);
        self.project = None;
        self.open = Some(doc);
        self.selected = Some(doc);
        self.view = View::Document;
        self.editor.select(doc, 0..0);
    }

    /// Opens `id` in the studio, whatever it is: a project, a document in
    /// one, or a document on its own.
    pub(crate) fn open_entity(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        match manuscript::manuscript_of(cx.arena(), id) {
            Some(root) if root == id => self.open_project(cx, root, None),
            Some(root) => self.open_project(cx, root, Some(id)),
            None if cx
                .arena()
                .get_by_id(id)
                .is_ok_and(|entity| entity.string_buffer().is_some()) =>
            {
                self.open_loose(cx, id);
            }
            None => {}
        }
    }

    /// Writes this sitting's words into the project's history.
    pub(crate) fn end_session(&mut self, cx: &mut AppletContext<'_>) {
        let Some((root, start, day)) = self.session.take() else {
            return;
        };
        let Ok(entity) = cx.arena().get_by_id(root) else {
            return;
        };
        let now = manuscript::total_words(cx.arena(), root);
        let written = now as i64 - start as i64;
        if written == 0 {
            return;
        }
        let value = project::history_with(entity, &day, written);
        let _ = cx.apply(ArenaEdit(project::set(root, project::HISTORY, Some(value))));
    }

    /// Words in the project and in this sitting (counted again only when
    /// something changed).
    pub(crate) fn counts(&mut self, cx: &AppletContext<'_>) -> (usize, i64) {
        let revision = cx.arena().revision();
        if self.words.0 != revision {
            let total = self
                .project
                .map_or(0, |root| manuscript::total_words(cx.arena(), root));
            let draft = self
                .project
                .and_then(|root| manuscript::folder(cx.arena(), root, role::DRAFT))
                .map_or(total, |draft| manuscript::total_words(cx.arena(), draft));
            self.words = (revision, total, draft);
        }
        let session = self
            .session
            .as_ref()
            .map_or(0, |(_, start, _)| self.words.1 as i64 - *start as i64);
        (self.words.2, session)
    }

    // ----- Changing things ---------------------------------------------------

    pub(crate) fn apply(
        &mut self,
        cx: &mut AppletContext<'_>,
        batch: sue_core::txn::Batch<sue_core::Arena>,
    ) -> bool {
        match cx.apply(ArenaEdit(batch)) {
            Ok(_) => true,
            Err(error) => {
                cx.fail(error.to_string());
                false
            }
        }
    }

    /// Makes a project from template `key` and opens it.
    pub(crate) fn new_project(&mut self, cx: &mut AppletContext<'_>, key: &str, title: &str) {
        let fonts = Fonts::installed(12.0);
        let (batch, root) = project::create(key, title, &fonts);
        if !self.apply(cx, batch) {
            return;
        }
        self.open_project(cx, root, None);
        cx.notify(tr!("made", title = title));
    }

    /// Adds a new document (or folder) after the selection.
    pub(crate) fn add_item(&mut self, cx: &mut AppletContext<'_>, folder: bool) {
        let Some(root) = self.project else {
            cx.notify(tr!("open-a-project-first"));
            return;
        };
        let at = self
            .selected
            .and_then(|selected| project::place_for(cx.arena(), selected))
            .or_else(|| {
                let draft = manuscript::folder(cx.arena(), root, role::DRAFT)?;
                Some((draft, manuscript::children(cx.arena(), draft).len()))
            });
        let Some((parent, index)) = at else {
            return;
        };
        let entity = if folder {
            Entity::empty(tr!("new-folder"), class::FOLDER)
        } else {
            project::new_document(&tr!("untitled"))
        };
        let id = entity.id();
        let batch = project::add(
            cx.arena(),
            parent,
            index,
            entity,
            &if folder {
                tr!("new-folder")
            } else {
                tr!("new-document")
            },
        );
        if self.apply(cx, batch) {
            self.expanded.insert(parent);
            self.selected = Some(id);
            self.renaming = Some((
                id,
                if folder {
                    tr!("new-folder")
                } else {
                    tr!("untitled")
                }
                .to_owned(),
            ));
            if !folder {
                self.open = Some(id);
                self.view = View::Document;
                self.editor.select(id, 0..0);
            }
        }
    }

    /// Takes a picture (or any entity) into the project: into the open
    /// document at the caret if it is a picture, else into Research.
    pub(crate) fn take(&mut self, cx: &mut AppletContext<'_>, entity: EntityId) {
        let is_picture = cx.arena().get_by_id(entity).is_ok_and(|found| {
            picture::is_picture(found)
                || found
                    .component(sue_core::ComponentKind::RasterTile2D)
                    .is_some()
        });
        if is_picture && self.editor.current().is_some() {
            self.view = View::Document;
            self.editor.insert_embed(cx, entity);
            cx.notify(tr!("put-picture-text"));
            return;
        }
        if let Some(root) = self.project
            && let Some(research) = manuscript::folder(cx.arena(), root, role::RESEARCH)
        {
            let end = manuscript::children(cx.arena(), research).len();
            let step = sue_core::txn::Relate::new(
                sue_core::Relation::new(research, sue_core::CHILD, entity)
                    .ordered(cx.arena().relations().next_order(research, sue_core::CHILD)),
            );
            let _ = end;
            if cx.apply(ArenaEdit(step)).is_ok() {
                self.expanded.insert(research);
                cx.notify(tr!("added-to-research"));
            }
        }
    }

    /// Imports a document file into the project (or as a project of its
    /// own).
    pub(crate) fn import(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let name = path.file_stem().map_or_else(
            || tr!("imported"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("could-not-read", path = path.display(), error = error));
                return;
            }
        };
        let sue_io::publish::Imported {
            text,
            marks,
            entities,
        } = match compile::import(&path.to_string_lossy(), &bytes) {
            Ok(read) => read,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        if self.project.is_none() {
            self.new_project(cx, "blank", &name);
        }
        let Some(root) = self.project else {
            return;
        };
        // A blank project's empty first document takes the text itself.
        let docs = self.shown_docs(cx);
        let target = docs.first().copied().filter(|doc| {
            cx.arena()
                .get_by_id(*doc)
                .ok()
                .and_then(Entity::string_buffer)
                .is_some_and(|text| text.as_str().trim().is_empty())
        });
        // The tables the text shows come first, in the same step.
        let spawn = |batch: &mut sue_core::txn::Batch<sue_core::Arena>| {
            for entity in &entities {
                batch.push(Box::new(sue_core::txn::SpawnEntity::new(entity.clone())));
            }
        };
        if let Some(doc) = target
            && let Some(step) =
                project::replace_all(cx.arena(), doc, &text, marks.clone(), &tr!("import"))
        {
            let mut batch = sue_core::txn::Batch::new(tr!("import"));
            spawn(&mut batch);
            batch.push(Box::new(step));
            self.apply(cx, batch);
            let _ = cx.apply(ArenaEdit(project::rename(doc, &name)));
            self.editor.select(doc, 0..0);
            cx.notify(tr!("imported-2", name = name));
            return;
        }
        let parent = self
            .selected
            .and_then(|selected| project::place_for(cx.arena(), selected))
            .or_else(|| {
                let draft = manuscript::folder(cx.arena(), root, role::DRAFT)?;
                Some((draft, manuscript::children(cx.arena(), draft).len()))
            });
        if let Some((parent, index)) = parent {
            let entity = project::document_entity(&name, text, marks);
            let id = entity.id();
            let mut batch = sue_core::txn::Batch::new(tr!("import"));
            spawn(&mut batch);
            batch.push(Box::new(project::add(
                cx.arena(),
                parent,
                index,
                entity,
                &tr!("import"),
            )));
            if self.apply(cx, batch) {
                self.open = Some(id);
                self.selected = Some(id);
                self.editor.select(id, 0..0);
                cx.notify(tr!("imported-2", name = name));
            }
        }
    }

    /// Where exports go: the project's own folder setting, else home.
    fn export_dir(cx: &AppletContext<'_>) -> Option<PathBuf> {
        cx.catalog.folders.get("home").cloned()
    }

    /// Compiles with the dialog's choices to `path`.
    pub(crate) fn compile_to(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        choices: &CompileChoices,
    ) {
        let Some(from) = self.compile_from(cx, choices.whole) else {
            cx.fail(tr!("nothing-to-compile"));
            return;
        };
        let options = compile::Options {
            format: FORMATS[choices.format.min(FORMATS.len() - 1)].0.to_owned(),
            from,
            titles: choices.titles,
            separator: choices.separator,
            title_page: choices.title_page,
            chapter_pages: choices.chapter_pages,
        };
        let look = self.look(cx);
        match compile::compile(cx.arena(), &options, &look) {
            Ok(bytes) => match std::fs::write(path, &bytes) {
                Ok(()) => {
                    let name =
                        compile::format_name(FORMATS[choices.format.min(FORMATS.len() - 1)].0);
                    cx.saved(
                        path.to_path_buf(),
                        tr!("kib", value = bytes.len().div_ceil(1024), name = name),
                    );
                }
                Err(error) => cx.fail(tr!("could-not-write", path = path.display(), error = error)),
            },
            Err(error) => cx.fail(error),
        }
    }

    /// What to compile from: the draft (`whole`), else what is open.
    pub(crate) fn compile_from(&self, cx: &AppletContext<'_>, whole: bool) -> Option<EntityId> {
        match self.project {
            Some(root) if whole => manuscript::folder(cx.arena(), root, role::DRAFT).or(Some(root)),
            _ => self.open,
        }
    }

    /// The project's look for compiling.
    /// The project's title (an open file's name without one).
    pub(crate) fn title(&self, cx: &AppletContext<'_>) -> String {
        self.project_entity(cx)
            .map(|entity| {
                entity
                    .property(manuscript::TITLE)
                    .and_then(Value::as_str)
                    .filter(|title| !title.is_empty())
                    .unwrap_or(entity.name())
                    .to_owned()
            })
            .or_else(|| {
                self.open
                    .and_then(|id| cx.arena().get_by_id(id).ok())
                    .map(|entity| entity.name().to_owned())
            })
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    pub(crate) fn look(&self, cx: &AppletContext<'_>) -> compile::Look {
        compile::Look {
            sheet: self.sheet(cx),
            page: self.page(cx),
            title: self.title(cx),
            author: self.project_text(cx, manuscript::AUTHOR, ""),
        }
    }

    /// Who writes comments: the project's author.
    pub(crate) fn author_name(&self, cx: &AppletContext<'_>) -> String {
        let author = self.project_text(cx, manuscript::AUTHOR, "");
        if author.trim().is_empty() {
            tr!("author")
        } else {
            author
        }
    }

    /// Puts a new comment saying `text` on the selection.
    pub(crate) fn add_comment(&mut self, cx: &mut AppletContext<'_>, text: &str) {
        let comment = sue_model::comments::comment(&self.author_name(cx), &project::today(), text);
        if self.editor.add_comment(cx, comment) {
            self.inspector = true;
            self.comments = true;
        } else {
            cx.notify(tr!("select-words-comment"));
        }
    }

    /// Replies `text` to comment `id`.
    pub(crate) fn reply(&mut self, cx: &mut AppletContext<'_>, id: EntityId, text: &str) {
        let reply = sue_model::comments::comment(&self.author_name(cx), &project::today(), text);
        let reply_id = reply.id();
        let order = cx
            .arena()
            .relations()
            .next_order(id, sue_model::comments::REPLY);
        let step = sue_core::txn::SpawnEntity::new(reply).with_relation(
            sue_core::Relation::new(id, sue_model::comments::REPLY, reply_id).ordered(order),
        );
        let _ = cx.apply(ArenaEdit(
            sue_core::txn::Batch::new(tr!("reply")).then(step),
        ));
    }

    /// Resolves comment `id`, or opens it again.
    pub(crate) fn resolve(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        let resolved =
            sue_model::comments::thread(cx.arena(), id).is_some_and(|thread| thread.resolved);
        let label = if resolved {
            tr!("reopen-comment")
        } else {
            tr!("resolve-comment")
        };
        let step = sue_core::txn::Batch::new(label).then(project::set(
            id,
            sue_model::comments::RESOLVED,
            (!resolved).then_some(Value::Bool(true)),
        ));
        let _ = cx.apply(ArenaEdit(step));
    }

    /// `true` while misspelled words are underlined.
    pub(crate) fn spell_check(&self, cx: &AppletContext<'_>) -> bool {
        cx.rice().get_bool(SPELL_CHECK).unwrap_or(true)
    }

    /// The words the project's (or the open file's) own dictionary holds.
    pub(crate) fn own_words(&self, cx: &AppletContext<'_>) -> std::collections::HashSet<String> {
        sue_type::spell::word_set(
            self.project
                .or(self.editor.current())
                .and_then(|id| cx.arena().get_by_id(id).ok())
                .and_then(|entity| entity.property(sue_type::spell::WORDS)),
        )
    }

    /// Adds `word` to the project's (or the open file's) dictionary.
    pub(crate) fn learn(&mut self, cx: &mut AppletContext<'_>, word: &str) {
        let Some(target) = self.project.or(self.editor.current()) else {
            return;
        };
        let mut words: Vec<String> = self.own_words(cx).into_iter().collect();
        if words.iter().any(|known| known == word) {
            return;
        }
        words.push(word.to_owned());
        words.sort();
        let value = Value::List(words.into_iter().map(Value::Text).collect());
        let step = sue_core::txn::Batch::new(tr!("add-to-dictionary")).then(project::set(
            target,
            sue_type::spell::WORDS,
            Some(value),
        ));
        if cx.apply(ArenaEdit(step)).is_ok() {
            cx.notify(tr!("added-to-dictionary", word = word.to_owned()));
        }
    }

    /// Selects the next misspelled word after the caret, in the documents
    /// shown.
    pub(crate) fn next_misspelling(&mut self, cx: &mut AppletContext<'_>) {
        let own = self.own_words(cx);
        let also = |word: &str| own.contains(word);
        let docs = self.editor.docs().to_vec();
        let here = (self.editor.caret().doc, self.editor.selection().end);
        let mut first = None;
        for (index, doc) in docs.iter().enumerate() {
            let text = sue_type::edit::text_of(cx.arena(), *doc).unwrap_or("");
            for range in sue_type::spell::misspelled(text, &also) {
                if (index, range.start) >= here {
                    self.editor.select(*doc, range);
                    self.editor.focus();
                    return;
                }
                first.get_or_insert((*doc, range));
            }
        }
        match first {
            Some((doc, range)) => {
                self.editor.select(doc, range);
                self.editor.focus();
            }
            None => cx.notify(tr!("no-misspellings")),
        }
    }

    /// `true` while the project (or the open file) records its changes.
    pub(crate) fn tracking(&self, cx: &AppletContext<'_>) -> bool {
        self.project
            .or(self.editor.current())
            .and_then(|id| cx.arena().get_by_id(id).ok())
            .and_then(|entity| entity.property(sue_model::changes::TRACK))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Selects the next (or previous) tracked change after the caret.
    pub(crate) fn next_change(&mut self, cx: &mut AppletContext<'_>, backward: bool) {
        let changes = self.editor.changes();
        let here = (self.editor.caret().doc, self.editor.selection().start);
        let pick = if backward {
            changes
                .iter()
                .rev()
                .find(|(doc, tracked)| (*doc, tracked.range.start) < here)
                .or(changes.last())
        } else {
            changes
                .iter()
                .find(|(doc, tracked)| (*doc, tracked.range.start) > here)
                .or(changes.first())
        };
        let Some((doc, tracked)) = pick.cloned() else {
            cx.notify(tr!("no-changes"));
            return;
        };
        if let Some(id) = self.editor.docs().get(doc).copied() {
            self.editor.select(id, tracked.range);
            self.editor.focus();
        }
    }

    /// Accepts (`keep`) or rejects the change at the caret or the ones the
    /// selection touches, or with `all` every change shown, as one undo
    /// step.
    pub(crate) fn settle_changes(&mut self, cx: &mut AppletContext<'_>, keep: bool, all: bool) {
        self.editor.commit(cx);
        let label = match (keep, all) {
            (true, false) => tr!("accept-change"),
            (false, false) => tr!("reject-change"),
            (true, true) => tr!("accept-all-changes"),
            (false, true) => tr!("reject-all-changes"),
        };
        let label = label.as_str();
        let mut batch = sue_core::txn::Batch::new(label);
        let selection = self.editor.selection();
        let current = self.editor.current();
        let mut count = 0;
        for doc in self.editor.docs().to_vec() {
            if !all && Some(doc) != current {
                continue;
            }
            let Some(text) = sue_type::edit::text_of(cx.arena(), doc).map(str::to_owned) else {
                continue;
            };
            let marks = sue_type::edit::marks_of(cx.arena(), doc);
            let range = if all {
                0..text.len()
            } else {
                selection.clone()
            };
            let before = sue_model::changes::changes(&marks).len();
            let (text, marks) = if keep {
                sue_model::changes::accept(&text, &marks, range)
            } else {
                sue_model::changes::reject(&text, &marks, range)
            };
            let settled = before - sue_model::changes::changes(&marks).len();
            if settled == 0 {
                continue;
            }
            count += settled;
            if let Some(step) = project::replace_all(cx.arena(), doc, &text, marks, label) {
                batch.push(Box::new(step));
            }
        }
        if count == 0 {
            cx.notify(tr!("no-changes-here"));
        } else if self.apply(cx, batch) {
            cx.notify(if keep {
                tr!("accepted-count", count = count)
            } else {
                tr!("rejected-count", count = count)
            });
        }
    }

    /// Selects the next (or previous) comment's words after the caret.
    pub(crate) fn next_comment(&mut self, cx: &mut AppletContext<'_>, backward: bool) {
        let comments = self.editor.comments();
        let here = (self.editor.caret().doc, self.editor.selection().start);
        let pick = if backward {
            comments
                .iter()
                .rev()
                .find(|(doc, range, _)| (*doc, range.start) < here)
                .or(comments.last())
        } else {
            comments
                .iter()
                .find(|(doc, range, _)| (*doc, range.start) > here)
                .or(comments.first())
        };
        let Some((doc, range, _)) = pick.cloned() else {
            cx.notify(tr!("no-comments"));
            return;
        };
        if let Some(id) = self.editor.docs().get(doc).copied() {
            self.editor.select(id, range);
            self.editor.focus();
        }
    }

    /// Asks where to save a compile in `format`.
    pub(crate) fn ask_compile(&mut self, cx: &mut AppletContext<'_>, choices: CompileChoices) {
        let (key, _, ext) = FORMATS[choices.format.min(FORMATS.len() - 1)];
        let name = compile::format_name(key);
        let title = self.look(cx).title;
        let file = format!("{}.{ext}", sanitize(&title));
        let pick = FilePick::save(
            Some(me()),
            &format!("compile.{key}"),
            &tr!("export-as-2", name = name),
            &file,
        )
        .extensions(&[ext])
        .starting_in(Self::export_dir(cx));
        self.dialog = None;
        self.last_compile = Some(choices);
        cx.request(ShellRequest::PickFile(pick));
    }

    // ----- Commands ----------------------------------------------------------------

    /// Runs `command`. Returns `false` if there is no such command.
    #[allow(
        clippy::too_many_lines,
        reason = "one match over the studio's commands"
    )]
    pub(crate) fn run(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        let sheet = self.sheet(cx);
        match command {
            "file.new" => {
                self.dialog = Some(Dialog::NewProject {
                    template: 1,
                    title: tr!("untitled"),
                });
            }
            "binder.new_doc" => self.add_item(cx, false),
            "binder.new_folder" => self.add_item(cx, true),
            "file.open" | "file.import" => {
                let pick = FilePick::open(Some(me()), "import", &tr!("import-a-document"))
                    .extensions(&["docx", "md", "markdown", "txt", "html", "htm", "fountain"])
                    .starting_in(Self::export_dir(cx));
                cx.request(ShellRequest::PickFile(pick));
            }
            "compile.open" => {
                let choices = self.last_compile.clone().unwrap_or_else(|| CompileChoices {
                    format: 0,
                    titles: Titles::Folders,
                    separator: if sheet.key == "manuscript" {
                        Separator::SceneBreak
                    } else {
                        Separator::None
                    },
                    title_page: sheet.key == "manuscript",
                    chapter_pages: sheet.key != "standard",
                    whole: true,
                });
                self.dialog = Some(Dialog::Compile(choices));
            }
            command
                if command.starts_with("compile.")
                    && FORMATS
                        .iter()
                        .any(|(key, ..)| command == format!("compile.{key}")) =>
            {
                let format = FORMATS
                    .iter()
                    .position(|(key, ..)| command == format!("compile.{key}"))
                    .unwrap_or(0);
                let mut choices = self.last_compile.clone().unwrap_or(CompileChoices {
                    format,
                    titles: Titles::Folders,
                    separator: Separator::None,
                    title_page: false,
                    chapter_pages: sheet.key != "standard",
                    whole: true,
                });
                choices.format = format;
                self.ask_compile(cx, choices);
            }
            "project.settings" => {
                let fonts = self.fonts(cx);
                self.dialog = Some(Dialog::Settings(Settings {
                    title: self.look(cx).title,
                    author: self.project_text(cx, manuscript::AUTHOR, ""),
                    sheet: sheet.key.clone(),
                    page: self.project_text(cx, project::PAGE, "Letter"),
                    margin: self.project_number(cx, project::MARGIN, 72.0) as f32,
                    font: fonts.body,
                    heading_font: fonts.heading,
                    size: fonts.size,
                    target: self.project_number(cx, manuscript::TARGET, 0.0) as i64,
                    session: self.project_number(cx, project::SESSION_TARGET, 500.0) as i64,
                    font_filter: String::new(),
                }));
            }
            "review.comment" => {
                if self.editor.current().is_some() {
                    self.dialog = Some(Dialog::Comment(String::new()));
                }
            }
            "review.spelling" => {
                let on = !self.spell_check(cx);
                let _ = cx.edit_rice(sue_rice::SetRiceValue::new(
                    SPELL_CHECK,
                    sue_rice::RiceValue::Bool(on),
                ));
            }
            "review.next_misspelling" => self.next_misspelling(cx),
            "review.track" => {
                let target = self.project.or(self.editor.current());
                if let Some(target) = target {
                    let on = !self.tracking(cx);
                    let step = sue_core::txn::Batch::new(if on {
                        tr!("record-changes")
                    } else {
                        tr!("stop-recording-changes")
                    })
                    .then(project::set(
                        target,
                        sue_model::changes::TRACK,
                        on.then_some(Value::Bool(true)),
                    ));
                    if cx.apply(ArenaEdit(step)).is_ok() {
                        cx.notify(if on {
                            tr!("recording-changes")
                        } else {
                            tr!("no-longer-recording-changes")
                        });
                    }
                }
            }
            "review.next_change" | "review.previous_change" => {
                self.next_change(cx, command == "review.previous_change");
            }
            "review.accept" | "review.reject" | "review.accept_all" | "review.reject_all" => {
                let keep = command.starts_with("review.accept");
                let all = command.ends_with("_all");
                self.settle_changes(cx, keep, all);
            }
            "review.comments" => {
                let shown = self.inspector && self.comments;
                self.inspector = !shown;
                self.comments = !shown;
            }
            "review.next_comment" | "review.previous_comment" => {
                self.next_comment(cx, command == "review.previous_comment");
            }
            "review.resolve" => match self.editor.comment_here() {
                Some(id) => self.resolve(cx, id),
                None => cx.notify(tr!("put-caret-comments-words-first")),
            },
            "review.delete_comment" => match self.editor.comment_here() {
                Some(id) => {
                    self.editor.remove_comment(cx, id);
                }
                None => cx.notify(tr!("put-caret-comments-words-first")),
            },
            "review.delete_all_comments" => {
                for (_, _, id) in self.editor.comments() {
                    self.editor.remove_comment(cx, id);
                }
            }
            "insert.contents" => {
                self.dialog = Some(Dialog::Contents(sue_model::contents::Contents::default()));
            }
            "insert.header_footer" => {
                let page = self.page(cx);
                self.dialog = Some(Dialog::Furniture(Furniture {
                    header: page.header,
                    footer: page.footer,
                    different_first: page.different_first,
                }));
            }
            "project.statistics" => self.dialog = Some(Dialog::Statistics),
            "help.shortcuts" => self.dialog = Some(Dialog::Shortcuts),
            "view.document" => self.view = View::Document,
            "view.corkboard" => self.view = View::Corkboard,
            "view.outliner" => self.view = View::Outliner,
            "view.page" | "view.draft" => {
                let pages = command == "view.page";
                if self.phone {
                    self.phone_pages = pages;
                } else {
                    self.paged = pages;
                }
                self.view = View::Document;
            }
            "view.composition" => {
                self.composition = !self.composition;
                self.view = View::Document;
                self.editor.focus();
            }
            "view.typewriter" => self.typewriter = !self.typewriter,
            "view.binder" => {
                if self.phone {
                    self.phone_binder = !self.phone_binder;
                } else {
                    self.binder = !self.binder;
                }
            }
            "view.inspector" => self.inspector = !self.inspector,
            "view.zoom_in" => {
                self.fit = false;
                self.zoom = (self.shown_zoom * 1.1).min(4.0);
            }
            "view.zoom_out" => {
                self.fit = false;
                self.zoom = (self.shown_zoom / 1.1).max(0.25);
            }
            "view.zoom_reset" => {
                self.fit = false;
                self.zoom = 1.0;
            }
            "view.zoom_fit" => self.fit = !self.fit,
            "edit.find" => self.open_find(false),
            "edit.replace" => self.open_find(true),
            "edit.find_next" => self.find_next(cx, false),
            "edit.find_previous" => self.find_next(cx, true),
            "edit.undo" => {
                self.editor.commit(cx);
                let _ = cx.undo(false);
            }
            "edit.redo" => {
                self.editor.commit(cx);
                let _ = cx.undo(true);
            }
            "table.insert" => {
                self.dialog = Some(Dialog::Table {
                    rows: 3,
                    columns: 3,
                    heading: true,
                });
            }
            size if size.starts_with("table.insert.") => {
                // `table.insert.3x4`: three rows of four columns at once,
                // as a size picker asks.
                let size = &size["table.insert.".len()..];
                let Some((rows, columns)) = size
                    .split_once('x')
                    .and_then(|(rows, columns)| Some((rows.parse().ok()?, columns.parse().ok()?)))
                else {
                    return false;
                };
                self.editor.insert_table(cx, rows, columns, 0);
                self.editor.focus();
            }
            "table.row_above"
            | "table.row_below"
            | "table.column_left"
            | "table.column_right"
            | "table.delete_row"
            | "table.delete_column"
            | "table.delete"
            | "table.header" => {
                let edit = match command {
                    "table.row_above" => TableEdit::RowAbove,
                    "table.row_below" => TableEdit::RowBelow,
                    "table.column_left" => TableEdit::ColumnLeft,
                    "table.column_right" => TableEdit::ColumnRight,
                    "table.delete_row" => TableEdit::DeleteRow,
                    "table.delete_column" => TableEdit::DeleteColumn,
                    "table.delete" => TableEdit::DeleteTable,
                    _ => TableEdit::HeaderRow,
                };
                if self.editor.cell().is_none() {
                    cx.notify(tr!("click-in-a-table-first"));
                } else {
                    self.editor.edit_table(cx, edit);
                }
            }
            "table.to_text" => {
                if !self.editor.table_to_text(cx) {
                    cx.notify(tr!("click-in-a-table-first"));
                }
            }
            "table.from_text" => {
                self.editor.text_to_table(cx);
            }
            "format.bold" => self.editor.toggle(cx, kind::BOLD, &tr!("bold")),
            "format.italic" => self.editor.toggle(cx, kind::ITALIC, &tr!("italic")),
            "format.underline" => self.editor.toggle(cx, kind::UNDERLINE, &tr!("underline")),
            "format.strike" => self.editor.toggle(cx, kind::STRIKE, &tr!("strikethrough")),
            "format.code" => self.editor.toggle(cx, kind::CODE, &tr!("code")),
            case if case.starts_with("format.case.") => {
                let Some(case) = sue_type::case::Case::parse(&case["format.case.".len()..]) else {
                    return false;
                };
                self.editor.change_case(cx, case);
            }
            "format.caps" => self.editor.toggle(cx, kind::CAPS, &tr!("all-capitals")),
            "format.grow" | "format.shrink" => {
                let (para, chars) = self.editor.look_here(&sheet);
                let now = chars.size.unwrap_or(para.size);
                let next = font_step(now, command == "format.grow");
                self.editor.set_char(
                    cx,
                    kind::SIZE,
                    Some(Value::Float(f64::from(next))),
                    &tr!("font-size"),
                );
            }
            mark if mark.starts_with("insert.mark.") => {
                let ch = match &mark["insert.mark.".len()..] {
                    "nbsp" => "\u{A0}",
                    "nbhyphen" => "\u{2011}",
                    "soft_hyphen" => "\u{AD}",
                    "narrow" => "\u{202F}",
                    "zwsp" => "\u{200B}",
                    _ => return false,
                };
                self.editor.insert(cx, ch);
            }
            "format.small_caps" => self
                .editor
                .toggle(cx, kind::SMALL_CAPS, &tr!("small-capitals")),
            "format.superscript" | "format.subscript" => {
                let script = if command.ends_with("superscript") {
                    "super"
                } else {
                    "sub"
                };
                let (_, look) = self.editor.look_here(&sheet);
                let on = look.script == if script == "super" { 1 } else { -1 };
                self.editor.set_char(
                    cx,
                    kind::SCRIPT,
                    (!on).then(|| Value::Text(script.to_owned())),
                    &tr!("raise-or-lower"),
                );
            }
            "format.clear" => self.editor.clear_formatting(cx),
            command if command.starts_with("format.highlight.") => {
                let color = match &command["format.highlight.".len()..] {
                    "yellow" => Some("#fff176"),
                    "green" => Some("#a5e3a1"),
                    "blue" => Some("#9fd2ff"),
                    "pink" => Some("#ffb3d1"),
                    _ => None,
                };
                self.editor.set_char(
                    cx,
                    kind::HIGHLIGHT,
                    color.map(|hex| Value::Text(hex.to_owned())),
                    &tr!("highlight"),
                );
            }
            command if command.starts_with("format.color.") => {
                let hex = &command["format.color.".len()..];
                let value = (hex != "none").then(|| Value::Text(format!("#{hex}")));
                self.editor
                    .set_char(cx, kind::COLOR, value, &tr!("text-color"));
            }
            command if command.starts_with("style.") => {
                let key = &command["style.".len()..];
                self.editor.set_style(cx, key, &tr!("paragraph-style"));
            }
            command if command.starts_with("align.") => {
                let align = &command["align.".len()..];
                let value = (align != "left").then(|| Value::Text(align.to_owned()));
                self.editor
                    .set_paragraph(cx, kind::ALIGN, value, &tr!("alignment"));
            }
            command if command.starts_with("spacing.") => {
                let spacing: f64 = command["spacing.".len()..].parse().unwrap_or(1.0);
                self.editor.set_paragraph(
                    cx,
                    kind::LINE_SPACING,
                    Some(Value::Float(spacing)),
                    &tr!("line-spacing"),
                );
            }
            "list.bullet" | "list.number" => {
                let list = &command["list.".len()..];
                let (para, _) = self.editor.look_here(&sheet);
                let on = para.list.map(sue_type::ListKind::name) == Some(list);
                self.editor.set_paragraph(
                    cx,
                    kind::LIST,
                    (!on).then(|| Value::Text(list.to_owned())),
                    &tr!("list"),
                );
            }
            "indent.more" | "indent.less" => {
                let (para, _) = self.editor.look_here(&sheet);
                if para.list.is_some() {
                    let level = if command.ends_with("more") {
                        (para.level + 1).min(8)
                    } else {
                        para.level.saturating_sub(1)
                    };
                    self.editor.set_paragraph(
                        cx,
                        kind::LEVEL,
                        Some(Value::Int(level as i64)),
                        &tr!("list-level"),
                    );
                } else {
                    let style = sheet.get(&para.style).indent;
                    let own = para.indent - style;
                    let next = if command.ends_with("more") {
                        own + 36.0
                    } else {
                        (own - 36.0).max(0.0)
                    };
                    self.editor.set_paragraph(
                        cx,
                        kind::INDENT,
                        (next > 0.0).then(|| Value::Float(f64::from(style + next))),
                        &tr!("indent"),
                    );
                }
            }
            "insert.page_break" => {
                self.editor.insert(cx, "\n");
                self.editor.set_paragraph(
                    cx,
                    kind::PAGE_BREAK,
                    Some(Value::Bool(true)),
                    &tr!("page-break"),
                );
            }
            "insert.scene_break" => {
                self.editor.insert(cx, "\n#\n");
                // The caret sits after the break; style the line above.
                let caret = self.editor.caret();
                if let Some(doc) = self.editor.current() {
                    let at = caret.offset.saturating_sub(2);
                    self.editor.select(doc, at..at);
                    self.editor.set_style(
                        cx,
                        if sheet.has("break") {
                            "break"
                        } else {
                            sue_type::BODY
                        },
                        &tr!("scene-break"),
                    );
                    self.editor.select(doc, caret.offset..caret.offset);
                }
            }
            "insert.date" => {
                let date = project::today();
                self.editor.insert(cx, &date);
            }
            "insert.link" => {
                let (_, look) = self.editor.look_here(&sheet);
                self.dialog = Some(Dialog::Link(
                    look.link.unwrap_or_else(|| "https://".to_owned()),
                ));
            }
            "insert.footnote" => {
                if self.editor.selection().is_empty() {
                    cx.notify(tr!("select-word-footnote-follows"));
                } else {
                    self.dialog = Some(Dialog::Footnote(String::new()));
                }
            }
            "insert.picture" => {
                let pick = FilePick::open(Some(me()), "picture", &tr!("insert-a-picture"))
                    .extensions(&[
                        "png", "jpg", "jpeg", "gif", "bmp", "webp", "tga", "tif", "tiff",
                    ])
                    .starting_in(Self::export_dir(cx));
                cx.request(ShellRequest::PickFile(pick));
            }
            "project.split" => {
                if let Some(doc) = self.editor.current() {
                    let at = self.editor.selection().start;
                    if let Some((batch, new)) = project::split(cx.arena(), doc, at, "Untitled")
                        && self.apply(cx, batch)
                    {
                        self.selected = Some(new);
                        self.open = Some(new);
                        self.editor.select(new, 0..0);
                    }
                }
            }
            "project.merge" => {
                // Joins the selected document with the one after it.
                if let Some(doc) = self.selected
                    && let Some(parent) = manuscript::parent(cx.arena(), doc)
                {
                    let siblings = manuscript::children(cx.arena(), parent);
                    let at = siblings.iter().position(|id| *id == doc).unwrap_or(0);
                    if let Some(next) = siblings.get(at + 1)
                        && let Some(batch) = project::merge(cx.arena(), &[doc, *next])
                    {
                        self.apply(cx, batch);
                    }
                }
            }
            "project.duplicate" => {
                if let Some(id) = self.selected
                    && let Some(batch) = project::duplicate(cx.arena(), id)
                {
                    self.apply(cx, batch);
                }
            }
            "project.trash" => {
                if let Some(id) = self.selected
                    && let Some(batch) = project::trash(cx.arena(), id)
                {
                    let parent = manuscript::parent(cx.arena(), id);
                    if self.apply(cx, batch) {
                        if self.open == Some(id) {
                            self.open = parent;
                        }
                        self.selected = parent;
                    }
                }
            }
            "project.empty_trash" => {
                if let Some(root) = self.project
                    && let Some(batch) = project::empty_trash(cx.arena(), root)
                {
                    self.apply(cx, batch);
                }
            }
            "project.snapshot" => {
                if let Some(doc) = self.editor.current().or(self.selected) {
                    let taken = project::today();
                    if let Some(batch) = project::take_snapshot(
                        cx.arena(),
                        doc,
                        &tr!("snapshot", taken = taken),
                        &taken,
                    ) && self.apply(cx, batch)
                    {
                        cx.notify(tr!("took-a-snapshot"));
                    }
                }
            }
            "project.include" => {
                if let Some(id) = self.selected {
                    let on = cx.arena().get_by_id(id).is_ok_and(manuscript::included);
                    let _ = cx.apply(ArenaEdit(project::set(
                        id,
                        manuscript::INCLUDE,
                        Some(Value::Bool(!on)),
                    )));
                }
            }
            command if command.starts_with("status.") || command.starts_with("label.") => {
                let (key, value) = if let Some(value) = command.strip_prefix("status.") {
                    (manuscript::STATUS, value)
                } else {
                    (manuscript::LABEL, &command["label.".len()..])
                };
                if let Some(id) = self.selected {
                    let value = (value != "none").then(|| Value::Text(value.to_owned()));
                    let _ = cx.apply(ArenaEdit(project::set(id, key, value)));
                }
            }
            "studio.leave" => cx.request(ShellRequest::Hub),
            _ => return false,
        }
        true
    }

    fn open_find(&mut self, replacing: bool) {
        let query = self.editor.selected_text().to_owned();
        let find = self.find.get_or_insert_with(Find::default);
        if !query.is_empty() && !query.contains('\n') {
            find.query = query;
        }
        find.replacing |= replacing;
        find.focus = true;
    }

    /// The find bar's query, ready to search with (`None`, with the reason
    /// shown in the bar, when its regular expression is wrong).
    pub(crate) fn finder(&mut self) -> Option<Finder> {
        let find = self.find.as_mut()?;
        match Finder::new(&Query {
            text: find.query.clone(),
            case: find.case,
            words: find.words,
            regex: find.regex,
        }) {
            Ok(finder) => Some(finder),
            Err(error) => {
                find.message = error;
                None
            }
        }
    }

    /// Selects the next (or previous) match, across the project's documents
    /// when the find bar says so.
    pub(crate) fn find_next(&mut self, cx: &mut AppletContext<'_>, backward: bool) {
        let Some(find) = self.find.clone() else {
            return;
        };
        if find.query.is_empty() {
            return;
        }
        let Some(finder) = self.finder() else {
            return;
        };
        let mut docs = self.shown_docs(cx);
        if find.project
            && let Some(root) = self.project
        {
            docs = manuscript::walk(cx.arena(), root)
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| cx.arena().get_by_id(*id).is_ok_and(manuscript::is_document))
                .collect();
        }
        if docs.is_empty() {
            return;
        }
        let current = self
            .editor
            .current()
            .and_then(|doc| docs.iter().position(|id| *id == doc))
            .unwrap_or(0);
        let selection = self.editor.selection();
        let count = docs.len();
        for step in 0..=count {
            let index = if backward {
                (current + count * 2 - step) % count
            } else {
                (current + step) % count
            };
            let doc = docs[index];
            let text = sue_type::edit::text_of(cx.arena(), doc)
                .unwrap_or("")
                .to_owned();
            let found = finder.matches(&text);
            let pick = if step == 0 {
                if backward {
                    found
                        .iter()
                        .rev()
                        .find(|range| range.start < selection.start)
                        .cloned()
                } else {
                    found
                        .iter()
                        .find(|range| {
                            range.start > selection.start
                                || (range.start == selection.start && selection.is_empty())
                        })
                        .cloned()
                }
            } else if backward {
                found.last().cloned()
            } else {
                found.first().cloned()
            };
            if let Some(range) = pick {
                if !self.shown_docs(cx).contains(&doc) {
                    self.open = Some(doc);
                    self.selected = Some(doc);
                    self.view = View::Document;
                }
                self.editor.select(doc, range);
                if let Some(find) = &mut self.find {
                    find.message.clear();
                }
                return;
            }
        }
        if let Some(find) = &mut self.find {
            find.message = tr!("not-found");
        }
    }

    /// Replaces the selected match and finds the next.
    pub(crate) fn replace_one(&mut self, cx: &mut AppletContext<'_>) {
        let Some(find) = self.find.clone() else {
            return;
        };
        let Some(finder) = self.finder() else {
            return;
        };
        let selected = self.editor.selected_text().to_owned();
        if finder.is_match(&selected) {
            let with = finder.replacement(&selected, 0..selected.len(), &find.replace);
            self.editor.insert(cx, &with);
        }
        self.find_next(cx, false);
    }

    /// Replaces every match (in the shown documents, or the project).
    pub(crate) fn replace_all(&mut self, cx: &mut AppletContext<'_>) {
        let Some(find) = self.find.clone() else {
            return;
        };
        let Some(finder) = self.finder() else {
            return;
        };
        let mut docs = self.shown_docs(cx);
        if find.project
            && let Some(root) = self.project
        {
            docs = manuscript::walk(cx.arena(), root)
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| cx.arena().get_by_id(*id).is_ok_and(manuscript::is_document))
                .collect();
        }
        let mut batch = sue_core::txn::Batch::new(tr!("replace-all"));
        let mut total = 0;
        for doc in docs {
            let text = sue_type::edit::text_of(cx.arena(), doc)
                .unwrap_or("")
                .to_owned();
            let found = finder.matches(&text);
            if found.is_empty() {
                continue;
            }
            total += found.len();
            let mut marks = sue_type::edit::marks_of(cx.arena(), doc);
            let mut new = text.clone();
            for range in found.into_iter().rev() {
                let with = finder.replacement(&text, range.clone(), &find.replace);
                let before = new.clone();
                marks.delete_text(&before, range.clone());
                new.replace_range(range.clone(), "");
                let before = new.clone();
                marks.insert_text(&before, range.start, &with, None);
                new.insert_str(range.start, &with);
            }
            if let Some(step) =
                project::replace_all(cx.arena(), doc, &new, marks, &tr!("replace-all"))
            {
                batch.push(Box::new(step));
            }
        }
        if total > 0 && self.apply(cx, batch) {
            if let Some(find) = &mut self.find {
                find.message = tr!("replaced", total = total);
            }
        } else if let Some(find) = &mut self.find {
            find.message = tr!("not-found");
        }
    }

    // ----- Keys -------------------------------------------------------------------

    /// The studio's shortcuts, Word's and Scrivener's.
    pub(crate) fn shortcuts() -> Vec<(Modifiers, Key, &'static str)> {
        let command = Modifiers::COMMAND;
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let alt = Modifiers::COMMAND | Modifiers::ALT;
        vec![
            (shift, Key::N, "binder.new_folder"),
            (command, Key::N, "binder.new_doc"),
            (shift, Key::E, "compile.open"),
            (command, Key::P, "compile.pdf"),
            (command, Key::B, "format.bold"),
            (command, Key::I, "format.italic"),
            (command, Key::U, "format.underline"),
            (shift, Key::X, "format.strike"),
            (shift, Key::K, "project.split"),
            (shift, Key::Plus, "format.superscript"),
            (shift, Key::Equals, "format.superscript"),
            (command, Key::Equals, "format.subscript"),
            (command, Key::Space, "format.clear"),
            (command, Key::E, "align.center"),
            (command, Key::L, "align.left"),
            (command, Key::R, "align.right"),
            (command, Key::J, "align.justify"),
            (alt, Key::Num1, "style.heading1"),
            (alt, Key::Num2, "style.heading2"),
            (alt, Key::Num3, "style.heading3"),
            (alt, Key::Num0, "style.body"),
            (alt, Key::F, "insert.footnote"),
            (alt, Key::C, "review.comment"),
            (shift, Key::C, "review.track"),
            (command, Key::CloseBracket, "format.grow"),
            (command, Key::OpenBracket, "format.shrink"),
            (shift, Key::Space, "insert.mark.nbsp"),
            (Modifiers::NONE, Key::F7, "review.next_misspelling"),
            (command, Key::Enter, "insert.page_break"),
            (command, Key::K, "insert.link"),
            (command, Key::F, "edit.find"),
            (command, Key::H, "edit.replace"),
            (Modifiers::SHIFT, Key::F3, "edit.find_previous"),
            (Modifiers::NONE, Key::F3, "edit.find_next"),
            (Modifiers::NONE, Key::F11, "view.composition"),
            (command, Key::Num1, "view.document"),
            (command, Key::Num2, "view.corkboard"),
            (command, Key::Num3, "view.outliner"),
            (command, Key::Num5, "project.snapshot"),
            (shift, Key::L, "list.bullet"),
        ]
    }

    pub(crate) fn keys(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        if self.dialog.is_some() || self.menubar.is_open() || self.renaming.is_some() {
            return;
        }
        // Shortcuts work while typing; other text fields keep theirs. The
        // chords are the settings' (Settings, Keymaps), any number each.
        let other_field = ctx.text_edit_focused() && !self.editor.has_focus(ctx);
        let table: Vec<sue_wm::shortcuts::Shortcut> = shortcut_table()
            .iter()
            .filter(|shortcut| {
                !other_field
                    || matches!(
                        shortcut.command,
                        "edit.find" | "edit.replace" | "view.composition"
                    )
            })
            .cloned()
            .collect();
        let taken =
            ctx.input_mut(|input| sue_wm::shortcuts::take(input, cx.rice(), KEYS_SECTION, &table));
        for name in taken {
            self.run(name, cx);
        }
        if self.composition
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.composition = false;
        }
        if self.find.is_some()
            && !other_field
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.find = None;
            self.editor.focus();
        }
    }
}

/// A picture file's pixels.
fn read_picture(bytes: &[u8]) -> Result<sue_core::RasterTile2D, String> {
    if let Ok(png) = sue_io::decode_png(bytes) {
        return Ok(png.tile);
    }
    if let Ok(jpeg) = sue_io::decode_jpeg(bytes) {
        return Ok(jpeg.tile);
    }
    sue_io::decode_picture(bytes, None)
}

/// The settings section Sue Writer's shortcuts live in.
pub(crate) const KEYS_SECTION: &str = "sue_writer_keys";

/// Sue Writer's shortcuts with their default chords, each command once,
/// labeled as its menu item.
pub(crate) fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: std::sync::OnceLock<Vec<sue_wm::shortcuts::Shortcut>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let menus = crate::menus::menus();
        fn find(items: &[sue_wm::MenuItem], command: &str) -> Option<String> {
            items.iter().find_map(|item| match item {
                sue_wm::MenuItem::Action {
                    label,
                    command: found,
                    ..
                } if found == command => Some(label.trim_end_matches("...").to_owned()),
                sue_wm::MenuItem::Sub { items, .. } => find(items, command),
                _ => None,
            })
        }
        sue_wm::shortcuts::table(SueWriter::shortcuts(), |command| {
            menus
                .iter()
                .find_map(|menu| find(&menu.items, command))
                .unwrap_or_else(|| match command {
                    "style.heading1" => tr!("heading-1"),
                    "style.heading2" => tr!("heading-2"),
                    "style.heading3" => tr!("heading-3"),
                    "style.body" => tr!("body-text"),
                    "edit.find_next" => tr!("find-next"),
                    "edit.find_previous" => tr!("find-previous"),
                    other => other.to_owned(),
                })
        })
    })
}

/// A file name made of `title`.
pub(crate) fn sanitize(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|ch| {
            if "<>:\"/\\|?*".contains(ch) || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.').to_owned();
    if clean.is_empty() {
        tr!("untitled")
    } else {
        clean
    }
}

impl SueApplet for SueWriter {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Sue Writer", AppletRole::Studio, "\u{1F4DD}")
            .localized("writer");
        manifest.summary = "Write documents, novels, scripts and papers, then publish them as PDF, Word, EPUB and more.".to_owned();
        manifest.order = 26;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![
            RiceDeclaration::new(
                "sue_writer.theme",
                RiceParameter::EnumChoice {
                    value: sue_wm::STUDIO_THEME_SUE.to_owned(),
                    options: [
                        sue_wm::STUDIO_THEME_SUE,
                        "gray",
                        "void",
                        "graphite",
                        "paper",
                    ]
                    .map(String::from)
                    .to_vec(),
                },
                "Sue Writer's colors: Sue's own theme (follows it) or a preset",
            ),
            RiceDeclaration::new(
                "sue_writer.smart_quotes",
                RiceParameter::Bool { value: true },
                "Turn straight quotes into curly ones while typing",
            ),
            RiceDeclaration::new(
                "sue_writer.dark_paper",
                RiceParameter::Bool { value: false },
                "Show the page in the theme's dark colors instead of white paper",
            ),
            RiceDeclaration::new(
                SPELL_CHECK,
                RiceParameter::Bool { value: true },
                "Underline misspelled words while typing",
            ),
        ]
        .into_iter()
        .chain(sue_wm::shortcuts::declarations(
            KEYS_SECTION,
            shortcut_table(),
        ))
        .collect()
    }

    fn accepts(&self, entity: &Entity) -> bool {
        manuscript::is_manuscript(entity) || manuscript::is_document(entity)
    }

    fn focus(&self) -> Option<EntityId> {
        self.project.or(self.open)
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.frame(ui, cx);
    }

    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        self.menu_bar(ui, cx);
    }

    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        // Every chord of the studio's shortcuts wins while it is open (Bold
        // and Justify, as in every word processor).
        self.bound.chords(shortcut_table())
    }

    fn captures_keys(&self) -> bool {
        self.dialog.is_some()
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        project::TEMPLATES
            .iter()
            .map(|(key, ..)| {
                sue_wm::FileTemplate::new(tr!("writing"), project::template_name(key), *key)
            })
            .collect()
    }

    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        let label = project::TEMPLATES
            .iter()
            .find(|(known, ..)| *known == key)
            .map_or_else(
                || tr!("untitled"),
                |(known, ..)| project::template_name(known),
            );
        self.new_project(cx, key, &label);
        cx.request(ShellRequest::Open(me()));
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        // Looking finds the installed fonts in the background, so they are
        // there by the time anyone writes.
        let _ = sue_wm::fonts::ready();
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) && self.shown {
            self.shown = false;
            self.composition = false;
            self.end_session(cx);
        }
        sue_wm::Tick::default()
    }

    fn commands(&self) -> Vec<PaletteEntry> {
        [
            (tr!("sue-writer-new-project-ellipsis"), "file.new", ""),
            (
                tr!("sue-writer-import-a-document-ellipsis"),
                "file.import",
                "",
            ),
            (
                tr!("sue-writer-compile-ellipsis"),
                "compile.open",
                "Ctrl+Shift+E",
            ),
            (
                tr!("sue-writer-export-as-pdf-ellipsis"),
                "compile.pdf",
                "Ctrl+P",
            ),
            (
                tr!("sue-writer-composition-mode"),
                "view.composition",
                "F11",
            ),
            (
                tr!("sue-writer-writing-statistics"),
                "project.statistics",
                "",
            ),
            (tr!("sue-writer-keyboard-shortcuts"), "help.shortcuts", ""),
        ]
        .into_iter()
        .map(|(label, command, hint)| PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: hint.to_owned(),
        })
        .collect()
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me()));
        }
        self.run(name, cx)
    }

    fn navigate(&mut self, target: &str) -> bool {
        if let Some(title) = target.strip_prefix("menu/") {
            return self.menubar.open(&self.menus, title);
        }
        match target {
            "dialog/close" => {
                self.dialog = None;
                self.menubar.close();
                true
            }
            "view/document" => {
                self.view = View::Document;
                self.composition = false;
                true
            }
            "view/corkboard" => {
                self.view = View::Corkboard;
                true
            }
            "view/outliner" => {
                self.view = View::Outliner;
                true
            }
            "view/composition" => {
                self.composition = true;
                true
            }
            // The caret at the end of the open text, with the keyboard.
            "text/end" => {
                if let Some(doc) = self.editor.current() {
                    let end = self.editor.text().len();
                    self.editor.select(doc, end..end);
                }
                self.view = View::Document;
                true
            }
            "dialog/compile" => {
                self.dialog = Some(Dialog::Compile(CompileChoices {
                    format: 0,
                    titles: Titles::Folders,
                    separator: Separator::SceneBreak,
                    title_page: true,
                    chapter_pages: true,
                    whole: true,
                }));
                true
            }
            _ => false,
        }
    }

    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<SueAction> {
        // Sue Writer's own commands are in its menus, right beside the Sue
        // button.
        if current {
            return Vec::new();
        }
        let Some(entity) = focus else {
            return Vec::new();
        };
        // Text from anywhere opens in Sue Writer; pictures go into the
        // text being written.
        if manuscript::is_manuscript(entity)
            || entity
                .string_buffer()
                .is_some_and(|_| entity.meta("kind") != Some("code"))
        {
            return vec![SueAction::new(
                sue_menu::SEND_TO,
                "Sue Writer",
                ShellRequest::OpenIn {
                    entity: entity.id(),
                    applet: me(),
                },
            )];
        }
        if picture::is_picture(entity)
            || entity
                .component(sue_core::ComponentKind::RasterTile2D)
                .is_some()
        {
            return vec![SueAction::new(
                sue_menu::SEND_TO,
                "Sue Writer",
                ShellRequest::SendEntity {
                    to: me(),
                    purpose: "figure".to_owned(),
                    entity: entity.id(),
                },
            )];
        }
        Vec::new()
    }

    fn take_entity(&mut self, purpose: &str, entity: EntityId, cx: &mut AppletContext<'_>) {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me()));
        }
        if purpose == "open" {
            self.open_entity(cx, entity);
        } else {
            self.take(cx, entity);
        }
    }

    fn opens_file(&self, path: &Path) -> bool {
        path.extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|ext| {
                matches!(
                    ext.as_str(),
                    "docx"
                        | "fountain"
                        | "md"
                        | "markdown"
                        | "txt"
                        | "text"
                        | "html"
                        | "htm"
                        | "xhtml"
                )
            })
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        match pick.purpose.as_str() {
            "import" => {
                if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
                    cx.request(ShellRequest::Open(me()));
                }
                self.import(cx, path);
            }
            "picture" => {
                // Read the picture into the arena, then into the text.
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let name = path.file_name().map_or_else(
                            || tr!("picture"),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        match read_picture(&bytes) {
                            Ok(tile) => {
                                let entity = Entity::new(&name, tile)
                                    .with_class(class::IMAGE)
                                    .with_meta(sue_io::meta::SOURCE, path.to_string_lossy());
                                let id = entity.id();
                                let spawn = sue_core::txn::SpawnEntity::new(entity);
                                if cx.apply(ArenaEdit(spawn)).is_ok() {
                                    self.editor.insert_embed(cx, id);
                                }
                            }
                            Err(error) => cx.fail(tr!("could-not-read-the-picture", error = error)),
                        }
                    }
                    Err(error) => {
                        cx.fail(tr!("could-not-read", path = path.display(), error = error))
                    }
                }
            }
            purpose if purpose.starts_with("compile.") => {
                let format = FORMATS
                    .iter()
                    .position(|(key, ..)| purpose == format!("compile.{key}"))
                    .unwrap_or(0);
                let mut choices = self.last_compile.clone().unwrap_or(CompileChoices {
                    format,
                    titles: Titles::Folders,
                    separator: Separator::None,
                    title_page: false,
                    chapter_pages: true,
                    whole: true,
                });
                choices.format = format;
                self.compile_to(cx, path, &choices);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_grow_and_shrink_along_the_usual_sizes() {
        assert_eq!(font_step(12.0, true), 14.0);
        assert_eq!(font_step(12.0, false), 11.0);
        assert_eq!(font_step(13.0, true), 14.0);
        assert_eq!(font_step(72.0, true), 80.0);
        assert_eq!(font_step(80.0, false), 72.0);
        assert_eq!(font_step(8.0, false), 1.0);
    }
}
