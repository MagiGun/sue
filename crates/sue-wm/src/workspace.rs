//! Tabs, docked applets, focus and the global keymap, tied to the scheduler.

use std::fmt;

use sue_core::AssetHandle;
use sue_platform::{FileDrop, FormFactor, InputEvent, SafeAreaInsets, ScreenEdge, TouchGesture};
use sue_rice::HardwareProfile;

use crate::{
    AppletContext, AppletId, AppletManifest, BootTarget, FrameScheduler, FrameTicket,
    InputResponse, Launchpad, LayoutRequest, LayoutSolution, MediaClass, Rect, RenderDemand,
    ShellRequest, SpringDamper, SpringMode, SueApplet, Ui, WmConfig, WmError, World, Zone,
    solve_layout,
};

/// One Center Stage tab: the Hub, or a mounted applet.
pub enum Tab {
    /// The Launchpad Hub.
    Hub,
    /// A mounted applet.
    Applet {
        /// Which applet.
        id: AppletId,
        /// The live instance.
        applet: Box<dyn SueApplet>,
    },
}

impl Tab {
    /// The applet's id, or `None` for the Hub.
    #[must_use]
    pub fn applet_id(&self) -> Option<&AppletId> {
        match self {
            Self::Hub => None,
            Self::Applet { id, .. } => Some(id),
        }
    }
}

impl fmt::Debug for Tab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hub => f.write_str("Hub"),
            Self::Applet { id, .. } => write!(f, "Applet({id})"),
        }
    }
}

/// Which applet receives keyboard input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The active Center Stage tab.
    Center,
    /// The docked applet at this index (see [`Workspace::docked`]).
    Dock(usize),
}

struct Docked {
    zone: Zone,
    id: AppletId,
    applet: Box<dyn SueApplet>,
}

/// The running shell: tabs, docked applets, the two dock springs, the global
/// keymap and the frame scheduler.
pub struct Workspace {
    launchpad: Launchpad,
    config: WmConfig,
    profile: HardwareProfile,
    tabs: Vec<Tab>,
    active: usize,
    docks: Vec<Docked>,
    focus: Focus,
    left_open: bool,
    right_open: bool,
    left_spring: SpringDamper,
    right_spring: SpringDamper,
    scheduler: FrameScheduler,
}

impl fmt::Debug for Workspace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Workspace")
            .field("tabs", &self.tabs)
            .field("active", &self.active)
            .field(
                "docks",
                &self
                    .docks
                    .iter()
                    .map(|dock| dock.id.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("focus", &self.focus)
            .finish_non_exhaustive()
    }
}

impl Workspace {
    /// A workspace with one Hub tab. Docks start open on desktops and closed
    /// on tablets and phones.
    #[must_use]
    pub fn new(
        launchpad: Launchpad,
        config: WmConfig,
        profile: HardwareProfile,
        display_hz: u32,
        form_factor: FormFactor,
    ) -> Self {
        let open = form_factor == FormFactor::Desktop;
        let reveal = if open { 1.0 } else { 0.0 };
        let mode = SpringMode::for_profile(profile, config.instant_transitions);
        let spring = SpringDamper::new(config.spring, mode, reveal);
        let mut scheduler = FrameScheduler::new(profile, display_hz);
        scheduler.invalidate_all();
        let mut workspace = Self {
            launchpad,
            config,
            profile,
            tabs: vec![Tab::Hub],
            active: 0,
            docks: Vec::new(),
            focus: Focus::Center,
            left_open: open,
            right_open: open,
            left_spring: spring,
            right_spring: spring,
            scheduler,
        };
        workspace.sync_pin();
        workspace
    }

    /// The applet registry.
    #[must_use]
    pub fn launchpad(&self) -> &Launchpad {
        &self.launchpad
    }

    /// The current window-manager config.
    #[must_use]
    pub fn config(&self) -> &WmConfig {
        &self.config
    }

    /// The active hardware profile.
    #[must_use]
    pub fn profile(&self) -> HardwareProfile {
        self.profile
    }

    fn sync_pin(&mut self) {
        let pinned = AppletId::new(&self.config.boot_into)
            .ok()
            .filter(|id| self.launchpad.manifest(id).is_some());
        match pinned {
            Some(id) => {
                // The id was just checked against the launchpad.
                let _ = self.launchpad.pin_boot(&id);
            }
            None => self.launchpad.unpin_boot(),
        }
    }

    /// Applies a new config (after a rice edit, undo or redo): metrics, springs,
    /// keymap and the pinned boot applet.
    pub fn apply_config(&mut self, config: WmConfig) {
        let mode = SpringMode::for_profile(self.profile, config.instant_transitions);
        for spring in [&mut self.left_spring, &mut self.right_spring] {
            let (position, target) = (spring.position(), spring.target());
            *spring = SpringDamper::new(config.spring, mode, position);
            spring.set_target(target);
        }
        self.config = config;
        self.sync_pin();
        self.scheduler.invalidate_all();
    }

    /// Mounts every dock applet, then opens the boot target (the pinned
    /// applet, or the Hub) in the first tab.
    ///
    /// # Errors
    /// If an applet fails to mount.
    pub fn boot(&mut self, ctx: &mut AppletContext<'_>) -> Result<BootTarget, WmError> {
        let docked: Vec<AppletManifest> = self
            .launchpad
            .manifests()
            .filter(|manifest| matches!(manifest.home_zone, Zone::LeftDock | Zone::RightDock))
            .cloned()
            .collect();
        for manifest in docked {
            let mut applet = self.launchpad.instantiate(&manifest.id)?;
            applet.mount(ctx)?;
            self.docks.push(Docked {
                zone: manifest.home_zone,
                id: manifest.id,
                applet,
            });
        }
        let target = self.launchpad.boot_target();
        if let BootTarget::Applet(id) = &target {
            self.open_applet(id, ctx)?;
        }
        self.process_requests(ctx)?;
        Ok(target)
    }

    /// Opens a new tab on the Hub and makes it active. Returns its index.
    pub fn new_tab(&mut self) -> usize {
        self.tabs.push(Tab::Hub);
        self.active = self.tabs.len() - 1;
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
        self.active
    }

    /// Makes tab `index` active.
    pub fn activate_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.focus = Focus::Center;
            self.scheduler.invalidate_all();
        }
    }

    /// Closes tab `index`; the last tab turns back into the Hub.
    pub fn close_tab(&mut self, index: usize, ctx: &mut AppletContext<'_>) {
        if index >= self.tabs.len() {
            return;
        }
        let closed = if self.tabs.len() == 1 {
            std::mem::replace(&mut self.tabs[0], Tab::Hub)
        } else {
            self.tabs.remove(index)
        };
        if let Tab::Applet { mut applet, .. } = closed {
            applet.unmount(ctx);
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.scheduler.invalidate_all();
    }

    /// Mounts `id` in the active tab, unmounting whatever was there.
    ///
    /// # Errors
    /// [`WmError::UnknownApplet`], or the applet's mount error.
    pub fn open_applet(
        &mut self,
        id: &AppletId,
        ctx: &mut AppletContext<'_>,
    ) -> Result<(), WmError> {
        let mut applet = self.launchpad.instantiate(id)?;
        applet.mount(ctx)?;
        let previous = std::mem::replace(
            &mut self.tabs[self.active],
            Tab::Applet {
                id: id.clone(),
                applet,
            },
        );
        if let Tab::Applet {
            applet: mut old, ..
        } = previous
        {
            old.unmount(ctx);
        }
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
        Ok(())
    }

    /// Opens `handle` in the first Center Stage applet whose manifest opens
    /// `class`: in the active tab if it already shows that applet or the Hub,
    /// otherwise in a new tab.
    ///
    /// # Errors
    /// [`WmError::NothingOpens`] if no installed applet opens `class`, or the
    /// applet's own error.
    pub fn open_asset(
        &mut self,
        handle: AssetHandle,
        class: MediaClass,
        ctx: &mut AppletContext<'_>,
    ) -> Result<(), WmError> {
        let id = self
            .launchpad
            .manifests()
            .find(|manifest| {
                manifest.home_zone == Zone::CenterStage && manifest.opens.contains(&class)
            })
            .map(|manifest| manifest.id.clone())
            .ok_or(WmError::NothingOpens(class))?;
        let reuse = match &self.tabs[self.active] {
            Tab::Hub => false,
            Tab::Applet { id: current, .. } => current == &id,
        };
        if !reuse {
            if self.tabs[self.active].applet_id().is_some() {
                self.new_tab();
            }
            self.open_applet(&id, ctx)?;
        }
        if let Tab::Applet { applet, .. } = &mut self.tabs[self.active] {
            applet.open(handle, ctx)?;
        }
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
        Ok(())
    }

    /// Carries out the requests applets queued in `ctx`.
    ///
    /// # Errors
    /// The first request that fails.
    pub fn process_requests(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), WmError> {
        let requests = std::mem::take(ctx.requests);
        for request in requests {
            match request {
                ShellRequest::OpenAsset { handle, class } => self.open_asset(handle, class, ctx)?,
                ShellRequest::OpenApplet(id) => self.open_applet(&id, ctx)?,
            }
        }
        Ok(())
    }

    /// The tabs, in order.
    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Index of the active tab.
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab.
    #[must_use]
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// A short title for tab `index`.
    #[must_use]
    pub fn tab_title(&self, index: usize, world: &World) -> String {
        match self.tabs.get(index) {
            Some(Tab::Applet { id, applet }) => applet.title(world).unwrap_or_else(|| {
                self.launchpad
                    .manifest(id)
                    .map_or_else(|| id.to_string(), |manifest| manifest.name.clone())
            }),
            Some(Tab::Hub) | None => "Launchpad".to_owned(),
        }
    }

    /// Docked applets as `(index, zone, manifest)`.
    pub fn docked(&self) -> impl Iterator<Item = (usize, Zone, Option<&AppletManifest>)> {
        self.docks
            .iter()
            .enumerate()
            .map(|(index, dock)| (index, dock.zone, self.launchpad.manifest(&dock.id)))
    }

    /// Draws docked applet `index` into `area`.
    pub fn draw_dock(
        &mut self,
        index: usize,
        ui: &mut Ui<'_>,
        area: Rect,
        ctx: &mut AppletContext<'_>,
    ) {
        ctx.focused = self.focus == Focus::Dock(index);
        if let Some(dock) = self.docks.get_mut(index) {
            dock.applet.draw(ui, area, ctx);
        }
        ctx.focused = false;
    }

    /// Draws the active tab's applet into `area`. Returns `false` for the Hub.
    pub fn draw_active(
        &mut self,
        ui: &mut Ui<'_>,
        area: Rect,
        ctx: &mut AppletContext<'_>,
    ) -> bool {
        ctx.focused = self.focus == Focus::Center;
        let drawn = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => {
                applet.draw(ui, area, ctx);
                true
            }
            Tab::Hub => false,
        };
        ctx.focused = false;
        drawn
    }

    /// Which applet receives keyboard input.
    #[must_use]
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// Moves keyboard focus.
    pub fn set_focus(&mut self, focus: Focus) {
        if self.focus != focus {
            self.focus = focus;
            self.scheduler.invalidate_all();
        }
    }

    /// `true` if the active applet edits time-based media, which shows the
    /// timeline.
    #[must_use]
    pub fn timeline_active(&self) -> bool {
        self.active_tab()
            .applet_id()
            .and_then(|id| self.launchpad.manifest(id))
            .is_some_and(|manifest| manifest.uses_timeline)
    }

    /// Opens or closes the left dock.
    pub fn set_left_dock(&mut self, open: bool) {
        self.left_open = open;
        self.left_spring.set_target(if open { 1.0 } else { 0.0 });
        self.scheduler.invalidate_all();
    }

    /// Opens or closes the right dock.
    pub fn set_right_dock(&mut self, open: bool) {
        self.right_open = open;
        self.right_spring.set_target(if open { 1.0 } else { 0.0 });
        self.scheduler.invalidate_all();
    }

    /// Whether each dock is open (`(left, right)`), regardless of animation.
    #[must_use]
    pub fn docks_open(&self) -> (bool, bool) {
        (self.left_open, self.right_open)
    }

    /// How far each dock is revealed right now (`(left, right)`, `0.0..=1.0`).
    #[must_use]
    pub fn dock_reveal(&self) -> (f32, f32) {
        (self.left_spring.position(), self.right_spring.position())
    }

    /// Switches the hardware profile, e.g. when the device enters battery saver.
    pub fn set_profile(&mut self, profile: HardwareProfile) {
        self.profile = profile;
        let mode = SpringMode::for_profile(profile, self.config.instant_transitions);
        self.left_spring.set_mode(mode);
        self.right_spring.set_mode(mode);
        self.scheduler.set_profile(profile);
        self.scheduler.invalidate_all();
    }

    /// Asks for a full redraw.
    pub fn invalidate(&mut self) {
        self.scheduler.invalidate_all();
    }

    fn focused_applet(&mut self) -> Option<&mut Box<dyn SueApplet>> {
        match self.focus {
            Focus::Center => match &mut self.tabs[self.active] {
                Tab::Applet { applet, .. } => Some(applet),
                Tab::Hub => None,
            },
            Focus::Dock(index) => self.docks.get_mut(index).map(|dock| &mut dock.applet),
        }
    }

    /// Routes one input event: global shortcuts and edge swipes first, dropped
    /// files to applets that accept drops, everything else to the focused
    /// applet. Requests queued by applets are carried out before returning.
    ///
    /// # Errors
    /// Undo/redo failures, or an applet's error.
    pub fn dispatch(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, WmError> {
        let response = self.route(event, ctx)?;
        self.process_requests(ctx)?;
        if response == (InputResponse::Consumed { redraw: true }) {
            self.scheduler.invalidate_all();
        }
        Ok(response)
    }

    fn route(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, WmError> {
        let consumed = InputResponse::Consumed { redraw: true };
        match event {
            InputEvent::Keyboard(key) => {
                let keymap = self.config.keymap;
                if keymap.toggle_left_dock.matches(key) {
                    self.set_left_dock(!self.left_open);
                    return Ok(consumed);
                }
                if keymap.toggle_right_dock.matches(key) {
                    self.set_right_dock(!self.right_open);
                    return Ok(consumed);
                }
                if keymap.new_tab.matches(key) {
                    self.new_tab();
                    return Ok(consumed);
                }
                if keymap.redo.matches(key) || keymap.redo_alt.matches(key) {
                    let changed = ctx.history.redo(ctx.world)?;
                    return Ok(InputResponse::Consumed { redraw: changed });
                }
                if keymap.undo.matches(key) {
                    let changed = ctx.history.undo(ctx.world)?;
                    return Ok(InputResponse::Consumed { redraw: changed });
                }
            }
            InputEvent::TouchGesture(TouchGesture::EdgeSwipe { edge, progress }) => {
                let open = *progress >= 0.5;
                match edge {
                    ScreenEdge::Left => self.set_left_dock(open),
                    ScreenEdge::Right => self.set_right_dock(open),
                    ScreenEdge::Top | ScreenEdge::Bottom => return Ok(InputResponse::Ignored),
                }
                return Ok(consumed);
            }
            InputEvent::FileDrop(FileDrop::Dropped(_)) => {
                let mut response = InputResponse::Ignored;
                for dock in &mut self.docks {
                    let accepts = self
                        .launchpad
                        .manifest(&dock.id)
                        .is_some_and(|manifest| manifest.accepts_drops);
                    if accepts && dock.applet.handle_input(event, ctx)? != InputResponse::Ignored {
                        response = consumed;
                    }
                }
                return Ok(response);
            }
            _ => {}
        }
        ctx.focused = true;
        let response = match self.focused_applet() {
            Some(applet) => applet.handle_input(event, ctx),
            None => Ok(InputResponse::Ignored),
        };
        ctx.focused = false;
        Ok(response?)
    }

    /// Advances springs and applets by `dt` seconds and returns what the host
    /// should do next.
    pub fn update(&mut self, dt: f32, ctx: &mut AppletContext<'_>) -> RenderDemand {
        let left_moving = self.left_spring.step(dt);
        let right_moving = self.right_spring.step(dt);
        let mut applets = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.update(dt, ctx),
            Tab::Hub => RenderDemand::Sleep,
        };
        for dock in &mut self.docks {
            applets = applets.combine(dock.applet.update(dt, ctx));
        }
        self.scheduler
            .set_animating(left_moving || right_moving || !applets.is_sleep());
        self.scheduler.demand().combine(applets)
    }

    /// Starts a frame if one is needed (see [`FrameScheduler::begin_frame`]).
    pub fn begin_frame(&mut self) -> Option<FrameTicket> {
        self.scheduler.begin_frame()
    }

    /// The scheduler, for diagnostics.
    #[must_use]
    pub fn scheduler(&self) -> &FrameScheduler {
        &self.scheduler
    }

    /// The layout for the current dock animation and active applet.
    #[must_use]
    pub fn layout(
        &self,
        viewport: Rect,
        safe_area: SafeAreaInsets,
        form_factor: FormFactor,
    ) -> LayoutSolution {
        let (left_reveal, right_reveal) = self.dock_reveal();
        solve_layout(&LayoutRequest {
            viewport,
            safe_area,
            form_factor,
            left_reveal,
            right_reveal,
            timeline_active: self.timeline_active(),
            metrics: self.config.metrics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launchpad::tests::fixture;
    use crate::{AppletCategory, AppletError, Placement, register_rice};
    use sue_core::{StringBuffer, UndoStack, txn::InsertText};
    use sue_platform::{Key, KeyEvent, Modifiers};

    /// Types characters into a note, through the undo tree.
    #[derive(Default)]
    struct Typist {
        note: Option<AssetHandle>,
    }

    impl SueApplet for Typist {
        fn manifest(&self) -> AppletManifest {
            AppletManifest {
                id: AppletId::new("typist").unwrap(),
                name: "Typist".into(),
                category: AppletCategory::Utility,
                home_zone: Zone::CenterStage,
                uses_timeline: false,
                summary: String::new(),
                opens: vec![MediaClass::Text],
                accepts_drops: false,
            }
        }

        fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
            self.note = Some(ctx.world.arena.spawn("note", StringBuffer::new()));
            Ok(())
        }

        fn open(
            &mut self,
            asset: AssetHandle,
            _ctx: &mut AppletContext<'_>,
        ) -> Result<(), AppletError> {
            self.note = Some(asset);
            Ok(())
        }

        fn handle_input(
            &mut self,
            event: &InputEvent,
            ctx: &mut AppletContext<'_>,
        ) -> Result<InputResponse, AppletError> {
            let (
                Some(note),
                InputEvent::Keyboard(KeyEvent {
                    text: Some(text),
                    modifiers,
                    ..
                }),
            ) = (self.note, event)
            else {
                return Ok(InputResponse::Ignored);
            };
            if modifiers.is_command() {
                return Ok(InputResponse::Ignored);
            }
            let end = ctx
                .arena()
                .get(note)?
                .string_buffer()
                .map_or(0, StringBuffer::len);
            ctx.edit_arena(InsertText::new(note, end, text.clone()))?;
            Ok(InputResponse::Consumed { redraw: true })
        }
    }

    struct Engine {
        world: World,
        history: UndoStack<World>,
        requests: Vec<ShellRequest>,
    }

    impl Engine {
        fn new() -> Self {
            let mut world = World::default();
            register_rice(&mut world.rice).unwrap();
            Self {
                world,
                history: UndoStack::new(),
                requests: Vec::new(),
            }
        }

        fn ctx(&mut self, profile: HardwareProfile) -> AppletContext<'_> {
            AppletContext {
                world: &mut self.world,
                history: &mut self.history,
                form_factor: FormFactor::Desktop,
                profile,
                requests: &mut self.requests,
                focused: false,
            }
        }

        fn workspace(&self, profile: HardwareProfile, form_factor: FormFactor) -> Workspace {
            let mut launchpad = Launchpad::new();
            launchpad.register(|| Box::new(Typist::default())).unwrap();
            launchpad
                .register(fixture("video", "Video", AppletCategory::Studio, true))
                .unwrap();
            launchpad
                .pin_boot(&AppletId::new("typist").unwrap())
                .unwrap();
            let mut config = WmConfig::from_rice(&self.world.rice).unwrap();
            config.boot_into = "typist".to_owned();
            Workspace::new(launchpad, config, profile, 60, form_factor)
        }
    }

    fn text_of(engine: &Engine, name: &str) -> String {
        engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == name)
            .and_then(|(_, entity)| entity.string_buffer().map(|text| text.as_str().to_owned()))
            .unwrap_or_default()
    }

    #[test]
    fn boots_into_the_pinned_applet_and_routes_typing_and_undo() {
        let mut engine = Engine::new();
        let mut workspace = engine.workspace(HardwareProfile::Balanced, FormFactor::Desktop);
        let profile = HardwareProfile::Balanced;
        let target = workspace.boot(&mut engine.ctx(profile)).unwrap();
        assert_eq!(target, BootTarget::Applet(AppletId::new("typist").unwrap()));
        for ch in "Sue".chars() {
            workspace
                .dispatch(
                    &InputEvent::Keyboard(KeyEvent::typed(ch)),
                    &mut engine.ctx(profile),
                )
                .unwrap();
        }
        assert_eq!(text_of(&engine, "note"), "Sue");
        let undo = InputEvent::Keyboard(KeyEvent::press(Key::Character('Z'), Modifiers::CTRL));
        workspace.dispatch(&undo, &mut engine.ctx(profile)).unwrap();
        assert_eq!(text_of(&engine, "note"), "Su");
        let redo = InputEvent::Keyboard(KeyEvent::press(Key::Character('Y'), Modifiers::CTRL));
        workspace.dispatch(&redo, &mut engine.ctx(profile)).unwrap();
        assert_eq!(text_of(&engine, "note"), "Sue");
    }

    #[test]
    fn dock_shortcuts_animate_with_springs_and_then_sleep() {
        let mut engine = Engine::new();
        let mut workspace = engine.workspace(HardwareProfile::Balanced, FormFactor::Desktop);
        let profile = HardwareProfile::Balanced;
        let toggle = InputEvent::Keyboard(KeyEvent::press(
            Key::Character('b'),
            Modifiers {
                super_key: true,
                ..Modifiers::NONE
            },
        ));
        workspace
            .dispatch(&toggle, &mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.docks_open(), (false, true));
        let mut frames = 0;
        while workspace.update(1.0 / 60.0, &mut engine.ctx(profile)) != RenderDemand::Sleep {
            workspace.begin_frame();
            frames += 1;
            assert!(frames < 1_000, "dock never settled");
        }
        assert!(frames > 1, "a Balanced profile animates the dock");
        assert_eq!(workspace.dock_reveal(), (0.0, 1.0));
        assert!(workspace.begin_frame().is_none());
    }

    #[test]
    fn legacy_hardware_and_instant_transitions_snap() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::LegacyLowSpec;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.set_left_dock(false);
        assert_eq!(workspace.dock_reveal().0, 0.0);
        assert!(workspace.begin_frame().is_some());
        assert_eq!(
            workspace.update(1.0 / 30.0, &mut engine.ctx(profile)),
            RenderDemand::Sleep
        );

        let mut fast = engine.workspace(HardwareProfile::HighRefresh, FormFactor::Desktop);
        let mut config = fast.config().clone();
        config.instant_transitions = true;
        fast.apply_config(config);
        fast.set_right_dock(false);
        assert_eq!(fast.dock_reveal().1, 0.0);
    }

    #[test]
    fn phones_open_drawers_with_edge_swipes() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::MobileBatterySaver;
        let mut workspace = engine.workspace(profile, FormFactor::Mobile);
        assert_eq!(workspace.docks_open(), (false, false));
        let swipe = InputEvent::TouchGesture(TouchGesture::EdgeSwipe {
            edge: ScreenEdge::Left,
            progress: 0.8,
        });
        workspace
            .dispatch(&swipe, &mut engine.ctx(profile))
            .unwrap();
        let mut frames = 0;
        while !workspace
            .update(1.0 / 30.0, &mut engine.ctx(profile))
            .is_sleep()
        {
            workspace.begin_frame();
            frames += 1;
            assert!(frames < 1_000, "drawer never settled");
        }
        let layout = workspace.layout(
            Rect::new(0.0, 0.0, 390.0, 844.0),
            SafeAreaInsets::ZERO,
            FormFactor::Mobile,
        );
        assert!(matches!(layout.left, Placement::Drawer { reveal, .. } if reveal == 1.0));
        assert_eq!(layout.right, Placement::Hidden);
    }

    #[test]
    fn assets_open_in_the_applet_that_declares_their_class() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.boot(&mut engine.ctx(profile)).unwrap();
        let doc = engine
            .world
            .arena
            .spawn("doc.md", StringBuffer::from("# hi"));
        engine.requests.push(ShellRequest::OpenAsset {
            handle: doc,
            class: MediaClass::Text,
        });
        workspace
            .process_requests(&mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.tabs().len(), 1, "the typist tab is reused");
        workspace
            .dispatch(
                &InputEvent::Keyboard(KeyEvent::typed('!')),
                &mut engine.ctx(profile),
            )
            .unwrap();
        assert_eq!(text_of(&engine, "doc.md"), "# hi!");
        assert!(matches!(
            workspace.open_asset(doc, MediaClass::Image, &mut engine.ctx(profile)),
            Err(WmError::NothingOpens(MediaClass::Image))
        ));
    }

    #[test]
    fn new_tabs_open_the_hub_and_timelines_follow_the_applet() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.boot(&mut engine.ctx(profile)).unwrap();
        let new_tab = InputEvent::Keyboard(KeyEvent::press(Key::Character('t'), Modifiers::CTRL));
        workspace
            .dispatch(&new_tab, &mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.tabs().len(), 2);
        assert!(workspace.active_tab().applet_id().is_none());
        assert_eq!(workspace.tab_title(1, &engine.world), "Launchpad");
        workspace
            .open_applet(&AppletId::new("video").unwrap(), &mut engine.ctx(profile))
            .unwrap();
        assert!(workspace.timeline_active());
        let layout = workspace.layout(
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            SafeAreaInsets::ZERO,
            FormFactor::Desktop,
        );
        assert!(matches!(layout.bottom, Placement::Tiled(_)));
        workspace.close_tab(1, &mut engine.ctx(profile));
        assert_eq!((workspace.tabs().len(), workspace.active_index()), (1, 0));
        assert_eq!(workspace.launchpad().len(), 2);
    }
}
