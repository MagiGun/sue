//! Sue VE driven through real frames: the Hub's New, the pages, importing
//! a movie Sue made itself, placing it on the timeline by dragging it
//! from the pool, cutting it with the keys, undo and redo, the inspector,
//! and an export from the Deliver page.

use super::*;
use egui::pos2;
use sue_core::{EntityId, Time};
use sue_model::Sequence;

fn sve() -> AppletId {
    AppletId::new(sue_ve::ID).unwrap()
}

/// A runtime in a 1440 by 900 window with Sue VE's page open.
struct Driver {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
    texts: Vec<(String, egui::Rect)>,
}

impl Driver {
    fn new(tag: &str) -> Self {
        let (runtime, root) = fresh_runtime(tag);
        let mut driver = Self {
            runtime,
            ctx: egui::Context::default(),
            now: Instant::now(),
            root,
            texts: Vec::new(),
        };
        driver.settle(2);
        driver
    }

    fn request(&mut self, request: ShellRequest) {
        self.runtime.request(request, &mut NoTextures, self.now);
        self.settle(2);
    }

    fn command(&mut self, command: &str) {
        self.request(ShellRequest::AppletCommand {
            applet: sve(),
            command: command.to_owned(),
        });
    }

    fn step(&mut self, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        let found = text_rects(&output);
        output.drop_without_applying_deltas();
        self.texts.clone_from(&found);
        found
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
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

    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(found, _)| found.contains(text))
    }

    fn press(at: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        }
    }

    fn click(&mut self, at: Pos2) {
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![Self::press(at, true, egui::Modifiers::NONE)]);
        self.step(vec![Self::press(at, false, egui::Modifiers::NONE)]);
        self.settle(3);
    }

    /// A click on the page bar's button named `text` (the lowest of the
    /// texts so named: a palette's tab may share the name).
    fn click_page(&mut self, text: &str) {
        let at = self
            .texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.texts))
            .center();
        self.click(at);
    }

    fn click_text(&mut self, text: &str) {
        let at = self
            .rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.texts))
            .center();
        self.click(at);
    }

    /// A drag from `from` to `to` in steps, as a hand makes it.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![Self::press(from, true, egui::Modifiers::NONE)]);
        for step in 1..=12 {
            let at = from + (to - from) * (step as f32 / 12.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![Self::press(to, false, egui::Modifiers::NONE)]);
        self.settle(4);
    }

    fn keys(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.step(vec![super::key(key, modifiers)]);
        self.settle(2);
    }

    /// The open project's sequences: the only `ve.project` there is.
    fn sequence(&self) -> Option<(EntityId, Sequence)> {
        let arena = &self.runtime.engine.world.arena;
        let project = arena
            .iter()
            .find(|(_, entity)| entity.class() == sue_ve::model::class::PROJECT)?
            .1
            .id();
        let id = *sue_ve::model::sequences(arena, project).first()?;
        Some((id, sue_ve::model::sequence(arena, id)?.clone()))
    }

    fn clips(&self, track: &str) -> usize {
        self.sequence().map_or(0, |(_, sequence)| {
            sequence
                .tracks
                .iter()
                .find(|candidate| candidate.name == track)
                .map_or(0, |found| found.clips().len())
        })
    }
}

#[test]
fn the_hub_makes_a_video_and_the_pages_say_what_they_hold() {
    let mut driver = Driver::new("sve-pages");
    driver.request(ShellRequest::Create {
        applet: sve(),
        key: "hd".to_owned(),
    });
    driver.settle(40);
    assert_eq!(driver.runtime.shell.page(), Some(&sve()));
    let (_, sequence) = driver.sequence().expect("a project with a sequence");
    assert_eq!(sue_ve::model::frame_size(&sequence), (1920, 1080));
    let names: Vec<&str> = sequence
        .tracks
        .iter()
        .map(|track| track.name.as_str())
        .collect();
    assert_eq!(names, ["V3", "V2", "V1", "A1", "A2", "A3"]);
    // The Edit page: the pages along the bottom, the palettes, the tracks.
    for wanted in [
        "Media",
        "Edit",
        "Effects",
        "Color",
        "Audio",
        "Deliver",
        "Media Pool",
        "Inspector",
        "Timeline",
        "V1",
        "A1",
    ] {
        assert!(
            driver.rect_of(wanted).is_some(),
            "{wanted} is on the Edit page: {:?}",
            driver.texts
        );
    }
    // The pool holds the new sequence.
    assert!(driver.rect_of("Sequence 1").is_some(), "{:?}", driver.texts);
    // Pages not built yet say so, and lead back.
    driver.click_page("Color");
    assert!(
        driver.has("Color grading comes in a later version"),
        "{:?}",
        driver.texts
    );
    driver.click_text("Back to the Edit page");
    assert!(driver.rect_of("Timeline").is_some());
    driver.click_page("Effects");
    assert!(driver.has("Effects and compositing come in a later version"));
    driver.click_page("Deliver");
    assert!(
        driver.has("A bit rate of 0 lets the quality decide"),
        "{:?}",
        driver.texts
    );
    // Shift+4 is Resolve's Edit page.
    driver.keys(egui::Key::Num4, egui::Modifiers::SHIFT);
    assert!(driver.rect_of("Timeline").is_some());
    // The menus are Premiere's.
    driver.request(ShellRequest::Hub);
    driver.settle(30);
    assert!(driver.runtime.shell.hub_open());
}

#[test]
fn a_movie_dragged_from_the_pool_lands_on_the_timeline_and_the_keys_cut_it() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let mut driver = Driver::new("sve-drag");
    driver.request(ShellRequest::Create {
        applet: sve(),
        key: "hd".to_owned(),
    });
    driver.settle(10);
    let movie = driver.root.join("bars.mp4");
    sue_ve::sample::write_movie(&movie, 640, 360, 25, 4.0, true).unwrap();
    driver.request(ShellRequest::SendFile {
        to: sve(),
        purpose: "import".to_owned(),
        path: movie,
    });
    driver.settle(60);
    let card = driver
        .rect_of("bars.mp4")
        .expect("the movie's card in the pool");
    let lane = driver.rect_of("V1").expect("V1's header");
    // Onto V1, a little way in.
    let target = pos2(lane.right() + 260.0, lane.center().y + 6.0);
    driver.drag(card.center() - vec2(0.0, 30.0), target);
    driver.settle(20);
    assert_eq!(
        driver.clips("V1"),
        1,
        "{:?}",
        driver.sequence().map(|s| s.1)
    );
    assert_eq!(driver.clips("A1"), 1, "its sound came along");
    let (_, sequence) = driver.sequence().unwrap();
    let v1 = sequence
        .tracks
        .iter()
        .find(|track| track.name == "V1")
        .unwrap();
    let a1 = sequence
        .tracks
        .iter()
        .find(|track| track.name == "A1")
        .unwrap();
    let (video, sound) = (&v1.clips()[0], &a1.clips()[0]);
    assert!(
        video.link != 0 && video.link == sound.link,
        "picture and sound are linked"
    );
    assert!(
        (video.duration.seconds() - 4.0).abs() < 0.1,
        "{:?}",
        video.duration
    );
    // The playhead into the clip, by clicking the ruler; Ctrl+K cuts both.
    let at = pos2(lane.right() + (260.0 + 60.0), lane.top() - 40.0);
    // The ruler's labels: two seconds in is on the ruler only.
    let ruler_y = driver
        .rect_of("00:00:02:00")
        .map_or(at.y, |code| code.center().y);
    let middle_x = target.x + (video.duration.seconds() as f32 * 60.0) / 2.0;
    driver.click(pos2(middle_x, ruler_y));
    driver.keys(egui::Key::K, egui::Modifiers::COMMAND);
    assert_eq!(driver.clips("V1"), 2, "Add Edit cut the picture");
    assert_eq!(driver.clips("A1"), 2, "and its sound");
    // Ctrl+Z takes the cut back, Ctrl+Y makes it again.
    driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(driver.clips("V1"), 1);
    driver.keys(egui::Key::Y, egui::Modifiers::COMMAND);
    assert_eq!(driver.clips("V1"), 2);
    // A click selects a clip; the inspector shows its transform.
    let v1_lane = driver.rect_of("V1").unwrap();
    driver.click(pos2(target.x + 20.0, v1_lane.center().y + 8.0));
    assert!(
        driver.has("Transform") && driver.has("Opacity"),
        "{:?}",
        driver.texts
    );
    // Shift+Delete ripples it away: what follows moves up to the start.
    let before = driver.sequence().unwrap().1;
    let start = before
        .tracks
        .iter()
        .find(|track| track.name == "V1")
        .unwrap()
        .clips()[0]
        .start;
    driver.keys(egui::Key::Delete, egui::Modifiers::SHIFT);
    let after = driver.sequence().unwrap().1;
    let v1 = after
        .tracks
        .iter()
        .find(|track| track.name == "V1")
        .unwrap();
    assert_eq!(v1.clips().len(), 1);
    assert_eq!(v1.clips()[0].start, start, "the gap closed");
}

#[test]
fn the_deliver_page_exports_the_edit_in_the_background() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let mut driver = Driver::new("sve-deliver");
    let dir = driver.root.join("media");
    std::fs::create_dir_all(&dir).unwrap();
    let made = driver
        .runtime
        .with_context(&mut NoTextures, driver.now, |cx| {
            sue_ve::sample::sample_project(cx, &dir)
        });
    let (project, sequence) = made.expect("the sample edit");
    driver.request(ShellRequest::OpenIn {
        entity: project,
        applet: sve(),
    });
    // The page glides in; it takes clicks once it is in place.
    driver.settle(90);
    driver.click_page("Deliver");
    assert!(driver.has("Format"), "the Deliver page: {:?}", driver.texts);
    // Small and short: the in to out range at 320 wide.
    driver.click_text("The whole sequence (else from the in to the out mark)");
    let _ = sequence;
    driver.click_text("Start Export");
    let ctx = driver.ctx.clone();
    let mut now = driver.now;
    until_exports_end(&mut driver.runtime, |runtime| {
        now += Duration::from_millis(16);
        frame_with(runtime, &ctx, now, Vec::new());
    });
    driver.settle(10);
    let out = sue_ve::media::import::cache_dir()
        .join("exports")
        .join("Sequence 1.mp4");
    let facts = sue_platform::video::probe(&out).expect("the movie was written");
    assert_eq!((facts.width, facts.height), (1280, 720));
    // In 1 s to out 6 s: five seconds.
    assert!(
        (facts.duration as f64 / 1e7 - 5.0).abs() < 0.2,
        "{}",
        facts.duration
    );
    assert!(facts.audio.is_some());
    let _ = std::fs::remove_file(out);
    let _ = Time::ZERO;
}
