//! Templates for new documents, as data. Each is a file: the built-in
//! ones in `crates/sue-writer/templates` (embedded by `build.rs`, so adding
//! one is adding a file), and people's own in Sue's templates folder, where
//! Word, OpenDocument, Markdown, HTML and plain text files are templates
//! too. `templates/README.md` describes the `.template` format.
//!
//! A template is a card on the start page (`group: template`, as Word's
//! letter, resume and report) or an entry in the Presets list (`group:
//! preset`: a novel, a screenplay and the other kinds of long work).

use std::path::{Path, PathBuf};

use sue_core::{Arena, EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::{TextMarks, mark_kind as kind};

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// The extension of Sue Writer's own template files.
pub const EXTENSION: &str = "template";

/// Other files people may keep as templates: read as one document.
pub const DOCUMENTS: &[&str] = &[
    "docx", "dotx", "odt", "ott", "md", "markdown", "html", "htm", "txt",
];

/// Where a template is offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// A card on the start page, as a word processor's templates.
    Template,
    /// An entry in the Presets list: the kinds of long work.
    Preset,
}

/// A paragraph to write: its style key, how it is set apart (`bullet`,
/// `number`, `center`, `right`, `justify`, `bold`, `italic`, `break`) and
/// its text as written in the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paragraph {
    /// The paragraph style key, e.g. `heading1`.
    pub style: String,
    /// What sets it apart.
    pub mods: Vec<String>,
    /// Its text, maybe `tr:key` and fields.
    pub text: String,
}

/// A folder or a document of a new project.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A folder: name, synopsis and what it holds.
    Folder {
        /// Its name as written.
        name: String,
        /// Its synopsis as written.
        synopsis: String,
        /// What it holds.
        items: Vec<Item>,
    },
    /// A document: name, synopsis and paragraphs.
    Document {
        /// Its name as written.
        name: String,
        /// Its synopsis as written.
        synopsis: String,
        /// Its paragraphs.
        paragraphs: Vec<Paragraph>,
    },
}

/// What a template makes.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// The binder of a `.template` file: the draft and the research.
    Outline {
        /// The work itself.
        draft: Vec<Item>,
        /// Notes beside it.
        research: Vec<Item>,
    },
    /// A document file (Word, OpenDocument, ...), read when used.
    File(PathBuf),
}

/// A template for new documents.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    /// Its key: the file's name without its extension.
    pub key: String,
    /// Its name as written (maybe `tr:key`).
    pub name: String,
    /// What it is for, as written.
    pub about: String,
    /// Where it is offered.
    pub group: Group,
    /// Where it sorts in its group.
    pub order: i64,
    /// The rest of the header, in order: `sheet`, `page`, `margin`, ...
    pub settings: Vec<(String, String)>,
    /// What it makes.
    pub body: Body,
    /// `true` for one that comes with Sue.
    pub built_in: bool,
}

/// Text as people read it: `tr:key` (at the start) looked up in Sue
/// Writer's messages, followed by any text after it, and `{date}` filled
/// with today's date.
#[must_use]
pub fn words(raw: &str) -> String {
    let text = match raw.strip_prefix("tr:") {
        Some(rest) => {
            let (key, after) = rest.split_once(' ').unwrap_or((rest, ""));
            let found = sue_lang::text_or("writer", key, key);
            if after.is_empty() {
                found
            } else {
                format!("{found} {after}")
            }
        }
        None => raw.to_owned(),
    };
    if text.contains("{date}") {
        text.replace("{date}", &crate::project::today())
    } else {
        text
    }
}

impl Template {
    /// Reads a `.template` file named `key`.
    ///
    /// # Errors
    /// A line that is not understood, with its number.
    pub fn parse(key: &str, source: &str) -> Result<Self, String> {
        let mut template = Self {
            key: key.to_owned(),
            name: key.to_owned(),
            about: String::new(),
            group: Group::Template,
            order: 1000,
            settings: Vec::new(),
            body: Body::Outline {
                draft: Vec::new(),
                research: Vec::new(),
            },
            built_in: false,
        };
        let lines: Vec<(usize, usize, &str)> = source
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map(|(number, line)| {
                let trimmed = line.trim_start();
                (number + 1, line.len() - trimmed.len(), trimmed.trim_end())
            })
            .collect();
        let mut draft = Vec::new();
        let mut research = Vec::new();
        let mut at = 0;
        while at < lines.len() {
            let (number, indent, line) = lines[at];
            if indent > 0 {
                return Err(format!(
                    "line {number}: indented outside draft: or research:"
                ));
            }
            at += 1;
            match line {
                "draft:" => draft = items(&lines, &mut at, 1, number)?.0,
                "research:" => research = items(&lines, &mut at, 1, number)?.0,
                _ => {
                    let Some((name, value)) = line.split_once(':') else {
                        return Err(format!("line {number}: expected `key: value`"));
                    };
                    let value = value.trim().to_owned();
                    match name.trim() {
                        "name" => template.name = value,
                        "about" => template.about = value,
                        "group" => {
                            template.group = match value.as_str() {
                                "preset" => Group::Preset,
                                "template" => Group::Template,
                                other => return Err(format!("line {number}: no group {other}")),
                            };
                        }
                        "order" => {
                            template.order = value
                                .parse()
                                .map_err(|_| format!("line {number}: order is a number"))?;
                        }
                        other => template.settings.push((other.to_owned(), value)),
                    }
                }
            }
        }
        template.body = Body::Outline { draft, research };
        Ok(template)
    }

    /// A document file kept as a template: one document, on pages.
    #[must_use]
    pub fn file(path: &Path) -> Self {
        let key = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            name: key.clone(),
            key,
            about: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            group: Group::Template,
            order: 1000,
            settings: vec![("binder".to_owned(), "false".to_owned())],
            body: Body::File(path.to_path_buf()),
            built_in: false,
        }
    }

    /// Its name in the language in use.
    #[must_use]
    pub fn name(&self) -> String {
        words(&self.name)
    }

    /// What it is for, in the language in use.
    #[must_use]
    pub fn about(&self) -> String {
        words(&self.about)
    }

    /// The header value `key`, if it is set.
    #[must_use]
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings
            .iter()
            .rev()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.as_str())
    }

    /// `true` when it shows the binder (chapters, research, the trash).
    #[must_use]
    pub fn binder(&self) -> bool {
        self.setting("binder") == Some("true")
    }

    /// The paragraphs of its first document, their text as people read
    /// it, for a picture of its first page.
    #[must_use]
    pub fn preview(&self) -> Vec<Paragraph> {
        fn first(items: &[Item]) -> Option<&[Paragraph]> {
            items.iter().find_map(|item| match item {
                Item::Document { paragraphs, .. } => Some(paragraphs.as_slice()),
                Item::Folder { items, .. } => first(items),
            })
        }
        match &self.body {
            Body::Outline { draft, .. } => first(draft)
                .unwrap_or_default()
                .iter()
                .map(|paragraph| Paragraph {
                    text: words(&paragraph.text),
                    ..paragraph.clone()
                })
                .collect(),
            Body::File(_) => Vec::new(),
        }
    }

    /// The template as a `.template` file.
    #[must_use]
    pub fn to_text(&self) -> String {
        fn write(out: &mut String, items: &[Item], depth: usize) {
            let pad = "  ".repeat(depth);
            for item in items {
                match item {
                    Item::Folder {
                        name,
                        synopsis,
                        items,
                    } => {
                        out.push_str(&format!("{pad}folder {}\n", one_line(name)));
                        if !synopsis.is_empty() {
                            out.push_str(&format!("{pad}  synopsis {}\n", one_line(synopsis)));
                        }
                        write(out, items, depth + 1);
                    }
                    Item::Document {
                        name,
                        synopsis,
                        paragraphs,
                    } => {
                        out.push_str(&format!("{pad}document {}\n", one_line(name)));
                        if !synopsis.is_empty() {
                            out.push_str(&format!("{pad}  synopsis {}\n", one_line(synopsis)));
                        }
                        for paragraph in paragraphs {
                            let mut head = paragraph.style.clone();
                            for one in &paragraph.mods {
                                head.push('.');
                                head.push_str(one);
                            }
                            if paragraph.text.is_empty() {
                                out.push_str(&format!("{pad}  {head}\n"));
                            } else {
                                out.push_str(&format!(
                                    "{pad}  {head} {}\n",
                                    one_line(&paragraph.text)
                                ));
                            }
                        }
                    }
                }
            }
        }
        let mut out = String::new();
        out.push_str(&format!("name: {}\n", one_line(&self.name)));
        if !self.about.is_empty() {
            out.push_str(&format!("about: {}\n", one_line(&self.about)));
        }
        out.push_str(match self.group {
            Group::Template => "group: template\n",
            Group::Preset => "group: preset\n",
        });
        out.push_str(&format!("order: {}\n", self.order));
        for (key, value) in &self.settings {
            out.push_str(&format!("{key}: {}\n", one_line(value)));
        }
        if let Body::Outline { draft, research } = &self.body {
            out.push_str("\ndraft:\n");
            write(&mut out, draft, 1);
            if !research.is_empty() {
                out.push_str("research:\n");
                write(&mut out, research, 1);
            }
        }
        out
    }

    /// A template made from project `root` as it stands: its page, fonts
    /// and running heads, and its binder with each paragraph's style.
    #[must_use]
    pub fn from_project(arena: &Arena, root: EntityId, key: &str, name: &str) -> Option<Self> {
        use crate::project as p;
        let project = arena.get_by_id(root).ok()?;
        let mut settings = Vec::new();
        let text = |property: &str| {
            project
                .property(property)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let number = |property: &str| {
            project
                .property(property)
                .and_then(|value| value.as_f64().or_else(|| value.as_i64().map(|n| n as f64)))
        };
        for (setting, property) in [("sheet", p::SHEET), ("page", p::PAGE)] {
            if let Some(value) = text(property) {
                settings.push((setting.to_owned(), value));
            }
        }
        if let Some(true) = project.property(p::LANDSCAPE).and_then(Value::as_bool) {
            settings.push(("orientation".to_owned(), "landscape".to_owned()));
        }
        if let Some(margin) = number(p::MARGIN) {
            settings.push(("margin".to_owned(), format!("{margin}")));
        }
        for (setting, property) in [("font", p::FONT), ("heading-font", p::HEADING_FONT)] {
            if let Some(value) = text(property) {
                settings.push((setting.to_owned(), value));
            }
        }
        if let Some(size) = number(p::SIZE) {
            settings.push(("size".to_owned(), format!("{size}")));
        }
        for (setting, property) in [("header", p::HEADER), ("footer", p::FOOTER)] {
            if let Some(value) = text(property) {
                settings.push((setting.to_owned(), value.replace('\t', "|")));
            }
        }
        if let Some(true) = project
            .property(p::DIFFERENT_FIRST)
            .and_then(Value::as_bool)
        {
            settings.push(("different-first".to_owned(), "true".to_owned()));
        }
        for (setting, property) in [
            ("target", manuscript::TARGET),
            ("session-target", p::SESSION_TARGET),
        ] {
            if let Some(value) = number(property) {
                settings.push((setting.to_owned(), format!("{value}")));
            }
        }
        let binder = !project
            .property(p::SIMPLE)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        settings.push(("binder".to_owned(), binder.to_string()));
        let draft = manuscript::folder(arena, root, role::DRAFT)
            .map(|folder| outline(arena, folder))
            .unwrap_or_default();
        let research = manuscript::folder(arena, root, role::RESEARCH)
            .map(|folder| outline(arena, folder))
            .unwrap_or_default();
        Some(Self {
            key: key.to_owned(),
            name: name.to_owned(),
            about: String::new(),
            group: if binder {
                Group::Preset
            } else {
                Group::Template
            },
            order: 1000,
            settings,
            body: Body::Outline { draft, research },
            built_in: false,
        })
    }
}

/// A one-line form of `text` for a template file.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '\u{2028}'], " ")
}

/// The items of folder `folder` in `arena`, as a template writes them.
fn outline(arena: &Arena, folder: EntityId) -> Vec<Item> {
    manuscript::children(arena, folder)
        .into_iter()
        .filter_map(|id| {
            let entity = arena.get_by_id(id).ok()?;
            let synopsis = entity
                .property(manuscript::SYNOPSIS)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let name = entity.name().to_owned();
            if manuscript::is_folder(entity) {
                return Some(Item::Folder {
                    name,
                    synopsis,
                    items: outline(arena, id),
                });
            }
            if !manuscript::is_document(entity) {
                return None;
            }
            let text = entity
                .string_buffer()
                .map(|buffer| buffer.as_str().to_owned())
                .unwrap_or_default();
            let empty = TextMarks::new();
            let marks = entity.ext::<TextMarks>().unwrap_or(&empty);
            let paragraphs = sue_model::paragraphs(&text)
                .into_iter()
                .enumerate()
                .filter(|(_, range)| range.start < text.len())
                .map(|(index, range)| {
                    let value = |kind: &str| marks.paragraph_value(&text, index, kind);
                    let style = value(kind::STYLE)
                        .and_then(Value::as_str)
                        .unwrap_or(sue_type::BODY)
                        .to_owned();
                    let mut mods = Vec::new();
                    if let Some(list) = value(kind::LIST).and_then(Value::as_str) {
                        mods.push(list.to_owned());
                    }
                    if let Some(align) = value(kind::ALIGN).and_then(Value::as_str)
                        && align != "left"
                    {
                        mods.push(align.to_owned());
                    }
                    if value(kind::PAGE_BREAK).and_then(Value::as_bool) == Some(true) {
                        mods.push("break".to_owned());
                    }
                    let line = text[range.clone()].trim_end_matches('\n');
                    if !line.is_empty() {
                        for (mark, name) in [(kind::BOLD, "bold"), (kind::ITALIC, "italic")] {
                            if marks.covers(range.start..range.start + line.len(), mark) {
                                mods.push(name.to_owned());
                            }
                        }
                    }
                    Paragraph {
                        style,
                        mods,
                        text: line.to_owned(),
                    }
                })
                .collect();
            Some(Item::Document {
                name,
                synopsis,
                paragraphs,
            })
        })
        .collect()
}

/// A template file's lines: number, indent and text.
type Lines<'a> = [(usize, usize, &'a str)];

/// Reads the items indented deeper than `outer` from `lines[*at]` on
/// (all at the first one's indent), and the synopsis among them, for the
/// folder above (line `parent`).
fn items(
    lines: &Lines<'_>,
    at: &mut usize,
    outer: usize,
    parent: usize,
) -> Result<(Vec<Item>, String), String> {
    let mut out = Vec::new();
    let mut synopsis = String::new();
    let Some(&(_, indent, _)) = lines.get(*at) else {
        return Ok((out, synopsis));
    };
    if indent < outer {
        return Ok((out, synopsis));
    }
    while let Some(&(number, here, line)) = lines.get(*at) {
        if here < indent {
            break;
        }
        if here > indent {
            return Err(format!(
                "line {number}: indented too far (under line {parent})"
            ));
        }
        *at += 1;
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim().to_owned();
        match word {
            "folder" => {
                let (children, about) = items(lines, at, indent + 1, number)?;
                out.push(Item::Folder {
                    name: rest,
                    synopsis: about,
                    items: children,
                });
            }
            "document" => {
                let mut about = String::new();
                let mut paragraphs = Vec::new();
                while let Some(&(inner_number, inner, text)) = lines.get(*at) {
                    if inner <= indent {
                        break;
                    }
                    *at += 1;
                    let (head, body) = text.split_once(' ').unwrap_or((text, ""));
                    if head == "synopsis" {
                        body.trim().clone_into(&mut about);
                        continue;
                    }
                    let mut parts = head.split('.');
                    let style = parts.next().unwrap_or_default();
                    if style.is_empty() {
                        return Err(format!("line {inner_number}: a paragraph needs a style"));
                    }
                    paragraphs.push(Paragraph {
                        style: style.to_owned(),
                        mods: parts.map(str::to_owned).collect(),
                        text: body.trim().to_owned(),
                    });
                }
                out.push(Item::Document {
                    name: rest,
                    synopsis: about,
                    paragraphs,
                });
            }
            "synopsis" if outer > 1 => synopsis = rest,
            other => {
                return Err(format!(
                    "line {number}: expected folder or document, not {other}"
                ));
            }
        }
    }
    Ok((out, synopsis))
}

/// The templates people can start from: Sue's own and those in the
/// templates folder (one there with the same key takes the place of
/// Sue's).
#[derive(Clone, Debug, Default)]
pub struct Templates {
    list: Vec<Template>,
    /// Files in the folder that could not be read, and why.
    pub problems: Vec<String>,
}

impl Templates {
    /// Sue's own templates.
    #[must_use]
    pub fn built_in() -> Self {
        let mut templates = Self::default();
        for (file, source) in BUILT_IN {
            let key = file.trim_end_matches(".template");
            match Template::parse(key, source) {
                Ok(mut template) => {
                    template.built_in = true;
                    templates.list.push(template);
                }
                Err(error) => templates.problems.push(format!("{file}: {error}")),
            }
        }
        templates.sort();
        templates
    }

    /// Sue's own and those in [`folder`].
    #[must_use]
    pub fn load() -> Self {
        let mut templates = Self::built_in();
        if let Some(dir) = folder() {
            templates.add_folder(&dir);
        }
        templates
    }

    /// Adds the templates in `dir`.
    pub fn add_folder(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        for path in paths {
            let ext = path
                .extension()
                .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let template = if ext == EXTENSION {
                let key = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match std::fs::read_to_string(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|source| Template::parse(&key, &source))
                {
                    Ok(template) => template,
                    Err(error) => {
                        self.problems.push(format!("{}: {error}", path.display()));
                        continue;
                    }
                }
            } else if DOCUMENTS.contains(&ext.as_str()) {
                Template::file(&path)
            } else {
                continue;
            };
            self.list.retain(|known| known.key != template.key);
            self.list.push(template);
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.list.sort_by(|a, b| {
            (a.order, a.name())
                .partial_cmp(&(b.order, b.name()))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Every template, in order.
    #[must_use]
    pub fn all(&self) -> &[Template] {
        &self.list
    }

    /// The templates of `group`, in order.
    pub fn group(&self, group: Group) -> impl Iterator<Item = &Template> {
        self.list
            .iter()
            .filter(move |template| template.group == group)
    }

    /// The template `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Template> {
        self.list.iter().find(|template| template.key == key)
    }
}

/// The folder people keep their own Writer templates in.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    sue_platform::PlatformPaths::detect()
        .ok()
        .map(|paths| paths.templates_dir().join("writer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_template_reads() {
        let templates = Templates::built_in();
        assert!(templates.problems.is_empty(), "{:?}", templates.problems);
        assert_eq!(templates.all().len(), BUILT_IN.len());
        assert!(templates.group(Group::Template).count() >= 5);
        assert!(templates.group(Group::Preset).count() >= 4);
        // The old kinds of project keep their keys.
        for key in [
            "blank",
            "novel",
            "short-story",
            "screenplay",
            "nonfiction",
            "essay",
            "poetry",
            "letter",
            "report",
        ] {
            assert!(templates.get(key).is_some(), "{key}");
        }
        assert_eq!(
            templates
                .group(Group::Template)
                .next()
                .map(|t| t.key.as_str()),
            Some("blank")
        );
    }

    #[test]
    fn every_message_a_template_names_is_in_the_language_database() {
        fn texts(items: &[Item], out: &mut Vec<String>) {
            for item in items {
                match item {
                    Item::Folder {
                        name,
                        synopsis,
                        items,
                    } => {
                        out.push(name.clone());
                        out.push(synopsis.clone());
                        texts(items, out);
                    }
                    Item::Document {
                        name,
                        synopsis,
                        paragraphs,
                    } => {
                        out.push(name.clone());
                        out.push(synopsis.clone());
                        out.extend(paragraphs.iter().map(|p| p.text.clone()));
                    }
                }
            }
        }
        for template in Templates::built_in().all() {
            let mut all = vec![template.name.clone(), template.about.clone()];
            if let Body::Outline { draft, research } = &template.body {
                texts(draft, &mut all);
                texts(research, &mut all);
            }
            for text in all {
                if let Some(rest) = text.strip_prefix("tr:") {
                    let key = rest.split(' ').next().unwrap_or_default();
                    assert!(
                        sue_lang::has("writer", key),
                        "{}: no message {key}",
                        template.key
                    );
                } else {
                    // Sue's own templates name their words in the database.
                    assert!(
                        text.is_empty() || text.starts_with('{'),
                        "{}: {text}",
                        template.key
                    );
                }
            }
        }
    }

    #[test]
    fn a_template_reads_back_as_it_was_written() {
        for template in Templates::built_in().all() {
            let again = Template::parse(&template.key, &template.to_text()).unwrap();
            assert_eq!(again.body, template.body, "{}", template.key);
            assert_eq!(again.settings, template.settings, "{}", template.key);
            assert_eq!(again.name, template.name);
            assert_eq!(again.group, template.group);
            assert_eq!(again.order, template.order);
        }
    }

    #[test]
    fn folders_hold_synopses_and_folders() {
        let template = Template::parse(
            "x",
            "draft:
  folder Part
    synopsis The start.
    folder Chapter
      document Scene
        synopsis A scene.
        body.italic Once.
  document After
",
        )
        .unwrap();
        let Body::Outline { draft, .. } = &template.body else {
            panic!("an outline");
        };
        assert_eq!(draft.len(), 2);
        let Item::Folder {
            synopsis, items, ..
        } = &draft[0]
        else {
            panic!("a folder");
        };
        assert_eq!(synopsis, "The start.");
        let Item::Folder { items, .. } = &items[0] else {
            panic!("a folder in it");
        };
        assert_eq!(
            items[0],
            Item::Document {
                name: "Scene".to_owned(),
                synopsis: "A scene.".to_owned(),
                paragraphs: vec![Paragraph {
                    style: "body".to_owned(),
                    mods: vec!["italic".to_owned()],
                    text: "Once.".to_owned(),
                }],
            }
        );
        let again = Template::parse("x", &template.to_text()).unwrap();
        assert_eq!(again.body, template.body);
    }

    #[test]
    fn mistakes_are_named_by_line() {
        let error = Template::parse("x", "name: X\ndraft:\n  sheet tr:x\n").unwrap_err();
        assert!(error.contains("line 3"), "{error}");
        let error = Template::parse("x", "name X\n").unwrap_err();
        assert!(error.contains("line 1"), "{error}");
        assert!(Template::parse("x", "group: poem\n").is_err());
    }

    #[test]
    fn a_folder_of_templates_adds_and_replaces() {
        let dir = std::env::temp_dir().join(format!("sue-writer-templates-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("letter.template"),
            "name: Our Letter\ngroup: template\norder: 1\n\ndraft:\n  document Letter\n    title.center Acme\n",
        )
        .unwrap();
        std::fs::write(dir.join("brief.docx"), b"not really").unwrap();
        std::fs::write(
            dir.join("broken.template"),
            "draft:\n    document x\n  document y\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.bin"), b"ignored").unwrap();
        let mut templates = Templates::built_in();
        let before = templates.all().len();
        templates.add_folder(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(templates.all().len(), before + 1);
        assert_eq!(templates.problems.len(), 1, "{:?}", templates.problems);
        let letter = templates.get("letter").unwrap();
        assert_eq!(letter.name(), "Our Letter");
        assert!(!letter.built_in);
        assert_eq!(
            letter.preview(),
            vec![Paragraph {
                style: "title".to_owned(),
                mods: vec!["center".to_owned()],
                text: "Acme".to_owned(),
            }]
        );
        assert!(matches!(
            templates.get("brief").unwrap().body,
            Body::File(_)
        ));
    }

    #[test]
    fn words_look_up_messages_and_fill_the_date() {
        assert_eq!(words("tr:no-such-message-here"), "no-such-message-here");
        assert_eq!(words("Plain"), "Plain");
        assert!(!words("{date}").contains('{'));
        assert!(words("tr:no-such-message-here {date}").starts_with("no-such-message-here 2"));
    }
}
