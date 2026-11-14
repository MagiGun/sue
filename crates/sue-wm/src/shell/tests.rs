//! Tests of the shell.

use std::sync::{Arc, Mutex};

use sue_core::Arena;
use sue_platform::NoTextures;
use sue_rice::{RiceDeclaration, RiceParameter};

use super::*;
use crate::{AppletManifest, Launchpad, SueApplet};

/// A studio that wears Paper and notes the widget fill its page and its
/// menus are drawn with.
struct Probe(Arc<Mutex<Vec<egui::Color32>>>);

impl SueApplet for Probe {
    fn manifest(&self) -> AppletManifest {
        AppletManifest::new("probe", "Probe", AppletRole::Studio, "*")
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![RiceDeclaration::new(
            "probe.theme",
            RiceParameter::Text {
                value: "paper".to_owned(),
            },
            "The probe's colors",
        )]
    }

    fn page_ui(&mut self, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
        self.0
            .lock()
            .unwrap()
            .push(ui.visuals().widgets.inactive.bg_fill);
    }

    fn bar_ui(&mut self, ui: &mut Ui, _cx: &mut AppletContext<'_>) {
        self.0
            .lock()
            .unwrap()
            .push(ui.visuals().widgets.inactive.bg_fill);
    }
}

#[test]
fn a_page_and_its_menus_wear_their_own_theme() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut launchpad = Launchpad::new();
    let probe = Arc::clone(&seen);
    launchpad
        .register(move || Box::new(Probe(Arc::clone(&probe))))
        .unwrap();
    let mut rice = RiceSchema::new();
    crate::register_rice(&mut rice).unwrap();
    rice.register_all(&launchpad.rice_declarations()).unwrap();
    let mut world = World {
        arena: Arena::new(),
        rice,
    };
    let mut history = UndoStack::new();
    let mut typing = Typing::default();
    let mut applets = launchpad.instantiate();
    let catalog = Catalog::default();
    let mut transport = Transport::default();
    let mut requests = Vec::new();
    let work = WorkList::default();
    let mut transfers = crate::Transfers::default();
    let theme = Theme::from_rice(&world.rice);
    let mut shell = Shell::new(
        WmConfig::from_rice(&world.rice).unwrap(),
        theme.clone(),
        HardwareProfile::Balanced,
        60,
        FormFactor::Desktop,
    );
    shell.open(AppletId::new("probe").unwrap(), &applets);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        let mut io = ShellIo {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            applets: &mut applets,
            catalog: &catalog,
            textures: &mut NoTextures,
            audio: None,
            transport: &mut transport,
            requests: &mut requests,
            work: &work,
            transfers: &mut transfers,
            fps: None,
        };
        ctx.run_ui(egui::RawInput::default(), |ui| {
            shell.ui(ui, &mut io, Instant::now());
        })
        .drop_without_applying_deltas();
    }
    // Sue wears its dark default; the page and its menus wear Paper,
    // whose fields are white.
    let paper = sue_rice::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == "paper")
        .map(|preset| theme.with_preset(&preset))
        .unwrap();
    assert_ne!(paper.pill, theme.pill);
    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 4, "{seen:?}");
    assert!(seen.iter().all(|fill| *fill == paper.pill), "{seen:?}");
    // Sue's own style is back for the Hub, the status bar and windows.
    assert_eq!(
        ctx.global_style().visuals.widgets.inactive.bg_fill,
        theme.pill
    );
}

/// A studio that notes the layout it is handed and the room it has.
struct Layouts(Arc<Mutex<Vec<(FormFactor, f32)>>>);

impl SueApplet for Layouts {
    fn manifest(&self) -> AppletManifest {
        AppletManifest::new("layouts", "Layouts", AppletRole::Studio, "*")
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.0
            .lock()
            .unwrap()
            .push((cx.view.form_factor, ui.max_rect().width()));
    }
}

/// Everything a shell draws with, for a few frames at a time.
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
    ctx: egui::Context,
}

impl Rig {
    fn new(launchpad: &Launchpad) -> Self {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        rice.register_all(&launchpad.rice_declarations()).unwrap();
        Self {
            world: World {
                arena: Arena::new(),
                rice,
            },
            history: UndoStack::new(),
            typing: Typing::default(),
            applets: launchpad.instantiate(),
            catalog: Catalog::default(),
            transport: Transport::default(),
            requests: Vec::new(),
            work: WorkList::default(),
            transfers: crate::Transfers::default(),
            ctx: egui::Context::default(),
        }
    }

    fn with<R>(&mut self, work: impl FnOnce(&mut ShellIo<'_>) -> R) -> R {
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
        work(&mut io)
    }

    fn frames(&mut self, shell: &mut Shell) {
        let ctx = self.ctx.clone();
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))),
                ..egui::RawInput::default()
            };
            self.with(|io| {
                ctx.run_ui(raw, |ui| shell.ui(ui, io, Instant::now()))
                    .drop_without_applying_deltas();
            });
        }
    }

    fn pick(&mut self, shell: &mut Shell, path: &[&str]) {
        self.with(|io| {
            shell.open_sue_menu(io);
            assert!(shell.sue_menu_pick(path, io, Instant::now()), "{path:?}");
        });
        self.frames(shell);
    }
}

#[test]
fn the_sue_menus_layout_changes_what_every_studio_draws() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut launchpad = Launchpad::new();
    let probe = Arc::clone(&seen);
    launchpad
        .register(move || Box::new(Layouts(Arc::clone(&probe))))
        .unwrap();
    let mut rig = Rig::new(&launchpad);
    let mut shell = Shell::new(
        WmConfig::from_rice(&rig.world.rice).unwrap(),
        Theme::from_rice(&rig.world.rice),
        HardwareProfile::Balanced,
        60,
        FormFactor::Desktop,
    );
    shell.open(AppletId::new("layouts").unwrap(), &rig.applets);
    let last = |seen: &Arc<Mutex<Vec<(FormFactor, f32)>>>| *seen.lock().unwrap().last().unwrap();
    rig.frames(&mut shell);
    assert_eq!(last(&seen), (FormFactor::Desktop, 1440.0));

    // A computer offers every layout, Automatic naming its own.
    rig.with(|io| shell.open_sue_menu(io));
    assert_eq!(
        shell.sue_menu_labels(&["Layout"]),
        ["Automatic (Desktop)", "Desktop", "Tablet", "Phone"]
    );
    rig.pick(&mut shell, &["Layout", "Phone"]);
    assert_eq!(rig.world.rice.get_str(INTERFACE_KEY), Some("phone"));
    assert_eq!(shell.form_factor(), FormFactor::Mobile);
    assert_eq!(shell.device_form_factor(), FormFactor::Desktop);
    assert_eq!(last(&seen), (FormFactor::Mobile, 1440.0));

    // Phone shaped keeps the whole app to a phone-wide column.
    rig.pick(&mut shell, &["Layout", "Phone Shaped"]);
    assert_eq!(last(&seen), (FormFactor::Mobile, PHONE_FRAME_WIDTH));
    assert!(
        shell.sue_button().left() > 500.0,
        "{:?}",
        shell.sue_button()
    );
    rig.pick(&mut shell, &["Layout", "Tablet"]);
    assert_eq!(last(&seen), (FormFactor::Tablet, 1440.0));

    // One undo step each, like any other change.
    rig.with(|io| shell.undo(io, false));
    rig.frames(&mut shell);
    assert_eq!(rig.world.rice.get_str(INTERFACE_KEY), Some("phone"));
    rig.pick(&mut shell, &["Layout", "Automatic (Desktop)"]);
    assert_eq!(last(&seen), (FormFactor::Desktop, 1440.0));

    // A phone or tablet switches only between its two.
    let mut phone = Shell::new(
        WmConfig::from_rice(&rig.world.rice).unwrap(),
        Theme::from_rice(&rig.world.rice),
        HardwareProfile::Balanced,
        60,
        FormFactor::Mobile,
    );
    rig.with(|io| phone.open_sue_menu(io));
    assert_eq!(
        phone.sue_menu_labels(&["Layout"]),
        ["Automatic (Phone)", "Tablet", "Phone"]
    );
}
