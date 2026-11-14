//! Opening, importing, saving, exporting and printing pictures.

use std::path::{Path, PathBuf};

use sue_core::txn::{Batch, SpawnEntity};
use sue_core::{CHILD, ComponentKind, Entity, EntityId, Properties, RasterTile2D, Relation, Value};
use sue_model::class;
use sue_model::picture;
use sue_platform::write_atomic;
use sue_wm::{AppletContext, AppletId, FilePick, FileType, ShellRequest};

use super::{BRUSHES, ID, OPENS, SOUNDS, SuePaint, has_extension, me_id, opens, to_rgba};
use crate::composite::{self, RenderOptions};
use crate::doc;

impl SuePaint {
    /// Writes the flattened picture as `ext` (png, jpg, jxl, bmp, tif or tga).
    pub(crate) fn export(&mut self, cx: &mut AppletContext<'_>, ext: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(flat) = composite::flatten(arena, picture, options) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        let bytes = sue_io::encode_as(&flat, ext, jpeg_quality(cx), jxl_effort(cx));
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("export-failed", error = error));
                return;
            }
        };
        self.export_bytes(cx, bytes, ext);
    }

    /// Layered Photoshop document export (`.psd`, or `.psb` when `big`).
    pub(crate) fn export_psd(&mut self, cx: &mut AppletContext<'_>, big: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(psd) = crate::psd_io::to_psd(&cx.world.arena, picture) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        match sue_io::psd::encode_psd(&psd, big) {
            Ok(bytes) => self.export_bytes(cx, bytes, if big { "psb" } else { "psd" }),
            Err(error) => cx.fail(tr!("export-failed", error = error)),
        }
    }

    /// Asks where to write the exported `bytes` (`ext` files), or writes
    /// them to the exports folder when nobody is there to ask.
    pub(crate) fn export_bytes(&mut self, cx: &mut AppletContext<'_>, bytes: Vec<u8>, ext: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
        let file = format!("{name}.{ext}");
        if cx.view.interactive {
            // The save dialog asks where; `picked` writes it.
            let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
            self.pending_export = Some(bytes);
            cx.request(ShellRequest::PickFile(
                FilePick::save(
                    Some(AppletId::new(ID).expect("a valid id")),
                    "export",
                    &tr!("export"),
                    &file,
                )
                .extensions(&[ext])
                .starting_in(Some(start)),
            ));
        } else {
            let path = free_path(&export_dir(cx).join(&file));
            self.write_export(cx, &path, &bytes);
        }
    }

    /// Save As: the save dialog asks where and as what, a Sue project or
    /// any picture format (the picture flattened, as Export does).
    pub(crate) fn save_as(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            cx.request(ShellRequest::Command("file.save_as".to_owned()));
            return;
        };
        if !cx.view.interactive {
            cx.request(ShellRequest::Command("file.save_as".to_owned()));
            return;
        }
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
        let clip = FileType::new(&tr!("clip-studio-paint-file"), &["clip"]);
        let sue = FileType::new(&tr!("sue-project"), &["sue"]);
        let saves_clip = crate::clip_io::saves_clip(cx);
        let mut types = if saves_clip {
            vec![clip, sue]
        } else {
            vec![sue, clip]
        };
        types.extend(
            sue_io::PICTURE_SAVE_TYPES
                .iter()
                .map(|(what, extensions)| FileType::new(what, extensions)),
        );
        let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "save-as",
                &tr!("save-as"),
                &format!("{name}.{}", if saves_clip { "clip" } else { "sue" }),
            )
            .types(types)
            .starting_in(Some(start)),
        ));
    }

    /// Writes what Save As picked: the project, or the flattened picture in
    /// the format of `path`'s extension.
    pub(super) fn save_as_to(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if extension == "clip" {
            if let Some(picture) = self.picture() {
                self.write_clip(cx, picture, path);
            }
            return;
        }
        if extension == "sue" || extension.is_empty() {
            let path = if extension.is_empty() {
                path.with_extension("sue")
            } else {
                path.to_path_buf()
            };
            // The project is the shell's to write.
            cx.request(ShellRequest::Picked {
                pick: FilePick::save(None, "save-project", &tr!("save-project"), ""),
                path,
            });
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(flat) = composite::flatten(&cx.world.arena, picture, options) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        match sue_io::encode_as(&flat, &extension, jpeg_quality(cx), jxl_effort(cx)) {
            Ok(bytes) => {
                if self.write_export(cx, path, &bytes) {
                    cx.request(ShellRequest::Remember {
                        path: path.to_path_buf(),
                        saved: true,
                    });
                }
            }
            Err(error) => cx.fail(tr!("cannot-save-as", extension = extension, error = error)),
        }
    }

    /// Save when Sue Paint saves `.clip` files: the picture's own `.clip`
    /// again, or Save As for one never saved.
    pub(crate) fn save_clip(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        match self.clip_paths.get(&picture).cloned() {
            Some(path) => self.write_clip(cx, picture, &path),
            None => self.save_as(cx),
        }
    }

    /// Writes `picture` as a `.clip` at `path`, and remembers it there.
    pub(super) fn write_clip(
        &mut self,
        cx: &mut AppletContext<'_>,
        picture: EntityId,
        path: &Path,
    ) {
        let bytes = match crate::clip_io::encode(&cx.world.arena, picture) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("cannot-save-as", extension = "clip", error = error));
                return;
            }
        };
        if self.write_export(cx, path, &bytes) {
            self.clip_paths.insert(picture, path.to_path_buf());
            self.saved_at.insert(picture, cx.history.cursor());
            if let Some(doc) = self.docs.iter().find(|doc| doc.picture == picture) {
                let (_, _, version) = doc.compositor.display();
                self.clean.insert(picture, version);
            }
            cx.request(ShellRequest::Remember {
                path: path.to_path_buf(),
                saved: true,
            });
        }
    }

    /// Writes an encoded export to `path` and says where it went. Returns
    /// `true` once written.
    pub(super) fn write_export(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        bytes: &[u8],
    ) -> bool {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match write_atomic(path, bytes) {
            Ok(()) => {
                cx.saved(path, tr!("kib", value = bytes.len().div_ceil(1024)));
                true
            }
            Err(error) => {
                cx.fail(tr!("cannot-write", path = path.display(), error = error));
                false
            }
        }
    }

    /// Remembers every open picture as saved, for the tabs' modified dot.
    pub(crate) fn mark_clean(&mut self) {
        for doc in &self.docs {
            let (_, _, version) = doc.compositor.display();
            self.clean.insert(doc.picture, version);
        }
    }

    /// Asks for a picture file and opens it as a new canvas, or as a new
    /// layer of the current one.
    pub(crate) fn pick_image(&mut self, cx: &mut AppletContext<'_>, as_layer: bool) {
        let start = self
            .last_dir
            .clone()
            .or_else(|| cx.catalog.folders.get("home").cloned());
        let (purpose, title) = if as_layer {
            ("layer", tr!("import-picture-as-a-layer"))
        } else {
            ("open", tr!("open-picture"))
        };
        cx.request(ShellRequest::PickFile(
            FilePick::open(
                Some(AppletId::new(ID).expect("a valid id")),
                purpose,
                &title,
            )
            .extensions(OPENS)
            .starting_in(start),
        ));
    }

    /// Opens the picture file at `path` as a new canvas, or as a new layer.
    /// Batch import: each picture in `dir`, by name, as a layer on a page
    /// of its own, the first on the page open now and the rest on new
    /// pages after it.
    pub(super) fn batch_import(&mut self, cx: &mut AppletContext<'_>, dir: &Path) {
        if self.picture().is_none() {
            cx.notify(tr!("open-a-page-first"));
            return;
        }
        let mut files: Vec<PathBuf> = sue_platform::fs::list_dir(dir, false)
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| !entry.is_dir && opens(&entry.path))
                    .map(|entry| entry.path)
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|path| path.file_name().map(|name| name.to_ascii_lowercase()));
        if files.is_empty() {
            cx.notify(tr!("no-pictures-in-folder"));
            return;
        }
        let count = files.len();
        for (index, file) in files.iter().enumerate() {
            if index > 0 {
                self.run("story.add", cx);
            }
            self.open_picked(cx, file, true);
        }
        self.last_dir = Some(dir.to_path_buf());
        cx.notify(tr!("batch-imported", count = count));
    }

    /// A file handed to Sue that Sue Paint takes (see `opens_file`): a
    /// brush file becomes sub tools, a sound file goes on the timeline,
    /// and a picture opens as a new canvas.
    pub(super) fn take_file(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        if has_extension(path, BRUSHES) {
            self.import_brushes(cx, path);
        } else if has_extension(path, SOUNDS) {
            let timeline = self
                .picture()
                .and_then(|picture| crate::anim::timeline(&cx.world.arena, picture))
                .is_some();
            if timeline {
                self.import_audio(cx, path);
            } else {
                cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            }
        } else {
            self.open_picked(cx, path, false);
        }
    }

    pub(crate) fn open_picked(&mut self, cx: &mut AppletContext<'_>, path: &Path, as_layer: bool) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        match sue_platform::fs::read(path) {
            Ok(bytes) => match self.open_bytes(cx, path, &bytes, as_layer) {
                Ok(()) => {
                    if !as_layer {
                        Self::remember_recent(cx, path);
                    }
                }
                Err(error) => cx.fail(error),
            },
            Err(error) => cx.fail(tr!("cannot-read", path = path.display(), error = error)),
        }
    }

    /// Opens picture file `bytes` (read from `path`) as a new canvas, or as
    /// a new layer of the current canvas.
    ///
    /// # Errors
    /// If no format reads the file as pixels.
    pub fn open_bytes(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let name = path.file_stem().map_or_else(
            || tr!("picture"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let file = path
            .file_name()
            .map_or_else(String::new, |file| file.to_string_lossy().into_owned());
        if bytes.starts_with(b"8BPS") {
            return self.open_psd(cx, &name, &file, bytes, as_layer);
        }
        if sue_io::clip::sniff(bytes) {
            return self.open_clip(cx, path, &name, &file, bytes, as_layer);
        }
        let formats = sue_io::formats(sue_model::registry());
        let imported = formats.import(bytes, &file, &Properties::new());
        // Some pictures (WebP, GIF, TIFF, ...) come in kept as their file,
        // and some (TGA, QOI, ...) no format handler reads; Sue Paint
        // decodes those itself.
        let read = imported.as_ref().ok().and_then(|bundle| {
            bundle
                .entities
                .iter()
                .find_map(|entity| entity.component(ComponentKind::RasterTile2D)?.as_raster())
                .map(to_rgba)
        });
        let rgba = match read {
            Some(rgba) => rgba,
            None => sue_io::decode_png(bytes)
                .map(|decoded| decoded.tile)
                .or_else(|_| sue_io::decode_jpeg(bytes).map(|decoded| decoded.tile))
                // The extension names the format where the bytes cannot
                // (TGA has no signature).
                .or_else(|_| {
                    let hint = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .and_then(sue_io::PictureFormat::from_extension);
                    sue_io::decode_picture(bytes, hint)
                })
                .map(|raster| to_rgba(&raster))
                .map_err(|_| match &imported {
                    Ok(_) => tr!("has-no-pixels-sue-paint-can", file = file),
                    Err(error) => tr!("cannot-open", file = file, error = error),
                })?,
        };
        let target = self.picture().filter(|_| as_layer);
        match target {
            Some(picture) => {
                let layer = Entity::new(name, rgba).with_class(class::LAYER);
                let id = layer.id();
                let order = cx.world.arena.relations().next_order(picture, CHILD);
                let spawn = SpawnEntity::new(layer)
                    .with_relation(Relation::new(picture, CHILD, id).ordered(order));
                if Self::apply(cx, spawn)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(id);
                }
            }
            None => {
                let (width, height) = (rgba.width(), rgba.height());
                // The canvas says which file it came from and how large
                // that was, as a picture read by ingest does. Nothing else
                // of the file is kept: the canvas holds its pixels.
                let image = Entity::empty(name.clone(), class::IMAGE)
                    .with_property(picture::WIDTH, i64::from(width))
                    .with_property(picture::HEIGHT, i64::from(height))
                    .with_property(picture::DPI, 72.0)
                    .with_meta(sue_io::meta::SOURCE, path.display().to_string())
                    .with_meta(sue_io::meta::BYTES, bytes.len().to_string());
                let picture_id = image.id();
                let layer = Entity::new(name.clone(), rgba).with_class(class::LAYER);
                let layer_id = layer.id();
                let edit = Batch::new(tr!("open-3", file = file))
                    .then(SpawnEntity::new(image))
                    .then(
                        SpawnEntity::new(layer).with_relation(
                            Relation::new(picture_id, CHILD, layer_id)
                                .ordered(sue_core::OrderKey::default()),
                        ),
                    );
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture_id);
                    self.saved_at.insert(picture_id, cx.history.cursor());
                }
            }
        }
        Ok(())
    }

    /// Opens a Photoshop document with its layers and folders kept: as a
    /// new canvas, or into the current one above the current layer.
    pub(super) fn open_psd(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        file: &str,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let psd = sue_io::psd::decode_psd(bytes)
            .map_err(|error| tr!("cannot-open", file = file, error = error))?;
        self.open_layers(cx, name, file, &psd, 72.0, as_layer)
            .map(|_| ())
    }

    /// Opens a Clip Studio file. One Sue saved comes back whole, from the
    /// Sue project it carries; any other opens with its layers and folders.
    /// Save writes the file again.
    pub(super) fn open_clip(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        name: &str,
        file: &str,
        bytes: &[u8],
        as_layer: bool,
    ) -> Result<(), String> {
        let clip = sue_io::clip::decode_clip(bytes)
            .map_err(|error| tr!("cannot-open", file = file, error = error))?;
        if !as_layer
            && let Some(sue) = &clip.sue
            && let Some((edit, picture)) =
                crate::clip_io::embedded(&cx.world.arena, sue, &tr!("open-3", file = file))
        {
            if edit.is_none_or(|edit| Self::apply(cx, edit)) {
                self.activate(&cx.world.arena, picture);
                self.saved_at.insert(picture, cx.history.cursor());
                self.clip_paths.insert(picture, path.to_path_buf());
            }
            return Ok(());
        }
        if let Some(picture) = self.open_layers(cx, name, file, &clip.psd, clip.dpi, as_layer)? {
            self.clip_paths.insert(picture, path.to_path_buf());
        }
        Ok(())
    }

    /// Opens layered `psd` as a new canvas at `dpi`, or into the current
    /// one above the current layer. Answers the new canvas.
    pub(super) fn open_layers(
        &mut self,
        cx: &mut AppletContext<'_>,
        name: &str,
        file: &str,
        psd: &sue_io::psd::Psd,
        dpi: f64,
        as_layer: bool,
    ) -> Result<Option<EntityId>, String> {
        if let Some(picture) = self.picture().filter(|_| as_layer) {
            let below = self.doc().and_then(|doc| doc.layer);
            let edit = crate::psd_io::import(&cx.world.arena, picture, below, psd)?;
            Self::apply(cx, edit);
            return Ok(None);
        }
        let image = Entity::empty(name.to_owned(), class::IMAGE)
            .with_property(picture::WIDTH, i64::from(psd.width))
            .with_property(picture::HEIGHT, i64::from(psd.height))
            .with_property(picture::DPI, dpi);
        let picture = image.id();
        let layers = crate::psd_io::import(&cx.world.arena, picture, None, psd)?;
        let edit = Batch::new(tr!("open-3", file = file))
            .then(SpawnEntity::new(image))
            .then(layers);
        if Self::apply(cx, edit) {
            self.activate(&cx.world.arena, picture);
            self.saved_at.insert(picture, cx.history.cursor());
            if let Some(top) = doc::layers(&cx.world.arena, picture)
                .iter()
                .rev()
                .find(|info| !doc::is_folder(&cx.world.arena, info.id))
                .map(|info| info.id)
                && let Some(doc) = self.doc_mut()
            {
                doc.layer = Some(top);
            }
            return Ok(Some(picture));
        }
        Ok(None)
    }

    /// File, Import, Vector, Movie or Pattern from Image: the file at
    /// `path` as a vector layer, as cels of a new animation folder, or
    /// tiled over a new layer.
    pub(crate) fn import_file(&mut self, cx: &mut AppletContext<'_>, purpose: &str, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path.file_stem().map_or_else(
            || tr!("imported"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let bytes = match sue_platform::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                cx.fail(tr!("cannot-read", path = path.display(), error = error));
                return;
            }
        };
        let arena = &cx.world.arena;
        let below = self.doc().and_then(|doc| doc.layer);
        let size = arena
            .get_by_id(picture)
            .ok()
            .and_then(picture::size)
            .unwrap_or([1, 1]);
        let made: Result<(doc::Edit, Option<EntityId>), String> = match purpose {
            "import-vector" => String::from_utf8(bytes)
                .map_err(|_| tr!("the-file-is-not-text"))
                .and_then(|text| sue_io::svg::parse_svg(&text))
                .and_then(|drawing| {
                    let lines = crate::vector::from_svg(&drawing, size);
                    if lines.is_empty() {
                        return Err(tr!("the-drawing-has-no-lines"));
                    }
                    let (spawn, id) =
                        doc::add_vector_lines(arena, picture, below, name.clone(), &lines)?;
                    Ok((
                        doc::Edit::new(tr!("import-vector", name = name)).then(spawn),
                        Some(id),
                    ))
                }),
            "import-movie" => sue_io::decode_frames(&bytes).and_then(|(frames, fps)| {
                let (edit, track) =
                    crate::anim::import_frames(arena, picture, below, &name, &frames, fps)?;
                if let Some(doc) = self.doc_mut() {
                    doc.track = track;
                }
                Ok((edit, None))
            }),
            _ => sue_io::decode_picture(&bytes, None)
                .map(|tile| to_rgba(&tile))
                .and_then(|tile| {
                    crate::materials::tiled(&tile, size)
                        .ok_or_else(|| tr!("the-picture-has-no-pixels"))
                })
                .and_then(|raster| {
                    let (spawn, id) =
                        doc::add_layer_pixels(arena, picture, below, name.clone(), raster)?;
                    Ok((
                        doc::Edit::new(tr!("pattern-from", name = name)).then(spawn),
                        Some(id),
                    ))
                }),
        };
        match made {
            Ok((edit, layer)) => {
                if Self::apply(cx, edit) {
                    if purpose == "import-movie" {
                        self.panels.timeline = true;
                    }
                    if let Some(layer) = layer
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.layer = Some(layer);
                    }
                }
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }

    /// File, Print: the pages (every page of a story, or the canvas) laid
    /// on paper as Print Settings says, written as a PDF and opened in the
    /// system's viewer to print.
    pub(crate) fn print_now(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let ids = match self.story_here(arena) {
            Some((_, pages, _)) if self.print.all_pages => pages,
            _ => vec![picture],
        };
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let pages: Vec<RasterTile2D> = ids
            .iter()
            .filter_map(|page| composite::flatten(arena, *page, options))
            .collect();
        let entity = arena.get_by_id(picture).ok();
        let dpi = entity
            .and_then(|entity| entity.property(picture::DPI))
            .and_then(Value::as_f64)
            .unwrap_or(72.0) as f32;
        let name = entity.map_or_else(|| tr!("print"), |entity| entity.name().to_owned());
        let folder = crate::colorsets::folder(cx)
            .map_or_else(std::env::temp_dir, |folder| folder.join("print"));
        let path = folder.join(format!("{}.pdf", name.replace(['/', '\\', ':'], "_")));
        let printed = crate::print::pdf(&self.print, &pages, dpi, &name).and_then(|bytes| {
            sue_platform::fs::create_dir_all(&folder)
                .and_then(|()| sue_platform::fs::write(&path, bytes))
                .map_err(|error| error.to_string())?;
            sue_platform::open_document(&path).map_err(|error| error.to_string())
        });
        match printed {
            Ok(()) => cx.notify(tr!(
                "pages-ready-print-your-pdf-viewer",
                count = pages.len()
            )),
            Err(error) => cx.fail(tr!("cannot-print", error = error)),
        }
    }

    /// Batch Export of canvases: every open canvas flattened to `path`'s
    /// format, each named after `path` and its canvas.
    pub(super) fn export_canvases(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("canvas"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        self.last_dir = Some(dir.clone());
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let pictures: Vec<EntityId> = {
            let mut seen = Vec::new();
            for doc in &self.docs {
                if !seen.contains(&doc.picture) {
                    seen.push(doc.picture);
                }
            }
            seen
        };
        let mut written = 0;
        for picture in pictures {
            let arena = &cx.world.arena;
            let Some(flat) = composite::flatten(arena, picture, options) else {
                continue;
            };
            let name = arena
                .get_by_id(picture)
                .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
            let target = free_path(&dir.join(format!("{stem}-{name}.{extension}")));
            match sue_io::encode_as(&flat, &extension, jpeg_quality(cx), jxl_effort(cx)).and_then(
                |bytes| sue_platform::fs::write(&target, bytes).map_err(|error| error.to_string()),
            ) {
                Ok(()) => written += 1,
                Err(error) => {
                    cx.fail(tr!("cannot-export-2", name = name, error = error));
                    return;
                }
            }
        }
        cx.notify(tr!(
            "exported-canvases-to",
            dir = dir.display(),
            written = written
        ));
    }

    /// Goes back to where the picture was last saved or opened (File,
    /// Revert), as undoing step by step would; Redo brings the changes
    /// back.
    pub(crate) fn revert(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(saved) = self.saved_at.get(&picture).copied() else {
            cx.fail(tr!("save-picture-first-revert-goes-back"));
            return;
        };
        let mut steps = 0;
        let mut at = cx.history.cursor();
        while at != saved {
            let Some(parent) = cx.history.parent(at) else {
                cx.fail(tr!("last-save-another-branch-history-pick"));
                return;
            };
            at = parent;
            steps += 1;
        }
        if steps == 0 {
            cx.notify(tr!("nothing-changed-since-last-save"));
            return;
        }
        for _ in 0..steps {
            if let Err(error) = cx.undo(false) {
                cx.fail(tr!("cannot-revert", error = error));
                return;
            }
        }
        self.sync_docs(&cx.world.arena);
        cx.notify(tr!("reverted-changes-redo-brings-them-back", steps = steps));
    }

    /// Reads the recent pictures from the work Sue remembers (shared with
    /// the Hub and every studio) whenever it changed, into Open Recent.
    pub(super) fn load_recent(&mut self, cx: &AppletContext<'_>) {
        let revision = cx.work.revision();
        if self.recent_seen == Some(revision) {
            return;
        }
        self.recent_seen = Some(revision);
        self.recent_files = cx
            .work
            .recent()
            .filter(|file| opens(&file.path))
            .take(10)
            .map(|file| file.path.clone())
            .collect();
        crate::menus::set_recent(
            &mut self.menus,
            crate::menus::recent_items(&self.recent_files),
        );
    }

    /// Puts `path` first among the files Sue remembers.
    pub(super) fn remember_recent(cx: &mut AppletContext<'_>, path: &Path) {
        cx.request(ShellRequest::Remember {
            path: path.to_path_buf(),
            saved: false,
        });
    }
}

/// Where exports go without a dialog: `io.convert_dir`, or the home folder.
/// The JPEG quality Settings has for conversions.
pub(super) fn jpeg_quality(cx: &AppletContext<'_>) -> u8 {
    cx.rice()
        .get_i64("io.jpeg_quality")
        .and_then(|quality| u8::try_from(quality).ok())
        .unwrap_or(90)
}

/// The JPEG XL effort Settings has for conversions.
pub(super) fn jxl_effort(cx: &AppletContext<'_>) -> u8 {
    cx.rice()
        .get_i64("io.jxl_effort")
        .and_then(|effort| u8::try_from(effort).ok())
        .unwrap_or(7)
}

pub(crate) fn export_dir(cx: &AppletContext<'_>) -> PathBuf {
    let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
    if riced.is_empty() {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    } else {
        PathBuf::from(riced)
    }
}

/// `path`, or `name (2).ext` and so on if it exists.
pub(super) fn free_path(path: &Path) -> PathBuf {
    if !sue_platform::fs::exists(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| !sue_platform::fs::exists(candidate))
        .unwrap_or_else(|| path.to_path_buf())
}
