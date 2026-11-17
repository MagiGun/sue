//! A sub tool's brush as a brush preset and back, so every setting Tool
//! Property and Sub Tool Detail show is kept in the shareable brush files
//! (`docs/brush-files.md`): "Save all settings as default" (full guide
//! FGV-12) writes the sub tool there, and the brush shape presets
//! (00:29:47) are brushes of that file too.
//!
//! The file is a brush library (`sue_io::brushes::library`) in Sue
//! Paint's data folder, [`FILE`]: a set [`DEFAULTS`] with one brush for
//! each sub tool saved as its default, named `<tool id>/<sub tool name>`,
//! and a set [`SHAPES`] with the brush shapes a person registered. A
//! person can share the file, or install it in the brush library, as any
//! other brush file.

use std::collections::BTreeMap;
use std::path::Path;

use sue_core::{Arena, CHILD, Entity, EntityId, Relation};
use sue_model::brush::{Pick, PictureRef, TipSource};
use sue_model::{BrushPreset, class};
use sue_picture::tools::brush_rows::{BrushRows, PresetRows};

use crate::brush::Tip;
use crate::tools::{SubTool, Tool};
use crate::vector::{BrushPicture, LineBrush};

/// The brush file in Sue Paint's data folder.
pub(crate) const FILE: &str = "sub-tool-brushes.sue";
/// The set of sub tools saved as their defaults.
pub(crate) const DEFAULTS: &str = "Defaults";
/// The set of brush shapes a person registered.
pub(crate) const SHAPES: &str = "Brush shapes";

/// The tagged fields a sub tool's preset carries for Sue Paint.
mod field {
    /// The tip as its text (`Tip::to_text`).
    pub const TIP: &str = "sue_paint.tip";
    /// The names of the tip's pictures, one a line.
    pub const TIPS: &str = "sue_paint.tips";
    /// The sub tool's mode.
    pub const MODE: &str = "sue_paint.mode";
}

/// The shape numbers of the pictures `sub`'s tip lays, in order (empty
/// for a round tip).
pub(crate) fn tips_of(sub: &SubTool) -> Vec<u32> {
    if let Some(rows) = &sub.brush_rows.preset
        && !rows.tips.is_empty()
    {
        return rows.tips.clone();
    }
    if sub.tip.shape == 0 {
        return Vec::new();
    }
    (sub.tip.shape..sub.tip.shape.saturating_add(sub.tip.shape_count.max(1))).collect()
}

/// Lays `rows` over `brush`, the brush of a sub tool with `tip`: the
/// preset's settings, and the tip's pictures when a person chose them.
pub(crate) fn apply_preset_rows(brush: &mut LineBrush, rows: &PresetRows, tip: &Tip) {
    rows.apply(&mut brush.preset, tip.spacing);
    if rows.tips.is_empty() {
        return;
    }
    let mut refs = Vec::new();
    let mut pictures = Vec::new();
    for id in &rows.tips {
        if let Some(picture) = crate::brush::shape(*id)
            .as_deref()
            .and_then(BrushPicture::of_shape)
        {
            let hash = picture.hash();
            refs.push(PictureRef::Hash(hash));
            if !pictures.iter().any(|(known, _)| *known == hash) {
                pictures.push((hash, picture));
            }
        }
    }
    if refs.is_empty() {
        return;
    }
    let (pick, keep_colors) = match &brush.preset.tip {
        TipSource::Images {
            pick, keep_colors, ..
        } => (*pick, *keep_colors),
        _ => (Pick::Random, tip.keep_colors),
    };
    brush.preset.tip = TipSource::Images {
        pictures: refs,
        pick,
        keep_colors,
    };
    brush.pictures = pictures;
}

/// The preset settings `sub` paints with now: what a person set, else
/// its brush's own.
pub(crate) fn preset_rows_of(sub: &SubTool) -> PresetRows {
    sub.brush_rows.preset.clone().unwrap_or_else(|| {
        crate::canvas::engine::brush_of(sub, false, false)
            .map(|brush| PresetRows::of_preset(&brush.preset))
            .unwrap_or_default()
    })
}

/// Sets tagged field `key` of `preset`.
fn put(preset: &mut BrushPreset, key: &str, bytes: Vec<u8>) {
    if let Some(slot) = preset.extra.iter_mut().find(|(known, _)| known == key) {
        slot.1 = bytes;
    } else {
        preset.extra.push((key.to_owned(), bytes));
    }
}

fn text<'p>(preset: &'p BrushPreset, key: &str) -> Option<&'p str> {
    preset
        .extra(key)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
}

/// `sub` as a brush preset that keeps every one of its settings, its
/// pictures named as the brush tips Sue Paint keeps; `None` for a sub tool
/// that does not paint.
pub(crate) fn to_preset(sub: &SubTool) -> Option<BrushPreset> {
    let brush = crate::canvas::engine::brush_of(sub, false, false)?;
    let mut preset = brush.preset;
    let names: Vec<String> = tips_of(sub)
        .into_iter()
        .filter_map(crate::brush::shape)
        .map(|shape| shape.name.clone())
        .collect();
    if let TipSource::Images { pictures, .. } = &mut preset.tip
        && !names.is_empty()
    {
        *pictures = names.iter().cloned().map(PictureRef::Named).collect();
    }
    preset.name.clone_from(&sub.name);
    preset.id.clear();
    put(&mut preset, field::TIP, sub.tip.to_text().into_bytes());
    put(&mut preset, field::TIPS, names.join("\n").into_bytes());
    put(&mut preset, field::MODE, vec![sub.mode]);
    sub.brush_rows.write_to(&mut preset);
    Some(preset)
}

/// The sub tool of `tool` that `preset` keeps ([`to_preset`]): every
/// setting as it was saved, tips Sue Paint no longer has left out.
pub(crate) fn from_preset(tool: Tool, preset: &BrushPreset) -> SubTool {
    let mut sub = SubTool::builtin(tool, &preset.name)
        .unwrap_or_else(|| SubTool::new(tool, &preset.name, 0, Tip::default()));
    preset.name.clone_into(&mut sub.name);
    sub.tip = match text(preset, field::TIP) {
        Some(text) => Tip::from_text(text),
        None => Tip::from_text(&sue_brush::presets::legacy_text(preset)),
    };
    if let Some(mode) = preset.extra(field::MODE).and_then(|bytes| bytes.first()) {
        sub.mode = *mode;
    }
    let mut rows = BrushRows::read_from(preset);
    let names: Vec<u32> = text(preset, field::TIPS)
        .unwrap_or_default()
        .lines()
        .filter_map(|name| {
            crate::brush::shapes()
                .into_iter()
                .find(|(_, known)| known == name)
                .map(|(id, _)| id)
        })
        .collect();
    if let Some(preset_rows) = &mut rows.preset
        && names.len() > 1
    {
        preset_rows.tips = names;
    }
    sub.brush_rows = rows;
    sub.behaviours = Some(sue_brush::paint::Behaviours::from_preset(preset));
    sub.stabilization = preset.input.stabilization;
    sub.adjust_by_speed = preset.input.by_speed;
    sub.correction = preset.input.post_correction;
    sub.flick = preset.input.flick;
    sub.smart_shape = preset.input.smart_shape;
    sub
}

/// What a brush of [`DEFAULTS`] is named for `sub`.
pub(crate) fn default_name(tool: Tool, name: &str) -> String {
    format!("{}/{name}", tool.id())
}

/// The brushes a person keeps: the sub tools saved as defaults, and the
/// brush shapes registered, each by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Kept {
    /// The sub tools saved as their defaults, by [`default_name`].
    pub defaults: BTreeMap<String, BrushPreset>,
    /// The brush shapes registered, in order.
    pub shapes: Vec<BrushPreset>,
    /// The file was read (or found missing) this run.
    pub loaded: bool,
}

impl Kept {
    /// The sub tool `sub` was saved as, if it was.
    pub(crate) fn default_of(&self, tool: Tool, name: &str) -> Option<SubTool> {
        self.defaults
            .get(&default_name(tool, name))
            .map(|preset| from_preset(tool, preset))
    }

    /// The brushes as a brush library file.
    #[must_use]
    pub(crate) fn to_file(&self) -> Vec<u8> {
        let mut arena = Arena::new();
        let root = Entity::empty(sue_io::brushes::library::LIBRARY_NAME, class::FOLDER)
            .with_property(sue_io::brushes::library::LIBRARY, true);
        let root_id = root.id();
        let _ = arena.insert_keep_id(root);
        // A default is named by its key; a shape by its own name.
        let sets: [(&str, Vec<(&str, &BrushPreset)>); 2] = [
            (
                DEFAULTS,
                self.defaults
                    .iter()
                    .map(|(key, preset)| (key.as_str(), preset))
                    .collect(),
            ),
            (
                SHAPES,
                self.shapes
                    .iter()
                    .map(|preset| (preset.name.as_str(), preset))
                    .collect(),
            ),
        ];
        for (name, presets) in sets {
            let set = Entity::empty(name, class::FOLDER);
            let set_id = set.id();
            let _ = arena.insert_keep_id(set);
            let order = arena.relations().next_order(root_id, CHILD);
            let _ = arena.relate(Relation::new(root_id, CHILD, set_id).ordered(order));
            for (key, preset) in presets {
                let brush = Entity::empty(key, class::BRUSH).with_ext(preset.clone());
                let id: EntityId = brush.id();
                let _ = arena.insert_keep_id(brush);
                let order = arena.relations().next_order(set_id, CHILD);
                let _ = arena.relate(Relation::new(set_id, CHILD, id).ordered(order));
            }
        }
        sue_io::brushes::library::to_file(&arena)
    }

    /// The brushes of a file written by [`Self::to_file`] (or any brush
    /// file with sets of those names).
    ///
    /// # Errors
    /// When the bytes are not a brush file.
    pub(crate) fn from_file(bytes: &[u8]) -> Result<Self, String> {
        use sue_io::brushes::library;
        let arena = library::from_file(bytes)?;
        let mut kept = Self {
            loaded: true,
            ..Self::default()
        };
        for set in library::sets(&arena) {
            let name = arena
                .get_by_id(set)
                .map(|entity| entity.name().to_owned())
                .unwrap_or_default();
            for brush in library::brushes(&arena, set) {
                let Some(preset) = library::preset(&arena, brush) else {
                    continue;
                };
                match name.as_str() {
                    DEFAULTS => {
                        let key = arena
                            .get_by_id(brush)
                            .map(|entity| entity.name().to_owned())
                            .unwrap_or_default();
                        kept.defaults.insert(key, preset.clone());
                    }
                    SHAPES => kept.shapes.push(preset.clone()),
                    _ => {}
                }
            }
        }
        Ok(kept)
    }

    /// Reads the file in `folder` (nothing kept when it is missing).
    pub(crate) fn load(folder: &Path) -> Result<Self, String> {
        match sue_platform::fs::read(&folder.join(FILE)) {
            Ok(bytes) => Self::from_file(&bytes),
            Err(_) => Ok(Self {
                loaded: true,
                ..Self::default()
            }),
        }
    }

    /// Writes the file into `folder`.
    pub(crate) fn save(&self, folder: &Path) -> Result<(), String> {
        sue_platform::fs::create_dir_all(folder)
            .and_then(|()| sue_platform::fs::write(&folder.join(FILE), self.to_file()))
            .map_err(|error| error.to_string())
    }
}

/// A brush shape: what of a sub tool's brush makes its mark (the tip and
/// its pictures, hardness, thickness, angle and turning, the spraying
/// effect and the gap), apart from its size, ink and correction.
pub(crate) fn copy_shape(from: &SubTool, to: &mut SubTool) {
    let (s, t) = (&from.tip, &mut to.tip);
    t.shape = s.shape;
    t.shape_count = s.shape_count;
    t.hardness = s.hardness;
    t.angle = s.angle;
    t.follow = s.follow;
    t.follow_tilt = s.follow_tilt;
    t.spacing = s.spacing;
    t.scatter = s.scatter;
    t.size_jitter = s.size_jitter;
    t.turn_jitter = s.turn_jitter;
    t.ribbon = s.ribbon;
    t.keep_colors = s.keep_colors;
    let source = preset_rows_of(from);
    let mut rows = preset_rows_of(to);
    rows.thickness = source.thickness;
    rows.vertical = source.vertical;
    rows.gap = source.gap;
    rows.fixed_gap = source.fixed_gap;
    rows.tips = tips_of(from);
    if rows.tips.len() < 2 {
        rows.tips.clear();
    }
    to.brush_rows.preset = Some(rows);
    let source = crate::canvas::engine::behaviours_of(from);
    let mut b = crate::canvas::engine::behaviours_of(to);
    b.spray = source.spray;
    b.tip_order = source.tip_order;
    b.flip = source.flip;
    b.gap_density = source.gap_density;
    b.tip_blend = source.tip_blend;
    to.behaviours = Some(b);
}

/// The brush shapes to pick from for `tool`: Sue Paint's own (the shape of
/// every built-in sub tool that paints, each once), then the ones a person
/// registered (`kept`). Each is a sub tool to copy the shape from.
pub(crate) fn shape_presets(kept: &Kept) -> Vec<SubTool> {
    let mut out: Vec<SubTool> = Vec::new();
    for tool in [
        Tool::Pen,
        Tool::Pencil,
        Tool::Brush,
        Tool::Airbrush,
        Tool::Decoration,
    ] {
        for sub in tool.sub_tools() {
            if !out.iter().any(|known| known.name == sub.name) {
                out.push(sub);
            }
        }
    }
    out.extend(
        kept.shapes
            .iter()
            .map(|preset| from_preset(Tool::Brush, preset)),
    );
    out
}

/// A brush tip a person can add to a tip: its shape number, name and the
/// tags its material carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TipChoice {
    /// Its shape number (`crate::brush::shape`).
    pub id: u32,
    /// Its name.
    pub name: String,
    /// The words its material is tagged with.
    pub tags: Vec<String>,
}

impl TipChoice {
    /// `true` when `query` (words, any case) finds it: every word is in
    /// its name or one of its tags.
    pub(crate) fn matches(&self, query: &str) -> bool {
        query.split_whitespace().all(|word| {
            let word = word.to_lowercase();
            self.name.to_lowercase().contains(&word)
                || self
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&word))
        })
    }
}

/// What Sue Paint keeps of the brush rows between frames.
#[derive(Debug, Default)]
pub(crate) struct BrushWork {
    /// The sub tools saved as defaults and the brush shapes registered.
    pub kept: Kept,
    /// The brush tip picker, while it is open: what is typed in its
    /// search.
    pub tip_search: Option<String>,
    /// The tips the picker offers, found again while it is open.
    pub tips: Vec<TipChoice>,
    /// The brush shape picked in the Brush shape row.
    pub shape: usize,
}

impl crate::studio::SuePaint {
    /// What the brush rows do every frame: the kept brushes read once
    /// (each sub tool saved as a default starts as it), and the tips the
    /// picker offers while it is open.
    pub(crate) fn brush_rows_frame(&mut self, cx: &mut sue_wm::AppletContext<'_>) {
        if !self.brush_work.kept.loaded {
            // The papers made from materials, registered once.
            if let Some(folder) = crate::materials::location(cx) {
                crate::material_props::load_papers(&folder);
            }
            let kept = crate::colorsets::folder(cx)
                .map(|folder| Kept::load(&folder))
                .unwrap_or_else(|| {
                    Ok(Kept {
                        loaded: true,
                        ..Kept::default()
                    })
                });
            match kept {
                Ok(kept) => {
                    for sub in self.toolbox.every_sub_mut() {
                        if sub.locked {
                            continue;
                        }
                        if let Some(saved) = kept.default_of(sub.tool, &sub.name) {
                            let hidden = std::mem::take(&mut sub.hidden);
                            *sub = SubTool { hidden, ..saved };
                        }
                    }
                    self.brush_work.kept = kept;
                }
                Err(error) => {
                    self.brush_work.kept.loaded = true;
                    cx.fail(tr!("cannot-read-the-kept-brushes", error = error));
                }
            }
        }
        if self.brush_work.tip_search.is_some() {
            let folder = crate::materials::location(cx);
            let kinds = self.material_kinds.clone();
            let entries = self.hub.library(&kinds, folder).entries();
            self.brush_work.tips = crate::brush::shapes()
                .into_iter()
                .map(|(id, name)| TipChoice {
                    tags: entries
                        .iter()
                        .find(|entry| {
                            entry.material.kind == crate::materials::TIP
                                && entry.material.name == name
                        })
                        .map(|entry| entry.tags.clone())
                        .unwrap_or_default(),
                    id,
                    name,
                })
                .collect();
        }
    }

    /// Save all settings as default (full guide FGV-12): the picked sub
    /// tool as it is now becomes what it starts as and what Reset brings
    /// back, kept in the brush file.
    pub(crate) fn save_sub_tool_default(&mut self, cx: &mut sue_wm::AppletContext<'_>) {
        let sub = self.toolbox.current().clone();
        let Some(preset) = to_preset(&sub) else {
            return;
        };
        self.brush_work
            .kept
            .defaults
            .insert(default_name(sub.tool, &sub.name), preset);
        if self.save_kept_brushes(cx) {
            cx.notify(tr!(
                "sub-tool-saved-as-default",
                name = crate::localize(&sub.name)
            ));
        }
    }

    /// Registers the picked sub tool's brush shape (its tip, spraying
    /// effect and gap) under its own name, in the brush file.
    pub(crate) fn register_brush_shape(&mut self, cx: &mut sue_wm::AppletContext<'_>) {
        let sub = self.toolbox.current().clone();
        let Some(mut preset) = to_preset(&sub) else {
            return;
        };
        let name = crate::localize(&sub.name);
        preset.name.clone_from(&name);
        let shapes = &mut self.brush_work.kept.shapes;
        shapes.retain(|known| known.name != name);
        shapes.push(preset);
        let built_in = shape_presets(&Kept::default()).len();
        self.brush_work.shape = built_in + shapes.len() - 1;
        if self.save_kept_brushes(cx) {
            cx.notify(tr!("brush-shape-registered", name = name));
        }
    }

    /// Writes the brush file; `false`, said, when it could not.
    fn save_kept_brushes(&mut self, cx: &mut sue_wm::AppletContext<'_>) -> bool {
        let Some(folder) = crate::colorsets::folder(cx) else {
            return true;
        };
        match self.brush_work.kept.save(&folder) {
            Ok(()) => true,
            Err(error) => {
                cx.fail(tr!("cannot-keep-the-brushes", error = error));
                false
            }
        }
    }

    /// `true` when the picked sub tool has something Reset brings back:
    /// a default a person saved, or Sue Paint's own.
    pub(crate) fn has_saved_default(&self) -> bool {
        let sub = self.toolbox.current();
        self.brush_work
            .kept
            .defaults
            .contains_key(&default_name(sub.tool, &sub.name))
    }

    /// Reset to Initial Settings: the picked sub tool back to the default
    /// a person saved for it, else as Sue Paint brings it. A locked sub
    /// tool stays as it is.
    pub(crate) fn reset_current_sub(&mut self) {
        let current = self.toolbox.current();
        if current.locked {
            return;
        }
        if let Some(saved) = self.brush_work.kept.default_of(current.tool, &current.name) {
            let sub = self.toolbox.current_mut();
            let hidden = std::mem::take(&mut sub.hidden);
            *sub = SubTool { hidden, ..saved };
            return;
        }
        let entry = self.toolbox.current_entry();
        let (kind, index) = (entry.kind, entry.picked());
        self.toolbox.reset_sub(kind, index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_picture::tools::brush_rows::{AntiOverflow, Gap};

    fn changed_brush() -> SubTool {
        let mut sub = Tool::Brush.sub_tools().remove(0);
        sub.tip.size = 33.0;
        sub.tip.angle = 12.0;
        sub.stabilization = 17;
        sub.correction = 4;
        sub.brush_rows = BrushRows {
            preset: Some(PresetRows {
                blend: sue_model::BlendMode::Multiply,
                thickness: 0.4,
                vertical: true,
                density: 0.6,
                gap: Gap::Wide,
                ..preset_rows_of(&sub)
            }),
            snapping: false,
            correction_by_speed: true,
            correction_by_scale: false,
            bezier: true,
            overflow: Some(AntiOverflow::default()),
        };
        let mut b = crate::canvas::engine::behaviours_of(&sub);
        b.sharp_angles = false;
        b.spray = Some(sue_brush::paint::Spray::default());
        sub.behaviours = Some(b);
        sub
    }

    #[test]
    fn a_sub_tool_comes_back_from_its_preset_with_every_setting() {
        let sub = changed_brush();
        let preset = to_preset(&sub).expect("a brush");
        let bytes = preset.to_bytes();
        let back = from_preset(Tool::Brush, &BrushPreset::from_bytes(&bytes).unwrap());
        assert_eq!(back.tip, sub.tip);
        assert_eq!(back.brush_rows, sub.brush_rows);
        assert_eq!(back.behaviours, sub.behaviours);
        assert_eq!(
            (back.stabilization, back.correction, back.mode),
            (sub.stabilization, sub.correction, sub.mode)
        );
        // And it paints with the very same brush.
        let painted = |sub: &SubTool| {
            crate::canvas::engine::brush_of(sub, false, false)
                .unwrap()
                .preset
        };
        assert_eq!(painted(&back).to_bytes(), painted(&sub).to_bytes());
    }

    #[test]
    fn the_kept_file_round_trips_as_a_brush_library() {
        let sub = changed_brush();
        let mut kept = Kept::default();
        kept.defaults.insert(
            default_name(Tool::Brush, &sub.name),
            to_preset(&sub).unwrap(),
        );
        kept.shapes.push(to_preset(&sub).unwrap());
        let bytes = kept.to_file();
        let read = Kept::from_file(&bytes).expect("a brush file");
        assert_eq!(read.defaults, kept.defaults);
        assert_eq!(read.shapes, kept.shapes);
        assert!(read.default_of(Tool::Brush, &sub.name).is_some());
        // Any brush library takes it in.
        let library = sue_io::brushes::library::standard();
        assert!(sue_io::brushes::library::install(&library, &bytes).is_ok());
    }

    #[test]
    fn a_brush_shape_carries_the_tip_and_not_the_ink() {
        let from = changed_brush();
        let mut to = Tool::Pen.sub_tools().remove(0);
        let size = to.tip.size;
        copy_shape(&from, &mut to);
        let rows = to.brush_rows.preset.clone().unwrap();
        assert_eq!(rows.thickness, 0.4);
        assert_eq!(rows.gap, Gap::Wide);
        assert_eq!(
            rows.blend,
            sue_model::BlendMode::Normal,
            "the pen's own ink"
        );
        assert_eq!(to.tip.size, size);
        assert!(crate::canvas::engine::behaviours_of(&to).spray.is_some());
    }
}
