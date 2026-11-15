//! What the files browser lists: the work Sue remembers (files opened and
//! saved lately, Sue projects and the work kept while not saved) as
//! [`Item`]s, for one studio or for every one, and which of them a
//! [`ProjectsState`] shows, in what order.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{AppletContext, AppletId, ShellRequest, WorkList};

/// How the cards lie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// Large cards.
    #[default]
    Large,
    /// Small cards.
    Small,
    /// Rows.
    List,
}

/// Which projects show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Shelf {
    /// Every one.
    #[default]
    All,
    /// The favorites.
    Favorite,
    /// The ones wearing a label.
    Label(String),
}

/// What a project card stands for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProjectKey {
    /// A file or project file.
    File(PathBuf),
    /// Work Sue kept while it was not saved, by its id.
    Kept(String),
}

/// What kind of project a card is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// A file a studio opens as it is: a picture in Sue Paint.
    Picture,
    /// A Sue project.
    Project,
    /// Work kept while it was not saved.
    Unsaved,
}

impl Type {
    /// Every type, in the order the chips show them.
    pub const ALL: [Self; 3] = [Self::Picture, Self::Project, Self::Unsaved];

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Picture => tr!("start-type-picture"),
            Self::Project => tr!("start-type-project"),
            Self::Unsaved => tr!("start-type-unsaved"),
        }
    }

    /// The glyph a card without a picture shows.
    #[must_use]
    pub fn glyph(self) -> crate::start::Glyph {
        use crate::start::Glyph;
        match self {
            Self::Picture => Glyph::Picture,
            Self::Project => Glyph::Projects,
            Self::Unsaved => Glyph::Clock,
        }
    }
}

/// One project in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// What it stands for.
    pub key: ProjectKey,
    /// The file (the kept project for unsaved work).
    pub path: PathBuf,
    /// Its name as people read it.
    pub title: String,
    /// When it was last opened, saved or kept.
    pub when: u64,
    /// What kind it is.
    pub kind: Type,
    /// It wears the favorite mark.
    pub favorite: bool,
    /// The studio whose work it is, when one is known.
    pub studio: Option<AppletId>,
}

/// A studio whose work the browser lists.
pub struct Studio<'a> {
    /// The studio.
    pub id: AppletId,
    /// Its name, in the language in use, for its chip.
    pub name: String,
    /// Which files that are not Sue projects are its work. With `None`,
    /// the files last worked on in it are.
    pub opens: Option<&'a dyn Fn(&Path) -> bool>,
}

impl Studio<'_> {
    /// `true` when `file`, not a Sue project, is this studio's work.
    fn takes(&self, path: &Path, page: Option<&AppletId>) -> bool {
        match self.opens {
            Some(opens) => opens(path),
            None => page == Some(&self.id),
        }
    }
}

/// The work of `studios` among the work Sue remembers: their Sue projects
/// (by the studio they were last worked on in), the other files each one
/// takes, and the work they kept. A file two studios take is the first
/// one's.
#[must_use]
pub fn items(work: &WorkList, studios: &[Studio<'_>]) -> Vec<Item> {
    let ours = |page: Option<&AppletId>| {
        page.and_then(|page| studios.iter().find(|studio| studio.id == *page))
    };
    let mut items: Vec<Item> = work
        .recent()
        .filter_map(|file| {
            let (kind, studio) = if file.is_project() {
                // A project saved where no studio was open (the Sue
                // menu over the Hub) is everyone's.
                match file.page.as_ref() {
                    None => (Type::Project, None),
                    Some(page) => (Type::Project, Some(ours(Some(page))?)),
                }
            } else {
                let studio = studios
                    .iter()
                    .find(|studio| studio.takes(&file.path, file.page.as_ref()))?;
                (Type::Picture, Some(studio))
            };
            Some(Item {
                key: ProjectKey::File(file.path.clone()),
                path: file.path.clone(),
                title: file.title(),
                when: file.last(),
                kind,
                favorite: work.is_favorite(&file.path),
                studio: studio.map(|studio| studio.id.clone()),
            })
        })
        .collect();
    items.extend(work.unsaved().filter_map(|kept| {
        let studio = ours(kept.page.as_ref())?;
        Some(Item {
            key: ProjectKey::Kept(kept.id.clone()),
            path: kept.file.clone(),
            title: kept.title(),
            when: kept.at,
            kind: Type::Unsaved,
            favorite: work.is_favorite(&kept.file),
            studio: Some(studio.id.clone()),
        })
    }));
    items
}

/// The projects `state` shows, in its order.
#[must_use]
pub fn shown(items: &[Item], state: &ProjectsState, work: &WorkList) -> Vec<Item> {
    let needle = state.search.trim().to_lowercase();
    let mut shown: Vec<Item> = items
        .iter()
        .filter(|item| match &state.shelf {
            Shelf::All => true,
            Shelf::Favorite => item.favorite,
            Shelf::Label(label) => work.has_label(&item.path, label),
        })
        .filter(|item| state.types.is_empty() || state.types.contains(&(item.kind as u8)))
        .filter(|item| {
            state.studios.is_empty()
                || item
                    .studio
                    .as_ref()
                    .is_some_and(|studio| state.studios.contains(studio))
        })
        .filter(|item| needle.is_empty() || item.title.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    match state.sort {
        1 => shown.sort_by_cached_key(|item| item.title.to_lowercase()),
        _ => shown.sort_by_key(|item| std::cmp::Reverse(item.when)),
    }
    if state.reversed {
        shown.reverse();
    }
    shown
}

/// The browser's state.
#[derive(Debug, Default)]
pub struct ProjectsState {
    /// Which projects show.
    pub shelf: Shelf,
    /// What the names must hold.
    pub search: String,
    /// 0 by date modified (newest first), 1 by name.
    pub sort: usize,
    /// The order is turned around.
    pub reversed: bool,
    /// How the cards lie.
    pub view: View,
    /// The type chips that are on (none: every type).
    pub types: HashSet<u8>,
    /// The studio chips that are on (none: every studio's work).
    pub studios: HashSet<AppletId>,
    /// The projects chosen for the toolbar's actions.
    pub chosen: HashSet<ProjectKey>,
    /// A label being renamed, and its new name so far.
    pub(crate) renaming: Option<(String, String)>,
    /// A label just added, to rename once it shows.
    pub(crate) adding: Option<String>,
    /// Remove from List was clicked once and asks again.
    pub(crate) armed: bool,
}

impl ProjectsState {
    /// Every project on `shelf`, newest first.
    #[must_use]
    pub fn on_shelf(shelf: Shelf) -> Self {
        Self {
            shelf,
            ..Self::default()
        }
    }
}

/// Forgets a project in the work Sue remembers (files stay on disk), or
/// throws away kept work.
pub fn forget(item: &Item, cx: &mut AppletContext<'_>) {
    match &item.key {
        ProjectKey::File(path) => cx.request(ShellRequest::Forget(path.clone())),
        ProjectKey::Kept(id) => cx.request(ShellRequest::Discard(id.clone())),
    }
}

/// A texture key for `text` (a file's path), apart from the canvases'
/// keys. The salt is the one Sue Paint's start page always used, so its
/// keys did not change when the browser moved here.
#[must_use]
pub fn texture_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-paint-start", text).hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LabelEdit;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sue-wm-files-{tag}-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn picture(path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "png")
    }

    fn titles(shown: &[Item]) -> Vec<String> {
        shown.iter().map(|item| item.title.clone()).collect()
    }

    #[test]
    fn projects_come_from_the_shared_work_and_sort_and_filter() {
        let root = temp("items");
        let paint = AppletId::new("sue-paint").unwrap();
        let writer = AppletId::new("sue-writer").unwrap();
        let cat = root.join("cat.png");
        let comic = root.join("Comic.sue");
        let story = root.join("story.sue");
        let notes = root.join("notes.txt");
        for file in [&cat, &comic, &story, &notes] {
            sue_platform::fs::write(file, b"x").unwrap();
        }
        let mut work = WorkList::open(root.join("work"));
        work.remember(&cat, false, Some(paint.clone()), 100)
            .unwrap();
        work.remember(&comic, true, Some(paint.clone()), 300)
            .unwrap();
        work.remember(&story, true, Some(writer.clone()), 400)
            .unwrap();
        work.remember(&notes, false, Some(writer.clone()), 500)
            .unwrap();
        work.keep(b"SUE", None, "Sketch", Some(paint.clone()), 200)
            .unwrap();
        work.start_over();
        work.edit_labels(&LabelEdit::Favorite {
            path: cat.clone(),
            on: true,
        })
        .unwrap();
        let label = work.labels()[0].clone();
        work.edit_labels(&LabelEdit::Mark {
            path: comic.clone(),
            label: label.clone(),
            on: true,
        })
        .unwrap();

        // One studio: a Writer project and a text file are not Sue
        // Paint's.
        let opens: &dyn Fn(&Path) -> bool = &picture;
        let paint_only = [Studio {
            id: paint.clone(),
            name: "Sue Paint".to_owned(),
            opens: Some(opens),
        }];
        let all = items(&work, &paint_only);
        let mut state = ProjectsState::default();
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["Comic", "Sketch", "cat"]
        );
        state.sort = 1;
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["cat", "Comic", "Sketch"]
        );
        state.reversed = true;
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["Sketch", "Comic", "cat"]
        );
        state.reversed = false;
        state.shelf = Shelf::Favorite;
        assert_eq!(titles(&shown(&all, &state, &work)), ["cat"]);
        state.shelf = Shelf::Label(label);
        assert_eq!(titles(&shown(&all, &state, &work)), ["Comic"]);
        state.shelf = Shelf::All;
        state.types.insert(Type::Unsaved as u8);
        assert_eq!(titles(&shown(&all, &state, &work)), ["Sketch"]);
        state.types.clear();
        state.search = "CO".to_owned();
        assert_eq!(titles(&shown(&all, &state, &work)), ["Comic"]);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_studios_work_shows_and_a_studio_chip_narrows_it() {
        let root = temp("studios");
        let paint = AppletId::new("sue-paint").unwrap();
        let writer = AppletId::new("sue-writer").unwrap();
        let cat = root.join("cat.png");
        let story = root.join("story.sue");
        let notes = root.join("notes.txt");
        let stray = root.join("song.flac");
        for file in [&cat, &story, &notes, &stray] {
            sue_platform::fs::write(file, b"x").unwrap();
        }
        let mut work = WorkList::open(root.join("work"));
        work.remember(&cat, false, None, 100).unwrap();
        work.remember(&story, true, Some(writer.clone()), 200)
            .unwrap();
        work.remember(&notes, false, Some(writer.clone()), 300)
            .unwrap();
        // A file no listed studio takes stays out.
        work.remember(&stray, false, None, 400).unwrap();
        work.keep(b"SUE", None, "Draft", Some(writer.clone()), 250)
            .unwrap();
        work.start_over();

        let opens: &dyn Fn(&Path) -> bool = &picture;
        let studios = [
            Studio {
                id: paint.clone(),
                name: "Sue Paint".to_owned(),
                opens: Some(opens),
            },
            // Sue Writer's files are the ones last worked on in it.
            Studio {
                id: writer.clone(),
                name: "Sue Writer".to_owned(),
                opens: None,
            },
        ];
        let all = items(&work, &studios);
        let mut state = ProjectsState::default();
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["notes", "Draft", "story", "cat"]
        );
        let studio_of = |title: &str| {
            all.iter()
                .find(|item| item.title == title)
                .and_then(|item| item.studio.clone())
        };
        assert_eq!(studio_of("cat"), Some(paint.clone()));
        assert_eq!(studio_of("story"), Some(writer.clone()));
        state.studios.insert(writer.clone());
        assert_eq!(
            titles(&shown(&all, &state, &work)),
            ["notes", "Draft", "story"]
        );
        state.studios.insert(paint.clone());
        assert_eq!(shown(&all, &state, &work).len(), 4);
        state.studios.clear();
        state.studios.insert(paint);
        state.types.insert(Type::Picture as u8);
        assert_eq!(titles(&shown(&all, &state, &work)), ["cat"]);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }
}
