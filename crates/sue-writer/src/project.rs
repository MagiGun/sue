//! Writer projects, on the shared manuscript model ([`sue_model::manuscript`]):
//! making one from a template, and every change to its binder (new
//! documents and folders, moving, duplicating, splitting, merging, the
//! trash, snapshots) as transactions, so all of it undoes.

use sue_core::txn::{
    Batch, DeleteText, DespawnEntity, InsertText, Relate, Rename, SetExtension, SetProperty,
    SpawnEntity, Unrelate,
};
use sue_core::{Arena, CHILD, Entity, EntityId, OrderKey, Relation, StringBuffer, Value};
use sue_model::manuscript::{self, role};
use sue_model::{TextMarks, class, mark_kind as kind};

/// The style sheet a project uses (`Text`, a key of `sue_type::SHEETS`).
pub const SHEET: &str = "sue_writer.sheet";
/// The page size, by name (`Text`, e.g. `Letter`).
pub const PAGE: &str = "sue_writer.page";
/// The page margins in points (`Float`).
pub const MARGIN: &str = "sue_writer.margin";
/// The body font (`Text`).
pub const FONT: &str = "sue_writer.font";
/// The heading font (`Text`).
pub const HEADING_FONT: &str = "sue_writer.heading_font";
/// The body size in points (`Float`).
pub const SIZE: &str = "sue_writer.size";
/// The words to write in one sitting (`Int`).
pub const SESSION_TARGET: &str = "sue_writer.session_target";
/// Words written each day: a map from `YYYY-MM-DD` to words (`Map`).
pub const HISTORY: &str = "sue_writer.history";
/// `true` for a single document shown without its binder, as a word
/// processor shows a file.
pub const SIMPLE: &str = "sue_writer.simple";
/// The running header on printed pages (`Text`: left, middle and right
/// parts parted by tabs, with fields such as `{page}`; see
/// [`sue_model::Running`]).
pub const HEADER: &str = "sue_writer.header";
/// The running footer, as the header (`Text`; the page number in the
/// middle without it).
pub const FOOTER: &str = "sue_writer.footer";
/// `true`: no header or footer on the first page (`Bool`).
pub const DIFFERENT_FIRST: &str = "sue_writer.different_first";
/// The relation from a document to its snapshots.
pub const SNAPSHOT: &str = "sue_writer.snapshot";
/// When a snapshot was taken, as text (`Text`).
pub const TAKEN: &str = "sue_writer.taken";

/// Where documents stand, in order.
pub const STATUSES: &[&str] = &[
    "To Do",
    "First Draft",
    "Revised Draft",
    "Final Draft",
    "Done",
];

/// Labels and their colors.
pub const LABELS: &[(&str, [u8; 3])] = &[
    ("Concept", [0x8e, 0x7c, 0xc3]),
    ("Chapter", [0x4a, 0x90, 0xd9]),
    ("Scene", [0x50, 0xb0, 0x7a]),
    ("Idea", [0xe0, 0xb0, 0x40]),
    ("Character", [0xd9, 0x6a, 0x4a]),
    ("Location", [0x40, 0xa8, 0xb0]),
    ("Research", [0x99, 0x99, 0x99]),
];

/// What people read for status `name`: one of [`STATUSES`] in the
/// language in use, any other as it is (statuses are kept in English).
#[must_use]
pub fn status_text(name: &str) -> String {
    match name {
        "To Do" => tr!("status-to-do"),
        "First Draft" => tr!("status-first-draft"),
        "Revised Draft" => tr!("status-revised-draft"),
        "Final Draft" => tr!("status-final-draft"),
        "Done" => tr!("status-done"),
        other => other.to_owned(),
    }
}

/// What people read for label `name`: one of [`LABELS`] in the language
/// in use, any other as it is (labels are kept in English).
#[must_use]
pub fn label_text(name: &str) -> String {
    match name {
        "Concept" => tr!("label-concept"),
        "Chapter" => tr!("label-chapter"),
        "Scene" => tr!("label-scene"),
        "Idea" => tr!("label-idea"),
        "Character" => tr!("label-character"),
        "Location" => tr!("label-location"),
        "Research" => tr!("label-research"),
        other => other.to_owned(),
    }
}

/// The name of template `key` (one of [`TEMPLATES`]) in the language in
/// use.
#[must_use]
pub fn template_name(key: &str) -> String {
    match key {
        "blank" => tr!("template-blank"),
        "novel" => tr!("template-novel"),
        "short-story" => tr!("template-short-story"),
        "screenplay" => tr!("template-screenplay"),
        "nonfiction" => tr!("template-nonfiction"),
        "essay" => tr!("template-essay"),
        "poetry" => tr!("template-poetry"),
        "letter" => tr!("template-letter"),
        "report" => tr!("template-report"),
        other => other.to_owned(),
    }
}

/// What template `key` is for, in the language in use.
#[must_use]
pub fn template_about(key: &str) -> String {
    match key {
        "blank" => tr!("template-blank-about"),
        "novel" => tr!("template-novel-about"),
        "short-story" => tr!("template-short-story-about"),
        "screenplay" => tr!("template-screenplay-about"),
        "nonfiction" => tr!("template-nonfiction-about"),
        "essay" => tr!("template-essay-about"),
        "poetry" => tr!("template-poetry-about"),
        "letter" => tr!("template-letter-about"),
        "report" => tr!("template-report-about"),
        _ => String::new(),
    }
}

/// The color of label `name`.
#[must_use]
pub fn label_color(name: &str) -> Option<[u8; 3]> {
    LABELS
        .iter()
        .find(|(label, _)| label.eq_ignore_ascii_case(name))
        .map(|(_, color)| *color)
}

/// The kinds of new projects: key, label, and what it is for.
pub const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "blank",
        "Blank Document",
        "One document on pages, as a word processor opens.",
    ),
    (
        "novel",
        "Novel",
        "Chapters of scenes, with research for characters and places.",
    ),
    (
        "short-story",
        "Short Story",
        "One story in manuscript format.",
    ),
    (
        "screenplay",
        "Screenplay",
        "Scenes in screenplay format, Enter and Tab moving between elements.",
    ),
    (
        "nonfiction",
        "Non-fiction Book",
        "Parts and chapters with notes and sources.",
    ),
    (
        "essay",
        "Essay or Paper",
        "A titled paper with headings and notes.",
    ),
    ("poetry", "Poetry Collection", "A poem to a document."),
    ("letter", "Letter", "A letter on one page."),
    (
        "report",
        "Report",
        "A report with a title, a summary and sections.",
    ),
];

/// A document to make: its name, its text, and the style of each of its
/// paragraphs.
struct Sheet {
    name: String,
    paragraphs: Vec<(&'static str, String)>,
    synopsis: String,
}

impl Sheet {
    fn new(name: &str, paragraphs: &[(&'static str, &str)]) -> Self {
        Self {
            name: name.to_owned(),
            paragraphs: paragraphs
                .iter()
                .map(|(style, text)| (*style, (*text).to_owned()))
                .collect(),
            synopsis: String::new(),
        }
    }

    fn about(mut self, synopsis: &str) -> Self {
        synopsis.clone_into(&mut self.synopsis);
        self
    }
}

/// Something in a new binder: a folder (its name, synopsis and items) or
/// a document. Their names are made in the language in use.
enum Item {
    Folder(String, &'static str, Vec<Item>),
    Text(Sheet),
}

/// Text and marks for `paragraphs` (style key, text).
#[must_use]
pub fn rich(paragraphs: &[(&str, String)]) -> (String, TextMarks) {
    let mut text = String::new();
    let mut marks = TextMarks::new();
    for (style, line) in paragraphs {
        let start = text.len();
        text.push_str(line);
        text.push('\n');
        if *style != sue_type::BODY {
            marks.set_paragraphs(
                &text,
                start..start,
                kind::STYLE,
                Some(Value::Text((*style).to_owned())),
            );
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    (text, marks)
}

/// A new rich text document entity.
#[must_use]
pub fn document_entity(name: &str, text: String, marks: TextMarks) -> Entity {
    Entity::new(name, StringBuffer::from(text))
        .with_class(class::DOCUMENT)
        .with_ext(marks)
}

fn structure(template: &str) -> (&'static str, Vec<Item>, Vec<Item>) {
    fn body(text: &str) -> (&'static str, &str) {
        (sue_type::BODY, text)
    }
    let character = || {
        Item::Text(
            Sheet::new(
                &tr!("character-sketch"),
                &[
                    ("heading2", &tr!("name")),
                    body(""),
                    ("heading3", &tr!("role-in-the-story")),
                    body(""),
                    ("heading3", &tr!("what-they-want")),
                    body(""),
                    ("heading3", &tr!("what-stands-in-the-way")),
                    body(""),
                    ("heading3", &tr!("how-they-change")),
                    body(""),
                ],
            )
            .about(&tr!("copy-sheet-each-character")),
        )
    };
    let place = || {
        Item::Text(
            Sheet::new(
                &tr!("setting-sketch"),
                &[
                    ("heading2", &tr!("place")),
                    body(""),
                    ("heading3", &tr!("sights-sounds-and-smells")),
                    body(""),
                    ("heading3", &tr!("why-it-matters")),
                    body(""),
                ],
            )
            .about(&tr!("copy-sheet-each-place")),
        )
    };
    match template {
        "novel" => (
            "manuscript",
            vec![
                Item::Folder(
                    tr!("chapter-1"),
                    "",
                    vec![Item::Text(
                        Sheet::new(&tr!("scene-1"), &[body("")])
                            .about(&tr!("what-happens-line-two")),
                    )],
                ),
                Item::Folder(
                    tr!("chapter-2"),
                    "",
                    vec![Item::Text(Sheet::new(&tr!("scene-1"), &[body("")]))],
                ),
            ],
            vec![
                Item::Folder(tr!("characters"), "", vec![character()]),
                Item::Folder(tr!("places"), "", vec![place()]),
                Item::Text(Sheet::new(&tr!("notes"), &[body("")])),
            ],
        ),
        "short-story" => (
            "manuscript",
            vec![Item::Text(Sheet::new(&tr!("story"), &[body("")]))],
            vec![
                Item::Folder(tr!("characters"), "", vec![character()]),
                Item::Text(Sheet::new(&tr!("notes"), &[body("")])),
            ],
        ),
        "screenplay" => (
            "screenplay",
            vec![
                Item::Folder(
                    tr!("act-one"),
                    "",
                    vec![Item::Text(Sheet::new(
                        &tr!("scene-1"),
                        &[("scene", &tr!("int-location-day")), body(&tr!("action"))],
                    ))],
                ),
                Item::Folder(tr!("act-two"), "", vec![]),
                Item::Folder(tr!("act-three"), "", vec![]),
            ],
            vec![
                Item::Folder(tr!("characters"), "", vec![character()]),
                Item::Text(Sheet::new(&tr!("notes"), &[body("")])),
            ],
        ),
        "nonfiction" => (
            "standard",
            vec![
                Item::Text(Sheet::new(
                    &tr!("introduction"),
                    &[("heading1", &tr!("introduction")), body("")],
                )),
                Item::Folder(
                    tr!("part-one"),
                    "",
                    vec![Item::Text(Sheet::new(
                        &tr!("chapter-1"),
                        &[("heading1", &tr!("chapter-1")), body("")],
                    ))],
                ),
                Item::Text(Sheet::new(
                    &tr!("conclusion"),
                    &[("heading1", &tr!("conclusion")), body("")],
                )),
            ],
            vec![
                Item::Text(Sheet::new(&tr!("sources"), &[body("")])),
                Item::Text(Sheet::new(&tr!("notes"), &[body("")])),
            ],
        ),
        "essay" => (
            "standard",
            vec![Item::Text(Sheet::new(
                &tr!("paper"),
                &[
                    ("title", &tr!("title")),
                    ("subtitle", &tr!("author")),
                    ("heading1", &tr!("introduction")),
                    body(""),
                    ("heading1", &tr!("discussion")),
                    body(""),
                    ("heading1", &tr!("conclusion")),
                    body(""),
                ],
            ))],
            vec![Item::Text(Sheet::new(&tr!("sources"), &[body("")]))],
        ),
        "poetry" => (
            "standard",
            vec![Item::Text(Sheet::new(
                &tr!("untitled-poem"),
                &[("heading1", &tr!("untitled")), body("")],
            ))],
            vec![Item::Text(Sheet::new(&tr!("ideas"), &[body("")]))],
        ),
        "letter" => (
            "standard",
            vec![Item::Text(Sheet::new(
                &tr!("letter"),
                &[
                    body(&tr!("your-name")),
                    body(&tr!("your-address")),
                    body(""),
                    body(&tr!("date")),
                    body(""),
                    body(&tr!("dear")),
                    body(""),
                    body(&tr!("kind-regards")),
                    body(""),
                    body(&tr!("your-name")),
                ],
            ))],
            vec![],
        ),
        "report" => (
            "standard",
            vec![Item::Text(Sheet::new(
                &tr!("report"),
                &[
                    ("title", &tr!("title")),
                    ("heading1", &tr!("summary")),
                    body(""),
                    ("heading1", &tr!("background")),
                    body(""),
                    ("heading1", &tr!("findings")),
                    body(""),
                    ("heading1", &tr!("next-steps")),
                    body(""),
                ],
            ))],
            vec![],
        ),
        _ => (
            "standard",
            vec![Item::Text(Sheet::new(&tr!("document"), &[body("")]))],
            vec![],
        ),
    }
}

/// Makes a project from template `key` named `title`: the spawns as one
/// undo step, and the project's id.
#[must_use]
pub fn create(key: &str, title: &str, fonts: &sue_type::Fonts) -> (Batch<Arena>, EntityId) {
    let (sheet, draft, research) = structure(key);
    let simple = matches!(key, "blank" | "letter" | "report" | "essay");
    let mut root = Entity::empty(title, class::MANUSCRIPT)
        .with_property(manuscript::TITLE, title)
        .with_property(SHEET, sheet)
        .with_property(PAGE, tr!("letter"))
        .with_property(MARGIN, 72.0);
    // Only fonts that are really installed are written down; a project
    // made before the fonts are known takes the best ones once they are.
    let body = if sheet == "screenplay" {
        &fonts.mono
    } else {
        &fonts.body
    };
    if body != sue_wm::fonts::SANS && body != sue_wm::fonts::MONO {
        root = root.with_property(FONT, body.clone());
    }
    if fonts.heading != sue_wm::fonts::SANS {
        root = root.with_property(HEADING_FONT, fonts.heading.clone());
    }
    let root = root
        .with_property(
            SIZE,
            if sheet == "screenplay" {
                12.0
            } else {
                f64::from(fonts.size)
            },
        )
        .with_property(SIMPLE, simple)
        .with_property(SESSION_TARGET, 500_i64);
    let root = match key {
        "novel" => root.with_property(manuscript::TARGET, 80_000_i64),
        "short-story" => root.with_property(manuscript::TARGET, 5_000_i64),
        _ => root,
    };
    let root_id = root.id();
    let mut batch = Batch::new(tr!("new", title = title)).then(SpawnEntity::new(root));
    let mut orders = OrderKey::sequence(None, 3).into_iter();
    let mut folders = vec![(tr!("manuscript"), role::DRAFT, draft)];
    if !simple {
        folders.push((tr!("research"), role::RESEARCH, research));
        folders.push((tr!("trash"), role::TRASH, Vec::new()));
    }
    for (name, folder_role, items) in folders {
        let folder =
            Entity::empty(name, class::FOLDER).with_property(manuscript::ROLE, folder_role);
        let id = folder.id();
        let order = orders.next().unwrap_or_default();
        batch
            .push(Box::new(SpawnEntity::new(folder).with_relation(
                Relation::new(root_id, CHILD, id).ordered(order),
            )));
        add_items(&mut batch, id, items);
    }
    (batch, root_id)
}

fn add_items(batch: &mut Batch<Arena>, parent: EntityId, items: Vec<Item>) {
    let orders = OrderKey::sequence(None, items.len());
    for (item, order) in items.into_iter().zip(orders) {
        match item {
            Item::Folder(name, synopsis, children) => {
                let mut folder = Entity::empty(&name, class::FOLDER);
                if !synopsis.is_empty() {
                    folder = folder.with_property(manuscript::SYNOPSIS, synopsis);
                }
                let id = folder.id();
                batch
                    .push(Box::new(SpawnEntity::new(folder).with_relation(
                        Relation::new(parent, CHILD, id).ordered(order),
                    )));
                add_items(batch, id, children);
            }
            Item::Text(sheet) => {
                let (text, marks) = rich(&sheet.paragraphs);
                let mut entity = document_entity(&sheet.name, text, marks);
                if !sheet.synopsis.is_empty() {
                    entity = entity.with_property(manuscript::SYNOPSIS, sheet.synopsis);
                }
                let id = entity.id();
                batch
                    .push(Box::new(SpawnEntity::new(entity).with_relation(
                        Relation::new(parent, CHILD, id).ordered(order),
                    )));
            }
        }
    }
}

/// The order key for a new child of `parent` at `index` among its children.
fn order_at(arena: &Arena, parent: EntityId, index: usize, skip: Option<EntityId>) -> OrderKey {
    let siblings: Vec<OrderKey> = manuscript::children(arena, parent)
        .into_iter()
        .filter(|child| Some(*child) != skip)
        .filter_map(|child| {
            arena
                .relations()
                .get(parent, CHILD, child)
                .map(|relation| relation.order)
        })
        .collect();
    let index = index.min(siblings.len());
    let before = index.checked_sub(1).and_then(|at| siblings.get(at));
    let after = siblings.get(index);
    match (before, after) {
        (Some(before), Some(after)) if before >= after => OrderKey::between(Some(before), None),
        _ => OrderKey::between(before, after),
    }
}

/// Where a new item goes when `selected` is chosen: inside a folder (at
/// its end), else right after the document.
#[must_use]
pub fn place_for(arena: &Arena, selected: EntityId) -> Option<(EntityId, usize)> {
    let entity = arena.get_by_id(selected).ok()?;
    if manuscript::is_folder(entity) || manuscript::is_manuscript(entity) {
        return Some((selected, manuscript::children(arena, selected).len()));
    }
    let parent = manuscript::parent(arena, selected)?;
    let index = manuscript::children(arena, parent)
        .iter()
        .position(|child| *child == selected)
        .map_or(0, |at| at + 1);
    Some((parent, index))
}

/// Adds `entity` under `parent` at `index` as one undo step.
#[must_use]
pub fn add(
    arena: &Arena,
    parent: EntityId,
    index: usize,
    entity: Entity,
    label: &str,
) -> Batch<Arena> {
    let id = entity.id();
    let order = order_at(arena, parent, index, None);
    Batch::new(label).then(
        SpawnEntity::new(entity).with_relation(Relation::new(parent, CHILD, id).ordered(order)),
    )
}

/// A new empty document named `name`.
#[must_use]
pub fn new_document(name: &str) -> Entity {
    document_entity(name, "\n".to_owned(), TextMarks::new())
}

/// Moves `id` under `parent` at `index` (among the others), as one undo
/// step. `None` when that would put a folder inside itself.
#[must_use]
pub fn move_to(
    arena: &Arena,
    id: EntityId,
    parent: EntityId,
    index: usize,
) -> Option<Batch<Arena>> {
    // Never into itself or its own children.
    let mut at = Some(parent);
    while let Some(here) = at {
        if here == id {
            return None;
        }
        at = manuscript::parent(arena, here);
    }
    let old = manuscript::parent(arena, id)?;
    let order = order_at(arena, parent, index, Some(id));
    Some(
        Batch::new(tr!("move"))
            .then(Unrelate::new(old, CHILD, id))
            .then(Relate::new(Relation::new(parent, CHILD, id).ordered(order))),
    )
}

/// Moves `id` to its project's trash.
#[must_use]
pub fn trash(arena: &Arena, id: EntityId) -> Option<Batch<Arena>> {
    let root = manuscript::manuscript_of(arena, id)?;
    let bin = manuscript::folder(arena, root, role::TRASH)?;
    if bin == id || manuscript::parent(arena, id).is_none() {
        return None;
    }
    let end = manuscript::children(arena, bin).len();
    move_to(arena, id, bin, end).map(|batch| {
        let mut out = Batch::new(tr!("move-to-trash"));
        out.push(Box::new(batch));
        out
    })
}

/// Empties the trash of `root` for good (still one undo step).
#[must_use]
pub fn empty_trash(arena: &Arena, root: EntityId) -> Option<Batch<Arena>> {
    let bin = manuscript::folder(arena, root, role::TRASH)?;
    let items = manuscript::walk(arena, bin);
    if items.is_empty() {
        return None;
    }
    let mut batch = Batch::new(tr!("empty-trash"));
    // Children before their parents.
    for (id, _) in items.into_iter().rev() {
        for snapshot in snapshots(arena, id) {
            batch.push(Box::new(DespawnEntity::new(snapshot)));
        }
        batch.push(Box::new(DespawnEntity::new(id)));
    }
    Some(batch)
}

/// Copies of the tables (and tables of contents) `marks` shows, the marks
/// pointing at the copies now: a copied document owns its tables, so
/// editing one copy leaves the other alone.
fn copy_tables(arena: &Arena, marks: &mut TextMarks) -> Vec<Entity> {
    let shown: Vec<(std::ops::Range<usize>, EntityId)> = marks
        .marks()
        .iter()
        .filter(|mark| mark.kind == kind::EMBED)
        .filter_map(|mark| match mark.value {
            Value::Entity(id) => Some((mark.range.clone(), id)),
            _ => None,
        })
        .collect();
    let mut copies = Vec::new();
    for (range, id) in shown {
        let Ok(table) = arena.get_by_id(id) else {
            continue;
        };
        let mut copy = match (table.class(), table.ext::<sue_model::Table>()) {
            (sue_model::TEXT_TABLE, Some(cells)) => {
                Entity::empty(table.name(), table.class()).with_ext(cells.clone())
            }
            (sue_model::contents::CONTENTS, _) => Entity::empty(table.name(), table.class()),
            _ => continue,
        };
        for (key, value) in table.properties().iter() {
            copy = copy.with_property(key, value.clone());
        }
        marks.set(range, kind::EMBED, Some(Value::Entity(copy.id())));
        copies.push(copy);
    }
    copies
}

/// Spawns each of `entities` in `batch`.
fn spawn_all(batch: &mut Batch<Arena>, entities: Vec<Entity>) {
    for entity in entities {
        batch.push(Box::new(SpawnEntity::new(entity)));
    }
}

/// A copy of `entity` named `name`, and copies of the tables it shows.
fn copy_of(arena: &Arena, entity: &Entity, name: &str) -> (Entity, Vec<Entity>) {
    let mut copy = match entity.string_buffer() {
        Some(text) => Entity::new(name, StringBuffer::from(text.as_str().to_owned())),
        None => Entity::empty(name, entity.class()),
    }
    .with_class(entity.class());
    for (key, value) in entity.properties().iter() {
        copy = copy.with_property(key, value.clone());
    }
    let mut tables = Vec::new();
    if let Some(marks) = entity.ext::<TextMarks>() {
        let mut marks = marks.clone();
        tables = copy_tables(arena, &mut marks);
        copy = copy.with_ext(marks);
    }
    (copy, tables)
}

/// A copy of `id` (and everything under it) right after it.
#[must_use]
pub fn duplicate(arena: &Arena, id: EntityId) -> Option<Batch<Arena>> {
    let parent = manuscript::parent(arena, id)?;
    let entity = arena.get_by_id(id).ok()?;
    let index = manuscript::children(arena, parent)
        .iter()
        .position(|child| *child == id)?
        + 1;
    let (copy, tables) = copy_of(arena, entity, &tr!("copy", entity = entity.name()));
    let copy_id = copy.id();
    let mut batch = add(arena, parent, index, copy, &tr!("duplicate"));
    spawn_all(&mut batch, tables);
    fn under(arena: &Arena, from: EntityId, to: EntityId, batch: &mut Batch<Arena>) {
        let children = manuscript::children(arena, from);
        for (child, order) in children
            .iter()
            .zip(OrderKey::sequence(None, children.len()))
        {
            let Ok(entity) = arena.get_by_id(*child) else {
                continue;
            };
            let (copy, tables) = copy_of(arena, entity, entity.name());
            let id = copy.id();
            batch.push(Box::new(
                SpawnEntity::new(copy).with_relation(Relation::new(to, CHILD, id).ordered(order)),
            ));
            spawn_all(batch, tables);
            under(arena, *child, id, batch);
        }
    }
    under(arena, id, copy_id, &mut batch);
    Some(batch)
}

/// Replaces document `id`'s text and marks with `text` and `marks`, as
/// one undo step.
#[must_use]
pub fn replace_all(
    arena: &Arena,
    id: EntityId,
    text: &str,
    marks: TextMarks,
    label: &str,
) -> Option<Batch<Arena>> {
    let handle = arena.handle_of(id)?;
    let old = arena.get(handle).ok()?.string_buffer()?.as_str().len();
    let mut batch = Batch::new(label);
    if old > 0 {
        batch.push(Box::new(DeleteText::new(handle, 0..old)));
    }
    if !text.is_empty() {
        batch.push(Box::new(InsertText::new(handle, 0, text)));
    }
    batch.push(Box::new(SetExtension::set(id, marks)));
    Some(batch)
}

/// Splits document `id` at byte `at`: the text after it moves into a new
/// document right after, named `name`.
#[must_use]
pub fn split(
    arena: &Arena,
    id: EntityId,
    at: usize,
    name: &str,
) -> Option<(Batch<Arena>, EntityId)> {
    let entity = arena.get_by_id(id).ok()?;
    let text = entity.string_buffer()?.as_str();
    let at = at.min(text.len());
    if !text.is_char_boundary(at) {
        return None;
    }
    let marks = entity.ext::<TextMarks>().cloned().unwrap_or_default();
    let (mut head, mut tail) = (text[..at].to_owned(), text[at..].to_owned());
    let mut head_marks = marks.slice(0..at);
    let mut tail_marks = marks.slice(at..text.len());
    sue_model::ensure_final_newline(&mut head);
    sue_model::ensure_final_newline(&mut tail);
    head_marks.fit(&head);
    tail_marks.fit(&tail);
    let parent = manuscript::parent(arena, id)?;
    let index = manuscript::children(arena, parent)
        .iter()
        .position(|child| *child == id)?
        + 1;
    let new = document_entity(name, tail, tail_marks);
    let new_id = new.id();
    let mut batch = Batch::new(tr!("split"));
    batch.push(Box::new(replace_all(
        arena,
        id,
        &head,
        head_marks,
        &tr!("split"),
    )?));
    batch.push(Box::new(add(arena, parent, index, new, &tr!("split"))));
    Some((batch, new_id))
}

/// Joins documents `ids` (in that order) into the first; the rest go to
/// the trash.
#[must_use]
pub fn merge(arena: &Arena, ids: &[EntityId]) -> Option<Batch<Arena>> {
    let (&first, rest) = ids.split_first()?;
    if rest.is_empty() {
        return None;
    }
    let mut text = String::new();
    let mut marks = TextMarks::new();
    for id in ids {
        let entity = arena.get_by_id(*id).ok()?;
        let own = entity.string_buffer()?.as_str();
        let own_marks = entity.ext::<TextMarks>().cloned().unwrap_or_default();
        let before = text.clone();
        let mut piece = own.to_owned();
        sue_model::ensure_final_newline(&mut piece);
        marks.insert_text(&before, before.len(), &piece, Some(&own_marks));
        text.push_str(&piece);
    }
    let mut batch = Batch::new(tr!("merge"));
    batch.push(Box::new(replace_all(
        arena,
        first,
        &text,
        marks,
        &tr!("merge"),
    )?));
    for id in rest {
        if let Some(step) = trash(arena, *id) {
            batch.push(Box::new(step));
        }
    }
    Some(batch)
}

/// The snapshots of document `id`, oldest first.
#[must_use]
pub fn snapshots(arena: &Arena, id: EntityId) -> Vec<EntityId> {
    arena.relations().targets(id, SNAPSHOT)
}

/// A snapshot of document `id` as it is now, titled `title` and dated
/// `taken`.
#[must_use]
pub fn take_snapshot(
    arena: &Arena,
    id: EntityId,
    title: &str,
    taken: &str,
) -> Option<Batch<Arena>> {
    let entity = arena.get_by_id(id).ok()?;
    let (copy, tables) = copy_of(arena, entity, title);
    let copy = copy.with_property(TAKEN, taken);
    let copy_id = copy.id();
    let order = arena.relations().next_order(id, SNAPSHOT);
    let mut batch = Batch::new(tr!("take-snapshot")).then(
        SpawnEntity::new(copy).with_relation(Relation::new(id, SNAPSHOT, copy_id).ordered(order)),
    );
    spawn_all(&mut batch, tables);
    Some(batch)
}

/// Puts snapshot `snapshot`'s text back into document `id` (after taking
/// a snapshot of what is there, so nothing is lost).
#[must_use]
pub fn restore(
    arena: &Arena,
    id: EntityId,
    snapshot: EntityId,
    taken: &str,
) -> Option<Batch<Arena>> {
    let saved = arena.get_by_id(snapshot).ok()?;
    let text = saved.string_buffer()?.as_str().to_owned();
    let mut marks = saved.ext::<TextMarks>().cloned().unwrap_or_default();
    let tables = copy_tables(arena, &mut marks);
    let mut batch = Batch::new(tr!("restore-snapshot"));
    spawn_all(&mut batch, tables);
    batch.push(Box::new(take_snapshot(
        arena,
        id,
        &tr!("before-restoring"),
        taken,
    )?));
    batch.push(Box::new(replace_all(
        arena,
        id,
        &text,
        marks,
        &tr!("restore-snapshot"),
    )?));
    Some(batch)
}

/// Renames `id`.
#[must_use]
pub fn rename(id: EntityId, name: &str) -> Rename {
    Rename::new(id, name)
}

/// Sets (or with `None` removes) property `key` of `id`.
#[must_use]
pub fn set(id: EntityId, key: &str, value: Option<Value>) -> SetProperty {
    match value {
        Some(value) => SetProperty::new(id, key, value),
        None => SetProperty::remove(id, key),
    }
}

/// The words written on each day, oldest first.
#[must_use]
pub fn history(entity: &Entity) -> Vec<(String, i64)> {
    match entity.property(HISTORY) {
        Some(Value::Map(days)) => days
            .iter()
            .filter_map(|(day, words)| Some((day.clone(), words.as_i64()?)))
            .collect(),
        _ => Vec::new(),
    }
}

/// `history` with `words` more on `day`.
#[must_use]
pub fn history_with(entity: &Entity, day: &str, words: i64) -> Value {
    let mut days = match entity.property(HISTORY) {
        Some(Value::Map(days)) => days.clone(),
        _ => std::collections::BTreeMap::new(),
    };
    let total = days.get(day).and_then(Value::as_i64).unwrap_or(0) + words;
    days.insert(day.to_owned(), Value::Int(total.max(0)));
    Value::Map(days)
}

/// Today's date, `YYYY-MM-DD` (UTC).
#[must_use]
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::UndoStack;

    fn project(key: &str) -> (Arena, UndoStack<Arena>, EntityId) {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (batch, root) = create(key, "Book", &sue_type::Fonts::default());
        history.apply(&mut arena, batch).unwrap();
        (arena, history, root)
    }

    #[test]
    fn a_novel_has_a_draft_research_and_trash() {
        let (arena, _, root) = project("novel");
        let draft = manuscript::folder(&arena, root, role::DRAFT).unwrap();
        assert!(manuscript::folder(&arena, root, role::RESEARCH).is_some());
        assert!(manuscript::folder(&arena, root, role::TRASH).is_some());
        let chapters = manuscript::children(&arena, draft);
        assert_eq!(chapters.len(), 2);
        assert_eq!(arena.get_by_id(chapters[0]).unwrap().name(), "Chapter 1");
        let scene = manuscript::children(&arena, chapters[0])[0];
        assert!(manuscript::is_document(arena.get_by_id(scene).unwrap()));
        let simple = project("blank");
        assert_eq!(
            manuscript::children(&simple.0, simple.2).len(),
            1,
            "a blank document has no research"
        );
    }

    #[test]
    fn binder_changes_undo() {
        let (mut arena, mut history, root) = project("novel");
        let draft = manuscript::folder(&arena, root, role::DRAFT).unwrap();
        let chapters = manuscript::children(&arena, draft);
        // Move chapter two before chapter one.
        let step = move_to(&arena, chapters[1], draft, 0).unwrap();
        history.apply(&mut arena, step).unwrap();
        assert_eq!(
            manuscript::children(&arena, draft),
            vec![chapters[1], chapters[0]]
        );
        assert!(
            move_to(&arena, draft, chapters[0], 0).is_none(),
            "not into itself"
        );
        history.undo(&mut arena).unwrap();
        assert_eq!(manuscript::children(&arena, draft), chapters);
        // Into the trash and back.
        let step = trash(&arena, chapters[1]).unwrap();
        history.apply(&mut arena, step).unwrap();
        let bin = manuscript::folder(&arena, root, role::TRASH).unwrap();
        assert_eq!(manuscript::children(&arena, bin), vec![chapters[1]]);
        let step = empty_trash(&arena, root).unwrap();
        history.apply(&mut arena, step).unwrap();
        assert!(arena.get_by_id(chapters[1]).is_err());
        history.undo(&mut arena).unwrap();
        assert!(arena.get_by_id(chapters[1]).is_ok());
        // Duplicate a folder with what is in it.
        let step = duplicate(&arena, chapters[0]).unwrap();
        history.apply(&mut arena, step).unwrap();
        let copy = manuscript::children(&arena, draft)[1];
        assert_eq!(arena.get_by_id(copy).unwrap().name(), "Chapter 1 (copy)");
        assert_eq!(manuscript::children(&arena, copy).len(), 1);
    }

    #[test]
    fn split_merge_and_snapshots_keep_every_word() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let folder = Entity::empty("Draft", class::FOLDER);
        let folder_id = folder.id();
        arena.insert_keep_id(folder).unwrap();
        let (text, marks) = rich(&[
            ("heading1", "One".to_owned()),
            (sue_type::BODY, "First half.".to_owned()),
            (sue_type::BODY, "Second half.".to_owned()),
        ]);
        let doc = document_entity("Doc", text.clone(), marks);
        let doc_id = doc.id();
        let step = add(&arena, folder_id, 0, doc, "Add");
        history.apply(&mut arena, step).unwrap();
        let at = text.find("Second").unwrap();
        let (step, tail) = split(&arena, doc_id, at, "Doc 2").unwrap();
        history.apply(&mut arena, step).unwrap();
        let read = |arena: &Arena, id| {
            arena
                .get_by_id(id)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str()
                .to_owned()
        };
        assert_eq!(read(&arena, doc_id), "One\nFirst half.\n");
        assert_eq!(read(&arena, tail), "Second half.\n");
        let heading = arena
            .get_by_id(doc_id)
            .unwrap()
            .ext::<TextMarks>()
            .unwrap()
            .paragraph_value(&read(&arena, doc_id), 0, kind::STYLE)
            .cloned();
        assert_eq!(heading, Some(Value::Text("heading1".into())));
        // Merge needs a trash: a manuscript around the folder.
        history.undo(&mut arena).unwrap();
        assert_eq!(read(&arena, doc_id), text);
        let step = take_snapshot(&arena, doc_id, "Draft 1", "today").unwrap();
        history.apply(&mut arena, step).unwrap();
        let shots = snapshots(&arena, doc_id);
        assert_eq!(shots.len(), 1);
        let step = replace_all(&arena, doc_id, "Changed.\n", TextMarks::new(), "Edit").unwrap();
        history.apply(&mut arena, step).unwrap();
        let step = restore(&arena, doc_id, shots[0], "later").unwrap();
        history.apply(&mut arena, step).unwrap();
        assert_eq!(read(&arena, doc_id), text);
        assert_eq!(snapshots(&arena, doc_id).len(), 2, "what was there is kept");
        assert_eq!(today().len(), 10);
    }
}
