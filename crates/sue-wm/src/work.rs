//! The work Sue remembers between runs, shared by every studio and tool and
//! listed on the Hub: the files and projects opened and saved lately, and
//! the work Sue kept by itself while it was not saved (autosaves).
//!
//! It lives in a folder of Sue's own. `recent.txt` lists the files, one per
//! line; `autosave/` holds each kept project as `<id>.sue` with a note
//! about it in `<id>.txt`. Both start with a format line, so a later Sue
//! reads them. A list without a folder (tests, the self-check) remembers
//! nothing on disk.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppletId;

/// The first line of `recent.txt`.
const RECENT_HEADER: &str = "# Sue recent files, format 1";
/// The first line of an autosave's note.
const NOTE_HEADER: &str = "# Sue autosave, format 1";
/// How many files the list remembers at most.
const KEEP: usize = 100;

/// Seconds since 1970, as the list keeps times.
#[must_use]
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// How long ago `then` was, as people say it: `just now`, `5 minutes ago`,
/// `yesterday`, or a date such as `2026-10-12` after a week.
#[must_use]
pub fn ago(then: u64, now: u64) -> String {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..60 => tr!("ago-just-now"),
        60..3_600 => tr!("ago-minutes", count = seconds / 60),
        3_600..86_400 => tr!("ago-hours", count = seconds / 3_600),
        86_400..172_800 => tr!("ago-yesterday"),
        172_800..604_800 => tr!("ago-days", count = seconds / 86_400),
        _ => {
            let (year, month, day) = civil_date(then / 86_400);
            format!("{year}-{month:02}-{day:02}")
        }
    }
}

/// The calendar date of a day counted from 1970-01-01 (Howard Hinnant's
/// civil-from-days).
fn civil_date(days: u64) -> (i64, u32, u32) {
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A file name as people read it: `story.sue` gives `story`.
fn title_of(path: &Path) -> String {
    path.file_stem().or_else(|| path.file_name()).map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A file or project opened or saved lately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFile {
    /// Where it is.
    pub path: PathBuf,
    /// When it was last opened, in seconds since 1970 (0: never).
    pub opened: u64,
    /// When it was last saved (0: never).
    pub saved: u64,
    /// The studio or tool it was last worked on in, if known.
    pub page: Option<AppletId>,
}

impl RecentFile {
    /// When it was last opened or saved.
    #[must_use]
    pub fn last(&self) -> u64 {
        self.opened.max(self.saved)
    }

    /// `true` for a Sue project (`.sue`).
    #[must_use]
    pub fn is_project(&self) -> bool {
        self.path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sue"))
    }

    /// Its name without the extension, e.g. `story`.
    #[must_use]
    pub fn title(&self) -> String {
        title_of(&self.path)
    }
}

/// Work Sue kept by itself while it was not saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Autosave {
    /// Its id, unique to the run that kept it.
    pub id: String,
    /// The kept project.
    pub file: PathBuf,
    /// The project file it belongs to, once saved somewhere.
    pub project: Option<PathBuf>,
    /// When it was kept, in seconds since 1970.
    pub at: u64,
    /// What it holds, as people read it, e.g. `Illustration`.
    pub name: String,
    /// The studio or tool it was worked on in.
    pub page: Option<AppletId>,
}

impl Autosave {
    /// Its title: the project's file name, or what it holds.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.project {
            Some(project) => title_of(project),
            None if self.name.is_empty() => tr!("untitled"),
            None => self.name.clone(),
        }
    }

    fn note(&self) -> String {
        let mut note = format!("{NOTE_HEADER}\nat={}\n", self.at);
        if let Some(project) = &self.project {
            note.push_str(&format!("project={}\n", project.display()));
        }
        if !self.name.is_empty() {
            note.push_str(&format!("name={}\n", one_line(&self.name)));
        }
        if let Some(page) = &self.page {
            note.push_str(&format!("page={page}\n"));
        }
        note
    }

    fn parse(id: &str, file: PathBuf, note: &str) -> Option<Self> {
        let mut lines = note.lines();
        if lines.next()?.trim() != NOTE_HEADER {
            return None;
        }
        let mut autosave = Self {
            id: id.to_owned(),
            file,
            project: None,
            at: 0,
            name: String::new(),
            page: None,
        };
        for line in lines {
            match line.split_once('=') {
                Some(("at", value)) => autosave.at = value.trim().parse().unwrap_or(0),
                Some(("project", value)) if !value.is_empty() => {
                    autosave.project = Some(PathBuf::from(value));
                }
                Some(("name", value)) => autosave.name = value.to_owned(),
                Some(("page", value)) => autosave.page = AppletId::new(value.trim()).ok(),
                _ => {}
            }
        }
        Some(autosave)
    }
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// The files and projects of late and the kept work, newest first.
#[derive(Debug, Default)]
pub struct WorkList {
    /// Where the list lives; `None` keeps it in memory only.
    dir: Option<PathBuf>,
    files: Vec<RecentFile>,
    autosaves: Vec<Autosave>,
    /// The autosave this run keeps, left out of [`Self::unsaved`].
    own: Option<String>,
    /// How many ids this run made, so each is new.
    made: u32,
    revision: u64,
}

impl WorkList {
    /// The list kept in `dir`, read from it.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        let mut list = Self {
            dir: Some(dir),
            ..Self::default()
        };
        list.read();
        list
    }

    /// The folder the list lives in, if any.
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Goes up on every change, so a menu built from the list knows when to
    /// build itself again.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn recent_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join("recent.txt"))
    }

    fn autosave_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|dir| dir.join("autosave"))
    }

    fn read(&mut self) {
        self.files = self
            .recent_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| parse_recent(&text))
            .unwrap_or_default();
        self.read_autosaves();
        self.revision += 1;
    }

    fn read_autosaves(&mut self) {
        let mut autosaves = Vec::new();
        if let Some(entries) = self
            .autosave_dir()
            .and_then(|dir| std::fs::read_dir(dir).ok())
        {
            for entry in entries.flatten() {
                let note = entry.path();
                if note.extension().and_then(|ext| ext.to_str()) != Some("txt") {
                    continue;
                }
                let Some(id) = note.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };
                let file = note.with_extension("sue");
                if !file.is_file() {
                    continue;
                }
                if let Some(autosave) = std::fs::read_to_string(&note)
                    .ok()
                    .and_then(|text| Autosave::parse(id, file, &text))
                {
                    autosaves.push(autosave);
                }
            }
        }
        autosaves.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
        self.autosaves = autosaves;
    }

    /// Reads the list again (another run may have changed it), forgets
    /// files that are gone, and lets go of kept work its project file has
    /// since caught up with.
    pub fn refresh(&mut self) {
        if self.dir.is_some() {
            self.files = self
                .recent_path()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .map(|text| parse_recent(&text))
                .unwrap_or_default();
            self.read_autosaves();
        }
        let before = self.files.len();
        self.files.retain(|file| file.path.exists());
        if self.files.len() != before {
            let _ = self.write_recent();
        }
        let stale: Vec<String> = self
            .autosaves
            .iter()
            .filter(|autosave| Some(&autosave.id) != self.own.as_ref())
            .filter(|autosave| {
                autosave
                    .project
                    .as_ref()
                    .and_then(|project| std::fs::metadata(project).ok())
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .is_some_and(|modified| modified.as_secs() > autosave.at)
            })
            .map(|autosave| autosave.id.clone())
            .collect();
        for id in stale {
            let _ = self.discard(&id);
        }
        self.revision += 1;
    }

    /// Puts `path` first: opened (or `saved`) at `now`, in `page`.
    ///
    /// # Errors
    /// If the list cannot be written; it is still remembered for this run.
    pub fn remember(
        &mut self,
        path: &Path,
        saved: bool,
        page: Option<AppletId>,
        now: u64,
    ) -> std::io::Result<()> {
        let text = path.to_string_lossy();
        if text.is_empty() || text.contains(['\n', '\r']) {
            return Ok(());
        }
        let mut file = match self.files.iter().position(|file| file.path == path) {
            Some(index) => self.files.remove(index),
            None => RecentFile {
                path: path.to_path_buf(),
                opened: 0,
                saved: 0,
                page: None,
            },
        };
        if saved {
            file.saved = now;
        } else {
            file.opened = now;
        }
        if page.is_some() {
            file.page = page;
        }
        self.files.insert(0, file);
        self.files.truncate(KEEP);
        self.revision += 1;
        self.write_recent()
    }

    /// Forgets `path`.
    ///
    /// # Errors
    /// If the list cannot be written.
    pub fn forget(&mut self, path: &Path) -> std::io::Result<()> {
        let before = self.files.len();
        self.files.retain(|file| file.path != path);
        if self.files.len() == before {
            return Ok(());
        }
        self.revision += 1;
        self.write_recent()
    }

    fn write_recent(&self) -> std::io::Result<()> {
        let Some(path) = self.recent_path() else {
            return Ok(());
        };
        let mut text = format!("{RECENT_HEADER}\n");
        for file in &self.files {
            let page = file.page.as_ref().map_or("", AppletId::as_str);
            text.push_str(&format!(
                "{}\t{}\t{page}\t{}\n",
                file.opened,
                file.saved,
                file.path.display()
            ));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        sue_platform::write_atomic(&path, text.as_bytes())
    }

    /// The files and projects of late, newest first.
    pub fn recent(&self) -> impl Iterator<Item = &RecentFile> {
        self.files.iter()
    }

    /// The Sue projects the list knows, by name.
    #[must_use]
    pub fn projects(&self) -> Vec<&RecentFile> {
        let mut projects: Vec<&RecentFile> =
            self.files.iter().filter(|file| file.is_project()).collect();
        projects.sort_by_cached_key(|file| (file.title().to_lowercase(), file.path.clone()));
        projects
    }

    /// The work other runs kept and nobody saved, newest first.
    pub fn unsaved(&self) -> impl Iterator<Item = &Autosave> {
        self.autosaves
            .iter()
            .filter(|autosave| Some(&autosave.id) != self.own.as_ref())
    }

    /// The kept work with `id`.
    #[must_use]
    pub fn autosave(&self, id: &str) -> Option<&Autosave> {
        self.autosaves.iter().find(|autosave| autosave.id == id)
    }

    /// The id this run keeps its work under, made on first use.
    pub fn own_id(&mut self, now: u64) -> String {
        if let Some(id) = &self.own {
            return id.clone();
        }
        self.made += 1;
        let id = format!("{now}-{}-{}", std::process::id(), self.made);
        self.own = Some(id.clone());
        id
    }

    /// Keeps whatever comes next apart from the work kept so far, which
    /// stays for the Hub to offer (another project replaced it).
    pub fn start_over(&mut self) {
        self.own = None;
        self.revision += 1;
    }

    /// This run keeps its work under `id` from now on (it took over that
    /// kept work).
    pub fn adopt(&mut self, id: &str) {
        self.own = Some(id.to_owned());
        self.revision += 1;
    }

    /// Keeps `bytes` (a `.sue` project) as this run's work, with what it
    /// holds; `project` is the file it belongs to, if saved somewhere.
    ///
    /// # Errors
    /// If the files cannot be written.
    pub fn keep(
        &mut self,
        bytes: &[u8],
        project: Option<&Path>,
        name: &str,
        page: Option<AppletId>,
        now: u64,
    ) -> std::io::Result<()> {
        let id = self.own_id(now);
        let Some(dir) = self.autosave_dir() else {
            return Ok(());
        };
        std::fs::create_dir_all(&dir)?;
        let autosave = Autosave {
            file: dir.join(format!("{id}.sue")),
            id: id.clone(),
            project: project.map(Path::to_path_buf),
            at: now,
            name: name.to_owned(),
            page,
        };
        sue_platform::write_atomic(&autosave.file, bytes)?;
        sue_platform::write_atomic(&dir.join(format!("{id}.txt")), autosave.note().as_bytes())?;
        self.autosaves.retain(|kept| kept.id != id);
        self.autosaves.insert(0, autosave);
        self.revision += 1;
        Ok(())
    }

    /// Lets go of this run's kept work (it was saved, or there is none).
    ///
    /// # Errors
    /// If the files cannot be removed.
    pub fn release(&mut self) -> std::io::Result<()> {
        match self.own.clone() {
            Some(id) => self.discard(&id),
            None => Ok(()),
        }
    }

    /// Throws away the kept work with `id`.
    ///
    /// # Errors
    /// If its files cannot be removed.
    pub fn discard(&mut self, id: &str) -> std::io::Result<()> {
        let Some(index) = self.autosaves.iter().position(|kept| kept.id == id) else {
            return Ok(());
        };
        let autosave = self.autosaves.remove(index);
        self.revision += 1;
        for file in [autosave.file.clone(), autosave.file.with_extension("txt")] {
            match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

/// Reads `recent.txt`: `opened`, `saved`, the page and the path, split by
/// tabs (the path last, so it may hold anything but a line break).
fn parse_recent(text: &str) -> Vec<RecentFile> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(RECENT_HEADER) {
        return Vec::new();
    }
    lines
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let opened = parts.next()?.parse().ok()?;
            let saved = parts.next()?.parse().ok()?;
            let page = parts.next()?;
            let path = parts.next().filter(|path| !path.is_empty())?;
            Some(RecentFile {
                path: PathBuf::from(path),
                opened,
                saved,
                page: AppletId::new(page).ok(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("sue-work-{tag}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn files_come_back_newest_first_and_survive_a_restart() {
        let root = temp("recent");
        let picture = root.join("cat.png");
        let story = root.join("story.sue");
        let album = root.join("Album.sue");
        for file in [&picture, &story, &album] {
            std::fs::write(file, b"x").unwrap();
        }
        let paint = AppletId::new("sue-paint").unwrap();
        let mut list = WorkList::open(root.join("work"));
        list.remember(&story, true, None, 100).unwrap();
        list.remember(&picture, false, Some(paint.clone()), 200)
            .unwrap();
        list.remember(&album, false, None, 300).unwrap();
        // Opened again later, a file moves back to the top and keeps when it
        // was saved.
        list.remember(&story, false, None, 400).unwrap();
        let order: Vec<&Path> = list.recent().map(|file| file.path.as_path()).collect();
        assert_eq!(order, [story.as_path(), album.as_path(), picture.as_path()]);
        assert_eq!(list.recent().next().unwrap().saved, 100);
        assert_eq!(list.recent().next().unwrap().last(), 400);

        // Read again, the same list; the projects come by name.
        let mut again = WorkList::open(root.join("work"));
        assert_eq!(
            again.recent().cloned().collect::<Vec<_>>(),
            list.recent().cloned().collect::<Vec<_>>()
        );
        assert_eq!(again.recent().nth(2).unwrap().page, Some(paint));
        let projects: Vec<String> = again.projects().iter().map(|file| file.title()).collect();
        assert_eq!(projects, ["Album", "story"]);

        // A file that is gone is forgotten; so is one asked for.
        std::fs::remove_file(&album).unwrap();
        again.refresh();
        assert_eq!(again.recent().count(), 2);
        again.forget(&picture).unwrap();
        assert_eq!(WorkList::open(root.join("work")).recent().count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn kept_work_shows_in_other_runs_until_saved_or_discarded() {
        let root = temp("autosave");
        let project = root.join("story.sue");
        let writer = AppletId::new("sue-writer").unwrap();
        let mut first = WorkList::open(root.join("work"));
        first
            .keep(b"one", None, "Novel", Some(writer.clone()), 1_000)
            .unwrap();
        assert_eq!(
            first.unsaved().count(),
            0,
            "a run's own work is not shown to it"
        );
        let own = first.own_id(0);

        // The next run sees it, with what it holds.
        let mut second = WorkList::open(root.join("work"));
        let kept = second.unsaved().next().cloned().unwrap();
        assert_eq!(kept.id, own);
        assert_eq!(kept.title(), "Novel");
        assert_eq!(kept.page, Some(writer));
        assert_eq!(std::fs::read(&kept.file).unwrap(), b"one");

        // Kept again after it was saved as a project, it says which.
        first
            .keep(b"two", Some(&project), "Novel", None, 2_000)
            .unwrap();
        second.refresh();
        let kept = second.unsaved().next().cloned().unwrap();
        assert_eq!(kept.title(), "story");
        assert_eq!(kept.project.as_deref(), Some(project.as_path()));

        // A run that takes it over keeps it as its own.
        second.adopt(&kept.id);
        assert_eq!(second.unsaved().count(), 0);
        // Discarded, it is gone for everyone.
        second.discard(&kept.id).unwrap();
        assert!(!kept.file.exists());
        assert_eq!(WorkList::open(root.join("work")).unsaved().count(), 0);

        // Kept work its project file caught up with is let go.
        let mut third = WorkList::open(root.join("work"));
        third.keep(b"three", Some(&project), "", None, 10).unwrap();
        std::fs::write(&project, b"saved later").unwrap();
        let mut fourth = WorkList::open(root.join("work"));
        assert_eq!(fourth.unsaved().count(), 1);
        fourth.refresh();
        assert_eq!(fourth.unsaved().count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_list_without_a_folder_writes_nothing() {
        let mut list = WorkList::default();
        list.remember(Path::new("/nowhere/a.sue"), true, None, 5)
            .unwrap();
        list.keep(b"x", None, "", None, 5).unwrap();
        assert_eq!(list.recent().count(), 1);
        assert_eq!(list.unsaved().count(), 0);
        assert!(list.dir().is_none());
    }

    #[test]
    fn times_read_as_people_say_them() {
        let now = 1_761_696_000; // 2025-10-29
        assert_eq!(ago(now - 5, now), "just now");
        assert_eq!(ago(now - 60, now), "a minute ago");
        assert_eq!(ago(now - 600, now), "10 minutes ago");
        assert_eq!(ago(now - 7_200, now), "2 hours ago");
        assert_eq!(ago(now - 90_000, now), "yesterday");
        assert_eq!(ago(now - 3 * 86_400, now), "3 days ago");
        assert_eq!(ago(now - 30 * 86_400, now), "2025-09-29");
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(19_782), (2024, 2, 29));
    }
}
