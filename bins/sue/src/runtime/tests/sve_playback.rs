//! Sue VE playing in real time, driven as the desktop loop drives it:
//! the timer runs only when the pacing asks for it, a frame is drawn when
//! something animates, the timer asks for one or egui does. Nothing else
//! happens unless a test sends input.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use super::*;
use egui::pos2;
use sue_core::{EntityId, Time};

fn sve() -> AppletId {
    AppletId::new(sue_ve::ID).unwrap()
}

/// Remembers the textures asked for (there is no graphics card here).
#[derive(Default)]
struct Uploads {
    seen: HashSet<(u64, u64)>,
}

impl sue_platform::TextureUpload for Uploads {
    fn texture(
        &mut self,
        key: u64,
        version: u64,
        _size: [u32; 2],
        _channels: u8,
        _pixels: &[u8],
        _nearest: bool,
    ) -> Option<egui::TextureId> {
        self.seen.insert((key, version));
        None
    }
}

struct Desk {
    runtime: Runtime,
    ctx: egui::Context,
    root: PathBuf,
    texts: Vec<(String, egui::Rect)>,
    uploads: Uploads,
    started: Instant,
    /// egui asked for another frame.
    repaint: bool,
    /// Frames drawn.
    frames: usize,
}

impl Desk {
    fn new(tag: &str) -> Self {
        let (runtime, root) = fresh_runtime(tag);
        Self {
            runtime,
            ctx: egui::Context::default(),
            root,
            texts: Vec::new(),
            uploads: Uploads::default(),
            started: Instant::now(),
            repaint: true,
            frames: 0,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.started.elapsed().as_secs_f64()),
            ..egui::RawInput::default()
        };
        let runtime = &mut self.runtime;
        let uploads = &mut self.uploads;
        let now = Instant::now();
        let output = self.ctx.run_ui(raw, |ui| runtime.ui(ui, uploads, now));
        self.repaint = output
            .viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero());
        self.texts = text_rects(&output);
        output.drop_without_applying_deltas();
        self.frames += 1;
    }

    /// One turn of the desktop loop: the timer when it is due, then a
    /// frame when one is wanted. Returns `false` when the loop would
    /// sleep until input.
    fn turn(&mut self) -> bool {
        let now = Instant::now();
        let mut redraw = false;
        if let FramePacing::WakeAt(at) = self.runtime.pacing(now)
            && at <= now
        {
            redraw |= self.runtime.timer(&mut self.uploads, now);
        }
        let pacing = self.runtime.pacing(Instant::now());
        if matches!(pacing, FramePacing::Animate { .. }) || redraw || self.repaint {
            self.frame(Vec::new());
            return true;
        }
        !matches!(pacing, FramePacing::Sleep)
    }

    /// Runs the loop for `seconds` of real time.
    fn run(&mut self, seconds: f64, mut each: impl FnMut(&Self)) {
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < seconds {
            self.turn();
            each(self);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn input(&mut self, events: Vec<egui::Event>) {
        self.frame(events);
        self.run(0.05, |_| {});
    }

    fn button(at: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: sve(),
                command: command.to_owned(),
            },
            &mut self.uploads,
            Instant::now(),
        );
        self.run(0.5, |_| {});
    }

    fn rect_of(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| {
                a.top()
                    .total_cmp(&b.top())
                    .then(a.left().total_cmp(&b.left()))
            })
    }

    /// The timecodes on screen.
    fn timecodes(&self) -> Vec<String> {
        self.texts
            .iter()
            .map(|(text, _)| text.clone())
            .filter(|text| {
                text.len() == 11
                    && text.chars().enumerate().all(|(at, c)| {
                        if at % 3 == 2 {
                            c == ':'
                        } else {
                            c.is_ascii_digit()
                        }
                    })
            })
            .collect()
    }

    /// The movie's clip on V1: its source and where it starts.
    fn clip(&self) -> Option<(EntityId, Time)> {
        let arena = &self.runtime.engine.world.arena;
        let project = arena
            .iter()
            .find(|(_, entity)| entity.class() == sue_ve::model::class::PROJECT)?
            .1
            .id();
        let id = *sue_ve::model::sequences(arena, project).first()?;
        let sequence = sue_ve::model::sequence(arena, id)?;
        let clip = sequence
            .tracks
            .iter()
            .find(|track| track.name == "V1")?
            .clips()
            .first()?;
        Some((clip.source, clip.start))
    }

    /// A new HD project with a movie Sue made dragged from the pool onto
    /// V1, on the Edit page. Returns V1's header.
    fn with_a_clip(tag: &str, size: (u32, u32), fps: u32) -> (Self, egui::Rect) {
        let mut desk = Self::new(tag);
        desk.runtime.request(
            ShellRequest::Create {
                applet: sve(),
                key: "hd".to_owned(),
            },
            &mut desk.uploads,
            Instant::now(),
        );
        desk.run(1.5, |_| {});
        let movie = desk.root.join("clip.mp4");
        sue_ve::sample::write_movie(&movie, size.0, size.1, fps, 6.0, true).unwrap();
        desk.runtime.request(
            ShellRequest::SendFile {
                to: sve(),
                purpose: "import".to_owned(),
                path: movie,
            },
            &mut desk.uploads,
            Instant::now(),
        );
        desk.run(2.0, |_| {});
        desk.command("view.page_media");
        desk.command("view.page_edit");
        let card = desk
            .texts
            .iter()
            .filter(|(found, _)| found == "clip.mp4")
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.left().total_cmp(&b.left()))
            .expect("the movie's card in the pool");
        let lane = desk.rect_of("V1").expect("V1's header");
        let from = card.center() - vec2(0.0, 30.0);
        let to = pos2(lane.right() + 260.0, lane.center().y + 6.0);
        desk.input(vec![egui::Event::PointerMoved(from)]);
        desk.input(vec![Self::button(from, true)]);
        for step in 1..=12 {
            let at = from + (to - from) * (step as f32 / 12.0);
            desk.input(vec![egui::Event::PointerMoved(at)]);
        }
        desk.input(vec![Self::button(to, false)]);
        desk.run(0.5, |_| {});
        assert!(desk.clip().is_some(), "the clip is on V1");
        (desk, lane)
    }

    /// A click on the ruler `seconds` into the clip (60 points a second).
    fn click_ruler(&mut self, lane: egui::Rect, seconds: f32) {
        let ruler_y = self
            .rect_of("00:00:02:00")
            .map_or(lane.top() - 40.0, |code| code.center().y);
        let at = pos2(lane.right() + 260.0 + seconds * 60.0, ruler_y);
        self.input(vec![egui::Event::PointerMoved(at)]);
        self.frame(vec![Self::button(at, true)]);
        self.frame(vec![Self::button(at, false)]);
    }
}

#[test]
fn sve_playback_runs_without_input_in_the_desktop_loop() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let (mut desk, lane) = Desk::with_a_clip("sve-play-loop", (320, 180), 30);
    desk.click_ruler(lane, 0.0);
    desk.run(0.5, |_| {});
    let before: HashSet<String> = desk.timecodes().into_iter().collect();
    desk.input(vec![
        key(egui::Key::Space, egui::Modifiers::NONE),
        egui::Event::Text(" ".to_owned()),
    ]);
    // No input: the loop alone moves the playhead.
    let mut shown = HashSet::new();
    desk.run(1.0, |desk| {
        shown.extend(
            desk.timecodes()
                .into_iter()
                .filter(|code| !before.contains(code)),
        );
    });
    assert!(
        shown.len() >= 15,
        "the playhead went through a second's frames, not {}: {shown:?}",
        shown.len()
    );
}

#[test]
fn sve_program_monitor_shows_the_frame_after_a_click_without_more_input() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let (mut desk, lane) = Desk::with_a_clip("sve-frame-wait", (320, 180), 30);
    let (source, _) = desk.clip().unwrap();
    // The program monitor's picture, as the monitor names its texture.
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ("sve-layer", source, 0_usize).hash(&mut hasher);
        hasher.finish()
    };
    let shown_before: HashSet<u64> = desk
        .uploads
        .seen
        .iter()
        .filter(|(seen, _)| *seen == key)
        .map(|(_, version)| *version)
        .collect();
    // Into the middle of the clip, where no picture was asked for yet;
    // then nothing but the loop.
    desk.click_ruler(lane, 3.0);
    let start = Instant::now();
    let mut came = None;
    let mut asleep = false;
    while start.elapsed() < Duration::from_secs(4) {
        let awake = desk.turn();
        if came.is_none()
            && desk
                .uploads
                .seen
                .iter()
                .any(|(seen, version)| *seen == key && !shown_before.contains(version))
        {
            came = Some(start.elapsed());
        }
        if came.is_some() && !awake {
            asleep = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let came = came.expect("the frame at the playhead was drawn without input");
    assert!(came < Duration::from_secs(2), "{came:?}");
    assert!(asleep, "the loop rests once the picture is in");
}

#[test]
#[ignore = "needs a sound device and runs in real time; --ignored --nocapture"]
fn sve_plays_with_the_real_sound_device() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let (mut desk, lane) = Desk::with_a_clip("sve-play-device", (1920, 1080), 30);
    desk.click_ruler(lane, 0.0);
    desk.run(0.5, |_| {});
    let config = desk.runtime.engine.audio_config();
    let output = sue_platform::AudioOutput::open(&config).expect("a sound device");
    eprintln!("{:?}", output.info());
    desk.runtime.engine.audio = Some(Box::new(output));
    desk.input(vec![
        key(egui::Key::Space, egui::Modifiers::NONE),
        egui::Event::Text(" ".to_owned()),
    ]);
    let start = Instant::now();
    let mut last = Duration::ZERO;
    desk.run(3.0, |desk| {
        if start.elapsed() >= last + Duration::from_millis(250) {
            last = start.elapsed();
            let played = desk
                .runtime
                .engine
                .audio
                .as_ref()
                .map_or(0, |sink| sink.played_frames());
            let notes: Vec<&String> = desk
                .texts
                .iter()
                .filter(|(text, _)| text.contains("dropped"))
                .map(|(text, _)| text)
                .collect();
            eprintln!(
                "wall {:4.2} played {played:6} frames {} {:?} {notes:?}",
                last.as_secs_f64(),
                desk.frames,
                desk.timecodes()
            );
        }
    });
    let played = desk
        .runtime
        .engine
        .audio
        .as_ref()
        .map_or(0, |sink| sink.played_frames());
    assert!(played > 96_000, "the device played the sound: {played}");
}
