//! Timelapse recording, the camera, audio, and the exports of cels,
//! webtoons, layer comps and vectors.

use std::path::Path;

use sue_core::{Arena, EntityId, Value};
use sue_model::picture;
use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, ShellRequest};

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

    /// Asks where to export the timelapse laid out as `export`.
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
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(me_id()),
                "timelapse",
                &tr!("export-timelapse"),
                &tr!("timelapse-avi", name = name),
            )
            .types(vec![
                sue_wm::FileType::new(&tr!("motion-jpeg-avi-video"), &["avi"]),
                sue_wm::FileType::new(&tr!("animated-gif"), &["gif"]),
            ])
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the current canvas's timelapse to `path`, as a video or a
    /// GIF by its extension.
    pub(super) fn export_timelapse(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let (Some(picture), Some(base)) = (self.picture(), crate::colorsets::folder(cx)) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let dir = crate::timelapse::folder(&base, picture);
        let gif = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
        let result = if gif {
            crate::timelapse::video_frames(&dir, self.timelapse_export).and_then(|(frames, fps)| {
                let count = frames.len();
                sue_io::encode_gif_animation(&frames, 1000 / fps.max(1)).map(|bytes| (bytes, count))
            })
        } else {
            crate::timelapse::export_avi(&dir, self.timelapse_export)
        };
        match result.and_then(|(bytes, count)| {
            write_atomic(path, &bytes)
                .map_err(|error| error.to_string())
                .map(|()| (bytes.len(), count))
        }) {
            Ok((size, count)) => cx.saved(
                path,
                tr!("frames-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!("cannot-export-the-timelapse", error = error)),
        }
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
    /// folder cel with everything in it), named for it. With the
    /// settings' "a folder for each animation folder" (the default, as
    /// Clip Studio Paint does it), each animation folder's cels go in a
    /// folder of that name beside `path`, so cel "1" of two folders never
    /// overwrites the other; without it, the files are `<name>-<folder>-
    /// <cel>` beside `path`.
    pub(super) fn export_cels(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let form = self.anim_ui.pending.cels.take().unwrap_or_default();
        let Some(picture) = self.picture() else {
            return;
        };
        if crate::anim::timeline(&cx.world.arena, picture).is_none() {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        }
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
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
        let mut written = 0;
        for (folder, cels) in self.cels_by_folder(&cx.world.arena) {
            let folder = plain(&folder);
            let into = if form.per_folder {
                dir.join(&folder)
            } else {
                dir.clone()
            };
            if let Err(error) = sue_platform::fs::create_dir_all(&into) {
                cx.fail(tr!(
                    "cannot-export-2",
                    name = folder,
                    error = error.to_string()
                ));
                return;
            }
            for cel in cels {
                let Ok(entity) = cx.world.arena.get_by_id(cel) else {
                    continue;
                };
                let name = plain(entity.name());
                let tile = crate::composite::thumbnail(&cx.world.arena, cel, size)
                    .and_then(|(pixels, [width, height])| {
                        sue_core::RasterTile2D::from_bytes(
                            width,
                            height,
                            sue_core::PixelFormat::Rgba8,
                            pixels,
                        )
                        .ok()
                    })
                    .or_else(|| doc::raster(&cx.world.arena, cel).cloned());
                let Some(tile) = tile else {
                    continue;
                };
                let file = if form.per_folder {
                    format!("{name}.{extension}")
                } else {
                    format!("{stem}-{folder}-{name}.{extension}")
                };
                match sue_io::encode_as(&tile, &extension, 92, 7).and_then(|bytes| {
                    write_atomic(&into.join(file), &bytes).map_err(|error| error.to_string())
                }) {
                    Ok(()) => written += 1,
                    Err(error) => {
                        cx.fail(tr!("cannot-export-2", name = name, error = error));
                        return;
                    }
                }
            }
        }
        cx.notify(tr!("exported-cels", written = written));
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
