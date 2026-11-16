//! Exporting the animation: a movie, an animated picture, a sequence of
//! pictures or the cels. Each export asks for its settings first (the
//! dialogs of `dialogs/anim.rs`), then where the file goes; what the
//! settings dialog kept waits on the studio until the file is picked.

use std::path::{Path, PathBuf};

use sue_core::{EntityId, PixelFormat, RasterTile2D};
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest};

use super::forms::{Animated, AnimatedForm, CelsForm, MovieForm, Range, SequenceForm};
use crate::anim;
use crate::composite::{self, RenderOptions};
use crate::dialogs::{AnimDialog, Dialog};
use crate::studio::SuePaint;
use crate::studio::exports::{Done, Staged, Steps, What, write_whole};

/// What a picked file is exported with: the settings kept in the dialog
/// before it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pending {
    pub movie: Option<MovieForm>,
    pub sequence: Option<SequenceForm>,
    pub animated: Option<AnimatedForm>,
    pub cels: Option<CelsForm>,
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

/// The frames of an export: a snapshot of the document, and which of its
/// timeline's frames go out, rendered one at a time as the export needs
/// them ([`SuePaint::export_frames`]).
pub(crate) struct Frames {
    /// The document as it was when the export began.
    arena: sue_core::Arena,
    picture: EntityId,
    /// The timeline frames written (from 0).
    frames: std::ops::Range<u32>,
    /// Seen through the 2D camera folders.
    camera: bool,
    /// The output frame's rect, when there is one.
    output: Option<[i64; 4]>,
    /// The timeline's frames a second.
    pub fps: f64,
    /// The timeline, for its sound.
    sequence: sue_model::Sequence,
}

impl Frames {
    /// How many frames go out.
    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }

    /// The first frame written (from 0).
    pub(crate) fn first(&self) -> u32 {
        self.frames.start
    }

    /// The `index`th frame written, flattened as it plays; `None` when it
    /// will not render.
    pub(crate) fn render(&self, index: usize) -> Option<RasterTile2D> {
        let frame = self.frames.start.checked_add(u32::try_from(index).ok()?)?;
        if frame >= self.frames.end {
            return None;
        }
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let view = anim::frame_view(&self.arena, self.picture, frame, None, Some(self.camera))?;
        let tile = composite::flatten_view(&self.arena, self.picture, options, Some(&view))?;
        match self.output {
            Some(rect) => cropped(&tile, rect),
            None => Some(tile),
        }
    }

    /// The timeline's sound under the frames written, interleaved stereo
    /// at [`anim::MIX_RATE`], as long as the frames last.
    fn sound(&self) -> Option<Vec<f32>> {
        let pcm = anim::mix(&self.arena, &self.sequence)?;
        let fps = self.fps.max(1.0);
        let rate = f64::from(anim::MIX_RATE);
        let from = (f64::from(self.first()) / fps * rate) as usize;
        // The movie lasts as long as the timeline's frames, whatever its
        // own frame rate (see `retimed`).
        let frames = (self.len() as f64 / fps * rate).ceil() as usize;
        let samples = pcm.samples();
        let start = (from * 2).min(samples.len());
        let end = ((from + frames) * 2).min(samples.len());
        Some(samples[start..end].to_vec())
    }
}

/// The movie of `frames` with the settings of `form`, and how many frames
/// it holds. The same whether it runs in the background or at once
/// ([`Steps::NOW`]): the frames are rendered one at a time as the sink
/// takes them.
///
/// # Errors
/// The movie error's English text (`io.ftl`), or that it was cancelled.
pub(crate) fn movie_bytes(
    frames: &Frames,
    form: &MovieForm,
    steps: Steps<'_>,
) -> Result<(Vec<u8>, usize), String> {
    let settings = form.settings();
    let sound = settings.sound.and_then(|_| frames.sound());
    let mut sink = sue_platform::movie::open(form.chosen(), settings)
        .map_err(|error| error.text().to_owned())?;
    let order = retimed(frames.len(), frames.fps, form.fps);
    let mut last: Option<(usize, RasterTile2D)> = None;
    for (at, index) in order.iter().copied().enumerate() {
        steps.go_on()?;
        steps.part(at, order.len(), 0.95);
        // A frame shown twice in a row is rendered and scaled once; one
        // that will not render shows the one before it.
        if last.as_ref().is_none_or(|(was, _)| *was != index)
            && let Some(frame) = frames.render(index)
        {
            last = Some((index, scaled(frame, form.size)));
        }
        if let Some((_, frame)) = &last {
            sink.push_frame(frame)
                .map_err(|error| error.text().to_owned())?;
        }
    }
    if let Some(samples) = &sound {
        sink.push_sound(samples)
            .map_err(|error| error.text().to_owned())?;
    }
    steps.go_on()?;
    let count = sink.frames();
    let bytes = sink.finish().map_err(|error| error.text().to_owned())?;
    Ok((bytes, count))
}

/// The animated picture of `frames` (`kind`), each frame scaled to `size`
/// when one is given, looping `loops` times (0 for ever) and dithered when
/// asked (GIF), and how many frames it holds (a frame that will not render
/// is left out).
///
/// # Errors
/// When a frame will not encode, or it was cancelled.
pub(crate) fn animated_bytes(
    frames: &Frames,
    kind: Animated,
    size: Option<[u32; 2]>,
    (loops, dither): (u32, bool),
    steps: Steps<'_>,
) -> Result<(Vec<u8>, usize), String> {
    let mut tiles = Vec::with_capacity(frames.len());
    for index in 0..frames.len() {
        steps.go_on()?;
        steps.part(index, frames.len(), 0.8);
        if let Some(frame) = frames.render(index) {
            tiles.push(match size {
                Some(size) => scaled(frame, size),
                None => frame,
            });
        }
    }
    steps.go_on()?;
    let delay = (1000.0 / frames.fps.max(1.0)).round() as u32;
    let bytes = match kind {
        Animated::WebP => sue_io::encode_webp_animation(&tiles, delay),
        Animated::Apng => sue_io::encode_apng(&tiles, delay),
        Animated::Gif => sue_io::encode_gif_animation_with(
            &tiles,
            delay,
            u16::try_from(loops).ok().filter(|loops| *loops > 0),
            dither,
        ),
    }?;
    Ok((bytes, tiles.len()))
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
                dither: false,
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
    /// encoder, else the Motion JPEG AVI), the timeline's sound with it,
    /// in the background ([`crate::studio::exports`]).
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
        let Some(frames) = self.export_frames(&cx.world.arena, form.range, form.camera) else {
            return;
        };
        let target = path.to_path_buf();
        self.run_export(cx, What::Movie, path, move |steps| {
            let (bytes, count) = movie_bytes(&frames, &form, steps)?;
            write_whole(&target, &bytes, steps)?;
            Ok(Done {
                frames: count,
                bytes: bytes.len(),
                files: 1,
            })
        });
    }

    /// The frames `range` of the timeline covers, to be flattened as they
    /// play (layer keyframes, clips, folder cels), through the 2D cameras
    /// when `camera` says so, and cut to the output frame when there is
    /// one: read from a snapshot of the document taken now, so an export
    /// made from them never sees what is drawn meanwhile.
    pub(crate) fn export_frames(
        &self,
        arena: &sue_core::Arena,
        range: Range,
        camera: bool,
    ) -> Option<Frames> {
        let (picture, sequence, length) = self.timeline_here(arena)?;
        let played = anim::setup::Settings::of(arena, picture).range(length);
        Some(Frames {
            // Shares every raster (D21): a reference count, not a copy.
            arena: arena.clone(),
            picture,
            frames: range.frames(length, played),
            camera,
            output: anim::setup::OutputFrame::of(arena, picture).map(|frame| frame.rect()),
            fps: anim::fps(&sequence),
            sequence,
        })
    }

    /// Writes the animation to `path`: one GIF, animated PNG or animated
    /// WebP (`animated`, with the settings its dialog kept), or a sequence
    /// of pictures named as its dialog said, in the background.
    pub(crate) fn export_animation(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &Path,
        animated: bool,
    ) {
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
        let Some(frames) = self.export_frames(&cx.world.arena, range, camera) else {
            return;
        };
        let size = form.as_ref().map(|form| form.size);
        let loops = form.as_ref().map_or(0, |form| form.loops);
        let dither = form.as_ref().is_some_and(|form| form.dither);
        let target = path.to_path_buf();
        self.run_export(cx, What::Animation, path, move |steps| {
            let (bytes, count) = animated_bytes(&frames, kind, size, (loops, dither), steps)?;
            write_whole(&target, &bytes, steps)?;
            Ok(Done {
                frames: count,
                bytes: bytes.len(),
                files: 1,
            })
        });
    }

    /// Numbered pictures, one a frame. They appear together when the last
    /// is made, and none does when the export stops early.
    fn export_sequence(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let form = self.anim_ui.pending.sequence.take().unwrap_or_default();
        let Some(frames) = self.export_frames(&cx.world.arena, form.range, form.camera) else {
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
        let targets: Vec<PathBuf> = (0..frames.len())
            .map(|index| {
                let number = form.first_number.saturating_add(index as u32);
                dir.join(form.file_name(&stem, number, &extension))
            })
            .collect();
        let Some(first) = targets.first().cloned() else {
            return;
        };
        let scale = form.scale.max(1);
        let held = first.clone();
        self.run_export(cx, What::Sequence, &first, move |steps| {
            let mut staged = Staged::default();
            for (index, target) in targets.iter().enumerate() {
                steps.go_on()?;
                steps.part(index, targets.len(), 0.95);
                let Some(frame) = frames.render(index) else {
                    continue;
                };
                let size = [
                    (u64::from(frame.width()) * u64::from(scale) / 100).max(1) as u32,
                    (u64::from(frame.height()) * u64::from(scale) / 100).max(1) as u32,
                ];
                let frame = scaled(frame, size);
                let bytes = sue_io::encode_as(&frame, &extension, 90, 7).map_err(|error| {
                    tr!("cannot-export-frame", value = index + 1, error = error)
                })?;
                staged.write(target, &bytes)?;
            }
            let files = staged.finish(&held, steps)?;
            Ok(Done {
                frames: files,
                bytes: 0,
                files,
            })
        });
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
