//! Media Sue makes itself, for the self-check, the tests and the
//! snapshot tour: a movie of moving color bars with a tone, written with
//! Sue's own MP4 export, so nothing of anyone's is needed to try SVE.

use std::path::Path;

use sue_core::{PixelFormat, RasterTile2D};

/// The color of bar `index` of eight.
const BARS: [[u8; 3]; 8] = [
    [235, 235, 235],
    [235, 235, 16],
    [16, 235, 235],
    [16, 235, 16],
    [235, 16, 235],
    [235, 16, 16],
    [16, 16, 235],
    [16, 16, 16],
];

/// Frame `index` of the sample movie: eight bars sliding left by
/// `index` pixels a frame, and a square whose shade counts the frames.
#[must_use]
pub fn frame(width: u32, height: u32, index: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut pixels = vec![0_u8; w * h * 4];
    let bar = (w / 8).max(1);
    let shade = ((index * 7) % 220 + 20) as u8;
    for y in 0..h {
        for x in 0..w {
            let at = (y * w + x) * 4;
            let rgb = if y < h / 3 && x < w / 4 {
                [shade, shade, shade]
            } else {
                BARS[((x + index as usize * 4) / bar) % 8]
            };
            pixels[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    pixels
}

/// Writes the sample movie: `seconds` long at `fps`, `width` by
/// `height`, with a 440 Hz tone (`sound`) at half scale.
///
/// # Errors
/// When this system has no MP4 encoder, or the file cannot be written.
pub fn write_movie(
    path: &Path,
    width: u32,
    height: u32,
    fps: u32,
    seconds: f64,
    sound: bool,
) -> Result<(), String> {
    let mut settings = sue_io::MovieSettings::new(width, height, fps);
    settings.quality = sue_io::MovieQuality::High;
    if sound {
        settings.sound = Some(sue_io::MovieSound {
            rate: 48_000,
            channels: 2,
        });
    }
    let mut sink = sue_platform::movie::open(sue_io::MovieFormat::Mp4H264, settings)
        .map_err(|error| error.text().to_owned())?;
    let frames = (seconds * f64::from(fps)).round() as u32;
    if sound {
        let count = (seconds * 48_000.0) as usize;
        let tone: Vec<f32> = (0..count)
            .flat_map(|at| {
                let value = (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [value, value]
            })
            .collect();
        sink.push_sound(&tone)
            .map_err(|error| error.text().to_owned())?;
    }
    for index in 0..frames {
        let tile = RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            frame(width, height, index),
        )
        .map_err(|error| error.to_string())?;
        sink.push_frame(&tile)
            .map_err(|error| error.text().to_owned())?;
    }
    let bytes = sink.finish().map_err(|error| error.text().to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

/// Makes a sample project in the arena, every step undoable: the sample
/// movie (written into `dir`, where this system can write MP4) in the
/// pool, two pieces of it on V1 with their sound linked on A1 and a
/// dissolve between them, a title over the first with a fade in, a
/// marker and in and out marks. Returns the project and its sequence.
pub fn sample_project(
    cx: &mut sue_wm::AppletContext<'_>,
    dir: &Path,
) -> Option<(sue_core::EntityId, sue_core::EntityId)> {
    use sue_core::{Rate, Time};
    use sue_wm::ArenaEdit;

    use crate::edit;
    use crate::model;

    let made = model::new_project(
        &tr!("sample-project"),
        &tr!("sequence-name", number = 1),
        1280,
        720,
        Rate::FPS_25,
    );
    let (project, sequence) = (made.project, made.sequence);
    cx.apply(ArenaEdit(made.batch)).ok()?;
    let movie = dir.join("sue-ve-sample.mp4");
    if !movie.is_file() {
        let _ = write_movie(&movie, 1280, 720, 25, 8.0, true);
    }
    let film = crate::media::import::entity_for(&movie, false)
        .ok()
        .map(|entity| {
            let id = entity.id();
            (id, entity)
        });
    let title = crate::media::import::title(&tr!("title-name"), &tr!("sample-title"));
    let title_id = title.id();
    let mut batch = sue_core::txn::Batch::new(tr!("import-media"));
    let mut entities = vec![title];
    if let Some((_, entity)) = &film {
        entities.insert(0, entity.clone());
    }
    model::spawn_all_under(cx.arena(), project, entities, &mut batch);
    cx.apply(ArenaEdit(batch)).ok()?;
    let second = Time::SECOND;
    let secs = Time::from_seconds;
    let (step, _) = model::plan(cx.arena(), sequence, &tr!("overwrite"), |edit_sequence| {
        if let Some((film, _)) = film {
            let first = sue_model::Clip::new(film, Time::ZERO, secs(4.0));
            let mut later = sue_model::Clip::new(film, secs(4.0), secs(3.0));
            later.source_in = secs(5.0);
            let ids = edit::overwrite(
                edit_sequence,
                vec![
                    (2, first.clone()),
                    (3, first),
                    (2, later.clone()),
                    (3, later),
                ],
            )?;
            edit::link(edit_sequence, &[ids[0], ids[1]])?;
            edit::link(edit_sequence, &[ids[2], ids[3]])?;
            edit::add_transition(edit_sequence, 2, "dissolve", secs(4.0), second)?;
            edit::add_transition(edit_sequence, 3, "dissolve", secs(4.0), second)?;
        }
        let mut words =
            sue_model::Clip::new(title_id, Time::from_seconds(0.5), Time::from_seconds(2.5));
        words.properties.set(model::key::POS_Y, 220.0);
        edit::overwrite(edit_sequence, vec![(1, words)])?;
        edit::add_transition(
            edit_sequence,
            1,
            "dissolve",
            Time::from_seconds(0.5),
            Time::from_seconds(0.5),
        )?;
        edit::add_marker(edit_sequence, secs(2.0), &tr!("marker-name", number = 1));
        edit::set_mark(edit_sequence, false, Some(second));
        edit::set_mark(edit_sequence, true, Some(secs(6.0)));
        Ok(())
    })
    .ok()?;
    if let Some(step) = step {
        cx.apply(ArenaEdit(step)).ok()?;
    }
    Some((project, sequence))
}

/// Dresses the sample edit with what stage 2 adds, for the tour and the
/// tests: the first shot gets a vignette (its amount keyed and eased) and
/// a warmer color balance, the second a grade, the title a typewriter
/// animator, and the sound track an equalizer.
pub fn dress(cx: &mut sue_wm::AppletContext<'_>, sequence: sue_core::EntityId) -> Option<()> {
    use crate::{fx, model};
    use sue_core::Time;
    let (step, ()) = model::plan(
        cx.arena(),
        sequence,
        &tr!("sample-dress"),
        |edit_sequence| {
            // V1 holds the shots; the tracks over it the title.
            let main = edit_sequence.tracks.iter().rposition(model::is_video);
            let mut shots: Vec<u64> = Vec::new();
            let mut words: Vec<u64> = Vec::new();
            for (index, track) in edit_sequence.tracks.iter().enumerate() {
                if !model::is_video(track) {
                    continue;
                }
                for clip in track.clips() {
                    if Some(index) == main {
                        shots.push(clip.id);
                    } else {
                        words.push(clip.id);
                    }
                }
            }
            if let Some(first) = shots.first().copied()
                && let Some((_, clip)) = edit_sequence.find_mut(first)
            {
                let vignette = fx::add(clip, "stylize.vignette");
                let warm = fx::add(clip, "color.balance");
                fx::update(clip, warm, |effect| {
                    effect.params.set("mid_red", 25.0);
                    effect.params.set("mid_blue", -20.0);
                });
                let curve = clip.animation.channel_mut(&fx::channel(vignette, "amount"));
                for (seconds, value) in [(0.0, 0.0), (2.0, 80.0)] {
                    curve.insert(sue_model::Key {
                        time: Time::from_seconds(seconds),
                        value,
                        interpolation: sue_model::Interpolation::Linear,
                    });
                }
                let at = Time::ZERO;
                crate::keys::ease_key(curve, at, crate::keys::Ease::Both);
                crate::keys::ease_key(curve, Time::from_seconds(2.0), crate::keys::Ease::Both);
            }
            if let Some(second) = shots.get(1).copied()
                && let Some((_, clip)) = edit_sequence.find_mut(second)
            {
                crate::grade::set_grade(
                    clip,
                    &[crate::grade::Corrector {
                        lift: [0.0, 0.0, 0.04, -0.02],
                        gain: [0.06, 0.02, 0.0, 0.05],
                        saturation: 1.25,
                        contrast: 1.1,
                        ..crate::grade::Corrector::default()
                    }],
                );
            }
            if let Some(title) = words.first().copied()
                && let Some((_, clip)) = edit_sequence.find_mut(title)
            {
                let animator = crate::layers::Animator {
                    softness: 1.0,
                    ..crate::layers::Animator::default()
                };
                clip.properties
                    .set(crate::layers::ANIMATOR, animator.to_value());
                let (start, end) = (clip.source_in, clip.source_in + Time::from_seconds(1.5));
                let curve = clip.animation.channel_mut(crate::layers::RANGE);
                for (time, value) in [(start, 0.0), (end, 100.0)] {
                    curve.insert(sue_model::Key {
                        time,
                        value,
                        interpolation: sue_model::Interpolation::Linear,
                    });
                }
            }
            if let Some(track) = edit_sequence
                .tracks
                .iter_mut()
                .find(|track| !model::is_video(track))
            {
                let mut fx = crate::audio::TrackFx {
                    eq_on: true,
                    comp_on: true,
                    ..crate::audio::TrackFx::default()
                };
                fx.bands[0].gain = -4.0;
                fx.bands[3].gain = 3.0;
                track
                    .properties
                    .set(crate::fx::key::AUDIO_FX, fx.to_value());
            }
            Ok(())
        },
    )
    .ok()?;
    if let Some(step) = step {
        cx.apply(sue_wm::ArenaEdit(step)).ok()?;
    }
    Some(())
}
