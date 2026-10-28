//! Tabs, docked applets, focus and the global keymap, tied to the scheduler.

use std::fmt;
use std::time::Instant;

use sue_core::AssetHandle;
use sue_platform::{FileDrop, FormFactor, InputEvent, SafeAreaInsets, ScreenEdge, TouchGesture};
use sue_rice::HardwareProfile;

use crate::{
    AppletCategory, AppletContext, AppletId, AppletManifest, BootTarget, FrameScheduler,
    FrameTicket, InputResponse, Launchpad, LayoutRequest, LayoutSolution, MediaClass,
    PropertyGroup, Rect, RenderDemand, ShellRequest, SpringDamper, SpringMode, SueApplet, Tick, Ui,
    WmConfig, WmError, World, Zone, solve_layout,
};

/// One Center Stage tab.
pub enum Tab {
    /// The permanent first tab: Home, listing every studio and inspector.
    Home,
    /// A new tab, showing Home until something opens in it.
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
    /// The applet's id, or `None` for Home and new tabs.
    #[must_use]
    pub fn applet_id(&self) -> Option<&AppletId> {
        match self {
            Self::Home | Self::Hub => None,
            Self::Applet { id, .. } => Some(id),
        }
    }

    /// `true` for tabs that show Home.
    #[must_use]
    pub fn shows_home(&self) -> bool {
        matches!(self, Self::Home | Self::Hub)
    }
}

impl fmt::Debug for Tab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Home => f.write_str("Home"),
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
    left_active: Option<usize>,
    right_active: Option<usize>,
    focus: Focus,
    left_open: bool,
    right_open: bool,
    left_spring: SpringDamper,
    right_spring: SpringDamper,
    scheduler: FrameScheduler,
    outbox: Vec<ShellRequest>,
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
    /// A workspace with the Home tab. Docks start open on desktops and closed
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
        scheduler.set_frame_cap(config.frame_rate_cap);
        scheduler.invalidate_all();
        let mut workspace = Self {
            launchpad,
            config,
            profile,
            tabs: vec![Tab::Home],
            active: 0,
            docks: Vec::new(),
            left_active: None,
            right_active: None,
            focus: Focus::Center,
            left_open: open,
            right_open: open,
            left_spring: spring,
            right_spring: spring,
            scheduler,
            outbox: Vec::new(),
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
    /// keymap, frame cap and the pinned boot applet.
    pub fn apply_config(&mut self, config: WmConfig) {
        let mode = SpringMode::for_profile(self.profile, config.instant_transitions);
        for spring in [&mut self.left_spring, &mut self.right_spring] {
            let (position, target) = (spring.position(), spring.target());
            *spring = SpringDamper::new(config.spring, mode, position);
            spring.set_target(target);
        }
        self.scheduler.set_frame_cap(config.frame_rate_cap);
        self.config = config;
        self.sync_pin();
        self.scheduler.invalidate_all();
    }

    /// Mounts every dock applet (the first of each dock is shown), then opens
    /// the boot target: the pinned applet in its own tab, or just Home.
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
            let index = self.docks.len();
            match manifest.home_zone {
                Zone::LeftDock => {
                    self.left_active.get_or_insert(index);
                }
                _ => {
                    self.right_active.get_or_insert(index);
                }
            }
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

    /// Opens a new tab showing Home and makes it active. Returns its index.
    pub fn new_tab(&mut self) -> usize {
        self.tabs.push(Tab::Hub);
        self.activate_tab(self.tabs.len() - 1);
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

    /// Activates the next (`step = 1`) or previous (`step = -1`) tab, wrapping.
    pub fn cycle_tab(&mut self, step: isize) {
        let count = self.tabs.len() as isize;
        let next = (self.active as isize + step).rem_euclid(count.max(1));
        self.activate_tab(next as usize);
    }

    /// Closes tab `index`. Home cannot be closed.
    pub fn close_tab(&mut self, index: usize, ctx: &mut AppletContext<'_>) {
        if index == 0 || index >= self.tabs.len() {
            return;
        }
        if let Tab::Applet { mut applet, .. } = self.tabs.remove(index) {
            applet.unmount(ctx);
        }
        if self.active >= index {
            self.active = self.active.saturating_sub(1);
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
    }

    /// The tab `id` is open in, if any.
    #[must_use]
    pub fn tab_of(&self, id: &AppletId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.applet_id() == Some(id))
    }

    /// Opens `id` in Center Stage: a single-instance applet that is already
    /// open is activated; otherwise it replaces a new tab's Home view, or
    /// gets a tab of its own.
    ///
    /// # Errors
    /// [`WmError::UnknownApplet`], or the applet's mount error.
    pub fn open_applet(
        &mut self,
        id: &AppletId,
        ctx: &mut AppletContext<'_>,
    ) -> Result<(), WmError> {
        let manifest = self
            .launchpad
            .manifest(id)
            .ok_or_else(|| WmError::UnknownApplet(id.clone()))?;
        if manifest.singleton
            && let Some(index) = self.tab_of(id)
        {
            self.activate_tab(index);
            return Ok(());
        }
        let mut applet = self.launchpad.instantiate(id)?;
        applet.mount(ctx)?;
        let tab = Tab::Applet {
            id: id.clone(),
            applet,
        };
        if matches!(self.tabs[self.active], Tab::Hub) {
            self.tabs[self.active] = tab;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
        Ok(())
    }

    /// Opens `id` and moves to `target` inside it (see
    /// [`SueApplet::navigate`]).
    ///
    /// # Errors
    /// As [`open_applet`](Self::open_applet).
    pub fn open_applet_at(
        &mut self,
        id: &AppletId,
        target: Option<&str>,
        ctx: &mut AppletContext<'_>,
    ) -> Result<(), WmError> {
        self.open_applet(id, ctx)?;
        if let Some(target) = target
            && let Tab::Applet { applet, .. } = &mut self.tabs[self.active]
        {
            applet.navigate(target);
        }
        Ok(())
    }

    /// The first registered applet of the System category (Settings).
    #[must_use]
    pub fn system_applet(&self) -> Option<AppletId> {
        self.launchpad
            .manifests()
            .find(|manifest| manifest.category == AppletCategory::System)
            .map(|manifest| manifest.id.clone())
    }

    /// Opens `handle` in the first applet whose manifest opens `class`. Dock
    /// applets (a player) are shown in their dock; Center Stage applets reuse
    /// the active tab if it shows the same applet, else get a tab.
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
        let manifest = self
            .launchpad
            .manifests()
            .find(|manifest| manifest.opens.contains(&class))
            .cloned()
            .ok_or(WmError::NothingOpens(class))?;
        if manifest.home_zone != Zone::CenterStage {
            let index = self
                .docks
                .iter()
                .position(|dock| dock.id == manifest.id)
                .ok_or_else(|| WmError::UnknownApplet(manifest.id.clone()))?;
            self.docks[index].applet.open(handle, ctx)?;
            self.show_dock(index);
            return Ok(());
        }
        let reuse = self.tabs[self.active].applet_id() == Some(&manifest.id);
        if !reuse {
            self.open_applet(&manifest.id, ctx)?;
            if self.tabs[self.active].applet_id() != Some(&manifest.id) {
                // A single-instance applet elsewhere was activated instead.
                let index = self.tab_of(&manifest.id).unwrap_or(self.active);
                self.activate_tab(index);
            }
        }
        if let Tab::Applet { applet, .. } = &mut self.tabs[self.active] {
            applet.open(handle, ctx)?;
        }
        self.focus = Focus::Center;
        self.scheduler.invalidate_all();
        Ok(())
    }

    /// Carries out the requests applets queued in `ctx`. Requests only the
    /// engine can serve (files, URLs, commands, messages) go to the outbox.
    ///
    /// # Errors
    /// The first request that fails.
    pub fn process_requests(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), WmError> {
        let requests = std::mem::take(ctx.requests);
        for request in requests {
            match request {
                ShellRequest::OpenAsset { handle, class } => self.open_asset(handle, class, ctx)?,
                ShellRequest::OpenApplet { id, target } => {
                    self.open_applet_at(&id, target.as_deref(), ctx)?;
                }
                other => self.outbox.push(other),
            }
        }
        Ok(())
    }

    /// Takes the requests waiting for the engine.
    pub fn take_outbox(&mut self) -> Vec<ShellRequest> {
        std::mem::take(&mut self.outbox)
    }

    /// Queues a request for the engine.
    pub fn post(&mut self, request: ShellRequest) {
        self.outbox.push(request);
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
            Some(Tab::Hub) => "New Tab".to_owned(),
            Some(Tab::Home) | None => "Home".to_owned(),
        }
    }

    /// Docked applets as `(index, zone, manifest)`.
    pub fn docked(&self) -> impl Iterator<Item = (usize, Zone, Option<&AppletManifest>)> {
        self.docks
            .iter()
            .enumerate()
            .map(|(index, dock)| (index, dock.zone, self.launchpad.manifest(&dock.id)))
    }

    /// The applets of one dock, in rail order, as `(index, manifest)`.
    #[must_use]
    pub fn dock_applets(&self, zone: Zone) -> Vec<(usize, &AppletManifest)> {
        self.docks
            .iter()
            .enumerate()
            .filter(|(_, dock)| dock.zone == zone)
            .filter_map(|(index, dock)| Some((index, self.launchpad.manifest(&dock.id)?)))
            .collect()
    }

    /// The applet a dock shows.
    #[must_use]
    pub fn active_dock(&self, zone: Zone) -> Option<usize> {
        match zone {
            Zone::LeftDock => self.left_active,
            Zone::RightDock => self.right_active,
            Zone::CenterStage | Zone::BottomTimeline => None,
        }
    }

    /// A rail click: shows dock applet `index`, or closes its dock if it is
    /// already showing.
    pub fn select_dock(&mut self, index: usize) {
        let Some(zone) = self.docks.get(index).map(|dock| dock.zone) else {
            return;
        };
        let showing = self.active_dock(zone) == Some(index);
        let open = match zone {
            Zone::LeftDock => self.left_open,
            _ => self.right_open,
        };
        if showing && open {
            self.set_dock(zone, false);
            if self.focus == Focus::Dock(index) {
                self.focus = Focus::Center;
            }
        } else {
            self.show_dock(index);
        }
    }

    /// Shows dock applet `index` and opens its dock.
    pub fn show_dock(&mut self, index: usize) {
        let Some(zone) = self.docks.get(index).map(|dock| dock.zone) else {
            return;
        };
        match zone {
            Zone::LeftDock => self.left_active = Some(index),
            _ => self.right_active = Some(index),
        }
        self.set_dock(zone, true);
        self.focus = Focus::Dock(index);
        self.scheduler.invalidate_all();
    }

    /// Shows the dock applet `id`.
    pub fn show_dock_applet(&mut self, id: &AppletId) {
        if let Some(index) = self.docks.iter().position(|dock| &dock.id == id) {
            self.show_dock(index);
        }
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

    /// Draws the active tab's applet into `area`. Returns `false` for Home.
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
            Tab::Home | Tab::Hub => false,
        };
        ctx.focused = false;
        drawn
    }

    /// Properties of the active tab's applet, for the Inspector.
    #[must_use]
    pub fn inspect_active(&self, world: &World) -> Vec<PropertyGroup> {
        match &self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.inspect(world),
            Tab::Home | Tab::Hub => Vec::new(),
        }
    }

    /// Runs command `name` on the active tab's applet. Returns `false` if it
    /// does not have one.
    ///
    /// # Errors
    /// The applet's error.
    pub fn command_active(
        &mut self,
        name: &str,
        ctx: &mut AppletContext<'_>,
    ) -> Result<bool, WmError> {
        let handled = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.command(name, ctx)?,
            Tab::Home | Tab::Hub => false,
        };
        self.scheduler.invalidate_all();
        Ok(handled)
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

    fn set_dock(&mut self, zone: Zone, open: bool) {
        match zone {
            Zone::LeftDock => self.set_left_dock(open),
            _ => self.set_right_dock(open),
        }
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
                Tab::Home | Tab::Hub => None,
            },
            Focus::Dock(index) => self.docks.get_mut(index).map(|dock| &mut dock.applet),
        }
    }

    /// Opens Settings (the System applet), optionally at a page.
    ///
    /// # Errors
    /// If it fails to mount.
    pub fn open_settings(
        &mut self,
        page: Option<&str>,
        ctx: &mut AppletContext<'_>,
    ) -> Result<(), WmError> {
        match self.system_applet() {
            Some(id) => self.open_applet_at(&id, page, ctx),
            None => {
                self.post(ShellRequest::Notify(
                    "No settings applet is installed".to_owned(),
                ));
                Ok(())
            }
        }
    }

    /// Routes one input event: global shortcuts and edge swipes first, dropped
    /// files to the engine, everything else to the focused applet. Requests
    /// queued by applets are carried out before returning.
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
        let captured = self
            .focused_applet()
            .is_some_and(|applet| applet.captures_keys());
        match event {
            InputEvent::Keyboard(key) if !captured => {
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
                if keymap.close_tab.matches(key) {
                    self.close_tab(self.active, ctx);
                    return Ok(consumed);
                }
                if keymap.next_tab.matches(key) {
                    self.cycle_tab(1);
                    return Ok(consumed);
                }
                if keymap.previous_tab.matches(key) {
                    self.cycle_tab(-1);
                    return Ok(consumed);
                }
                if keymap.home.matches(key) {
                    self.activate_tab(0);
                    return Ok(consumed);
                }
                if keymap.open_settings.matches(key) {
                    self.open_settings(None, ctx)?;
                    return Ok(consumed);
                }
                if keymap.open_file.matches(key) {
                    self.post(ShellRequest::Command("file.open".to_owned()));
                    return Ok(consumed);
                }
                if keymap.save.matches(key) {
                    self.post(ShellRequest::Command("file.save".to_owned()));
                    return Ok(consumed);
                }
                if keymap.quit.matches(key) {
                    self.post(ShellRequest::Command("app.quit".to_owned()));
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
            InputEvent::FileDrop(FileDrop::Dropped(path)) => {
                self.post(ShellRequest::OpenFile(path.clone()));
                return Ok(consumed);
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

    /// Advances springs and applets by `dt` seconds (scaled by the riced
    /// animation speed) and returns what the host should do next.
    pub fn update(&mut self, dt: f32, ctx: &mut AppletContext<'_>) -> RenderDemand {
        let dt = dt * self.config.animation_speed.max(0.01);
        let left_moving = self.left_spring.step(dt);
        let right_moving = self.right_spring.step(dt);
        let mut applets = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.update(dt, ctx),
            Tab::Home | Tab::Hub => RenderDemand::Sleep,
        };
        for dock in &mut self.docks {
            applets = applets.combine(dock.applet.update(dt, ctx));
        }
        self.scheduler
            .set_animating(left_moving || right_moving || applets.is_active());
        self.scheduler.demand().combine(applets)
    }

    /// Runs every mounted applet's timed work (see [`SueApplet::tick`]).
    pub fn tick(&mut self, now: Instant, ctx: &mut AppletContext<'_>) -> Tick {
        let mut tick = match &mut self.tabs[self.active] {
            Tab::Applet { applet, .. } => applet.tick(now, ctx),
            Tab::Home | Tab::Hub => Tick::default(),
        };
        for dock in &mut self.docks {
            tick = tick.combine(dock.applet.tick(now, ctx));
        }
        if tick.redraw {
            self.scheduler.invalidate_all();
        }
        tick
    }

    /// `true` while a dock spring is still moving, including one whose
    /// target changed after the last update.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.scheduler.animating()
            || !self.left_spring.is_settled()
            || !self.right_spring.is_settled()
    }

    /// `true` if something changed that has not been drawn yet.
    #[must_use]
    pub fn has_damage(&self) -> bool {
        self.scheduler.has_damage()
    }

    /// The frame rate animations run at.
    #[must_use]
    pub fn target_hz(&self) -> u32 {
        self.scheduler.target_hz()
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
    use crate::{AppletError, Catalog, Placement, register_rice};
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
                category: AppletCategory::Studio,
                home_zone: Zone::CenterStage,
                uses_timeline: false,
                summary: String::new(),
                opens: vec![MediaClass::Text],
                icon: "T".into(),
                singleton: false,
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

    /// A dock applet that opens audio, like a player.
    #[derive(Default)]
    struct Deck {
        opened: Option<AssetHandle>,
    }

    impl SueApplet for Deck {
        fn manifest(&self) -> AppletManifest {
            AppletManifest {
                id: AppletId::new("deck").unwrap(),
                name: "Deck".into(),
                category: AppletCategory::Utility,
                home_zone: Zone::LeftDock,
                uses_timeline: false,
                summary: String::new(),
                opens: vec![MediaClass::Audio],
                icon: "D".into(),
                singleton: false,
            }
        }

        fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
            Ok(())
        }

        fn open(
            &mut self,
            asset: AssetHandle,
            _ctx: &mut AppletContext<'_>,
        ) -> Result<(), AppletError> {
            self.opened = Some(asset);
            Ok(())
        }

        fn handle_input(
            &mut self,
            _event: &InputEvent,
            _ctx: &mut AppletContext<'_>,
        ) -> Result<InputResponse, AppletError> {
            Ok(InputResponse::Ignored)
        }
    }

    struct Engine {
        world: World,
        history: UndoStack<World>,
        requests: Vec<ShellRequest>,
        catalog: Catalog,
    }

    impl Engine {
        fn new() -> Self {
            let mut world = World::default();
            register_rice(&mut world.rice).unwrap();
            Self {
                world,
                history: UndoStack::new(),
                requests: Vec::new(),
                catalog: Catalog::default(),
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
                catalog: &self.catalog,
                audio: None,
                inspection: Vec::new(),
            }
        }

        fn workspace(&self, profile: HardwareProfile, form_factor: FormFactor) -> Workspace {
            let mut launchpad = Launchpad::new();
            launchpad.register(|| Box::new(Typist::default())).unwrap();
            launchpad
                .register(fixture("video", "Video", AppletCategory::Studio, true))
                .unwrap();
            launchpad
                .register(fixture(
                    "prefs",
                    "Preferences",
                    AppletCategory::System,
                    false,
                ))
                .unwrap();
            launchpad.register(|| Box::new(Deck::default())).unwrap();
            launchpad
                .register(fixture("notes", "Notes", AppletCategory::Utility, false))
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

    fn settle(workspace: &mut Workspace, engine: &mut Engine, profile: HardwareProfile) -> usize {
        let mut frames = 0;
        while workspace.update(1.0 / 60.0, &mut engine.ctx(profile)) != RenderDemand::Sleep {
            workspace.begin_frame();
            frames += 1;
            assert!(frames < 1_000, "never settled");
        }
        frames
    }

    #[test]
    fn boots_into_the_pinned_applet_beside_home_and_routes_typing_and_undo() {
        let mut engine = Engine::new();
        let mut workspace = engine.workspace(HardwareProfile::Balanced, FormFactor::Desktop);
        let profile = HardwareProfile::Balanced;
        let target = workspace.boot(&mut engine.ctx(profile)).unwrap();
        assert_eq!(target, BootTarget::Applet(AppletId::new("typist").unwrap()));
        assert_eq!(workspace.tabs().len(), 2, "Home stays first");
        assert_eq!(workspace.tab_title(0, &engine.world), "Home");
        assert_eq!(workspace.active_index(), 1);
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
    fn rails_pick_one_applet_per_dock_and_a_second_click_closes_it() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.boot(&mut engine.ctx(profile)).unwrap();
        let left = workspace.dock_applets(Zone::LeftDock);
        let names: Vec<&str> = left
            .iter()
            .map(|(_, manifest)| manifest.name.as_str())
            .collect();
        assert_eq!(names, ["Deck", "Notes"]);
        assert_eq!(workspace.active_dock(Zone::LeftDock), Some(left[0].0));
        let notes = left[1].0;
        workspace.select_dock(notes);
        assert_eq!(workspace.active_dock(Zone::LeftDock), Some(notes));
        assert_eq!(workspace.focus(), Focus::Dock(notes));
        workspace.select_dock(notes);
        assert!(!workspace.docks_open().0, "the second click closes it");
        assert_eq!(workspace.focus(), Focus::Center);
        workspace.select_dock(notes);
        assert!(workspace.docks_open().0);
    }

    #[test]
    fn dock_toggles_animate_with_springs_and_then_sleep() {
        let mut engine = Engine::new();
        let mut workspace = engine.workspace(HardwareProfile::Balanced, FormFactor::Desktop);
        let profile = HardwareProfile::Balanced;
        let toggle = InputEvent::Keyboard(KeyEvent::press(Key::Character('b'), Modifiers::CTRL));
        workspace
            .dispatch(&toggle, &mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.docks_open(), (false, true));
        assert!(workspace.is_animating(), "a new target counts as motion");
        let frames = settle(&mut workspace, &mut engine, profile);
        assert!(frames > 1, "a Balanced profile animates the dock");
        assert_eq!(workspace.dock_reveal(), (0.0, 1.0));
        assert!(!workspace.is_animating());
        assert!(workspace.begin_frame().is_none());
    }

    #[test]
    fn animation_speed_scales_spring_time() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut slow = engine.workspace(profile, FormFactor::Desktop);
        let mut fast = engine.workspace(profile, FormFactor::Desktop);
        let mut config = fast.config().clone();
        config.animation_speed = 4.0;
        fast.apply_config(config);
        slow.set_left_dock(false);
        fast.set_left_dock(false);
        let slow_frames = settle(&mut slow, &mut engine, profile);
        let fast_frames = settle(&mut fast, &mut engine, profile);
        assert!(
            fast_frames * 2 < slow_frames,
            "{fast_frames} vs {slow_frames}"
        );
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
        settle(&mut workspace, &mut engine, profile);
        let layout = workspace.layout(
            Rect::new(0.0, 0.0, 390.0, 844.0),
            SafeAreaInsets::ZERO,
            FormFactor::Mobile,
        );
        assert!(matches!(layout.left, Placement::Drawer { reveal, .. } if reveal == 1.0));
        assert_eq!(layout.right, Placement::Hidden);
    }

    #[test]
    fn assets_open_where_their_class_is_declared() {
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
        assert_eq!(workspace.tabs().len(), 2, "the typist tab is reused");
        workspace
            .dispatch(
                &InputEvent::Keyboard(KeyEvent::typed('!')),
                &mut engine.ctx(profile),
            )
            .unwrap();
        assert_eq!(text_of(&engine, "doc.md"), "# hi!");

        // Audio goes to the dock applet that opens it, which comes into view.
        workspace.set_left_dock(false);
        let tone = engine.world.arena.spawn("tone", StringBuffer::new());
        workspace
            .open_asset(tone, MediaClass::Audio, &mut engine.ctx(profile))
            .unwrap();
        assert!(workspace.docks_open().0);
        assert!(matches!(workspace.focus(), Focus::Dock(_)));
        assert!(matches!(
            workspace.open_asset(doc, MediaClass::Image, &mut engine.ctx(profile)),
            Err(WmError::NothingOpens(MediaClass::Image))
        ));
    }

    #[test]
    fn settings_open_once_and_engine_requests_reach_the_outbox() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.boot(&mut engine.ctx(profile)).unwrap();
        let open_settings =
            InputEvent::Keyboard(KeyEvent::press(Key::Character(','), Modifiers::CTRL));
        workspace
            .dispatch(&open_settings, &mut engine.ctx(profile))
            .unwrap();
        workspace.activate_tab(0);
        workspace
            .dispatch(&open_settings, &mut engine.ctx(profile))
            .unwrap();
        let prefs = AppletId::new("prefs").unwrap();
        let open = workspace
            .tabs()
            .iter()
            .filter(|tab| tab.applet_id() == Some(&prefs))
            .count();
        assert_eq!(open, 1, "Settings opens once");
        assert_eq!(workspace.tab_of(&prefs), Some(workspace.active_index()));
        let dropped = InputEvent::FileDrop(FileDrop::Dropped("photo.jxl".into()));
        workspace
            .dispatch(&dropped, &mut engine.ctx(profile))
            .unwrap();
        let quit = InputEvent::Keyboard(KeyEvent::press(Key::Character('q'), Modifiers::CTRL));
        workspace.dispatch(&quit, &mut engine.ctx(profile)).unwrap();
        assert_eq!(
            workspace.take_outbox(),
            [
                ShellRequest::OpenFile("photo.jxl".into()),
                ShellRequest::Command("app.quit".to_owned())
            ]
        );
        assert!(workspace.take_outbox().is_empty());
    }

    #[test]
    fn home_stays_put_and_new_tabs_fill_in_place() {
        let mut engine = Engine::new();
        let profile = HardwareProfile::Balanced;
        let mut workspace = engine.workspace(profile, FormFactor::Desktop);
        workspace.boot(&mut engine.ctx(profile)).unwrap();
        workspace.close_tab(0, &mut engine.ctx(profile));
        assert_eq!(workspace.tab_title(0, &engine.world), "Home");
        let new_tab = InputEvent::Keyboard(KeyEvent::press(Key::Character('t'), Modifiers::CTRL));
        workspace
            .dispatch(&new_tab, &mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.tabs().len(), 3);
        assert_eq!(workspace.tab_title(2, &engine.world), "New Tab");
        workspace
            .open_applet(&AppletId::new("video").unwrap(), &mut engine.ctx(profile))
            .unwrap();
        assert_eq!(workspace.tabs().len(), 3, "the new tab was filled in place");
        assert!(workspace.timeline_active());
        let layout = workspace.layout(
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            SafeAreaInsets::ZERO,
            FormFactor::Desktop,
        );
        assert!(matches!(layout.bottom, Placement::Tiled(_)));
        workspace.cycle_tab(1);
        assert_eq!(workspace.active_index(), 0, "cycling wraps to Home");
        workspace.cycle_tab(-1);
        assert_eq!(workspace.active_index(), 2);
        workspace.close_tab(2, &mut engine.ctx(profile));
        assert_eq!((workspace.tabs().len(), workspace.active_index()), (2, 1));
    }
}
