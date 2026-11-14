//! Tests of the applets' views in windows of their own, and of the
//! floating frame that takes their place where the system gives Sue one
//! window.

use std::sync::{Arc, Mutex};

use egui::{Event, PointerButton};
use sue_core::{Arena, UndoStack};
use sue_platform::{HeadlessWindows, MAIN_SURFACE, NoTextures, WindowSupport};
use sue_rice::{HardwareProfile, RiceSchema};

use super::*;
use crate::{
    AppletContext, AppletManifest, AppletRole, Applets, Catalog, Launchpad, ShellRequest,
    SueApplet, Theme, Transport, Typing, WmConfig, WorkList, World,
};

/// What the viewer heard: each view drawn (with the room it had), each
/// click on its button, its page drawn, each view closed.
#[derive(Default)]
struct Heard {
    drawn: Vec<(String, Rect)>,
    clicked: Vec<String>,
    pages: usize,
    closed: Vec<String>,
}

/// A studio with a page and views, each view one button filling it.
struct Viewer(Arc<Mutex<Heard>>);

impl SueApplet for Viewer {
    fn manifest(&self) -> AppletManifest {
        AppletManifest::new("viewer", "Viewer", AppletRole::Studio, "*")
    }

    fn page_ui(&mut self, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
        self.0.lock().unwrap().pages += 1;
        // A button where the views have theirs: a click meant for a view
        // must not reach it.
        let _ = ui.button("page");
    }

    fn view_ui(&mut self, view: &str, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
        let room = ui.max_rect();
        let clicked = ui.add_sized(room.size(), egui::Button::new(view)).clicked();
        let mut heard = self.0.lock().unwrap();
        heard.drawn.push((view.to_owned(), room));
        if clicked {
            heard.clicked.push(view.to_owned());
        }
    }

    fn view_closed(&mut self, view: &str, _cx: &mut AppletContext<'_>) {
        self.0.lock().unwrap().closed.push(view.to_owned());
    }
}

/// Everything a shell draws with.
struct Rig {
    world: World,
    history: UndoStack<World>,
    typing: Typing,
    applets: Applets,
    catalog: Catalog,
    transport: Transport,
    requests: Vec<ShellRequest>,
    work: WorkList,
    transfers: crate::Transfers,
    shell: Shell,
    heard: Arc<Mutex<Heard>>,
}

const SCREEN: [f32; 2] = [1440.0, 900.0];

impl Rig {
    fn new(support: WindowSupport, form: FormFactor) -> Self {
        let heard = Arc::new(Mutex::new(Heard::default()));
        let mut launchpad = Launchpad::new();
        let shared = Arc::clone(&heard);
        launchpad
            .register(move || Box::new(Viewer(Arc::clone(&shared))))
            .unwrap();
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        rice.register_all(&launchpad.rice_declarations()).unwrap();
        let world = World {
            arena: Arena::new(),
            rice,
        };
        let mut shell = Shell::new(
            WmConfig::from_rice(&world.rice).unwrap(),
            Theme::from_rice(&world.rice),
            HardwareProfile::Balanced,
            60,
            form,
        );
        shell.set_window_support(support);
        let applets = launchpad.instantiate();
        shell.open(viewer(), &applets);
        Self {
            world,
            history: UndoStack::new(),
            typing: Typing::default(),
            applets,
            catalog: Catalog::default(),
            transport: Transport::default(),
            requests: Vec::new(),
            work: WorkList::default(),
            transfers: crate::Transfers::default(),
            shell,
            heard,
        }
    }

    fn with<R>(&mut self, work: impl FnOnce(&mut Shell, &mut ShellIo<'_>) -> R) -> R {
        let mut io = ShellIo {
            world: &mut self.world,
            history: &mut self.history,
            sue_history: None,
            typing: &mut self.typing,
            applets: &mut self.applets,
            catalog: &self.catalog,
            textures: &mut NoTextures,
            audio: None,
            transport: &mut self.transport,
            requests: &mut self.requests,
            work: &self.work,
            transfers: &mut self.transfers,
            fps: None,
        };
        work(&mut self.shell, &mut io)
    }

    /// Hands `request` to the shell, as the runtime does.
    fn ask(&mut self, request: ShellRequest) {
        let left = self.with(|shell, io| shell.handle(request, io, Instant::now()));
        assert!(left.is_none(), "the shell takes it");
    }

    /// One frame of the main window, and of the window of each view.
    fn frame(&mut self, host: &mut HeadlessWindows) {
        let specs = self.shell.os_windows();
        host.sync(&specs, 1.0);
        self.with(|shell, io| {
            host.main_frame(0, &specs, |ui| shell.ui(ui, io, Instant::now()));
        });
        for spec in &specs {
            let Some(surface) = host.surface_of(&spec.key) else {
                continue;
            };
            self.with(|shell, io| {
                host.window_frame(surface, 0, |ui| {
                    shell.view_window_ui(&spec.key, ui, io, Instant::now());
                })
                .unwrap();
            });
        }
        // What the frames closed leaves the list.
        let specs = self.shell.os_windows();
        host.sync(&specs, 1.0);
    }

    fn frames(&mut self, host: &mut HeadlessWindows, count: usize) {
        for _ in 0..count {
            self.frame(host);
        }
    }
}

fn viewer() -> AppletId {
    AppletId::new("viewer").unwrap()
}

fn open(view: &str, title: &str) -> ShellRequest {
    ShellRequest::OpenView {
        applet: viewer(),
        view: view.to_owned(),
        title: title.to_owned(),
    }
}

/// A click at `at` for one surface, one event a frame.
fn click(host: &mut HeadlessWindows, rig: &mut Rig, surface: sue_platform::SurfaceId, at: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for event in [Event::PointerMoved(at), button(true), button(false)] {
        host.inject(surface, event).unwrap();
        rig.frame(host);
    }
}

#[test]
fn a_view_opens_in_a_window_of_its_own_where_the_system_has_many() {
    let mut rig = Rig::new(WindowSupport::Many, FormFactor::Desktop);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, SCREEN, 1.0);
    rig.ask(open("canvas", "Second canvas"));
    let specs = rig.shell.os_windows();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].key, view_key(&viewer(), "canvas"));
    assert_eq!(specs[0].title, "Second canvas");
    rig.frames(&mut host, 3);
    let surface = host.surface_of(&specs[0].key).unwrap();
    assert_ne!(surface, MAIN_SURFACE);
    {
        let heard = rig.heard.lock().unwrap();
        assert!(heard.pages >= 3, "the main window still draws the page");
        let (view, room) = heard.drawn.last().cloned().unwrap();
        assert_eq!(view, "canvas");
        // The view fills its own window, not the main one.
        let info = host.info(surface).unwrap();
        assert!((room.width() - info.width).abs() < 1.0, "{room:?}");
        assert!(room.width() < SCREEN[0]);
    }
    // No floating frame where the view has its window.
    assert!(rig.shell.view_rects(&specs[0].key).is_none());
    // A click in the view's window reaches the view only.
    let middle = pos2(200.0, 200.0);
    click(&mut host, &mut rig, surface, middle);
    assert_eq!(rig.heard.lock().unwrap().clicked, vec!["canvas".to_owned()]);
    // And a click at the same place of the main window does not.
    click(&mut host, &mut rig, MAIN_SURFACE, middle);
    assert_eq!(rig.heard.lock().unwrap().clicked.len(), 1);
}

#[test]
fn closing_a_views_window_leaves_the_main_window() {
    let mut rig = Rig::new(WindowSupport::Many, FormFactor::Desktop);
    rig.shell.set_own_frame(true);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, SCREEN, 1.0);
    rig.ask(open("one", "One"));
    rig.ask(open("two", "Two"));
    rig.frames(&mut host, 3);
    let one = host.surface_of(&view_key(&viewer(), "one")).unwrap();
    let two = host.surface_of(&view_key(&viewer(), "two")).unwrap();
    // Sue's own title bar: its close button is the last at the right.
    let width = host.info(two).unwrap().width;
    let close = pos2(width - 20.0, crate::widgets::TITLE_BAR / 2.0);
    click(&mut host, &mut rig, two, close);
    assert_eq!(rig.shell.views(), vec![view_key(&viewer(), "one")]);
    assert_eq!(rig.heard.lock().unwrap().closed, vec!["two".to_owned()]);
    assert!(host.info(two).is_none(), "its window closed");
    assert!(host.info(one).is_some(), "the other stays");
    // The main window and the first view still draw.
    let pages = rig.heard.lock().unwrap().pages;
    rig.heard.lock().unwrap().drawn.clear();
    rig.frames(&mut host, 2);
    let heard = rig.heard.lock().unwrap();
    assert!(heard.pages > pages);
    assert!(heard.drawn.iter().all(|(view, _)| view == "one"));
    assert!(!heard.drawn.is_empty());
}

#[test]
fn the_system_closing_a_views_window_tells_its_applet() {
    let mut rig = Rig::new(WindowSupport::Many, FormFactor::Desktop);
    rig.ask(open("canvas", "Canvas"));
    let key = view_key(&viewer(), "canvas");
    // The host hears the system's close button and tells the shell.
    rig.with(|shell, io| shell.close_view(&key, io, Instant::now()));
    assert!(rig.shell.os_windows().is_empty());
    assert_eq!(rig.heard.lock().unwrap().closed, vec!["canvas".to_owned()]);
    // So does a request; a view not open is let be.
    rig.ask(open("canvas", "Canvas"));
    rig.ask(ShellRequest::CloseView {
        applet: viewer(),
        view: "canvas".to_owned(),
    });
    rig.ask(ShellRequest::CloseView {
        applet: viewer(),
        view: "never".to_owned(),
    });
    assert!(rig.shell.views().is_empty());
    assert_eq!(rig.heard.lock().unwrap().closed.len(), 2);
}

#[test]
fn asking_again_for_an_open_view_brings_it_forward() {
    let mut rig = Rig::new(WindowSupport::Many, FormFactor::Desktop);
    rig.ask(open("a", "A"));
    rig.ask(open("b", "B"));
    rig.ask(open("a", "A, renamed"));
    let a = view_key(&viewer(), "a");
    assert_eq!(rig.shell.views(), vec![view_key(&viewer(), "b"), a.clone()]);
    let specs = rig.shell.os_windows();
    assert_eq!(specs.len(), 2, "one window a view");
    assert_eq!(specs[1].title, "A, renamed");
    // The next main frame asks the system to bring it to the front.
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, SCREEN, 1.0);
    host.sync(&specs, 1.0);
    let output =
        rig.with(|shell, io| host.main_frame(0, &specs, |ui| shell.ui(ui, io, Instant::now())));
    let asked = output
        .viewport_output
        .get(&sue_platform::windows::viewport_of(&a))
        .map(|viewport| viewport.commands.clone())
        .unwrap_or_default();
    assert!(asked.contains(&egui::ViewportCommand::Focus), "{asked:?}");
}

#[test]
fn where_the_system_has_one_window_a_view_floats_over_the_page() {
    let mut rig = Rig::new(WindowSupport::One, FormFactor::Desktop);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, SCREEN, 1.0);
    rig.ask(open("canvas", "Canvas"));
    assert!(rig.shell.os_windows().is_empty(), "no window of its own");
    rig.frames(&mut host, 3);
    assert!(host.windows().is_empty());
    let key = view_key(&viewer(), "canvas");
    let (frame, close) = rig.shell.view_rects(&key).unwrap();
    let page = rig.shell.page_rect();
    assert!(page.contains_rect(frame), "{frame:?} on {page:?}");
    assert!(frame.width() < page.width(), "a frame, not the page");
    assert!(frame.contains_rect(close));
    {
        let heard = rig.heard.lock().unwrap();
        let (view, room) = heard.drawn.last().cloned().unwrap();
        assert_eq!(view, "canvas");
        assert!(frame.contains_rect(room));
        assert!(room.top() >= frame.top() + crate::widgets::TITLE_BAR - 0.5);
    }
    // A click on it reaches the view, not the page under it.
    click(&mut host, &mut rig, MAIN_SURFACE, frame.center());
    assert_eq!(rig.heard.lock().unwrap().clicked, vec!["canvas".to_owned()]);
    // Dragging its title strip moves it.
    let grip = pos2(frame.left() + 40.0, frame.top() + 8.0);
    let to = grip + vec2(-120.0, 60.0);
    let press = |pressed, at| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut steps = vec![Event::PointerMoved(grip), press(true, grip)];
    for step in 1..=6 {
        steps.push(Event::PointerMoved(
            grip + (to - grip) * (step as f32 / 6.0),
        ));
    }
    steps.push(press(false, to));
    for event in steps {
        host.inject(MAIN_SURFACE, event).unwrap();
        rig.frame(&mut host);
    }
    rig.frames(&mut host, 2);
    let (moved, close) = rig.shell.view_rects(&key).unwrap();
    assert!(
        (moved.left() - (frame.left() - 120.0)).abs() < 2.0,
        "{moved:?}"
    );
    assert!(
        (moved.top() - (frame.top() + 60.0)).abs() < 2.0,
        "{moved:?}"
    );
    // Its close button closes it and tells the applet.
    click(&mut host, &mut rig, MAIN_SURFACE, close.center());
    assert!(rig.shell.views().is_empty());
    assert_eq!(rig.heard.lock().unwrap().closed, vec!["canvas".to_owned()]);
    assert!(rig.shell.view_rects(&key).is_none());
}

#[test]
fn a_floating_view_keeps_its_title_strip_on_the_page() {
    let mut rig = Rig::new(WindowSupport::One, FormFactor::Desktop);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, SCREEN, 1.0);
    rig.ask(open("canvas", "Canvas"));
    rig.frames(&mut host, 2);
    let key = view_key(&viewer(), "canvas");
    let (frame, _) = rig.shell.view_rects(&key).unwrap();
    // Dragged far past the bottom right corner.
    let grip = pos2(frame.left() + 40.0, frame.top() + 8.0);
    let to = grip + vec2(4000.0, 4000.0);
    for event in [
        Event::PointerMoved(grip),
        Event::PointerButton {
            pos: grip,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        Event::PointerMoved(grip + vec2(10.0, 10.0)),
        Event::PointerMoved(to),
        Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ] {
        host.inject(MAIN_SURFACE, event).unwrap();
        rig.frame(&mut host);
    }
    rig.frames(&mut host, 2);
    let (moved, _) = rig.shell.view_rects(&key).unwrap();
    let page = rig.shell.page_rect();
    assert!(moved.right() <= page.right() + 0.5, "{moved:?}");
    assert!(
        moved.top() + crate::widgets::TITLE_BAR <= page.bottom() + 0.5,
        "{moved:?}"
    );
}

#[test]
fn on_a_phone_a_floating_view_fills_the_page() {
    let mut rig = Rig::new(WindowSupport::One, FormFactor::Mobile);
    let ctx = egui::Context::default();
    let mut host = HeadlessWindows::new(&ctx, [412.0, 900.0], 1.0);
    rig.ask(open("canvas", "Canvas"));
    rig.frames(&mut host, 3);
    let (frame, _) = rig
        .shell
        .view_rects(&view_key(&viewer(), "canvas"))
        .unwrap();
    let page = rig.shell.page_rect();
    assert!((frame.width() - page.width()).abs() < 0.5);
    assert!((frame.height() - page.height()).abs() < 0.5);
}
