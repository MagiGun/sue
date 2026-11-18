//! Export round trips on movies made here, and the decoders under the
//! monitors.

use super::*;
use crate::edit;
use crate::media::{MediaHub, Need};
use crate::model::key;
use sue_core::{Entity, UndoStack};

/// A file name of its own for this test run: `name` with a number
/// before its extension.
fn scratch(name: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join("sue-ve-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "bin"));
    let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    dir.join(format!(
        "{stem}-{}-{number}.{extension}",
        std::process::id()
    ))
}

/// A project with the sample movie on V1 and its sound linked on A1.
fn project(width: u32, height: u32, seconds: f64) -> (Arena, EntityId, EntityId, PathBuf) {
    let path = scratch(&format!("bars-{width}.mp4"));
    crate::sample::write_movie(&path, width, height, 25, seconds, true).unwrap();
    let mut arena = Arena::new();
    let mut history = UndoStack::new();
    let made = model::new_project("Test", "Sequence 1", width, height, Rate::FPS_25);
    history.apply(&mut arena, made.batch).unwrap();
    let movie = import::entity_for(&path, false).unwrap();
    let movie_id = movie.id();
    let spawn = model::spawn_under(&arena, made.project, movie);
    history.apply(&mut arena, spawn).unwrap();
    let length = model::length_of(&arena, movie_id).unwrap();
    let (step, _) = model::plan(&arena, made.sequence, "Place", |sequence| {
        let video = sue_model::Clip::new(movie_id, Time::ZERO, length);
        let sound = sue_model::Clip::new(movie_id, Time::ZERO, length);
        let ids = edit::overwrite(sequence, vec![(2, video), (3, sound)])?;
        edit::link(sequence, &ids)?;
        Ok(())
    })
    .unwrap();
    history.apply(&mut arena, step.unwrap()).unwrap();
    (arena, made.sequence, movie_id, path)
}

#[test]
fn an_imported_movie_has_its_facts() {
    let (arena, _, movie, path) = project(320, 180, 2.0);
    let entity = arena.get_by_id(movie).unwrap();
    assert_eq!(model::number(entity, key::WIDTH), Some(320.0));
    assert_eq!(model::rate_of(entity), Some(Rate::FPS_25));
    assert!(model::has_sound(&arena, movie));
    let length = model::length_of(&arena, movie).unwrap();
    assert!((length.seconds() - 2.0).abs() < 0.05, "{length:?}");
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_monitor_decoders_land_on_the_exact_frame() {
    let (arena, _, movie, path) = project(320, 180, 2.0);
    let mut hub = MediaHub::new(256);
    hub.hardware = false;
    let kind = SourceKind::Movie;
    for target in [37_i64, 5, 30, 49, 0] {
        let time = Time::from_frames(target, Rate::FPS_25);
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let found = loop {
            let found = hub
                .frame(
                    &arena,
                    movie,
                    &kind,
                    time,
                    (320, 180),
                    Quality::Full,
                    Need::Exact,
                )
                .filter(|found| found.exact);
            if let Some(found) = found {
                break found;
            }
            assert!(Instant::now() < deadline, "frame {target} never came");
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        assert_eq!(found.frame.index, target);
        // The counting square's shade says which frame it is.
        let want = crate::sample::frame(320, 180, target as u32);
        let at = (20 * 320 + 20) * 4;
        let (got, expected) = (found.frame.rgba[at], want[at]);
        assert!(
            got.abs_diff(expected) <= 8,
            "frame {target}: {got} for {expected}"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn an_mp4_export_reads_back_with_its_frames_length_and_pixels() {
    let (arena, sequence, _, path) = project(320, 180, 2.0);
    let out = scratch("export.mp4");
    let mut settings = Settings::whole(model::sequence(&arena, sequence).unwrap());
    settings.bitrate = Some(4_000_000);
    let report = run(arena.clone(), sequence, settings, &out, false, None).unwrap();
    assert_eq!(report.frames, 50);
    let facts = sue_platform::video::probe(&out).unwrap();
    assert_eq!((facts.width, facts.height), (320, 180));
    assert_eq!(facts.rate, (25, 1));
    assert!(
        (facts.duration as f64 / 1e7 - 2.0).abs() < 0.1,
        "{}",
        facts.duration
    );
    assert_eq!(facts.audio, Some((48_000, 2)));
    let mut decoder = sue_platform::video::VideoDecoder::open(&out, None, false).unwrap();
    let mut rgba = Vec::new();
    let mut count = 0;
    let mut sampled = None;
    while let Some(_) = decoder.next_frame(&mut rgba).unwrap() {
        if count == 20 {
            sampled = Some(rgba.clone());
        }
        count += 1;
    }
    assert_eq!(count, 50);
    let sampled = sampled.unwrap();
    let source = crate::sample::frame(320, 180, 20);
    let error: f64 = sampled
        .iter()
        .zip(&source)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / sampled.len() as f64;
    assert!(error < 6.0, "frame 20 differs by {error:.2} on average");
    // Its sound: the tone, about two seconds of it.
    let sound = crate::media::sound::decode(&out, true).unwrap();
    assert!((sound.frames() as f64 / 48_000.0 - 2.0).abs() < 0.1);
    let peak = sound.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.3, "{peak}");
    for file in [path, out] {
        let _ = std::fs::remove_file(file);
    }
}

#[test]
fn wav_png_and_numbered_frames_are_written() {
    let (arena, sequence, _, path) = project(160, 90, 1.0);
    let mut settings = Settings::whole(model::sequence(&arena, sequence).unwrap());
    settings.target = Target::Wav;
    let wav = scratch("mix.wav");
    run(arena.clone(), sequence, settings, &wav, false, None).unwrap();
    let info = sue_io::probe_audio(&wav).unwrap();
    assert_eq!((info.sample_rate, info.channels), (48_000, 2));
    assert!(
        (info.frames as i64 - 48_000).abs() < 2_000,
        "{}",
        info.frames
    );
    settings.target = Target::Png;
    settings.range.0 = Time::from_frames(10, Rate::FPS_25);
    let png = scratch("still.png");
    run(arena.clone(), sequence, settings, &png, false, None).unwrap();
    let decoded = sue_io::decode_png(&std::fs::read(&png).unwrap()).unwrap();
    assert_eq!((decoded.tile.width(), decoded.tile.height()), (160, 90));
    settings.target = Target::PngSequence;
    settings.range = (Time::ZERO, Time::from_frames(3, Rate::FPS_25));
    let run_path = scratch("run.png");
    let report = run(arena, sequence, settings, &run_path, false, None).unwrap();
    assert_eq!(report.frames, 3);
    for index in 0..3 {
        assert!(numbered(&run_path, index).is_file());
        let _ = std::fs::remove_file(numbered(&run_path, index));
    }
    for file in [path, wav, png] {
        let _ = std::fs::remove_file(file);
    }
}

#[test]
fn a_cancelled_export_stops_and_says_so() {
    let (arena, sequence, _, path) = project(160, 90, 1.0);
    let pool = sue_core::JobPool::new(1);
    let out = scratch("cancelled.mp4");
    let target = out.clone();
    let job = pool.spawn("export", move |job| {
        // Waits for Cancel, as a person would press it while it runs.
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !job.is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let settings = Settings::whole(model::sequence(&arena, sequence).unwrap());
        run(arena, sequence, settings, &target, false, Some(job))
    });
    job.cancel();
    assert!(job.wait().is_err());
    assert!(!out.exists(), "nothing was written");
    let _ = std::fs::remove_file(path);
}

#[test]
fn titles_solids_and_stills_render_into_the_export() {
    let mut arena = Arena::new();
    let mut history = UndoStack::new();
    let made = model::new_project("Test", "Sequence 1", 64, 36, Rate::FPS_25);
    history.apply(&mut arena, made.batch).unwrap();
    let solid = import::solid("Red", [200, 0, 0]);
    let title = import::title("Title", "Hi").with_property(key::SIZE, 12.0);
    let still = Entity::new(
        "dot",
        RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, [0, 0, 255, 255].repeat(4)).unwrap(),
    )
    .with_class(sue_model::class::IMAGE);
    let ids = [solid.id(), title.id(), still.id()];
    for entity in [solid, title, still] {
        let spawn = model::spawn_under(&arena, made.project, entity);
        history.apply(&mut arena, spawn).unwrap();
    }
    let (step, _) = model::plan(&arena, made.sequence, "Place", |sequence| {
        let second = Time::SECOND;
        edit::overwrite(
            sequence,
            vec![
                (2, sue_model::Clip::new(ids[0], Time::ZERO, second)),
                (1, sue_model::Clip::new(ids[1], Time::ZERO, second)),
                (2, sue_model::Clip::new(ids[2], second, second)),
            ],
        )
    })
    .unwrap();
    history.apply(&mut arena, step.unwrap()).unwrap();
    let mut renderer = Renderer::new(arena, made.sequence, (64, 36), false).unwrap();
    let first = renderer.frame(Time::from_seconds(0.5));
    assert_eq!(&first[..4], &[200, 0, 0, 255], "the solid fills the corner");
    let letters = first.chunks_exact(4).filter(|p| p[1] > 100).count();
    assert!(letters > 0, "the title's white letters show over the red");
    let second = renderer.frame(Time::from_seconds(1.5));
    assert_eq!(
        &second[(18 * 64 + 32) * 4..(18 * 64 + 32) * 4 + 4],
        &[0, 0, 255, 255]
    );
}

/// Measures decoding and export on this machine:
/// `cargo test -p sue-ve --release measure -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, it prints a table"]
fn measure_decoding_and_export() {
    for (width, height, name) in [(1920_u32, 1080_u32, "1080p30"), (3840, 2160, "2160p30")] {
        let path = scratch(&format!("measure-{name}.mp4"));
        crate::sample::write_movie(&path, width, height, 30, 10.0, true).unwrap();
        for hardware in [true, false] {
            let mut decoder =
                sue_platform::video::VideoDecoder::open(&path, None, hardware).unwrap();
            let used = decoder.hardware();
            let started = Instant::now();
            let mut rgba = Vec::new();
            let mut count = 0;
            while decoder.next_frame(&mut rgba).unwrap().is_some() {
                count += 1;
            }
            let seconds = started.elapsed().as_secs_f64();
            println!(
                "| {name} | decode full size | {} | {count} frames | {:.1} fps |",
                if used { "graphics card" } else { "processor" },
                f64::from(count) / seconds
            );
            // Half size, as the program monitor decodes at Half quality.
            let mut half = sue_platform::video::VideoDecoder::open(
                &path,
                Some((width / 2, height / 2)),
                hardware,
            )
            .unwrap();
            let started = Instant::now();
            let mut count = 0;
            while half.next_frame(&mut rgba).unwrap().is_some() {
                count += 1;
            }
            let seconds = started.elapsed().as_secs_f64();
            println!(
                "| {name} | decode half size | {} | {count} frames | {:.1} fps |",
                if used { "graphics card" } else { "processor" },
                f64::from(count) / seconds
            );
            // Seeks to an exact frame, spread over the movie.
            let mut times = Vec::new();
            for target in [250_i64, 17, 133, 299, 64, 200] {
                let ticks = target * 10_000_000 / 30;
                let started = Instant::now();
                decoder.seek(ticks).unwrap();
                decoder.frame_at(ticks, &mut rgba).unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let mean = times.iter().sum::<f64>() / times.len() as f64;
            let worst = times.iter().fold(0.0_f64, |a, b| a.max(*b));
            println!(
                "| {name} | exact seek | {} | mean {mean:.1} ms | worst {worst:.1} ms |",
                if used { "graphics card" } else { "processor" }
            );
        }
        // Export the movie on a timeline at its own size.
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let made = model::new_project("Measure", "Sequence", width, height, Rate::FPS_30);
        history.apply(&mut arena, made.batch).unwrap();
        let movie = import::entity_for(&path, false).unwrap();
        let movie_id = movie.id();
        let spawn = model::spawn_under(&arena, made.project, movie);
        history.apply(&mut arena, spawn).unwrap();
        let length = model::length_of(&arena, movie_id).unwrap();
        let (step, _) = model::plan(&arena, made.sequence, "Place", |sequence| {
            edit::overwrite(
                sequence,
                vec![
                    (2, sue_model::Clip::new(movie_id, Time::ZERO, length)),
                    (3, sue_model::Clip::new(movie_id, Time::ZERO, length)),
                ],
            )
        })
        .unwrap();
        history.apply(&mut arena, step.unwrap()).unwrap();
        let out = scratch(&format!("measure-{name}-export.mp4"));
        let settings = Settings::whole(model::sequence(&arena, made.sequence).unwrap());
        let report = run(arena, made.sequence, settings, &out, true, None).unwrap();
        println!(
            "| {name} | export MP4 with sound | {} frames | {:.1} s | {:.1} fps |",
            report.frames,
            report.seconds,
            report.frames as f64 / report.seconds
        );
        for file in [path, out] {
            let _ = std::fs::remove_file(file);
        }
    }
}

#[test]
fn a_movies_sound_and_thumbnails_come_in_the_background() {
    let (arena, _, movie, path) = project(320, 180, 2.0);
    let mut hub = MediaHub::new(64);
    hub.prepare(&arena, movie);
    let deadline = Instant::now() + std::time::Duration::from_secs(20);
    while hub.busy() {
        assert!(Instant::now() < deadline, "the background work ends");
        hub.poll();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    hub.poll();
    assert_eq!(hub.sounds.error(movie), None);
    let sound = hub.sounds.get(movie).expect("the sound decoded");
    assert!((sound.frames() as f64 / 48_000.0 - 2.0).abs() < 0.1);
    assert!(
        sound.peaks.iter().any(|peak| peak[1] > 0.3),
        "the waveform has the tone"
    );
    let strip = hub.thumbs.strip(movie).unwrap();
    let strip = strip.lock().unwrap();
    assert_eq!(strip.frames.len(), 2, "a thumbnail a second");
    assert_eq!(strip.frames[0].height, crate::media::thumbs::THUMB_HEIGHT);
    let _ = std::fs::remove_file(path);
}
