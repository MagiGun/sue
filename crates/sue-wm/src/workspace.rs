//! Tabs, docks and the global keymap, tied to the scheduler.

use std::fmt;

use sue_platform::{FormFactor, InputEvent, SafeAreaInsets, ScreenEdge, TouchGesture};
use sue_rice::HardwareProfile;

use crate::{
    AppletContext, AppletId, BootTarget, FrameScheduler, FrameTicket, InputResponse, Launchpad,
    LayoutRequest, LayoutSolution, Rect, RenderDemand, SpringDamper, SpringMode, SueApplet,
    WmConfig, WmError, solve_layout,
};

/// One tab: the Hub, or a mounted applet.
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

/// The running shell: tabs, the two dock springs, the global keymap and the
/// frame scheduler.
pub struct Workspace {
    launchpad: Launchpad,
    config: WmConfig,
    tabs: Vec<Tab>,
    active: usize,
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
            .field("left_open", &self.left_open)
            .field("right_open", &self.right_open)
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
        let mode = SpringMode::for_profile(profile, config.animations);
        let spring = SpringDamper::new(config.spring, mode, reveal);
        let mut scheduler = FrameScheduler::new(profile, display_hz);
        scheduler.invalidate_all();
        Self {
            launchpad,
            config,
            tabs: vec![Tab::Hub],
            active: 0,
            left_open: open,
            right_open: open,
            left_spring: spring,
            right_spring: spring,
            scheduler,
        }
    }

    /// The applet registry.
    #[must_use]
    pub fn launchpad(&self) -> &Launchpad {
        &self.launchpad
    }

    /// Opens the boot target (the pinned applet, or the Hub) in the first tab.
    ///
    /// # Errors
    /// If the pinned applet fails to mount.
    pub fn boot(&mut self, ctx: &mut AppletContext<'_>) -> Result<BootTarget, WmError> {
        let target = self.launchpad.boot_target();
        if let BootTarget::Applet(id) = &target {
            self.open_applet(id, ctx)?;
        }
        Ok(target)
    }

    /// Opens a new tab on the Hub and makes it active. Returns its index.
    pub fn new_tab(&mut self) -> usize {
        self.tabs.push(Tab::Hub);
        self.active = self.tabs.len() - 1;
        self.scheduler.invalidate_all();
        self.active
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
        self.scheduler.invalidate_all();
        Ok(())
    }

    /// The tabs, in order.
    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// The active tab.
    #[must_use]
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
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
        let mode = SpringMode::for_profile(profile, self.config.animations);
        self.left_spring.set_mode(mode);
        self.right_spring.set_mode(mode);
        self.scheduler.set_profile(profile);
    }

    /// Routes one input event: global shortcuts and edge swipes first, then
    /// the active applet.
    ///
    /// # Errors
    /// Undo/redo failures, or the applet's own error.
    pub fn dispatch(
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
                if keymap.redo.matches(key) {
                    let changed = ctx.history.redo(ctx.arena)?;
                    self.scheduler.invalidate_all();
                    return Ok(InputResponse::Consumed { redraw: changed });
                }
                if keymap.undo.matches(key) {
                    let changed = ctx.history.undo(ctx.arena)?;
                    self.scheduler.invalidate_all();
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
            _ => {}
        }
        let response = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.handle_input(event, ctx)?,
            Tab::Hub => InputResponse::Ignored,
        };
        if response == consumed {
            self.scheduler.invalidate_all();
        }
        Ok(response)
    }

    /// Advances springs and the active applet by `dt` seconds and returns what
    /// the host should do next.
    pub fn update(&mut self, dt: f32, ctx: &mut AppletContext<'_>) -> RenderDemand {
        let left_moving = self.left_spring.step(dt);
        let right_moving = self.right_spring.step(dt);
        let applet_demand = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.update(dt, ctx),
            Tab::Hub => RenderDemand::Sleep,
        };
        self.scheduler
            .set_animating(left_moving || right_moving || !applet_demand.is_sleep());
        self.scheduler.demand().combine(applet_demand)
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
    use crate::{AppletCategory, Placement, register_rice};
    use sue_core::{Arena, AssetHandle, StringBuffer, UndoStack, txn::InsertText};
    use sue_platform::{Key, KeyEvent, Modifiers};
    use sue_rice::RiceSchema;

    /// Types characters into a note, through the undo tree.
    #[derive(Default)]
    struct Typist {
        note: Option<AssetHandle>,
    }

    impl SueApplet for Typist {
        fn manifest(&self) -> crate::AppletManifest {
            crate::AppletManifest {
                id: AppletId::new("typist").unwrap(),
                name: "Typist".into(),
                category: AppletCategory::Utility,
                home_zone: crate::Zone::LeftDock,
                uses_timeline: false,
                summary: String::new(),
            }
        }

        fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), crate::AppletError> {
            self.note = Some(ctx.arena.spawn("note", StringBuffer::new()));
            Ok(())
        }

        fn handle_input(
            &mut self,
            event: &InputEvent,
            ctx: &mut AppletContext<'_>,
        ) -> Result<InputResponse, crate::AppletError> {
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
                .arena
                .get(note)?
                .string_buffer()
                .map_or(0, StringBuffer::len);
            ctx.history
                .apply(ctx.arena, InsertText::new(note, end, text.clone()))?;
            Ok(InputResponse::Consumed { redraw: true })
        }
    }

    struct Engine {
        arena: Arena,
        history: UndoStack<Arena>,
        rice: RiceSchema,
    }

    impl Engine {
        fn new() -> Self {
            let mut rice = RiceSchema::new();
            register_rice(&mut rice).unwrap();
            Self {
                arena: Arena::new(),
                history: UndoStack::new(),
                rice,
            }
        }

        fn ctx(&mut self, profile: HardwareProfile) -> AppletContext<'_> {
            AppletContext {
                arena: &mut self.arena,
                history: &mut self.history,
                rice: &self.rice,
                form_factor: FormFactor::Desktop,
                profile,
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
            Workspace::new(
                launchpad,
                WmConfig::from_rice(&self.rice).unwrap(),
                profile,
                60,
                form_factor,
            )
        }
    }

    fn text_of(engine: &Engine) -> String {
        engine
            .arena
            .iter()
            .find_map(|(_, entity)| entity.string_buffer().map(|text| text.as_str().to_owned()))
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
        assert_eq!(text_of(&engine), "Sue");
        let undo = InputEvent::Keyboard(KeyEvent::press(Key::Character('Z'), Modifiers::CTRL));
        assert_eq!(
            workspace.dispatch(&undo, &mut engine.ctx(profile)).unwrap(),
            InputResponse::Consumed { redraw: true }
        );
        assert_eq!(text_of(&engine), "Su");
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
    fn legacy_hardware_snaps_instantly() {
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
        assert!(!workspace.timeline_active());
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
        assert_eq!(workspace.launchpad().len(), 2);
    }
}
