//! Sue VE driven through real frames: the Hub's New, the pages, importing
//! a movie Sue made itself, placing it on the timeline by dragging it
//! from the pool, cutting it with the keys, undo and redo, the inspector,
//! and an export from the Deliver page; then stage 2: an effect added from
//! the browser and keyframed, a key eased, a grade dragged on the Color
//! page, the mixer's equalizer switched, the phone layout's tool sheets,
//! and an export with effects that matches the preview, each with undo.

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
    // The page glides in; it takes clicks once it is in place.
    driver.settle(90);
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
    // The Color, Effects and Audio pages hold what they say.
    driver.click_page("Color");
    for wanted in ["Waveform", "Vectorscope", "Select a clip to grade it"] {
        assert!(
            driver.has(wanted),
            "{wanted} on the Color page: {:?}",
            driver.texts
        );
    }
    driver.click_page("Effects");
    for wanted in ["Search effects", "Gaussian Blur", "Chroma Key"] {
        assert!(
            driver.has(wanted),
            "{wanted} on the Effects page: {:?}",
            driver.texts
        );
    }
    driver.click_page("Audio");
    for wanted in ["Master", "Record Voice Over", "Normalize Loudness"] {
        assert!(
            driver.has(wanted),
            "{wanted} on the Audio page: {:?}",
            driver.texts
        );
    }
    driver.click_page("Edit");
    assert!(driver.rect_of("Timeline").is_some());
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
    // The Media page shows the movie just brought in, with its facts.
    driver.command("view.page_media");
    driver.settle(10);
    assert!(
        driver.has("About the clip") && driver.has("640 x 360, 25 fps, H264"),
        "{:?}",
        driver.texts
    );
    driver.command("view.page_edit");
    driver.settle(10);
    // The card in the pool (the source monitor's title names it too).
    let card = driver
        .texts
        .iter()
        .filter(|(found, _)| found == "bars.mp4")
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
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
    driver.click_text("Whole sequence (else in to out)");
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

impl Driver {
    /// Sends Sue VE to `target` (a page, a clip to pick, the playhead).
    fn go(&mut self, target: &str) {
        if let Some(applet) = self.runtime.applets.get_mut(&sve()) {
            applet.navigate(target);
        }
        self.settle(3);
    }

    /// The sample edit, opened, its page come to rest.
    fn sample(&mut self) -> EntityId {
        let dir = self.root.join("media");
        std::fs::create_dir_all(&dir).unwrap();
        let made = self.runtime.with_context(&mut NoTextures, self.now, |cx| {
            sue_ve::sample::sample_project(cx, &dir)
        });
        let (project, sequence) = made.expect("the sample edit");
        self.request(ShellRequest::OpenIn {
            entity: project,
            applet: sve(),
        });
        self.settle(90);
        sequence
    }

    /// The clip `index` of the track named `track`.
    fn clip(&self, track: &str, index: usize) -> sue_model::Clip {
        let (_, sequence) = self.sequence().expect("a sequence");
        sequence
            .tracks
            .iter()
            .find(|found| found.name == track)
            .and_then(|found| found.clips().get(index))
            .cloned()
            .unwrap_or_else(|| panic!("{track} has a clip {index}"))
    }

    /// A double click on the text `text` (the topmost so named).
    fn double_click_text(&mut self, text: &str) {
        let at = self
            .rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.texts))
            .center();
        // Long enough after the last click that this one is not counted
        // as its second or third.
        self.settle(30);
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![
            Self::press(at, true, egui::Modifiers::NONE),
            Self::press(at, false, egui::Modifiers::NONE),
        ]);
        self.step(vec![
            Self::press(at, true, egui::Modifiers::NONE),
            Self::press(at, false, egui::Modifiers::NONE),
        ]);
        self.settle(3);
    }
}

#[test]
fn an_effect_from_the_browser_is_keyframed_eased_and_undone() {
    let mut driver = Driver::new("sve-effects");
    driver.sample();
    // The title on V2 picked, the playhead in it.
    driver.go("select/title/0");
    driver.go("seek/1.5");
    driver.click_page("Effects");
    // A double click in the browser adds the effect to the picked clip.
    driver.double_click_text("Gaussian Blur");
    let effects = sue_ve::fx::effects(&driver.clip("V2", 0));
    assert_eq!(effects.len(), 1, "{effects:?}");
    assert_eq!(effects[0].kind, "blur.gaussian");
    // Its Radius in Effect Controls, picked by its name, then keyed at
    // the playhead (Ctrl+Alt+K) and moved on a second and keyed again.
    // Effect Controls scrolled down to the effect, under Motion and
    // Opacity.
    let over = driver.rect_of("Motion").expect("Effect Controls").center();
    driver.step(vec![egui::Event::PointerMoved(over)]);
    for _ in 0..4 {
        driver.step(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -200.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
            source: egui::MouseWheelSource::Wheel,
        }]);
    }
    driver.settle(10);
    driver.click_text("Radius");
    driver.keys(
        egui::Key::K,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    let channel = sue_ve::fx::channel(effects[0].id, "radius");
    let keys = |driver: &Driver| {
        driver
            .clip("V2", 0)
            .animation
            .channel(&channel)
            .map(|curve| curve.keys().to_vec())
            .unwrap_or_default()
    };
    assert_eq!(keys(&driver).len(), 1, "a key at the playhead");
    driver.go("seek/2.5");
    driver.keys(
        egui::Key::K,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    assert_eq!(keys(&driver).len(), 2);
    // F9 eases the key at the playhead (After Effects' Easy Ease): the
    // segment arriving there turns Bezier.
    driver.keys(egui::Key::F9, egui::Modifiers::NONE);
    let eased = keys(&driver);
    assert!(
        matches!(
            eased[0].interpolation,
            sue_model::Interpolation::Bezier { .. }
        ),
        "{eased:?}"
    );
    // The graph editor shows the curve's name.
    assert!(driver.has("Gaussian Blur: Radius"), "{:?}", driver.texts);
    // Undo takes back the ease, the second key, the first, the effect.
    for left in [2, 1, 0] {
        driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(keys(&driver).len(), left);
    }
    assert!(matches!(
        keys(&driver).first().map(|key| key.interpolation),
        None | Some(sue_model::Interpolation::Linear)
    ));
    driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(
        sue_ve::fx::effects(&driver.clip("V2", 0)).is_empty(),
        "the effect went"
    );
}

#[test]
fn a_grade_dragged_on_the_color_page_is_one_step() {
    let mut driver = Driver::new("sve-grade");
    driver.sample();
    driver.go("select/title/0");
    driver.go("seek/1.5");
    driver.click_page("Color");
    assert!(sue_ve::grade::grade_of(&driver.clip("V2", 0)).is_empty());
    // A drag across the Lift wheel's pad sets its balance.
    let label = driver.rect_of("Lift").expect("the Lift wheel's name");
    let from = pos2(label.left() + 40.0, label.bottom() + 50.0);
    driver.drag(from, from + vec2(30.0, -10.0));
    let graded = sue_ve::grade::grade_of(&driver.clip("V2", 0));
    assert_eq!(graded.len(), 1, "one corrector: {graded:?}");
    let lift = graded[0].lift;
    assert!(lift[..3].iter().any(|value| value.abs() > 0.01), "{lift:?}");
    // Alt+S adds a corrector after it (Resolve's serial node).
    driver.keys(egui::Key::S, egui::Modifiers::ALT);
    assert_eq!(sue_ve::grade::grade_of(&driver.clip("V2", 0)).len(), 2);
    driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
    driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(
        sue_ve::grade::grade_of(&driver.clip("V2", 0)).is_empty(),
        "the drag was one step"
    );
}

#[test]
fn the_mixer_switches_an_equalizer_and_undo_takes_it_back() {
    let mut driver = Driver::new("sve-mixer");
    driver.sample();
    driver.click_page("Audio");
    let a1 = |driver: &Driver| {
        let (_, sequence) = driver.sequence().unwrap();
        let track = sequence
            .tracks
            .iter()
            .find(|track| track.name == "A1")
            .unwrap()
            .clone();
        sue_ve::audio::track_fx(&track)
    };
    assert!(!a1(&driver).eq_on);
    // A1's strip is the first: its EQ chip is the topmost leftmost.
    driver.click_text("EQ");
    assert!(a1(&driver).eq_on, "{:?}", driver.texts);
    driver.keys(egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(!a1(&driver).eq_on);
}

#[test]
fn the_phone_layout_shows_capcuts_tools_and_its_sheets_edit_the_video() {
    let mut driver = Driver::new("sve-phone");
    driver.request(ShellRequest::SueMenu);
    let start = driver.now;
    let picked = driver
        .runtime
        .with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Interface", "Phone"], io, now)
        });
    assert!(picked);
    driver.settle(10);
    assert_eq!(
        driver.runtime.shell.form_factor(),
        sue_platform::FormFactor::Mobile
    );
    driver.request(ShellRequest::Create {
        applet: sve(),
        key: "hd".to_owned(),
    });
    driver.settle(90);
    for tool in [
        "Edit", "Audio", "Text", "Effects", "Filters", "Overlay", "Export",
    ] {
        assert!(
            driver.rect_of(tool).is_some(),
            "{tool} on the phone: {:?}",
            driver.texts
        );
    }
    // The Aspect sheet: 9:16 makes the video upright, undo takes it back.
    driver.go("sheet/aspect");
    assert!(driver.has("9:16"), "{:?}", driver.texts);
    driver.click_text("9:16");
    let size = sue_ve::model::frame_size(&driver.sequence().unwrap().1);
    assert_eq!(size, (1080, 1920));
    driver.command("edit.undo");
    assert_eq!(
        sue_ve::model::frame_size(&driver.sequence().unwrap().1),
        (1920, 1080)
    );
    // The Text sheet adds a title at the playhead.
    driver.go("sheet/text");
    driver.click_text("Add Text");
    assert_eq!(driver.clips("V1"), 1, "the title landed on the main track");
    // The Export sheet offers sizes and rates.
    driver.go("export-sheet");
    for wanted in ["720p", "1080p", "2160p", "30", "60", "Start Export"] {
        assert!(driver.has(wanted), "{wanted}: {:?}", driver.texts);
    }
}

#[test]
fn an_export_with_effects_matches_the_preview() {
    if !sue_platform::movie::mp4_available() {
        return;
    }
    let mut driver = Driver::new("sve-export-effects");
    let sequence = driver.sample();
    // Every shot gray (a hue and saturation effect at -100), as one step.
    let made = driver
        .runtime
        .with_context(&mut NoTextures, driver.now, |cx| {
            let (step, ()) = sue_ve::model::plan(cx.arena(), sequence, "Gray", |edit| {
                let main = edit
                    .tracks
                    .iter()
                    .rposition(sue_ve::model::is_video)
                    .unwrap();
                let ids: Vec<u64> = edit.tracks[main]
                    .clips()
                    .iter()
                    .map(|clip| clip.id)
                    .collect();
                for id in ids {
                    let (_, clip) = edit.find_mut(id).unwrap();
                    let effect = sue_ve::fx::add(clip, "color.hue_saturation");
                    sue_ve::fx::update(clip, effect, |found| {
                        found.params.set("saturation", -100.0);
                    });
                }
                Ok(())
            })
            .ok()?;
            cx.apply(sue_wm::ArenaEdit(step?)).ok()
        });
    assert!(made.is_some());
    driver.settle(10);
    // The preview's frame two seconds in, drawn as the monitor draws it.
    let arena = driver.runtime.engine.world.arena.clone();
    let mut renderer = sue_ve::export::Renderer::new(arena, sequence, (320, 180), false).unwrap();
    let preview = renderer.frame(Time::from_seconds(2.0));
    driver.click_page("Deliver");
    driver.click_text("Whole sequence (else in to out)");
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
    let mut decoder = sue_platform::video::VideoDecoder::open(&out, Some((320, 180)), false)
        .expect("the movie reads back");
    let mut rgba = Vec::new();
    // In 1 s: two seconds into the sequence is one second into the movie.
    let at = 10_000_000;
    decoder.seek(at).unwrap();
    decoder.frame_at(at, &mut rgba).unwrap().expect("a frame");
    // Gray everywhere, and as the preview drew it (within the encoder's
    // loss).
    let mut worst = 0_i32;
    let mut colorful = 0;
    for (written, drawn) in rgba.chunks(4).zip(preview.chunks(4)) {
        let spread = i32::from(written[0].max(written[1]).max(written[2]))
            - i32::from(written[0].min(written[1]).min(written[2]));
        if spread > 40 {
            colorful += 1;
        }
        for channel in 0..3 {
            worst = worst.max((i32::from(written[channel]) - i32::from(drawn[channel])).abs());
        }
    }
    assert!(
        colorful < rgba.len() / 4 / 100,
        "the export is gray: {colorful} colorful pixels"
    );
    let mean: f64 = rgba
        .iter()
        .zip(&preview)
        .enumerate()
        .filter(|(index, _)| index % 4 != 3)
        .map(|(_, (a, b))| f64::from((i32::from(*a) - i32::from(*b)).abs()))
        .sum::<f64>()
        / (rgba.len() as f64 * 0.75);
    assert!(
        mean < 8.0,
        "the export matches the preview: mean {mean}, worst {worst}"
    );
    let _ = std::fs::remove_file(out);
}
