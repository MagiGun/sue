//! Templates, 3D materials and poses dropped where they belong.
//!
//! A template is a set of layers kept to start pictures with (frame
//! borders, guides, a layered sketch): Sue Paint brings frame templates
//! of its own, and Edit, Register Material, Template keeps the open
//! canvas's layers as a `.clip` file in the materials location
//! (`templates/`), so a template is a file people share. File, New's
//! Template row and the Material palette put a template's layers on a
//! canvas, one undo step.

use std::path::{Path, PathBuf};

use sue_core::{EntityId, Value};
use sue_wm::AppletContext;

use super::SuePaint;
use crate::doc;
use crate::materials::{Material, Source};

/// A template file's name in the materials location for `name`.
fn template_path(folder: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.trim().is_empty() {
        tr!("template")
    } else {
        safe.trim().to_owned()
    };
    folder
        .join("templates")
        .join(format!("{safe}.{}", crate::materials::TEMPLATE_EXTENSION))
}

impl SuePaint {
    /// Edit, Register Material, Template: keeps every layer of the open
    /// canvas (the paper aside) as a template named `name`.
    pub(crate) fn register_template(&mut self, cx: &mut AppletContext<'_>, name: &str) {
        let (Some(folder), Some(picture)) = (crate::materials::location(cx), self.picture()) else {
            cx.fail(tr!("open-make-canvas-first"));
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let path = template_path(&folder, name);
        let written = crate::clip_io::encode(&cx.world.arena, picture).and_then(|bytes| {
            if let Some(dir) = path.parent() {
                sue_platform::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
            }
            sue_platform::write_atomic(&path, &bytes).map_err(|error| error.to_string())
        });
        match written {
            Ok(()) => {
                cx.notify(tr!("is-in-the-material-palette", name = name));
                self.material_kind = self
                    .material_kinds
                    .iter()
                    .position(|kind| kind.id == crate::materials::TEMPLATE)
                    .map_or(0, |index| index as u8 + 1);
                self.hub.materials_changed();
                self.material_pick = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned());
            }
            Err(error) => cx.fail(tr!("cannot-keep-the-template", error = error)),
        }
    }

    /// Every template: Sue Paint's own and the ones registered.
    pub(crate) fn templates(&mut self, cx: &AppletContext<'_>) -> Vec<Material> {
        self.material_entries(cx)
            .into_iter()
            .map(|entry| entry.material)
            .filter(|material| material.kind == crate::materials::TEMPLATE)
            .collect()
    }

    /// Puts the layers of template `material` on the open canvas, above
    /// the current layer, as one undo step: a frame template draws its
    /// frame borders to the page, a registered template brings its
    /// layers (its paper stays out: the canvas has its own).
    pub(crate) fn apply_template(&mut self, cx: &mut AppletContext<'_>, material: &Material) {
        if let Some(layout) = crate::materials::frames_of(material) {
            self.add_frame_layer(
                cx,
                crate::comic::FrameSpec {
                    layout,
                    ..crate::comic::FrameSpec::default()
                },
            );
            return;
        }
        let Some(picture) = self.picture() else {
            cx.fail(tr!("open-make-canvas-first"));
            return;
        };
        let Source::File(path) = &material.source else {
            return;
        };
        let read = sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| sue_io::clip::read(&bytes));
        let mut template = match read {
            Ok(template) => template,
            Err(error) => {
                cx.fail(tr!(
                    "cannot-read-the-template",
                    name = material.name,
                    error = error
                ));
                return;
            }
        };
        template.root.children.retain(|layer| !layer.is_paper());
        let label = tr!("template-name", name = material.name);
        let current = self.layer();
        match crate::clip_io::into_picture(&cx.world.arena, picture, current, &template, &label) {
            Ok((edit, _)) => {
                let before = doc::layers(&cx.world.arena, picture);
                if Self::apply(cx, edit) {
                    // The template's top layer is the one to draw on.
                    let made = doc::layers(&cx.world.arena, picture)
                        .into_iter()
                        .rev()
                        .find(|info| !before.iter().any(|old| old.id == info.id))
                        .map(|info| info.id);
                    if let (Some(made), Some(doc)) = (made, self.doc_mut()) {
                        doc.layer = Some(made);
                        doc.picked.clear();
                    }
                }
            }
            Err(error) => {
                cx.fail(tr!(
                    "cannot-read-the-template",
                    name = material.name,
                    error = error
                ));
            }
        }
    }

    /// Puts a 3D background or object material on the canvas as a 3D
    /// layer of its own: a background seen from inside, where a person
    /// would stand, an object from a corner above.
    pub(crate) fn place_model_material(&mut self, cx: &mut AppletContext<'_>, material: &Material) {
        let Some(model) = crate::materials::model_of(material) else {
            cx.fail(tr!("cannot-read-the-material", name = material.name));
            return;
        };
        let Some(view) = crate::materials::view_of(material) else {
            self.add_3d_layer(cx, &crate::localize(&material.name), model);
            return;
        };
        let Some(picture) = self.picture() else {
            return;
        };
        let name = crate::localize(&material.name);
        match crate::three::new_layer_viewed(
            &cx.world.arena,
            picture,
            self.layer(),
            &name,
            model,
            Some(view),
        ) {
            Ok((edit, id)) => {
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                    doc.picked.clear();
                }
                self.toolbox.pick(crate::tools::Tool::Operation);
                self.toolbox.pick_sub(crate::tools::Tool::Operation, 0);
            }
            Err(error) => cx.fail(error),
        }
    }

    /// The 3D layer whose drawing figure shows under the picture point
    /// `at`, topmost first, of figure kind `kind`.
    pub(crate) fn figure_layer_at(
        &self,
        arena: &sue_core::Arena,
        at: [f32; 2],
        kind: crate::figure::Kind,
    ) -> Option<EntityId> {
        let picture = self.picture()?;
        let p = [at[0].floor() as i64, at[1].floor() as i64];
        doc::layers(arena, picture)
            .into_iter()
            .rev()
            .filter(|info| info.style.visible)
            .filter(|info| {
                crate::figure::of_layer(arena, info.id)
                    .and_then(|model| arena.get_by_id(model).ok())
                    .is_some_and(|model| crate::figure::Body::of(model).kind == kind)
            })
            .find(|info| {
                let Some(raster) = doc::raster(arena, info.id) else {
                    return false;
                };
                let Some(bytes) = raster.bytes() else {
                    return false;
                };
                let [ox, oy] = info.style.offset;
                let (x, y) = (p[0] - ox, p[1] - oy);
                let (w, h) = (i64::from(raster.width()), i64::from(raster.height()));
                (0..w).contains(&x)
                    && (0..h).contains(&y)
                    && bytes
                        .get(((y * w + x) * 4 + 3) as usize)
                        .is_some_and(|alpha| *alpha > 0)
            })
            .map(|info| info.id)
    }

    /// A pose material let go at the picture point `at`: it poses the
    /// drawing figure of its kind shown there, picking that layer; let go
    /// where no such figure shows, it poses the current layer's figure,
    /// or makes a new figure in that pose.
    pub(crate) fn drop_pose(
        &mut self,
        cx: &mut AppletContext<'_>,
        material: &Material,
        at: Option<[f32; 2]>,
    ) {
        if material.kind == crate::materials::POSE
            && let (Some(at), Some((kind, pose))) = (at, crate::materials::pose_of(material))
        {
            // A figure of its kind shown there, or, for a hand pose, a
            // body (whose hands take it).
            let shown = self.figure_layer_at(&cx.world.arena, at, kind).or_else(|| {
                (kind == crate::figure::Kind::Hand)
                    .then(|| self.figure_layer_at(&cx.world.arena, at, crate::figure::Kind::Body))
                    .flatten()
            });
            match shown {
                Some(layer) => {
                    if let Some(doc) = self.doc_mut() {
                        doc.layer = Some(layer);
                        doc.picked.clear();
                    }
                }
                // A hand pose let go beside a body that is the current
                // layer still closes that body's hands.
                None if kind == crate::figure::Kind::Hand
                    && self.layer().is_some_and(|layer| {
                        crate::figure::of_layer(&cx.world.arena, layer)
                            .and_then(|model| cx.world.arena.get_by_id(model).ok())
                            .is_some_and(|model| {
                                crate::figure::Body::of(model).kind == crate::figure::Kind::Body
                            })
                    }) => {}
                None => {
                    // Where no figure of its kind shows: a new one.
                    let name = crate::localize(&material.name);
                    let model = crate::figure::new_of(&name, kind)
                        .with_property(crate::figure::POSE, Value::from(pose.to_text()));
                    self.add_3d_layer(cx, &name, model);
                    return;
                }
            }
        }
        self.paste_material(cx, material);
    }

    /// File, New's Template row: once the new canvas is made, the
    /// template named `name` goes onto it.
    pub(crate) fn new_from_template(&mut self, cx: &mut AppletContext<'_>, key: &str) {
        let Some(template) = self
            .templates(cx)
            .into_iter()
            .find(|material| material.key() == key)
        else {
            cx.fail(tr!("template-is-gone"));
            return;
        };
        self.apply_template(cx, &template);
    }

    /// Whether `layer` shows a file kept outside the picture (a File
    /// Object layer).
    pub(crate) fn is_file_object(arena: &sue_core::Arena, layer: EntityId) -> bool {
        arena.get_by_id(layer).is_ok_and(|entity| {
            entity
                .property(FILE_OBJECT)
                .and_then(Value::as_str)
                .is_some()
        })
    }
}

/// Adjust density of the interface color (`IntRange`, -100 to 100; the
/// shell shades Sue Paint's theme by it).
pub(crate) const THEME_DENSITY: &str = "sue_paint.theme_density";

/// The file a File Object layer shows (`Text`): where it was when the
/// layer was made or last updated. Saved data: never renamed without a
/// migration.
pub(crate) const FILE_OBJECT: &str = "sue_paint.file_object";

/// The picture files a File Object layer can show.
const FILE_OBJECT_TYPES: [&str; 11] = [
    "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "jxl", "qoi", "tga", "gif",
];

/// The pixels of the picture file `bytes` (read from `path`), as RGBA8.
fn decode_file(path: &Path, bytes: &[u8]) -> Result<sue_core::RasterTile2D, String> {
    use sue_core::ComponentKind;
    let file = path
        .file_name()
        .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
    let formats = sue_io::formats(sue_model::registry());
    let imported = formats.import(bytes, &file, &sue_core::Properties::new());
    let read = imported.as_ref().ok().and_then(|bundle| {
        bundle.entities.iter().find_map(|entity| {
            entity
                .component(ComponentKind::RasterTile2D)?
                .as_raster()
                .map(super::to_rgba)
        })
    });
    if let Some(rgba) = read {
        return Ok(rgba);
    }
    sue_io::decode_png(bytes)
        .map(|decoded| decoded.tile)
        .or_else(|_| sue_io::decode_jpeg(bytes).map(|decoded| decoded.tile))
        .or_else(|_| {
            let hint = path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(sue_io::PictureFormat::from_extension);
            sue_io::decode_picture(bytes, hint).map(|raster| super::to_rgba(&raster))
        })
        .map_err(|_| tr!("has-no-pixels-sue-paint-can", file = file))
}

impl SuePaint {
    /// The settings of Layer Settings' palette colors, Webtoon Display
    /// Settings and the interface color's density.
    pub(crate) fn asset_declarations() -> Vec<sue_rice::RiceDeclaration> {
        use sue_rice::{RiceDeclaration, RiceParameter};
        vec![
            RiceDeclaration::new(
                crate::commands::PALETTE_IN_FOLDER,
                RiceParameter::Bool { value: false },
                "A folder's row shows the palette colors of the layers in it (Layer, Layer Settings)",
            )
            .labeled("Palette colors in parent folder"),
            RiceDeclaration::new(
                crate::commands::PALETTE_ON_CANVAS,
                RiceParameter::Bool { value: false },
                "Each layer with a palette color shows its drawing in that color on the canvas; the picture itself is unchanged (Layer, Layer Settings)",
            )
            .labeled("Palette colors on canvas"),
            RiceDeclaration::new(
                crate::dialogs::assets::WEBTOON_GUIDES,
                RiceParameter::Bool { value: false },
                "The canvas shows where each screen of a reader's device ends (View, Webtoon Display Settings)",
            )
            .labeled("Webtoon screen guides"),
            RiceDeclaration::new(
                crate::dialogs::assets::WEBTOON_SCREEN,
                RiceParameter::EnumChoice {
                    value: crate::dialogs::assets::SCREENS[0].0.to_owned(),
                    options: crate::dialogs::assets::SCREENS
                        .iter()
                        .map(|(id, ..)| (*id).to_owned())
                        .collect(),
                },
                "The device a webtoon is read on, which sets how tall one screen is (View, Webtoon Display Settings)",
            )
            .labeled("Webtoon read on"),
            RiceDeclaration::new(
                THEME_DENSITY,
                RiceParameter::IntRange {
                    value: 0,
                    min: Some(-100),
                    max: Some(100),
                },
                "Adjust density of the interface color: above 0 darker, below 0 lighter; text always stays readable (%; Preferences, Interface)",
            )
            .labeled("Adjust density"),
        ]
    }

    /// New Layer, File Object: asks for the picture the layer shows.
    pub(crate) fn pick_file_object(&mut self, cx: &mut AppletContext<'_>) {
        if self.picture().is_none() {
            cx.fail(tr!("open-make-canvas-first"));
            return;
        }
        cx.request(sue_wm::ShellRequest::PickFile(
            sue_wm::FilePick::open(Some(super::me_id()), "file-object", &tr!("file-object"))
                .extensions(&FILE_OBJECT_TYPES)
                .starting_in(self.last_dir.clone()),
        ));
    }

    /// A File Object layer showing the picture at `path`, in the middle of
    /// the canvas above the current layer: one undo step. The layer keeps
    /// the picture's pixels, so the canvas shows it even where the file is
    /// gone; it takes no drawing until it is rasterized.
    pub(crate) fn make_file_object(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let tile = match sue_platform::fs::read(path)
            .map_err(|error| tr!("cannot-read", path = path.display(), error = error))
            .and_then(|bytes| decode_file(path, &bytes))
        {
            Ok(tile) => tile,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let size = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)
            .unwrap_or([tile.width(), tile.height()]);
        let at = [
            (i64::from(size[0]) - i64::from(tile.width())) / 2,
            (i64::from(size[1]) - i64::from(tile.height())) / 2,
        ];
        let name = path.file_stem().map_or_else(
            || tr!("file-object"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let layer = sue_core::Entity::new(name.clone(), tile)
            .with_class(sue_model::class::LAYER)
            .with_property(FILE_OBJECT, path.display().to_string())
            .with_property(sue_model::picture::X, at[0])
            .with_property(sue_model::picture::Y, at[1]);
        let id = layer.id();
        let edit = doc::Edit::new(tr!("new-file-object", name = name)).then(doc::landed(
            &cx.world.arena,
            picture,
            self.layer(),
            layer,
        ));
        if Self::apply(cx, edit)
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(id);
            doc.picked.clear();
        }
    }

    /// File Object, Update: the current File Object layer shows its file
    /// as it is now, one undo step; a notice says when the file is gone.
    pub(crate) fn update_file_object(&mut self, cx: &mut AppletContext<'_>) {
        let Some(layer) = self.layer() else {
            return;
        };
        let Some(path) = cx
            .world
            .arena
            .get_by_id(layer)
            .ok()
            .and_then(|entity| entity.property(FILE_OBJECT))
            .and_then(Value::as_str)
            .map(PathBuf::from)
        else {
            return;
        };
        let tile = match sue_platform::fs::read(&path)
            .map_err(|error| tr!("cannot-read", path = path.display(), error = error))
            .and_then(|bytes| decode_file(&path, &bytes))
        {
            Ok(tile) => tile,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        if doc::raster(&cx.world.arena, layer)
            .is_some_and(|now| now.bytes() == tile.bytes() && now.width() == tile.width())
        {
            cx.notify(tr!("file-object-is-up-to-date"));
            return;
        }
        let edit = doc::Edit::new(tr!("update-file-object")).then(
            sue_core::txn::ReplaceComponent::new(layer, tile, tr!("update-file-object")),
        );
        Self::apply(cx, edit);
    }

    /// Picks tool slot and sub tool `state` again (see
    /// [`Self::tool_state`]).
    pub(crate) fn set_tool_state(&mut self, (slot, sub): (usize, usize)) {
        self.toolbox.pick_slot(slot);
        let tool = self.toolbox.tool();
        self.toolbox.pick_sub(tool, sub);
    }

    /// Window, Material, a folder: the Material palette shows, open at
    /// the folder of kind `kind`.
    pub(crate) fn show_material_folder(&mut self, kind: &str) {
        if self.material_kinds.get(kind).is_none() {
            return;
        }
        self.show_palette(crate::dock::Palette::Material);
        self.extras.material.folder = crate::hub::Node::Kind(kind.to_owned());
        self.extras.material.picked.clear();
        self.extras.material.tags = None;
    }

    /// Moves the current layer up (`by` above 0) or down so many places
    /// in the stack: a layer drag the Auto Action recorded, played.
    pub(crate) fn move_layer_by(&mut self, cx: &mut AppletContext<'_>, by: i64) {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return;
        };
        let stack = doc::layers(&cx.world.arena, picture);
        let Some(at) = stack.iter().position(|info| info.id == layer) else {
            return;
        };
        let last = stack.len().saturating_sub(1) as i64;
        let to = (at as i64 + by).clamp(0, last) as usize;
        if to != at {
            self.move_layer_to(cx, to);
        }
    }

    /// Keeps a drag of `layer` in the Layer palette in the Auto Action
    /// being recorded: how many places it moved, from `from` in the stack.
    pub(crate) fn record_layer_move(
        &mut self,
        arena: &sue_core::Arena,
        layer: EntityId,
        from: usize,
    ) {
        if self.auto.recording.is_none() {
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(to) = doc::layers(arena, picture)
            .iter()
            .position(|info| info.id == layer)
        else {
            return;
        };
        let by = to as i64 - from as i64;
        if by != 0 {
            self.record_auto(&format!("{}{by}", crate::commands::MOVE_BY_PREFIX));
        }
    }

    /// Keeps a stroke just drawn in the Auto Action being recorded: the
    /// sub tool it was drawn with and its points (`x`, `y` in picture
    /// pixels and the pen's pressure), at most a few hundred of them.
    pub(crate) fn record_stroke(&mut self, points: &[[f32; 3]]) {
        if self.auto.recording.is_none() || points.len() < 2 {
            return;
        }
        let sub = self.toolbox.current();
        let tool = crate::commands::subtool::command_name(sub.tool, &sub.name);
        let every = points.len().div_ceil(400).max(1);
        let mut kept: Vec<String> = points
            .iter()
            .step_by(every)
            .map(|[x, y, pressure]| format!("{x:.1},{y:.1},{pressure:.2}"))
            .collect();
        if let Some([x, y, pressure]) = points.last()
            && !(points.len() - 1).is_multiple_of(every)
        {
            kept.push(format!("{x:.1},{y:.1},{pressure:.2}"));
        }
        self.record_auto(&format!(
            "{}{tool} {}",
            crate::commands::STROKE_PREFIX,
            kept.join(" ")
        ));
    }

    /// A recorded stroke (`arg`: the sub tool command, then the points)
    /// drawn again with that sub tool on the current layer, one undo
    /// step. The sub tool picked before stays picked afterwards.
    pub(crate) fn play_stroke(&mut self, arg: &str, cx: &mut AppletContext<'_>) {
        let Some((tool, rest)) = arg.split_once(' ') else {
            return;
        };
        let points: Vec<[f32; 3]> = rest
            .split_whitespace()
            .filter_map(|point| {
                let mut parts = point.split(',').map(str::parse::<f32>);
                Some([
                    parts.next()?.ok()?,
                    parts.next()?.ok()?,
                    parts.next()?.ok()?,
                ])
            })
            .collect();
        if points.len() < 2 {
            return;
        }
        let before = self.tool_state();
        self.run(tool, cx);
        self.engine_replay(&points, cx);
        self.set_tool_state(before);
    }
}
