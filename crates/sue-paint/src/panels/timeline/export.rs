//! Exporting the animation: a movie, an animated picture, a sequence of
//! pictures or the cels. Each export asks for its settings first (the
//! dialogs of `dialogs/anim.rs`), then where the file goes; what the
//! settings dialog kept waits on the studio until the file is picked.

use std::path::Path;

use sue_core::{EntityId, PixelFormat, RasterTile2D};
use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest};

use super::forms::{Animated, AnimatedForm, CelsForm, MovieForm, Range, SequenceForm};
use crate::anim;
use crate::composite::{self, RenderOptions};
use crate::dialogs::{AnimDialog, Dialog};
use crate::studio::SuePaint;

/// What a picked file is exported with: the settings kept in the dialog
/// before it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pending {
    pub movie: Option<MovieForm>,
    pub sequence: Option<SequenceForm>,
    pub animated: Option<AnimatedForm>,
    pub cels: Option<CelsForm>,
}

/// The frames of an export, rendered: the pictures and frames a second.
pub(crate) struct Rendered {
    pub frames: Vec<RasterTile2D>,
    pub fps: f64,
    /// The first frame rendered (from 0).
    pub first: u32,
}

/// `tile` scaled to `size` (straight RGBA8), as it is when it is that size
/// already.
fn scaled(tile: RasterTile2D, size: [u32; 2]) -> RasterTile2D {
    let [width, height] = size.map(|side| side.max(1));
    if (tile.width(), tile.height()) == (width, height) {
        return tile;
    }
    let Some(pixels) = tile.bytes() else {
        return tile;
    };
    let resized = sue_io::picture_ops::resize(
        pixels,
        (tile.width(), tile.height()),
        (width, height),
        sue_io::picture_ops::Filter::Bicubic,
    );
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, resized).unwrap_or(tile)
}

/// Which of `count` frames played at `from` frames a second each frame of
/// a movie at `to` frames a second shows, so the movie lasts as long as
/// the timeline and stays with its sound: at a lower rate frames are left
/// out, at a higher one they show for longer.
fn retimed(count: usize, from: f64, to: u32) -> Vec<usize> {
    let (from, to) = (from.max(1.0), f64::from(to.max(1)));
    if count == 0 || (from - to).abs() < 1e-9 {
        return (0..count).collect();
    }
    let total = (count as f64 / from * to).round().max(1.0) as usize;
    (0..total)
        .map(|index| ((index as f64 / to * from + 1e-9).floor() as usize).min(count - 1))
        .collect()
}

/// The part `[x, y, width, height]` of `tile`, outside it transparent.
fn cropped(tile: &RasterTile2D, [x, y, width, height]: [i64; 4]) -> Option<RasterTile2D> {
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    if x == 0 && y == 0 && (w, h) == (tile.width(), tile.height()) {
        return Some(tile.clone());
    }
    let pixels = tile.bytes()?;
    let (tw, th) = (i64::from(tile.width()), i64::from(tile.height()));
    let mut out = vec![0_u8; (w as usize) * (h as usize) * 4];
    for row in 0..i64::from(h) {
        let sy = y + row;
        if !(0..th).contains(&sy) {
            continue;
        }
        for column in 0..i64::from(w) {
            let sx = x + column;
            if !(0..tw).contains(&sx) {
                continue;
            }
            let from = ((sy * tw + sx) * 4) as usize;
            let to = ((row * i64::from(w) + column) * 4) as usize;
            out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }
    RasterTile2D::from_bytes(w, h, PixelFormat::Rgba8, out).ok()
}

impl SuePaint {
    /// The size an export starts from: the output frame's, else the
    /// canvas's.
    pub(crate) fn export_size(&self, arena: &sue_core::Arena) -> [u32; 2] {
        let Some(picture) = self.picture() else {
            return [1, 1];
        };
        anim::setup::OutputFrame::of(arena, picture).map_or_else(
            || {
                arena
                    .get_by_id(picture)
                    .ok()
                    .and_then(sue_model::picture::size)
                    .unwrap_or([1, 1])
            },
            |frame| frame.size,
        )
    }

    /// Asks for an animation export's settings: a movie, an animated GIF,
    /// PNG or WebP, the cels, or a picture sequence (any other command).
    pub(crate) fn ask_export_animation(&mut self, cx: &mut AppletContext<'_>, command: &str) {
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let arena = &cx.world.arena;
        let size = self.export_size(arena);
        let fps = anim::fps(&sequence).round().max(1.0) as u32;
        let has_sound = !anim::audio_tracks(&sequence).is_empty();
        let range = {
            let (start, end) = anim::setup::Settings::of(arena, picture).range(length);
            if (start, end) == (0, length.saturating_sub(1)) {
                Range::All
            } else {
                Range::Played
            }
        };
        let dialog = if command == "file.export.movie" {
            let formats = sue_platform::movie::formats();
            AnimDialog::Movie(MovieForm {
                formats,
                format: 0,
                size,
                source: size,
                keep_shape: true,
                fps,
                quality: sue_io::MovieQuality::Normal,
                camera: true,
                sound: true,
                has_sound,
                range,
            })
        } else if let Some(kind) = Animated::of_command(command) {
            AnimDialog::Animated(AnimatedForm {
                kind,
                size,
                source: size,
                loops: 0,
                range,
                fps,
                camera: true,
            })
        } else if command == "file.export.cels" {
            AnimDialog::Cels(self.anim_ui.pending.cels.clone().unwrap_or_default())
        } else {
            let mut form = self.anim_ui.pending.sequence.clone().unwrap_or_default();
            form.range = range;
            AnimDialog::Sequence(form)
        };
        self.dialog = Some(Dialog::Anim(Box::new(dialog)));
    }

    /// The file pick of an animation export, named for the picture.
    fn pick_export(
        &self,
        cx: &mut AppletContext<'_>,
        purpose: &str,
        types: Vec<FileType>,
        extension: &str,
    ) {
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("animation"), |entity| entity.name().to_owned());
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                purpose,
                &tr!("export-animation"),
                &format!("{name}.{extension}"),
            )
            .types(types)
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// The movie's settings were kept: where does it go?
    pub(crate) fn ask_movie_file(&mut self, cx: &mut AppletContext<'_>, form: MovieForm) {
        let format = form.chosen();
        let types = vec![FileType::new(
            &sue_lang::translate("io", format.name()),
            &[format.extension()],
        )];
        self.anim_ui.pending.movie = Some(form);
        self.pick_export(cx, "anim-movie", types, format.extension());
    }

    /// The sequence's settings were kept: where do the pictures go?
    pub(crate) fn ask_sequence_file(&mut self, cx: &mut AppletContext<'_>, form: SequenceForm) {
        let extension = form.extension();
        let types = sue_io::PICTURE_SAVE_TYPES
            .get(form.format)
            .map(|(what, extensions)| vec![FileType::new(what, extensions)])
            .unwrap_or_default();
        self.anim_ui.pending.sequence = Some(form);
        self.pick_export(cx, "anim-sequence", types, extension);
    }

    /// The animated picture's settings were kept: where does it go?
    pub(crate) fn ask_animated_file(&mut self, cx: &mut AppletContext<'_>, form: AnimatedForm) {
        let (what, extensions): (String, &[&str]) = match form.kind {
            Animated::Gif => (tr!("animated-gif"), &["gif"]),
            Animated::Apng => (tr!("animated-png"), &["png", "apng"]),
            Animated::WebP => (tr!("animated-webp"), &["webp"]),
        };
        let extension = form.kind.extension();
        self.anim_ui.pending.animated = Some(form);
        self.pick_export(
            cx,
            "anim-gif",
            vec![FileType::new(&what, extensions)],
            extension,
        );
    }

    /// The cels' settings were kept: where do they go?
    pub(crate) fn ask_cels_file(&mut self, cx: &mut AppletContext<'_>, form: CelsForm) {
        let extension = sue_io::PICTURE_SAVE_TYPES
            .get(form.format)
            .and_then(|(_, extensions)| extensions.first())
            .copied()
            .unwrap_or("png");
        let types = sue_io::PICTURE_SAVE_TYPES
            .get(form.format)
            .map(|(what, extensions)| vec![FileType::new(what, extensions)])
            .unwrap_or_default();
        self.anim_ui.pending.cels = Some(form);
        self.pick_export(cx, "anim-cels", types, extension);
    }

    /// Writes the animation to `path` as a movie, in the format and with
    /// the settings its dialog kept (MP4 first where the system has an
    /// encoder, else the Motion JPEG AVI), the timeline's sound with it.
    pub(crate) fn export_movie(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let form = self.anim_ui.pending.movie.take().unwrap_or_else(|| {
            // A pick with no dialog before it: the AVI at the canvas size.
            let size = self.export_size(&cx.world.arena);
            let fps = self
                .timeline_here(&cx.world.arena)
                .map_or(24, |(_, sequence, _)| {
                    anim::fps(&sequence).round().max(1.0) as u32
                });
            MovieForm {
                formats: vec![sue_io::MovieFormat::MjpegAvi],
                format: 0,
                size,
                source: size,
                keep_shape: true,
                fps,
                quality: sue_io::MovieQuality::Normal,
                camera: true,
                sound: false,
                has_sound: false,
                range: Range::All,
            }
        });
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let Some(rendered) = self.render_frames(&cx.world.arena, form.range, form.camera) else {
            return;
        };
        let settings = form.settings();
        let sound = settings.sound.and_then(|_| {
            let (_, sequence, _) = self.timeline_here(&cx.world.arena)?;
            let pcm = anim::mix(&cx.world.arena, &sequence)?;
            // From the first frame written, as long as the frames last.
            let from = (f64::from(rendered.first) / rendered.fps.max(1.0)
                * f64::from(anim::MIX_RATE)) as usize;
            // The movie lasts as long as the timeline's frames, whatever its
            // own frame rate (see `retimed`).
            let frames = (rendered.frames.len() as f64 / rendered.fps.max(1.0)
                * f64::from(anim::MIX_RATE))
            .ceil() as usize;
            let samples = pcm.samples();
            let start = (from * 2).min(samples.len());
            let end = ((from + frames) * 2).min(samples.len());
            Some(samples[start..end].to_vec())
        });
        let result = (|| -> Result<(usize, usize), sue_io::MovieError> {
            let mut sink = sue_platform::movie::open(form.chosen(), settings)?;
            let order = retimed(rendered.frames.len(), rendered.fps, form.fps);
            let mut last: Option<(usize, RasterTile2D)> = None;
            for index in order {
                // A frame shown twice in a row is scaled once.
                if last.as_ref().is_none_or(|(was, _)| *was != index) {
                    last = Some((index, scaled(rendered.frames[index].clone(), form.size)));
                }
                if let Some((_, frame)) = &last {
                    sink.push_frame(frame)?;
                }
            }
            if let Some(samples) = &sound {
                sink.push_sound(samples)?;
            }
            let count = sink.frames();
            let bytes = sink.finish()?;
            write_atomic(path, &bytes)
                .map_err(|error| sue_io::MovieError::Encoder(error.to_string()))?;
            Ok((bytes.len(), count))
        })();
        match result {
            Ok((size, count)) => cx.saved(
                path,
                tr!("frames-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!(
                "cannot-export-the-movie",
                error = sue_lang::translate("io", error.text())
            )),
        }
    }

    /// The frames `range` of the timeline covers, flattened as they play
    /// (layer keyframes, clips, folder cels), through the 2D cameras when
    /// `camera` says so, and cut to the output frame when there is one.
    pub(crate) fn render_frames(
        &self,
        arena: &sue_core::Arena,
        range: Range,
        camera: bool,
    ) -> Option<Rendered> {
        let (picture, sequence, length) = self.timeline_here(arena)?;
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let played = anim::setup::Settings::of(arena, picture).range(length);
        let frames = range.frames(length, played);
        let first = frames.start;
        let output = anim::setup::OutputFrame::of(arena, picture).map(|frame| frame.rect());
        let frames = frames
            .filter_map(|frame| {
                let view = anim::frame_view(arena, picture, frame, None, Some(camera))?;
                let tile = composite::flatten_view(arena, picture, options, Some(&view))?;
                match output {
                    Some(rect) => cropped(&tile, rect),
                    None => Some(tile),
                }
            })
            .collect();
        Some(Rendered {
            frames,
            fps: anim::fps(&sequence),
            first,
        })
    }

    /// Writes the animation to `path`: one GIF, animated PNG or animated
    /// WebP (`animated`, with the settings its dialog kept), or a sequence
    /// of pictures named as its dialog said.
    pub(crate) fn export_animation(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        animated: bool,
    ) {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if animated {
            self.export_animated(cx, path);
        } else {
            self.export_sequence(cx, path);
        }
    }

    /// An animated GIF, PNG or WebP.
    fn export_animated(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let kind = match extension.as_str() {
            "png" | "apng" => Animated::Apng,
            "webp" => Animated::WebP,
            _ => Animated::Gif,
        };
        let form = self.anim_ui.pending.animated.take();
        let (range, camera) = form
            .as_ref()
            .map_or((Range::All, true), |form| (form.range, form.camera));
        let Some(rendered) = self.render_frames(&cx.world.arena, range, camera) else {
            return;
        };
        let size = form.as_ref().map(|form| form.size);
        let frames: Vec<RasterTile2D> = rendered
            .frames
            .into_iter()
            .map(|frame| match size {
                Some(size) => scaled(frame, size),
                None => frame,
            })
            .collect();
        let delay = (1000.0 / rendered.fps.max(1.0)).round() as u32;
        let loops = form.as_ref().map_or(0, |form| form.loops);
        let encoded = match kind {
            Animated::WebP => sue_io::encode_webp_animation(&frames, delay),
            Animated::Apng => sue_io::encode_apng(&frames, delay),
            Animated::Gif => sue_io::encode_gif_animation_looping(
                &frames,
                delay,
                u16::try_from(loops).ok().filter(|loops| *loops > 0),
            ),
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
    }

    /// Numbered pictures, one a frame.
    fn export_sequence(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let form = self.anim_ui.pending.sequence.take().unwrap_or_default();
        let Some(rendered) = self.render_frames(&cx.world.arena, form.range, form.camera) else {
            return;
        };
        let extension = path.extension().map_or_else(
            || form.extension().to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("frame"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let count = rendered.frames.len();
        for (index, frame) in rendered.frames.into_iter().enumerate() {
            let size = [
                (u64::from(frame.width()) * u64::from(form.scale.max(1)) / 100).max(1) as u32,
                (u64::from(frame.height()) * u64::from(form.scale.max(1)) / 100).max(1) as u32,
            ];
            let frame = scaled(frame, size);
            let number = form.first_number.saturating_add(index as u32);
            let target = dir.join(form.file_name(&stem, number, &extension));
            if let Err(error) = sue_io::encode_as(&frame, &extension, 90, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                cx.fail(tr!("cannot-export-frame", value = index + 1, error = error));
                return;
            }
        }
        cx.notify(tr!(
            "exported-frames-to",
            count = count,
            dir = dir.display()
        ));
    }

    /// The cels each animation folder of the timeline shows, by folder:
    /// the folder's name and its cels (folder cels included), in timeline
    /// order with no cel twice.
    pub(crate) fn cels_by_folder(&self, arena: &sue_core::Arena) -> Vec<(String, Vec<EntityId>)> {
        let Some((_, sequence, _)) = self.timeline_here(arena) else {
            return Vec::new();
        };
        anim::folders(&sequence)
            .into_iter()
            .map(|index| {
                let track = &sequence.tracks[index];
                let mut cels: Vec<EntityId> = Vec::new();
                for (_, cel) in anim::keys(track, sequence.rate) {
                    if let Some(cel) = cel
                        && !cels.contains(&cel)
                    {
                        cels.push(cel);
                    }
                }
                (anim::track_name(arena, track), cels)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{cropped, retimed, scaled};
    use sue_core::{PixelFormat, RasterTile2D};

    #[test]
    fn a_movie_at_another_frame_rate_lasts_as_long_as_the_timeline() {
        // The same rate: every frame once.
        assert_eq!(retimed(4, 24.0, 24), vec![0, 1, 2, 3]);
        // Half the rate: every other frame, half as many.
        assert_eq!(retimed(6, 24.0, 12), vec![0, 2, 4]);
        // Twice the rate: each frame twice.
        assert_eq!(retimed(3, 12.0, 24), vec![0, 0, 1, 1, 2, 2]);
        // 24 frames at 24 a second make one second at 30 a second.
        let thirty = retimed(24, 24.0, 30);
        assert_eq!(thirty.len(), 30);
        assert_eq!((thirty[0], thirty[29]), (0, 23));
        assert!(retimed(0, 24.0, 30).is_empty());
    }

    fn card(width: u32, height: u32) -> RasterTile2D {
        let pixels = (0..width * height)
            .flat_map(|index| [(index % 251) as u8, 90, 200, 255])
            .collect();
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn a_crop_takes_the_output_frame_and_leaves_outside_clear() {
        let tile = card(10, 8);
        let part = cropped(&tile, [2, 1, 4, 3]).unwrap();
        assert_eq!((part.width(), part.height()), (4, 3));
        assert_eq!(
            &part.bytes().unwrap()[..4],
            &tile.bytes().unwrap()[(10 + 2) * 4..(10 + 2) * 4 + 4]
        );
        // Over the edge: transparent.
        let over = cropped(&tile, [8, 0, 4, 1]).unwrap();
        assert_eq!(over.bytes().unwrap()[2 * 4 + 3], 0);
    }

    #[test]
    fn a_scale_reaches_the_size_asked() {
        let tile = scaled(card(10, 8), [5, 4]);
        assert_eq!((tile.width(), tile.height()), (5, 4));
    }
}
