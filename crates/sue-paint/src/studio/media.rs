//! Timelapse recording, the camera, audio, and the exports of cels,
//! webtoons, layer comps and vectors.

use std::path::{Path, PathBuf};

use sue_core::{Arena, EntityId, Value};
use sue_model::picture;
use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, ShellRequest};

use super::exports::{Done, Staged, Steps, What, write_whole};
use super::{SuePaint, me_id};
use crate::composite::{self, RenderOptions};
use crate::doc;

impl SuePaint {
    /// `true` if `picture` records its timelapse.
    pub(crate) fn recording(&self, arena: &Arena, picture: EntityId) -> bool {
        arena
            .get_by_id(picture)
            .ok()
            .and_then(|entity| entity.property(crate::timelapse::RECORD))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Keeps a timelapse frame of the current canvas when it records and has
    /// changed since its last one, between strokes.
    pub(super) fn record_timelapse(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        if self.gesture.is_some() {
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        if !self.recording(&cx.world.arena, picture) {
            return;
        }
        let Some(base) = crate::colorsets::folder(cx) else {
            return;
        };
        let dir = crate::timelapse::folder(&base, picture);
        let (seen, waiting, next) = match self.timelapse.get(&picture) {
            Some(known) => *known,
            None => (u64::MAX, u64::MAX, crate::timelapse::frames(&dir).len()),
        };
        let Some(doc) = self.doc() else {
            return;
        };
        if doc.playing.is_some() {
            return;
        }
        let (pixels, size, version) = doc.compositor.display();
        if size[0] == 0 || pixels.is_empty() || seen == version {
            return;
        }
        if waiting != version {
            // A change often paints twice; keep it once it settles.
            self.timelapse.insert(picture, (seen, version, next));
            ctx.request_repaint();
            return;
        }
        let kept = crate::timelapse::capture(&dir, next, pixels, size);
        match kept {
            Ok(()) => {
                self.timelapse.insert(picture, (version, version, next + 1));
            }
            Err(error) => {
                // Stop trying this picture until it changes again.
                self.timelapse.insert(picture, (version, version, next));
                cx.fail(tr!("cannot-record-the-timelapse", error = error));
            }
        }
    }

    /// Record Timelapse: starts recording at once; stopping asks first
    /// whether the frames kept so far stay, when there are any.
    pub(crate) fn toggle_timelapse(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        if !self.recording(&cx.world.arena, picture) {
            self.set_recording(cx, picture, true);
            return;
        }
        let kept = crate::colorsets::folder(cx).map_or(0, |base| {
            crate::timelapse::frames(&crate::timelapse::folder(&base, picture)).len()
        });
        if kept == 0 {
            self.set_recording(cx, picture, false);
        } else {
            self.dialog = Some(crate::dialogs::Dialog::Anim(Box::new(
                crate::dialogs::AnimDialog::StopTimelapse(false, kept),
            )));
        }
    }

    /// Stops recording the timelapse, and with `delete` deletes the frames
    /// kept (the recording's own files, outside the document: what undoes
    /// is the switch, not the frames).
    pub(crate) fn stop_timelapse(&mut self, cx: &mut AppletContext<'_>, delete: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        if !self.set_recording(cx, picture, false) || !delete {
            return;
        }
        if let Some(base) = crate::colorsets::folder(cx) {
            let dir = crate::timelapse::folder(&base, picture);
            match sue_platform::fs::remove_dir_all(&dir) {
                Ok(()) => {
                    self.timelapse.remove(&picture);
                    cx.notify(tr!("the-timelapse-was-deleted"));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => cx.fail(tr!(
                    "cannot-delete-the-timelapse",
                    error = error.to_string()
                )),
            }
        }
    }

    /// Turns `picture`'s recording on or off, a step that undoes.
    fn set_recording(&mut self, cx: &mut AppletContext<'_>, picture: EntityId, on: bool) -> bool {
        let edit = doc::Edit::new(if on {
            tr!("record-timelapse-2")
        } else {
            tr!("stop-recording-timelapse")
        })
        .then(sue_core::txn::SetProperty::new(
            picture,
            crate::timelapse::RECORD,
            on,
        ));
        Self::apply(cx, edit)
    }

    /// Export Timelapse: its dialog, with a preview of the frames kept so
    /// far.
    pub(crate) fn open_timelapse_export(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let paths = crate::colorsets::folder(cx)
            .map(|base| crate::timelapse::frames(&crate::timelapse::folder(&base, picture)))
            .unwrap_or_default();
        self.dialog = Some(crate::dialogs::Dialog::Timelapse(Box::new(
            crate::dialogs::TimelapseDialog::open(paths, self.timelapse_export),
        )));
    }

    /// Asks where to export the timelapse laid out as `export`: an MP4
    /// first where the system has an encoder, then the AVI and the GIF.
    pub(crate) fn ask_export_timelapse(
        &mut self,
        cx: &mut AppletContext<'_>,
        export: crate::timelapse::Export,
    ) {
        self.timelapse_export = export;
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("timelapse"), |entity| entity.name().to_owned());
        let mp4 = sue_platform::movie::mp4_available();
        let mut types = Vec::new();
        if mp4 {
            types.push(sue_wm::FileType::new(&tr!("mp4-video"), &["mp4"]));
        }
        types.push(sue_wm::FileType::new(
            &tr!("motion-jpeg-avi-video"),
            &["avi"],
        ));
        types.push(sue_wm::FileType::new(&tr!("animated-gif"), &["gif"]));
        let file = if mp4 {
            tr!("timelapse-mp4", name = name)
        } else {
            tr!("timelapse-avi", name = name)
        };
        cx.request(ShellRequest::PickFile(
            FilePick::save(Some(me_id()), "timelapse", &tr!("export-timelapse"), &file)
                .types(types)
                .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the current canvas's timelapse to `path`, as an MP4 or AVI
    /// video or a GIF by its extension, in the background: from the
    /// frames kept when it begins (recording goes on meanwhile).
    pub(super) fn export_timelapse(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let (Some(picture), Some(base)) = (self.picture(), crate::colorsets::folder(cx)) else {
            return;
        };
        let paths = crate::timelapse::frames(&crate::timelapse::folder(&base, picture));
        if paths.is_empty() {
            cx.fail(tr!(
                "cannot-export-the-timelapse",
                error = tr!("canvas-has-no-timelapse-yet-record")
            ));
            return;
        }
        let export = self.timelapse_export;
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let target = path.to_path_buf();
        self.run_export(cx, What::Timelapse, path, move |steps| {
            let (bytes, frames) = timelapse_bytes(&paths, export, &extension, steps)?;
            write_whole(&target, &bytes, steps)?;
            Ok(Done {
                frames,
                bytes: bytes.len(),
                files: 1,
            })
        });
    }

    /// The current layer when it is a camera folder.
    pub(crate) fn camera_folder(&self, arena: &Arena) -> Option<EntityId> {
        self.layer()
            .filter(|layer| crate::camera::is_camera(arena, *layer))
    }

    /// The frame shown and the time it is at, by the timeline's rate.
    pub(crate) fn frame_time(&self, arena: &Arena) -> (u32, sue_core::Time) {
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let rate = self
            .picture()
            .and_then(|picture| crate::anim::timeline(arena, picture))
            .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
        (frame, crate::camera::time_of(frame, rate))
    }

    /// Where `folder`'s camera stands at the frame shown, set up or keyed.
    pub(crate) fn camera_now(&self, arena: &Arena, folder: EntityId) -> crate::camera::Camera {
        let (frame, time) = self.frame_time(arena);
        if let Some((pending, at, camera)) = self.camera_pending
            && pending == folder
            && at == frame
        {
            return camera;
        }
        let size = self
            .picture()
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(picture::size)
            .unwrap_or([0, 0]);
        crate::camera::Camera::of(arena, folder, Some(time), size)
    }

    /// Asks where to write the webtoon, cut every `height` pixels and
    /// scaled to `width`.
    pub(crate) fn ask_export_webtoon(
        &mut self,
        cx: &mut AppletContext<'_>,
        height: u32,
        width: u32,
    ) {
        self.webtoon = (height.max(1), width.max(1));
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("webtoon"), |entity| entity.name().to_owned());
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "webtoon",
                &tr!("export-webtoon"),
                &format!("{name}.png"),
            )
            .types(
                sue_io::PICTURE_SAVE_TYPES
                    .iter()
                    .map(|(what, extensions)| sue_wm::FileType::new(what, extensions))
                    .collect(),
            )
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the picture to numbered files next to `path`, scaled to the
    /// webtoon width and cut every webtoon height, top to bottom.
    /// Writes each layer comp of the picture as its own picture beside
    /// `path`, named `Name-Comp.png` (by `path`'s extension): the comp is
    /// shown, flattened and put back, so nothing changes.
    pub(super) fn export_comps(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        use sue_core::ReversibleTransaction;
        let Some(picture) = self.picture() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path.file_stem().map_or_else(
            || tr!("picture"),
            |stem| stem.to_string_lossy().into_owned(),
        );
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
        for comp in crate::comps::read(&cx.world.arena, picture) {
            let mut shown = crate::comps::apply(&cx.world.arena, picture, &comp, &comp.name);
            if shown.apply(&mut cx.world.arena).is_err() {
                continue;
            }
            let flat = composite::flatten(&cx.world.arena, picture, options);
            let _ = shown.revert(&mut cx.world.arena);
            let Some(flat) = flat else {
                continue;
            };
            let safe: String = comp
                .name
                .chars()
                .map(|ch| if r#"\/:*?"<>|"#.contains(ch) { '_' } else { ch })
                .collect();
            let target = dir.join(format!("{stem}-{safe}.{extension}"));
            match sue_io::encode_as(&flat, &extension, quality, effort)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                Ok(()) => written += 1,
                Err(error) => {
                    cx.fail(tr!("cannot-export", error = error));
                    return;
                }
            }
        }
        cx.notify(tr!(
            "exported-pages-to",
            dir = dir.display(),
            written = written
        ));
    }

    /// Writes the lines of the picture's visible vector layers to `path`
    /// as an SVG drawing, bottom layer first.
    pub(super) fn export_vectors(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let Some([width, height]) = arena.get_by_id(picture).ok().and_then(picture::size) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let mut shapes = Vec::new();
        for info in doc::layers(arena, picture) {
            if !info.style.visible || !crate::vector::is_vector(arena, info.id) {
                continue;
            }
            let lines = crate::vector::lines(&crate::vector::geometry(arena, info.id));
            let offset = info.style.offset.map(|value| value as f32);
            shapes.extend(crate::vector::svg_shapes(&lines, offset));
        }
        if shapes.is_empty() {
            cx.notify(tr!("no-vector-lines-to-export"));
            return;
        }
        let count = shapes.len();
        let text = sue_io::svg::write_svg(&sue_io::svg::SvgDrawing {
            width: width as f32,
            height: height as f32,
            shapes,
        });
        match write_atomic(path, text.as_bytes()) {
            Ok(()) => cx.saved(path, tr!("vector-lines-count", count = count)),
            Err(error) => cx.fail(tr!("cannot-export", error = error.to_string())),
        }
    }

    pub(super) fn export_webtoon(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(flat) = composite::flatten(
            &cx.world.arena,
            picture,
            RenderOptions {
                paper: true,
                drafts: false,
            },
        ) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let (cut, width) = self.webtoon;
        let from = [flat.width(), flat.height()];
        let to = [
            width,
            ((f64::from(from[1]) * f64::from(width) / f64::from(from[0].max(1))).round() as u32)
                .max(1),
        ];
        let Some(bytes) = flat.bytes() else {
            return;
        };
        let scaled = if to == from {
            bytes.to_vec()
        } else {
            doc::resize_rgba(bytes, from, to)
        };
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path.file_stem().map_or_else(
            || tr!("webtoon"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut written = 0;
        let mut top = 0;
        while top < to[1] {
            let rows = cut.min(to[1] - top);
            let start = (top * to[0] * 4) as usize;
            let piece = scaled[start..start + (rows * to[0] * 4) as usize].to_vec();
            let Ok(tile) = sue_core::RasterTile2D::from_bytes(
                to[0],
                rows,
                sue_core::PixelFormat::Rgba8,
                piece,
            ) else {
                break;
            };
            written += 1;
            let target = dir.join(format!("{stem}-{written:03}.{extension}"));
            if let Err(error) = sue_io::encode_as(&tile, &extension, 92, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                cx.fail(tr!("cannot-export-piece", written = written, error = error));
                return;
            }
            top += rows;
        }
        cx.notify(tr!(
            "exported-webtoon-as-pictures-px-wide",
            value = to[0],
            written = written
        ));
    }

    /// Writes each animation cel as a picture of its own, flattened (a
    /// folder cel with everything in it), named for it, in the
    /// background. With the settings' "a folder for each animation
    /// folder" (the default, as Clip Studio Paint does it), each animation
    /// folder's cels go in a folder of that name beside `path`, so cel "1"
    /// of two folders never overwrites the other; without it, the files
    /// are `<name>-<folder>-<cel>` beside `path`. The files appear
    /// together when the last is made, and none does when it stops early.
    pub(super) fn export_cels(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let form = self.anim_ui.pending.cels.take().unwrap_or_default();
        let Some(picture) = self.picture() else {
            return;
        };
        if crate::anim::timeline(&cx.world.arena, picture).is_none() {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        }
        let extension = path.extension().map_or_else(
            || {
                sue_io::PICTURE_SAVE_TYPES
                    .get(form.format)
                    .and_then(|(_, extensions)| extensions.first())
                    .map_or_else(|| "png".to_owned(), |ext| (*ext).to_owned())
            },
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("cel"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let size = cx
            .world
            .arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)
            .unwrap_or([1, 1]);
        // A file name of `text`, its odd characters made plain.
        let plain = |text: &str| -> String {
            text.chars()
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        };
        // Which cel goes to which file, read now.
        let mut cels: Vec<(EntityId, PathBuf)> = Vec::new();
        for (folder, ids) in self.cels_by_folder(&cx.world.arena) {
            let folder = plain(&folder);
            let into = if form.per_folder {
                dir.join(&folder)
            } else {
                dir.clone()
            };
            for cel in ids {
                let Ok(entity) = cx.world.arena.get_by_id(cel) else {
                    continue;
                };
                let name = plain(entity.name());
                let file = if form.per_folder {
                    format!("{name}.{extension}")
                } else {
                    format!("{stem}-{folder}-{name}.{extension}")
                };
                cels.push((cel, into.join(file)));
            }
        }
        let Some((_, first)) = cels.first().cloned() else {
            cx.notify(tr!("exported-cels", written = 0));
            return;
        };
        // Shares every raster (D21): a reference count, not a copy.
        let arena = cx.world.arena.clone();
        let held = first.clone();
        self.run_export(cx, What::Cels, &first, move |steps| {
            let mut staged = Staged::default();
            for (index, (cel, target)) in cels.iter().enumerate() {
                steps.go_on()?;
                steps.part(index, cels.len(), 0.95);
                let tile = crate::composite::thumbnail(&arena, *cel, size)
                    .and_then(|(pixels, [width, height])| {
                        sue_core::RasterTile2D::from_bytes(
                            width,
                            height,
                            sue_core::PixelFormat::Rgba8,
                            pixels,
                        )
                        .ok()
                    })
                    .or_else(|| doc::raster(&arena, *cel).cloned());
                let Some(tile) = tile else {
                    continue;
                };
                if let Some(into) = target.parent() {
                    staged.folder(into)?;
                }
                let bytes = sue_io::encode_as(&tile, &extension, 92, 7)?;
                staged.write(target, &bytes)?;
            }
            let files = staged.finish(&held, steps)?;
            Ok(Done {
                frames: 0,
                bytes: 0,
                files,
            })
        });
    }

    /// Puts the sound file at `path` on a new audio track of the timeline,
    /// from the frame shown.
    pub(super) fn import_audio(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some(sequence) = crate::anim::timeline(&cx.world.arena, picture).cloned() else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path
            .file_stem()
            .map_or_else(|| tr!("audio"), |stem| stem.to_string_lossy().into_owned());
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
        let decoded = sue_platform::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                sue_io::decode_audio(bytes, extension.as_deref()).map_err(|(error, _)| error)
            });
        match decoded {
            Ok((audio, _)) => {
                let frame = self.doc().map_or(0, |doc| doc.frame);
                let (edit, _) = crate::anim::add_audio(picture, &sequence, &name, audio.pcm, frame);
                Self::apply(cx, edit);
            }
            Err(error) => cx.fail(tr!("cannot-import", name = name, error = error)),
        }
    }
}

/// The timelapse of the frame files `paths` laid out as `export`: a GIF
/// when `extension` is `gif`, an MP4 when it is `mp4`, else the Motion
/// JPEG AVI; and how many frames it holds. The same in the background as
/// at once ([`Steps::NOW`]).
///
/// # Errors
/// When there are no frames, one will not read, the encoder refuses (its
/// English text, `io.ftl`), or it was cancelled.
pub(crate) fn timelapse_bytes(
    paths: &[PathBuf],
    export: crate::timelapse::Export,
    extension: &str,
    steps: Steps<'_>,
) -> Result<(Vec<u8>, usize), String> {
    let plan = crate::timelapse::Plan::new(paths, export)?;
    // A frame held (the last, for a second) is read once.
    let mut last: Option<(usize, sue_core::RasterTile2D)> = None;
    let mut frame = |index: usize| -> Result<sue_core::RasterTile2D, String> {
        let source = plan.source(index);
        if let Some((was, tile)) = &last
            && *was == source
        {
            return Ok(tile.clone());
        }
        let tile = plan.frame(index)?;
        last = Some((source, tile.clone()));
        Ok(tile)
    };
    if extension == "gif" {
        let mut frames = Vec::with_capacity(plan.len());
        for index in 0..plan.len() {
            steps.go_on()?;
            steps.part(index, plan.len(), 0.8);
            frames.push(frame(index)?);
        }
        steps.go_on()?;
        let count = frames.len();
        let bytes = sue_io::encode_gif_animation(&frames, 1000 / plan.fps.max(1))?;
        return Ok((bytes, count));
    }
    let format = if extension == "mp4" {
        sue_io::MovieFormat::Mp4H264
    } else {
        sue_io::MovieFormat::MjpegAvi
    };
    let mut settings = sue_io::MovieSettings::new(plan.size[0], plan.size[1], plan.fps.max(1));
    settings.quality = sue_io::MovieQuality::High;
    let text = |error: sue_io::MovieError| error.text().to_owned();
    let mut sink = sue_platform::movie::open(format, settings).map_err(text)?;
    for index in 0..plan.len() {
        steps.go_on()?;
        steps.part(index, plan.len(), 0.95);
        sink.push_frame(&frame(index)?).map_err(text)?;
    }
    steps.go_on()?;
    let count = sink.frames();
    Ok((sink.finish().map_err(text)?, count))
}
