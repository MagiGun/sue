//! Several pictures brought in at once (chosen together in the file
//! dialog, or dropped together on a canvas), and layers that come to be
//! cels of an animation folder: they show on its timeline at once.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::Ui;
use sue_core::txn::SetProperty;
use sue_core::{EntityId, NodeId};
use sue_wm::ui::radio;
use sue_wm::{AppletContext, Theme};

use super::SuePaint;
use crate::anim;
use crate::commands::fold_since;
use crate::dialogs::{Dialog, Outcome};

/// What several pictures become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportAs {
    /// A layer each, on the canvas open now.
    Layers,
    /// A cel each, in a new animation folder, one frame after another.
    Frames,
    /// A canvas each.
    Canvases,
}

/// Import Pictures: the files, in the order they come in, and what they
/// become.
#[derive(Clone, Debug)]
pub(crate) struct ImportMany {
    pub paths: Vec<PathBuf>,
    pub how: ImportAs,
}

/// File names in the order people count them: `img2` before `img10`,
/// whatever the case of the letters.
pub(crate) fn natural_order(a: &Path, b: &Path) -> Ordering {
    let name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    natural(&name(a), &name(b)).then_with(|| a.cmp(b))
}

fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    let digits = |from: &mut std::iter::Peekable<std::str::Chars<'_>>| {
        let mut run = String::new();
        while let Some(digit) = from.peek().copied().filter(char::is_ascii_digit) {
            run.push(digit);
            from.next();
        }
        run
    };
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                // A run of digits counts as its number.
                let (x, y) = (digits(&mut a), digits(&mut b));
                let (bare_x, bare_y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = bare_x
                    .len()
                    .cmp(&bare_y.len())
                    .then_with(|| bare_x.cmp(bare_y))
                    .then_with(|| x.len().cmp(&y.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The dialog's body: how many pictures, and the three ways in.
pub(crate) fn import_many_ui(ui: &mut Ui, many: &mut ImportMany, theme: &Theme) -> Outcome {
    crate::dialogs::note(
        ui,
        &tr!("import-pictures-count", count = many.paths.len()),
        theme,
    );
    for (index, (how, name)) in [
        (ImportAs::Layers, tr!("import-pictures-as-layers")),
        (ImportAs::Frames, tr!("import-pictures-as-frames")),
        (ImportAs::Canvases, tr!("import-pictures-as-canvases")),
    ]
    .into_iter()
    .enumerate()
    {
        let response =
            radio(("sue-paint-import-many", index), &name, many.how == how).show(ui, theme);
        if response.clicked() {
            many.how = how;
        }
    }
    Outcome::Open
}

impl SuePaint {
    /// Several pictures came in together: with a canvas open, asks what
    /// they become; with none, each opens as a canvas.
    pub(crate) fn ask_import_many(&mut self, cx: &mut AppletContext<'_>, paths: Vec<PathBuf>) {
        let mut many = ImportMany {
            paths,
            how: ImportAs::Layers,
        };
        many.paths.sort_by(|a, b| natural_order(a, b));
        many.paths.dedup();
        if self.picture().is_none() {
            many.how = ImportAs::Canvases;
            self.import_many_ok(cx, &many);
        } else {
            self.dialog = Some(Dialog::ImportMany(many));
        }
    }

    /// One more picture of a drop of several: it joins the pictures the
    /// dialog already asks about, in its place by name.
    pub(crate) fn add_to_import_many(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        if let Some(Dialog::ImportMany(many)) = &mut self.dialog {
            if !many.paths.iter().any(|known| known == path) {
                many.paths.push(path.to_path_buf());
                many.paths.sort_by(|a, b| natural_order(a, b));
            }
        } else {
            self.ask_import_many(cx, vec![path.to_path_buf()]);
        }
    }

    /// Brings the pictures in as the dialog says. The layers, and the
    /// frames with their folder, are one undo step.
    pub(crate) fn import_many_ok(&mut self, cx: &mut AppletContext<'_>, many: &ImportMany) {
        let picture = match (many.how, self.picture()) {
            (ImportAs::Layers | ImportAs::Frames, Some(picture)) => picture,
            _ => {
                for path in &many.paths {
                    self.open_picked(cx, path, false);
                }
                return;
            }
        };
        let label = tr!("import-pictures", count = many.paths.len());
        let before = self.unkeyed_cels(&cx.world.arena);
        let (mark, recorded) = (cx.history.cursor(), cx.history.len());
        if many.how == ImportAs::Layers {
            for path in &many.paths {
                self.open_picked(cx, path, true);
            }
            fold_since(cx, mark, label);
            self.cel_new_layers(cx, before.as_deref(), recorded);
            return;
        }
        let mut cels = Vec::new();
        for path in &many.paths {
            let file = path
                .file_name()
                .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
            let read = sue_platform::fs::read(path)
                .map_err(|error| tr!("cannot-read", path = path.display(), error = error))
                .and_then(|bytes| super::files::read_pixels(path, &file, &bytes));
            match read {
                Ok((_, rgba)) => {
                    let name = path
                        .file_stem()
                        .map_or_else(|| file.clone(), |stem| stem.to_string_lossy().into_owned());
                    cels.push((name, rgba));
                }
                Err(error) => {
                    cx.fail(error);
                    return;
                }
            }
        }
        // The folder is named after the first file, without its number.
        let name = cels
            .first()
            .map(|(name, _)| {
                name.trim_end_matches(|c: char| c.is_ascii_digit() || " _-.".contains(c))
                    .to_owned()
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| tr!("imported"));
        let below = self.doc().and_then(|doc| doc.layer);
        match anim::import_cels(&cx.world.arena, picture, below, &name, cels, 12.0, &label) {
            Ok((edit, track)) => {
                if Self::apply(cx, edit) {
                    self.panels.timeline = true;
                    if let Some(doc) = self.doc_mut() {
                        doc.track = track;
                    }
                    if let Some(dir) = many.paths.first().and_then(|path| path.parent()) {
                        self.last_dir = Some(dir.to_path_buf());
                    }
                }
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// The cels (layers right in an animation folder) no key of their
    /// folder's track shows, bottom first; `None` without a timeline.
    pub(crate) fn unkeyed_cels(&self, arena: &sue_core::Arena) -> Option<Vec<EntityId>> {
        let picture = self.picture()?;
        let sequence = anim::timeline(arena, picture)?;
        let mut out = Vec::new();
        for track in anim::folders(sequence) {
            let Some(folder) = anim::track_folder(&sequence.tracks[track]) else {
                continue;
            };
            let keyed: Vec<Option<EntityId>> = anim::keys(&sequence.tracks[track], sequence.rate)
                .into_iter()
                .map(|(_, cel)| cel)
                .collect();
            out.extend(
                anim::cels(arena, picture, folder)
                    .into_iter()
                    .filter(|cel| !keyed.contains(&Some(*cel))),
            );
        }
        Some(out)
    }

    /// A layer that just came to be in an animation folder (imported or
    /// dragged there; one made or pasted there stays a layer to draw on
    /// until Specify Cels puts it on a frame) shows on the timeline at once: it gets
    /// a key at the frame shown, several one frame after another, in the
    /// undo step that put it there. `before` is what
    /// [`Self::unkeyed_cels`] said before that step and `recorded` how
    /// many steps the history held: cels that had no key before keep
    /// none, and nothing is done unless a step was added.
    pub(crate) fn cel_new_layers(
        &mut self,
        cx: &mut AppletContext<'_>,
        before: Option<&[EntityId]>,
        recorded: usize,
    ) {
        let (Some(before), Some(picture)) = (before, self.picture()) else {
            return;
        };
        if cx.history.len() == recorded {
            return;
        }
        let arena = &cx.world.arena;
        let new: Vec<EntityId> = self
            .unkeyed_cels(arena)
            .unwrap_or_default()
            .into_iter()
            .filter(|cel| !before.contains(cel))
            .collect();
        let Some(sequence) = anim::timeline(arena, picture).cloned() else {
            return;
        };
        if new.is_empty() {
            return;
        }
        let was = anim::length(arena, picture);
        let shown = self.doc().map_or(0, |doc| doc.frame);
        let mut next: HashMap<usize, u32> = HashMap::new();
        let mut placed = Vec::new();
        for cel in new {
            if let Some(track) = anim::track_for_layer(arena, &sequence, cel) {
                let frame = next.entry(track).or_insert(shown);
                placed.push((track, *frame, cel));
                *frame += 1;
            }
        }
        let length = placed
            .iter()
            .map(|(_, frame, _)| frame + 1)
            .fold(was, u32::max);
        let mut out = sequence.clone();
        for (track, frame, cel) in placed {
            out = anim::assign(&out, track, frame, Some(cel), picture, length);
        }
        if out == sequence {
            return;
        }
        let last = cx.history.cursor();
        let Some(mark) = cx.history.parent(last) else {
            return;
        };
        let label = cx.history.label(last).unwrap_or_default().to_owned();
        let mut edit = anim::set(picture, out, &label);
        if length > was {
            edit = edit.then(SetProperty::new(picture, anim::LENGTH, i64::from(length)));
        }
        if Self::apply(cx, edit) {
            fold_since(cx, mark, label);
        }
    }

    /// Place Layer Here: `layer` shows on `track` from `frame` on. A
    /// layer from outside the track's animation folder goes into it
    /// first (as its top cel), all in one undo step.
    pub(crate) fn place_layer_on_cel(
        &mut self,
        cx: &mut AppletContext<'_>,
        track: usize,
        frame: u32,
        layer: EntityId,
    ) {
        let Some(picture) = self.picture() else {
            return;
        };
        let mark: NodeId = cx.history.cursor();
        let label = tr!("place-layer-here");
        let arena = &cx.world.arena;
        let Some(sequence) = anim::timeline(arena, picture) else {
            return;
        };
        if !anim::may_show(arena, sequence, track, layer) {
            let moved = sequence
                .tracks
                .get(track)
                .and_then(anim::track_folder)
                .and_then(|folder| anim::drop_layer(arena, picture, layer, folder));
            let Some(moved) = moved else {
                cx.fail(tr!("only-cels-of-this-animation-folder"));
                return;
            };
            if !Self::apply(cx, moved) {
                return;
            }
        }
        let arena = &cx.world.arena;
        if let Some(sequence) = anim::timeline(arena, picture)
            && let Some(track) = anim::track_for_layer(arena, sequence, layer)
        {
            let length = anim::length(arena, picture).max(frame + 1);
            let changed = anim::assign(sequence, track, frame, Some(layer), picture, length);
            Self::apply(cx, anim::set(picture, changed, &label));
            if let Some(doc) = self.doc_mut() {
                doc.track = track;
                doc.layer = Some(layer);
                doc.picked.clear();
            }
        }
        fold_since(cx, mark, label);
        self.go_to_frame(&cx.world.arena, i64::from(frame));
    }

    /// A layer dragged from the Layer palette and let go at `at`: over a
    /// frame of an animation folder's row of the timeline it is placed
    /// there. `false` when `at` is not over such a frame.
    pub(crate) fn place_dropped(
        &mut self,
        dragged: EntityId,
        at: egui::Pos2,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let Some((track, frame)) = self.anim_ui.cel_frame_at(at) else {
            return false;
        };
        self.place_layer_on_cel(cx, track, frame, dragged);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::natural_order;
    use std::path::PathBuf;

    #[test]
    fn file_names_sort_the_way_people_count() {
        let mut names: Vec<PathBuf> = ["img10.png", "img2.png", "IMG1.png", "img02.png", "a.png"]
            .iter()
            .map(PathBuf::from)
            .collect();
        names.sort_by(|a, b| natural_order(a, b));
        let names: Vec<String> = names
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["a.png", "IMG1.png", "img2.png", "img02.png", "img10.png"]
        );
    }
}
