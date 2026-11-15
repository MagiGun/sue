//! Exporting the animation: a movie, an animated picture or a sequence
//! of pictures.

use std::path::Path;

use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest};

use crate::anim;
use crate::composite::{self, RenderOptions};
use crate::studio::SuePaint;

impl SuePaint {
    /// Asks where to export the animation, as a GIF or as numbered pictures.
    pub(crate) fn ask_export_animation(&mut self, cx: &mut AppletContext<'_>, command: &str) {
        let Some((picture, _, _)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("animation"), |entity| entity.name().to_owned());
        let (purpose, types, file) = if command == "file.export.gif" {
            (
                "anim-gif",
                vec![FileType::new(&tr!("animated-gif"), &["gif"])],
                format!("{name}.gif"),
            )
        } else if command == "file.export.webp_animation" {
            (
                "anim-gif",
                vec![FileType::new(&tr!("animated-webp"), &["webp"])],
                format!("{name}.webp"),
            )
        } else if command == "file.export.apng" {
            // Clip Studio Paint's animated sticker.
            (
                "anim-gif",
                vec![FileType::new(&tr!("animated-png"), &["png", "apng"])],
                format!("{name}.png"),
            )
        } else if command == "file.export.movie" {
            (
                "anim-movie",
                vec![FileType::new(&tr!("motion-jpeg-avi-video"), &["avi"])],
                format!("{name}.avi"),
            )
        } else {
            (
                "anim-sequence",
                sue_io::PICTURE_SAVE_TYPES
                    .iter()
                    .map(|(what, extensions)| FileType::new(what, extensions))
                    .collect(),
                format!("{name}.png"),
            )
        };
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                purpose,
                &tr!("export-animation"),
                &file,
            )
            .types(types)
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the animation to `path` as a Motion JPEG AVI video.
    pub(crate) fn export_movie(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((frames, fps)) = self.render_frames(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let Some(first) = frames.first() else {
            return;
        };
        let mut video =
            sue_io::MjpegAvi::new(first.width(), first.height(), fps.round().max(1.0) as u32);
        let result = frames
            .iter()
            .try_for_each(|frame| video.push_frame(frame, 90))
            .and_then(|()| {
                let count = video.frames();
                let bytes = video.finish();
                write_atomic(path, &bytes)
                    .map_err(|error| error.to_string())
                    .map(|()| (bytes.len(), count))
            });
        match result {
            Ok((size, count)) => cx.saved(
                path,
                tr!("frames-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!("cannot-export-the-movie", error = error)),
        }
    }

    /// Every frame of the timeline, flattened.
    fn render_frames(&self, arena: &sue_core::Arena) -> Option<(Vec<sue_core::RasterTile2D>, f64)> {
        let (picture, sequence, length) = self.timeline_here(arena)?;
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let frames = (0..length)
            .filter_map(|frame| {
                let mut view = anim::view_in(arena, picture, &sequence, frame, None);
                // Seen through the camera folders, as the animation plays.
                view.cameras = crate::camera::views(
                    arena,
                    picture,
                    Some(crate::camera::time_of(frame, sequence.rate)),
                    arena
                        .get_by_id(picture)
                        .ok()
                        .and_then(sue_model::picture::size)
                        .unwrap_or([0, 0]),
                );
                composite::flatten_view(arena, picture, options, Some(&view))
            })
            .collect();
        Some((frames, anim::fps(&sequence)))
    }

    /// Writes the animation to `path`: one GIF or animated PNG (by its
    /// extension), or `Name-0001.png` and on.
    pub(crate) fn export_animation(&mut self, cx: &mut AppletContext<'_>, path: &Path, gif: bool) {
        let Some((frames, fps)) = self.render_frames(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if gif {
            let delay = (1000.0 / fps.max(1.0)).round() as u32;
            let apng = path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("apng")
            });
            let webp = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("webp"));
            let encoded = if webp {
                sue_io::encode_webp_animation(&frames, delay)
            } else if apng {
                sue_io::encode_apng(&frames, delay)
            } else {
                sue_io::encode_gif_animation(&frames, delay)
            };
            match encoded.and_then(|bytes| {
                write_atomic(path, &bytes)
                    .map_err(|error| error.to_string())
                    .map(|()| bytes.len())
            }) {
                Ok(size) => cx.saved(
                    path,
                    tr!(
                        "frames-kib",
                        count = frames.len(),
                        value = size.div_ceil(1024)
                    ),
                ),
                Err(error) => cx.fail(tr!("cannot-export-the-animation", error = error)),
            }
            return;
        }
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("frame"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (index, frame) in frames.iter().enumerate() {
            let target = dir.join(format!("{stem}-{:04}.{extension}", index + 1));
            if let Err(error) = sue_io::encode_as(frame, &extension, 90, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                cx.fail(tr!("cannot-export-frame", value = index + 1, error = error));
                return;
            }
        }
        cx.notify(tr!(
            "exported-frames-to",
            count = frames.len(),
            dir = dir.display()
        ));
    }
}
