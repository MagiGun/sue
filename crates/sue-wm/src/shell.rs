//! The shell: Sue's whole window as one immediate-mode egui frame.
//!
//! The infinite canvas fills the middle; every entity on it is a node drawn
//! by its applet inside a camera-transformed egui layer, so text fields,
//! paint strokes and waveforms zoom from 10% to 3200% with the canvas.
//! Around it: activity rails, spring-driven drawers, a spring-driven bottom
//! timeline and a status bar. Over it: the floating top pill (modes and
//! spawn buttons), the floating selection pill above the selected entity,
//! the radial marking menu, the command palette and the Settings window.
//!
//! Motion (LAW 4): every moving part is a [`SpringDamper`] stepped with the
//! real, clamped frame time. While any spring moves the shell calls
//! `request_repaint`, so frames come at the display rate with no input; once
//! all settle (plus a short grace period) the loop sleeps at 0 FPS.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::emath::TSTransform;
use egui::{
    Align, Align2, Id, LayerId, Layout, Order, Pos2, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use sue_core::{AssetHandle, Projection, TimeSpan, UndoStack};
use sue_platform::{AudioSink, FormFactor, FramePacing, SafeAreaInsets, TextureUpload};
use sue_rice::HardwareProfile;

use crate::config::GridStyle;
use crate::palette::{Palette, PaletteOutcome};
use crate::radial::{Radial, RadialOutcome};
use crate::timeline::{Block, TimelineEvent, TimelineState, timeline_ui};
use crate::transport::{Clip, pcm_of};
use crate::{
    AppletContext, AppletId, AppletRole, Applets, ArenaEdit, Batch, Camera, Catalog,
    FrameScheduler, LayoutRequest, NodeId, PaletteEntry, PlaceNode, RemoveNode, SceneNode,
    SetNodeProp, SetNodeRect, ShellRequest, ShellView, SpawnNode, SpringDamper, SpringMode, Theme,
    Transport, Typing, WmConfig, World, keys, solve_layout, widgets,
};

/// Height of a node's title bar, in canvas units.
pub const NODE_TITLE: f32 = 26.0;

/// Where the built-in Timeline mode sits among the node modes (by applet
/// order): after the visual kinds, before code.
const TIMELINE_ORDER: i32 = 35;

/// The mode that shows the whole canvas.
pub const CANVAS_MODE: &str = "canvas";

/// The mode that opens the timeline.
pub const TIMELINE_MODE: &str = "timeline";

/// A hardware profile as people read it, e.g. `Legacy Low Spec`.
#[must_use]
pub fn profile_label(profile: HardwareProfile) -> String {
    profile
        .rice_name()
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Everything outside the shell that a frame reads and changes.
pub struct ShellIo<'a> {
    /// The arena, the live rice and the scene.
    pub world: &'a mut World,
    /// The global undo tree.
    pub history: &'a mut UndoStack<World>,
    /// Typing waiting to become an undo step.
    pub typing: &'a mut Typing,
    /// The live applets.
    pub applets: &'a mut Applets,
    /// Settings pages and tables.
    pub catalog: &'a Catalog,
    /// Image uploads.
    pub textures: &'a mut dyn TextureUpload,
    /// The audio output, once opened.
    pub audio: Option<&'a mut (dyn AudioSink + 'static)>,
    /// The audio transport.
    pub transport: &'a mut Transport,
    /// Requests for the engine.
    pub requests: &'a mut Vec<ShellRequest>,
    /// Frames per second while animating; `None` when idle.
    pub fps: Option<u32>,
}

fn split<'b>(
    io: &'b mut ShellIo<'_>,
    theme: &'b Theme,
    view: ShellView,
) -> (&'b mut Applets, AppletContext<'b>) {
    let cx = AppletContext {
        world: &mut *io.world,
        history: &mut *io.history,
        typing: &mut *io.typing,
        requests: &mut *io.requests,
        catalog: io.catalog,
        theme,
        textures: &mut *io.textures,
        audio: io.audio.as_deref_mut(),
        transport: &mut *io.transport,
        view,
    };
    (&mut *io.applets, cx)
}

/// One drawer: which applet it shows and how far it is open.
#[derive(Clone, Debug, PartialEq)]
struct Drawer {
    shown: Option<AppletId>,
    open: bool,
    reveal: SpringDamper,
}

/// A node being moved or resized.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NodeDrag {
    node: NodeId,
    from: Rect,
    resize: bool,
}

/// The floating selection pill.
#[derive(Clone, Debug, PartialEq)]
struct SelectionPill {
    node: Option<NodeId>,
    appear: SpringDamper,
    glide_x: SpringDamper,
    glide_y: SpringDamper,
    anchor: Option<Pos2>,
}

/// A mode in the top pill.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModeButton {
    id: String,
    label: String,
    order: i32,
}

/// The shell's state between frames.
pub struct Shell {
    config: WmConfig,
    theme: Theme,
    applied_theme: Option<Theme>,
    profile: HardwareProfile,
    form_factor: FormFactor,
    safe_area: SafeAreaInsets,
    scheduler: FrameScheduler,
    left: Drawer,
    right: Drawer,
    timeline_open: bool,
    timeline_reveal: SpringDamper,
    timeline: TimelineState,
    timeline_clip: Option<NodeId>,
    camera: Camera,
    mode: String,
    selected: Option<NodeId>,
    pill: SelectionPill,
    top_pill: SpringDamper,
    radial: Option<Radial>,
    palette: Option<Palette>,
    window: Option<AppletId>,
    window_appear: SpringDamper,
    space_down: bool,
    space_panned: bool,
    middle_pan: bool,
    node_drag: Option<NodeDrag>,
    message: Option<(String, bool, Instant)>,
    last_frame: Option<Instant>,
    last_change: Option<Instant>,
    animating: bool,
    canvas: Rect,
    booted: bool,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("mode", &self.mode)
            .field("selected", &self.selected)
            .field("timeline_open", &self.timeline_open)
            .finish_non_exhaustive()
    }
}

impl Shell {
    /// A shell with closed drawers, centered on the origin.
    #[must_use]
    pub fn new(
        config: WmConfig,
        theme: Theme,
        profile: HardwareProfile,
        display_hz: u32,
        form_factor: FormFactor,
    ) -> Self {
        let mode = SpringMode::for_profile(profile, config.instant_transitions);
        let spring = |position| SpringDamper::new(config.spring, mode, position);
        let mut scheduler = FrameScheduler::new(profile, display_hz);
        scheduler.set_frame_cap(config.frame_rate_cap);
        let camera = Camera::new(
            config.camera_spring,
            mode,
            (config.canvas.zoom_min, config.canvas.zoom_max),
        );
        let mut top_pill = spring(0.0);
        top_pill.set_target(1.0);
        Self {
            left: Drawer {
                shown: None,
                open: false,
                reveal: spring(0.0),
            },
            right: Drawer {
                shown: None,
                open: false,
                reveal: spring(0.0),
            },
            timeline_open: false,
            timeline_reveal: spring(0.0),
            timeline: TimelineState::default(),
            timeline_clip: None,
            camera,
            mode: CANVAS_MODE.to_owned(),
            selected: None,
            pill: SelectionPill {
                node: None,
                appear: spring(0.0),
                glide_x: spring(0.0),
                glide_y: spring(0.0),
                anchor: None,
            },
            top_pill,
            radial: None,
            palette: None,
            window: None,
            window_appear: spring(0.0),
            space_down: false,
            space_panned: false,
            middle_pan: false,
            node_drag: None,
            message: None,
            last_frame: None,
            last_change: None,
            animating: true,
            canvas: Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 720.0)),
            booted: false,
            config,
            theme,
            applied_theme: None,
            profile,
            form_factor,
            safe_area: SafeAreaInsets::default(),
            scheduler,
        }
    }

    /// The config.
    #[must_use]
    pub fn config(&self) -> &WmConfig {
        &self.config
    }

    /// The theme.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The active hardware profile.
    #[must_use]
    pub fn profile(&self) -> HardwareProfile {
        self.profile
    }

    /// Frames per second while animating.
    #[must_use]
    pub fn target_hz(&self) -> u32 {
        self.scheduler.target_hz()
    }

    /// The canvas camera.
    #[must_use]
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// The canvas area of the last frame, in screen points.
    #[must_use]
    pub fn canvas_rect(&self) -> Rect {
        self.canvas
    }

    /// The active mode.
    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// The selected node.
    #[must_use]
    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    /// `true` while the timeline is open.
    #[must_use]
    pub fn timeline_open(&self) -> bool {
        self.timeline_open
    }

    /// The timeline playhead, in seconds.
    #[must_use]
    pub fn playhead(&self) -> f64 {
        self.timeline.playhead
    }

    /// The applet shown in the left drawer, if open.
    #[must_use]
    pub fn left_drawer(&self) -> Option<&AppletId> {
        self.left.shown.as_ref().filter(|_| self.left.open)
    }

    /// The applet shown in the right drawer, if open.
    #[must_use]
    pub fn right_drawer(&self) -> Option<&AppletId> {
        self.right.shown.as_ref().filter(|_| self.right.open)
    }

    /// The open window applet, if any.
    #[must_use]
    pub fn window(&self) -> Option<&AppletId> {
        self.window.as_ref()
    }

    /// `true` while the palette is open.
    #[must_use]
    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    /// `true` while the marking menu is open.
    #[must_use]
    pub fn radial_open(&self) -> bool {
        self.radial.is_some()
    }

    /// `true` while the selection pill is (at least partly) visible.
    #[must_use]
    pub fn selection_pill_visible(&self) -> bool {
        self.pill.node.is_some() && self.pill.appear.position() > 0.01
    }

    /// `true` while any spring moves.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Insets for notches and system bars.
    pub fn set_safe_area(&mut self, safe_area: SafeAreaInsets) {
        self.safe_area = safe_area;
    }

    fn spring_mode(&self) -> SpringMode {
        SpringMode::for_profile(self.profile, self.config.instant_transitions)
    }

    /// Follows a rice change.
    pub fn apply_config(&mut self, config: WmConfig, theme: Theme, profile: HardwareProfile) {
        self.profile = profile;
        self.scheduler.set_profile(profile);
        self.scheduler.set_frame_cap(config.frame_rate_cap);
        self.config = config;
        self.theme = theme;
        let mode = self.spring_mode();
        let params = self.config.spring;
        for spring in [
            &mut self.left.reveal,
            &mut self.right.reveal,
            &mut self.timeline_reveal,
            &mut self.pill.appear,
            &mut self.pill.glide_x,
            &mut self.pill.glide_y,
            &mut self.top_pill,
            &mut self.window_appear,
        ] {
            spring.set_params(params);
            spring.set_mode(mode);
        }
        self.camera.configure(
            self.config.camera_spring,
            mode,
            (self.config.canvas.zoom_min, self.config.canvas.zoom_max),
        );
        self.touch();
    }

    /// Marks a change: keep drawing through the grace period.
    pub fn touch(&mut self) {
        self.last_change = Some(Instant::now());
    }

    /// Shows a status message.
    pub fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((
            text.into(),
            error,
            Instant::now() + self.config.message_duration,
        ));
        self.touch();
    }

    /// The status message, if any.
    #[must_use]
    pub fn message(&self) -> Option<(&str, bool)> {
        self.message
            .as_ref()
            .map(|(text, error, _)| (text.as_str(), *error))
    }

    /// What the loop should do next (LAW 4).
    #[must_use]
    pub fn pacing(&self, now: Instant) -> FramePacing {
        let target_hz = self.scheduler.target_hz();
        if self.animating || self.timeline.playing {
            return FramePacing::Animate { target_hz };
        }
        if self
            .last_change
            .is_some_and(|change| now < change + self.config.settle_grace)
        {
            return FramePacing::Animate { target_hz };
        }
        self.message
            .as_ref()
            .map(|(_, _, until)| *until)
            .map_or(FramePacing::Sleep, FramePacing::WakeAt)
    }

    /// Clears an expired message. Returns `true` if the window must redraw.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self
            .message
            .as_ref()
            .is_some_and(|(_, _, until)| *until <= now)
        {
            self.message = None;
            return true;
        }
        false
    }

    fn view(&self, now: Instant, dt: f32) -> ShellView {
        ShellView {
            selected: self.selected,
            zoom: self.camera.zoom(),
            mode: self.mode.clone(),
            timeline_open: self.timeline_open,
            playhead: self.timeline.playhead,
            timeline_playing: self.timeline.playing,
            form_factor: self.form_factor,
            profile: self.profile,
            now,
            dt,
        }
    }

    // ----- Drawers, timeline, window -------------------------------------

    /// Shows `id` in its drawer, or hides the drawer if it already shows it.
    pub fn toggle_drawer_applet(&mut self, id: &AppletId, applets: &Applets) {
        let Some(manifest) = applets.manifest(id) else {
            return;
        };
        let drawer = match manifest.role {
            AppletRole::LeftDrawer => &mut self.left,
            AppletRole::RightDrawer => &mut self.right,
            _ => return,
        };
        if drawer.open && drawer.shown.as_ref() == Some(id) {
            drawer.open = false;
            drawer.reveal.set_target(0.0);
        } else {
            drawer.shown = Some(id.clone());
            drawer.open = true;
            drawer.reveal.set_target(1.0);
        }
        self.touch();
    }

    /// Shows `id` in its drawer.
    pub fn show_drawer(&mut self, id: &AppletId, applets: &Applets) {
        let open = match applets.manifest(id).map(|manifest| manifest.role) {
            Some(AppletRole::LeftDrawer) => self.left.open && self.left.shown.as_ref() == Some(id),
            Some(AppletRole::RightDrawer) => {
                self.right.open && self.right.shown.as_ref() == Some(id)
            }
            _ => return,
        };
        if !open {
            self.toggle_drawer_applet(id, applets);
        }
    }

    fn toggle_drawer(&mut self, left: bool, applets: &Applets) {
        let role = if left {
            AppletRole::LeftDrawer
        } else {
            AppletRole::RightDrawer
        };
        let first = applets
            .with_role(role)
            .next()
            .map(|manifest| manifest.id.clone());
        let drawer = if left {
            &mut self.left
        } else {
            &mut self.right
        };
        drawer.open = !drawer.open;
        if drawer.shown.is_none() {
            drawer.shown = first;
        }
        drawer
            .reveal
            .set_target(if drawer.open { 1.0 } else { 0.0 });
        self.touch();
    }

    /// Shows or hides the timeline.
    pub fn set_timeline(&mut self, open: bool) {
        self.timeline_open = open;
        self.timeline_reveal
            .set_target(if open { 1.0 } else { 0.0 });
        if !open {
            self.timeline.playing = false;
        }
        self.touch();
    }

    /// Closes the palette and the marking menu.
    pub fn dismiss_overlays(&mut self) {
        if let Some(palette) = self.palette.as_mut() {
            palette.close();
        }
        if let Some(radial) = self.radial.as_mut() {
            radial.close();
        }
        self.touch();
    }

    /// Opens the window applet `id` (Settings).
    pub fn open_window(&mut self, id: AppletId) {
        self.dismiss_overlays();
        self.window = Some(id);
        self.window_appear.set_target(1.0);
        self.touch();
    }

    /// Closes the window.
    pub fn close_window(&mut self) {
        self.window_appear.set_target(0.0);
        self.touch();
    }

    /// Opens the command palette.
    pub fn open_palette(&mut self) {
        if self.palette.is_none() {
            self.palette = Some(Palette::open(self.config.spring, self.spring_mode()));
            self.touch();
        }
    }

    /// Types into the open palette (scripted runs).
    pub fn palette_query(&mut self, text: &str) {
        if let Some(palette) = self.palette.as_mut() {
            palette.set_query(text);
        }
    }

    /// Opens the marking menu at a screen point.
    pub fn open_radial(&mut self, at: Pos2) {
        let canvas_point = self.camera.to_canvas(self.canvas, at);
        self.radial = Some(Radial::open(
            at,
            canvas_point,
            self.config.spring,
            self.spring_mode(),
        ));
        self.touch();
    }

    // ----- Selection, modes, camera ----------------------------------------

    /// Selects a node (or nothing); the selection pill follows.
    pub fn select(&mut self, node: Option<NodeId>, world: &mut World) {
        if node == self.selected {
            return;
        }
        if let Some(node) = node {
            world.scene.raise(node);
            if let (Some(old), Some(_)) = (self.pill.anchor, self.pill.node) {
                // Glide from where the pill was.
                let new = self.pill_anchor(node, world).unwrap_or(old);
                self.pill.glide_x.snap_to(old.x - new.x);
                self.pill.glide_y.snap_to(old.y - new.y);
                self.pill.glide_x.set_target(0.0);
                self.pill.glide_y.set_target(0.0);
            }
            self.pill.node = Some(node);
            self.pill.appear.set_target(1.0);
        } else {
            self.pill.appear.set_target(0.0);
        }
        self.selected = node;
        self.touch();
    }

    fn pill_anchor(&self, node: NodeId, world: &World) -> Option<Pos2> {
        let rect = world.scene.get(node)?.rect;
        let top = self.camera.to_screen(self.canvas, rect.center_top());
        let lowest = self.canvas.top() + self.config.pill_height + self.config.pill_gap + 52.0;
        Some(pos2(
            top.x
                .clamp(self.canvas.left() + 160.0, self.canvas.right() - 160.0),
            (top.y - self.config.pill_gap)
                .clamp(lowest, self.canvas.bottom() - self.config.pill_gap),
        ))
    }

    /// Glides the camera to show everything.
    pub fn frame_all(&mut self, world: &World) {
        let bounds = world
            .scene
            .bounds()
            .unwrap_or_else(|| Rect::from_center_size(Pos2::ZERO, vec2(800.0, 500.0)));
        self.camera.frame(
            self.canvas.size(),
            bounds,
            self.config.canvas.frame_margin + self.config.pill_height,
        );
        self.touch();
    }

    /// Glides the camera to frame a node.
    pub fn frame_node(&mut self, node: NodeId, world: &World, zoom_percent: Option<u32>) {
        let Some(rect) = world.scene.get(node).map(|node| node.rect) else {
            return;
        };
        match zoom_percent {
            Some(percent) => self.camera.glide_to(rect.center(), percent as f32 / 100.0),
            None => {
                // Leave room for the top pill and the selection pill.
                let margin = self.config.canvas.frame_margin;
                let room = self.canvas.size() - vec2(0.0, self.config.pill_height * 2.0);
                self.camera.frame(room, rect, margin);
                // Fitting never magnifies past 200%; 1:1 and the wheel go further.
                let zoom = self.camera.target_zoom().min(2.0);
                let lift = self.config.pill_height * 0.5 / zoom.max(1e-3);
                self.camera.glide_to(rect.center() - vec2(0.0, lift), zoom);
            }
        }
        self.touch();
    }

    fn mode_buttons(applets: &Applets) -> Vec<ModeButton> {
        let mut modes = vec![
            ModeButton {
                id: CANVAS_MODE.to_owned(),
                label: "Canvas".to_owned(),
                order: i32::MIN,
            },
            ModeButton {
                id: TIMELINE_MODE.to_owned(),
                label: "Timeline".to_owned(),
                order: TIMELINE_ORDER,
            },
        ];
        modes.extend(applets.manifests().filter_map(|manifest| {
            let mode = manifest.mode.as_ref().filter(|mode| mode.in_pill)?;
            Some(ModeButton {
                id: manifest.id.as_str().to_owned(),
                label: mode.label.clone(),
                order: manifest.order,
            })
        }));
        modes.sort_by_key(|mode| mode.order);
        modes
    }

    /// Switches mode: the canvas shows everything, the timeline opens, and a
    /// node mode frames (or creates) an entity of its applet.
    pub fn set_mode(&mut self, mode: &str, io: &mut ShellIo<'_>, now: Instant) {
        match mode {
            CANVAS_MODE => {
                self.mode = CANVAS_MODE.to_owned();
                self.frame_all(io.world);
            }
            TIMELINE_MODE => {
                self.mode = TIMELINE_MODE.to_owned();
                self.set_timeline(true);
            }
            other => {
                let Ok(id) = AppletId::new(other) else {
                    return;
                };
                if io
                    .applets
                    .manifest(&id)
                    .and_then(|m| m.mode.as_ref())
                    .is_none()
                {
                    self.notify(format!("There is no {other} mode"), true);
                    return;
                }
                let target = self
                    .selected
                    .filter(|node| io.world.scene.get(*node).is_some_and(|n| n.applet == id))
                    .or_else(|| io.world.scene.topmost_of(&id));
                let target = match target {
                    Some(node) => Some(node),
                    None => self.spawn(&id, None, io, now),
                };
                if let Some(node) = target {
                    self.mode = other.to_owned();
                    self.select(Some(node), io.world);
                    self.frame_node(node, io.world, None);
                }
            }
        }
        self.touch();
    }

    /// Creates a new entity of node applet `id` at `at` (canvas units), or
    /// in view. Returns its node.
    pub fn spawn(
        &mut self,
        id: &AppletId,
        at: Option<Pos2>,
        io: &mut ShellIo<'_>,
        now: Instant,
    ) -> Option<NodeId> {
        let center = at.unwrap_or_else(|| self.camera.center());
        let view = self.view(now, 0.0);
        let theme = self.theme.clone();
        let entity = {
            let (applets, mut cx) = split(io, &theme, view);
            applets.get_mut(id)?.new_entity(&mut cx)?
        };
        let rect = io.world.scene.free_spot(center, entity.size, 48.0);
        let mut txn = SpawnNode::new(entity, id.clone(), rect);
        io.typing.flush(io.history);
        // Applied here and recorded after, so the new node's id is known.
        match sue_core::ReversibleTransaction::apply(&mut txn, io.world) {
            Ok(()) => {
                let (node, _) = txn.placed()?;
                io.history.record(txn);
                self.select(Some(node), io.world);
                Some(node)
            }
            Err(error) => {
                self.notify(format!("Could not create it: {error}"), true);
                None
            }
        }
    }

    /// Puts an arena entity on the canvas, drawn by the applet that accepts
    /// it, and selects it.
    pub fn place(
        &mut self,
        entity: AssetHandle,
        focus: bool,
        io: &mut ShellIo<'_>,
    ) -> Option<NodeId> {
        let found = io.world.arena.get(entity).ok()?;
        let Some(applet) = io.applets.node_applet_for(found) else {
            self.notify("No applet shows this kind of file", true);
            return None;
        };
        let size = io.applets.get(&applet)?.node_size(found);
        let rect = io.world.scene.free_spot(self.camera.center(), size, 48.0);
        let node = io.world.scene.allocate();
        let txn = PlaceNode::with_id(SceneNode::new(entity, applet.clone(), rect), node);
        io.typing.flush(io.history);
        if let Err(error) = io.history.apply(io.world, txn) {
            self.notify(format!("Could not place it: {error}"), true);
            return None;
        }
        self.select(Some(node), io.world);
        if focus {
            if io
                .applets
                .manifest(&applet)
                .and_then(|m| m.mode.as_ref())
                .is_some()
            {
                self.mode = applet.as_str().to_owned();
            }
            self.frame_node(node, io.world, None);
        }
        Some(node)
    }

    /// Handles a request meant for the shell; returns the ones for the
    /// engine (messages, files, commands).
    pub fn handle(
        &mut self,
        request: ShellRequest,
        io: &mut ShellIo<'_>,
        now: Instant,
    ) -> Option<ShellRequest> {
        match request {
            ShellRequest::Select(node) => self.select(node, io.world),
            ShellRequest::Focus(node) => {
                if let Some(applet) = io.world.scene.get(node).map(|n| n.applet.clone()) {
                    if io
                        .applets
                        .manifest(&applet)
                        .and_then(|m| m.mode.as_ref())
                        .is_some()
                    {
                        self.mode = applet.as_str().to_owned();
                    }
                    self.select(Some(node), io.world);
                    self.frame_node(node, io.world, None);
                }
            }
            ShellRequest::Frame { node, zoom_percent } => {
                self.frame_node(node, io.world, zoom_percent);
            }
            ShellRequest::Timeline(open) => self.set_timeline(open),
            ShellRequest::OpenSettings(page) => {
                let id = io
                    .applets
                    .with_role(AppletRole::Window)
                    .next()
                    .map(|manifest| manifest.id.clone());
                if let Some(id) = id {
                    if let (Some(page), Some(applet)) = (page, io.applets.get_mut(&id)) {
                        applet.navigate(&page);
                    }
                    self.open_window(id);
                }
            }
            ShellRequest::Mode(mode) => self.set_mode(&mode, io, now),
            ShellRequest::Spawn(id) => {
                if let Some(node) = self.spawn(&id, None, io, now) {
                    self.frame_node(node, io.world, None);
                }
            }
            ShellRequest::Palette => self.open_palette(),
            ShellRequest::ResetCamera => {
                self.mode = CANVAS_MODE.to_owned();
                self.frame_all(io.world);
            }
            ShellRequest::ShowDrawer(id) => self.show_drawer(&id, io.applets),
            ShellRequest::Place { entity, focus } => {
                self.place(entity, focus, io);
            }
            other => return Some(other),
        }
        self.touch();
        None
    }

    /// Boots: applies the riced boot mode.
    pub fn boot(&mut self, io: &mut ShellIo<'_>, now: Instant) {
        if self.booted {
            return;
        }
        self.booted = true;
        let mode = self.config.boot_mode.clone();
        if mode.is_empty() || mode == CANVAS_MODE {
            let bounds = io.world.scene.bounds();
            if let Some(bounds) = bounds {
                self.camera.frame(
                    self.canvas.size(),
                    bounds,
                    self.config.canvas.frame_margin + self.config.pill_height,
                );
            }
        } else {
            self.set_mode(&mode, io, now);
        }
    }

    // ----- Undo -------------------------------------------------------------

    fn undo(&mut self, io: &mut ShellIo<'_>, redo: bool) {
        io.typing.flush(io.history);
        let result = if redo {
            io.history.redo(io.world)
        } else {
            io.history.undo(io.world)
        };
        match result {
            Ok(true) => {}
            Ok(false) => self.notify(
                if redo {
                    "Nothing to redo"
                } else {
                    "Nothing to undo"
                },
                false,
            ),
            Err(error) => self.notify(format!("Undo failed: {error}"), true),
        }
        if self
            .selected
            .is_some_and(|node| io.world.scene.get(node).is_none())
        {
            self.select(None, io.world);
        }
        self.touch();
    }

    // ----- The frame ----------------------------------------------------------

    /// Builds one frame into the root `ui`.
    #[allow(
        clippy::too_many_lines,
        reason = "one frame of the whole shell, in drawing order"
    )]
    pub fn ui(&mut self, root: &mut Ui, io: &mut ShellIo<'_>, now: Instant) {
        let ctx = root.ctx().clone();
        // Real wall-clock time, clamped; after a sleep, one nominal frame.
        let nominal = 1.0 / self.scheduler.target_hz().max(1) as f32;
        let raw_dt = self
            .last_frame
            .map_or(nominal, |last| now.duration_since(last).as_secs_f32());
        let dt = if raw_dt > 0.1 {
            nominal
        } else {
            raw_dt.clamp(0.0001, 0.05)
        };
        self.last_frame = Some(now);
        let motion_dt = dt * self.config.animation_speed.max(0.01);

        if self.applied_theme.as_ref() != Some(&self.theme) {
            self.theme
                .apply(&ctx, self.config.caret_blink, self.applied_theme.as_ref());
            ctx.set_zoom_factor(self.config.ui_scale.clamp(0.5, 3.0));
            ctx.options_mut(|options| {
                options.input_options.line_scroll_speed = self.config.wheel_line_px;
            });
            self.applied_theme = Some(self.theme.clone());
        }
        let theme = self.theme.clone();

        // Files dropped anywhere open on the canvas.
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            io.requests.push(ShellRequest::OpenFile(path));
        }

        self.global_keys(&ctx, io, now);

        // Springs.
        let mut moving = false;
        moving |= self.left.reveal.step(motion_dt);
        moving |= self.right.reveal.step(motion_dt);
        moving |= self.timeline_reveal.step(motion_dt);
        moving |= self.camera.step(motion_dt, self.canvas);
        moving |= self.pill.appear.step(motion_dt);
        moving |= self.pill.glide_x.step(motion_dt);
        moving |= self.pill.glide_y.step(motion_dt);
        moving |= self.top_pill.step(motion_dt);
        moving |= self.window_appear.step(motion_dt);
        if let Some(radial) = self.radial.as_mut() {
            moving |= radial.step(motion_dt);
        }
        if let Some(palette) = self.palette.as_mut() {
            moving |= palette.step(motion_dt);
        }
        if self.pill.appear.target() == 0.0 && self.pill.appear.is_settled() {
            self.pill.node = None;
        }
        if self.window_appear.target() == 0.0 && self.window_appear.is_settled() {
            self.window = None;
        }
        if self.radial.as_ref().is_some_and(Radial::is_gone) {
            self.radial = None;
        }
        if self.palette.as_ref().is_some_and(Palette::is_gone) {
            self.palette = None;
        }

        // Timeline playback.
        if self.timeline.playing {
            self.timeline.playhead += f64::from(dt);
            self.sync_timeline_audio(io);
        }

        let layout = solve_layout(&LayoutRequest {
            viewport: root.max_rect(),
            safe_area: self.safe_area,
            form_factor: self.form_factor,
            left_reveal: self.left.reveal.position(),
            right_reveal: self.right.reveal.position(),
            timeline_reveal: self.timeline_reveal.position(),
            metrics: self.config.metrics,
        });
        self.canvas = layout.canvas;

        self.canvas_ui(root, io, &theme, now, dt);

        // Structural panels around the canvas.
        let painter = root.painter().clone();
        for rect in [layout.left_rail, layout.right_rail, layout.status] {
            painter.rect_filled(rect, 0.0, theme.panel);
        }
        painter.vline(
            layout.left_rail.right(),
            layout.left_rail.y_range(),
            theme.hairline(),
        );
        painter.vline(
            layout.right_rail.left(),
            layout.right_rail.y_range(),
            theme.hairline(),
        );
        painter.hline(
            layout.status.x_range(),
            layout.status.top(),
            theme.hairline(),
        );
        self.rail_ui(root, layout.left_rail, true, io, &theme);
        self.rail_ui(root, layout.right_rail, false, io, &theme);
        self.drawer_ui(
            root,
            layout.left_drawer,
            layout.left_drawer_full,
            true,
            io,
            &theme,
            now,
            dt,
        );
        self.drawer_ui(
            root,
            layout.right_drawer,
            layout.right_drawer_full,
            false,
            io,
            &theme,
            now,
            dt,
        );
        if layout.timeline.height() > 0.5 {
            self.timeline_dock(root, layout.timeline, layout.timeline_full, io, &theme, now);
        }
        self.status_ui(root, layout.status, io, &theme);

        // Floating surfaces.
        self.top_pill_ui(&ctx, io, &theme, now);
        self.selection_pill_ui(&ctx, io, &theme, now, dt);
        self.radial_ui(&ctx, root.max_rect(), io, &theme, now);
        self.palette_ui(&ctx, io, &theme, now);
        self.window_ui(&ctx, root.max_rect(), io, &theme, now, dt);
        if self.space_down {
            // While Space is held, the canvas pans instead of editing.
            egui::Area::new(Id::unique("sue-space-pan"))
                .order(Order::Foreground)
                .fixed_pos(self.canvas.min)
                .show(&ctx, |ui| {
                    let response = ui.allocate_rect(self.canvas, Sense::drag());
                    if response.dragged() {
                        self.camera.pan_by(response.drag_delta());
                        self.space_panned = true;
                        self.touch();
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                });
        }

        // Applet ticks that need every frame (a stroke in progress).
        self.animating = moving || self.node_drag.is_some() || self.timeline.playing;
        self.scheduler.set_animating(self.animating);
        if self.animating {
            ctx.request_repaint();
        }
        if io.typing.flush_if_paused(io.history, now) {
            self.touch();
        }
    }

    fn global_keys(&mut self, ctx: &egui::Context, io: &mut ShellIo<'_>, now: Instant) {
        if io.applets.captures_keys() {
            return;
        }
        let keymap = self.config.keymap;
        let typing = ctx.text_edit_focused();
        let (undo, redo, settings, left, right, timeline, reset, open, quit) =
            ctx.input_mut(|input| {
                let redo =
                    keys::consume(input, &keymap.redo) || keys::consume(input, &keymap.redo_alt);
                (
                    keys::consume(input, &keymap.undo),
                    redo,
                    keys::consume(input, &keymap.open_settings),
                    keys::consume(input, &keymap.toggle_left_drawer),
                    keys::consume(input, &keymap.toggle_right_drawer),
                    keys::consume(input, &keymap.toggle_timeline),
                    keys::consume(input, &keymap.reset_camera),
                    keys::consume(input, &keymap.open_file),
                    keys::consume(input, &keymap.quit),
                )
            });
        if undo {
            self.undo(io, false);
        }
        if redo {
            self.undo(io, true);
        }
        if settings {
            self.handle(ShellRequest::OpenSettings(None), io, now);
        }
        if left {
            self.toggle_drawer(true, io.applets);
        }
        if right {
            self.toggle_drawer(false, io.applets);
        }
        if timeline {
            let open = !self.timeline_open;
            self.set_timeline(open);
        }
        if reset {
            self.handle(ShellRequest::ResetCamera, io, now);
        }
        if open {
            io.requests
                .push(ShellRequest::Command("file.open".to_owned()));
        }
        if quit {
            io.requests
                .push(ShellRequest::Command("app.quit".to_owned()));
        }
        if typing || self.palette.is_some() {
            self.space_down = false;
            return;
        }
        let (palette, delete, escape) = ctx.input_mut(|input| {
            (
                keys::consume(input, &keymap.command_palette)
                    || keys::consume(input, &keymap.command_palette_alt),
                keys::consume(input, &keymap.delete),
                input.key_pressed(egui::Key::Escape) && self.radial.is_none(),
            )
        });
        if palette {
            self.open_palette();
        }
        if delete && let Some(node) = self.selected {
            io.typing.flush(io.history);
            if let Err(error) = io.history.apply(io.world, RemoveNode::new(node)) {
                self.notify(format!("Could not remove it: {error}"), true);
            }
            self.select(None, io.world);
        }
        if escape {
            if self.window.is_some() && self.window_appear.target() > 0.0 {
                self.close_window();
            } else {
                self.select(None, io.world);
            }
        }
        // Space: hold and drag pans; a tap plays or pauses the timeline.
        if let Some(space) = keys::egui_key(keymap.play_pause.key) {
            let (pressed, released, down) = ctx.input(|input| {
                (
                    input.key_pressed(space),
                    input.key_released(space),
                    input.key_down(space),
                )
            });
            if pressed && !self.space_down {
                self.space_down = true;
                self.space_panned = false;
            }
            if released || (!down && self.space_down) {
                if self.space_down && !self.space_panned && self.timeline_open {
                    self.toggle_play(io);
                }
                self.space_down = false;
            }
        }
    }

    fn toggle_play(&mut self, io: &mut ShellIo<'_>) {
        self.timeline.playing = !self.timeline.playing;
        if self.timeline.playing {
            self.sync_timeline_audio(io);
        } else if io
            .transport
            .clip()
            .is_some_and(|clip| clip.owner.starts_with(TIMELINE_MODE))
        {
            io.transport.set_playing(false, io.audio.as_deref_mut());
            self.timeline_clip = None;
        }
        self.touch();
    }

    /// Plays the audio block under the playhead through the transport.
    fn sync_timeline_audio(&mut self, io: &mut ShellIo<'_>) {
        let playhead = self.timeline.playhead;
        let under = io
            .world
            .scene
            .iter()
            .filter(|(_, node)| {
                io.world
                    .arena
                    .get(node.entity)
                    .is_ok_and(|entity| entity.projections().contains(Projection::Timeline))
            })
            .filter_map(|(id, node)| {
                let pcm = pcm_of(io.world, node.entity)?;
                let rate = f64::from(pcm.sample_rate().max(1));
                let start_frame = node.prop("clip_start", 0.0).max(0.0);
                let end_frame = node
                    .prop("clip_end", pcm.frames() as f64)
                    .min(pcm.frames() as f64);
                let t0 = node.prop("t0", 0.0);
                let seconds = (end_frame - start_frame).max(0.0) / rate;
                (playhead >= t0 && playhead < t0 + seconds).then(|| {
                    (
                        id,
                        Clip {
                            source: node.entity,
                            start: start_frame as usize,
                            end: end_frame as usize,
                            gain: node.prop("gain", 1.0) as f32,
                            owner: format!("{TIMELINE_MODE}:{}", id.0),
                        },
                        start_frame + (playhead - t0) * rate,
                    )
                })
            })
            .next();
        match under {
            Some((id, clip, frame)) if self.timeline_clip != Some(id) => {
                if io.audio.is_none() {
                    io.requests
                        .push(ShellRequest::Command("audio.start".to_owned()));
                }
                io.transport.load(clip, io.audio.as_deref_mut());
                io.transport.seek(frame, io.audio.as_deref_mut());
                io.transport.set_playing(true, io.audio.as_deref_mut());
                self.timeline_clip = Some(id);
            }
            Some(_) => {}
            None => {
                if self.timeline_clip.take().is_some() {
                    io.transport.set_playing(false, io.audio.as_deref_mut());
                }
            }
        }
    }

    // ----- Canvas ---------------------------------------------------------------

    fn draw_grid(&self, painter: &egui::Painter, theme: &Theme) {
        let style = self.config.canvas.grid_style;
        if style == GridStyle::None {
            return;
        }
        let zoom = self.camera.zoom();
        let mut spacing = self.config.canvas.grid_spacing.max(4.0);
        while spacing * zoom < 14.0 {
            spacing *= 2.0;
        }
        while spacing * zoom > 112.0 {
            spacing /= 2.0;
        }
        let visible = self.camera.visible(self.canvas);
        let start_x = (visible.left() / spacing).floor() as i64;
        let end_x = (visible.right() / spacing).ceil() as i64;
        let start_y = (visible.top() / spacing).floor() as i64;
        let end_y = (visible.bottom() / spacing).ceil() as i64;
        if (end_x - start_x) * (end_y - start_y) > 40_000 {
            return;
        }
        match style {
            GridStyle::Dots => {
                let radius = (zoom * 0.9).clamp(0.6, 1.4);
                for gx in start_x..=end_x {
                    for gy in start_y..=end_y {
                        let point = pos2(gx as f32 * spacing, gy as f32 * spacing);
                        painter.circle_filled(
                            self.camera.to_screen(self.canvas, point),
                            radius,
                            theme.grid,
                        );
                    }
                }
            }
            GridStyle::Lines => {
                let stroke = Stroke::new(1.0, theme.grid);
                for gx in start_x..=end_x {
                    let x = self
                        .camera
                        .to_screen(self.canvas, pos2(gx as f32 * spacing, 0.0))
                        .x;
                    painter.vline(x, self.canvas.y_range(), stroke);
                }
                for gy in start_y..=end_y {
                    let y = self
                        .camera
                        .to_screen(self.canvas, pos2(0.0, gy as f32 * spacing))
                        .y;
                    painter.hline(self.canvas.x_range(), y, stroke);
                }
            }
            GridStyle::None => {}
        }
    }

    #[allow(clippy::too_many_lines, reason = "the canvas and its nodes, in order")]
    fn canvas_ui(
        &mut self,
        root: &mut Ui,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let ctx = root.ctx().clone();
        let canvas = self.canvas;
        let painter = root.painter().with_clip_rect(canvas);
        painter.rect_filled(canvas, 0.0, theme.void);
        self.draw_grid(&painter, theme);

        // Background: clicks on empty canvas deselect.
        let background = root.interact(canvas, Id::unique("sue-canvas"), Sense::click_and_drag());
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        let over_canvas = pointer.is_some_and(|at| {
            canvas.contains(at)
                && ctx
                    .layer_id_at(at)
                    .is_none_or(|layer| layer.order == Order::Background)
        });
        let modal = self.palette.is_some() || self.window.is_some() || self.radial.is_some();

        // Wheel and pinch zoom around the pointer.
        if over_canvas && !modal {
            let line = self.config.wheel_line_px.max(1.0);
            let (notches, pinch) = ctx.input_mut(|input| {
                let mut notches = 0.0;
                input.events.retain(|event| match event {
                    egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } if !modifiers.command => {
                        notches += match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / line,
                            egui::MouseWheelUnit::Page => delta.y * 10.0,
                        };
                        false
                    }
                    _ => true,
                });
                input.smooth_scroll_delta = Vec2::ZERO;
                (notches, input.zoom_delta())
            });
            if let Some(at) = pointer {
                if notches.abs() > 0.0 {
                    self.camera
                        .zoom_at(canvas, at, notches * self.config.canvas.zoom_step);
                    self.touch();
                }
                if (pinch - 1.0).abs() > 1e-4 {
                    self.camera.zoom_at(canvas, at, pinch.ln());
                    self.touch();
                }
            }
        }
        // Middle-drag pans from anywhere on the canvas.
        let (middle_pressed, middle_down, delta, secondary) = ctx.input(|input| {
            (
                input.pointer.button_pressed(egui::PointerButton::Middle),
                input.pointer.middle_down(),
                input.pointer.delta(),
                input.pointer.button_pressed(egui::PointerButton::Secondary),
            )
        });
        if middle_pressed && over_canvas {
            self.middle_pan = true;
        }
        if !middle_down {
            self.middle_pan = false;
        }
        if self.middle_pan && delta != Vec2::ZERO {
            self.camera.pan_by(delta);
            self.touch();
        }
        // Right-click opens the marking menu.
        if secondary
            && over_canvas
            && !modal
            && let Some(at) = pointer
        {
            self.open_radial(at);
        }

        // The scene, in a layer the camera transforms.
        let transform: TSTransform = self.camera.transform(canvas);
        let layer = LayerId::new(root.layer_id().order, Id::unique("sue-scene"));
        ctx.set_sublayer(root.layer_id(), layer);
        ctx.set_transform_layer(layer, transform);
        let visible = self.camera.visible(canvas);
        let mut scene_ui = root.new_child(
            UiBuilder::new()
                .layer_id(layer)
                .max_rect(visible)
                .sense(Sense::hover()),
        );
        scene_ui.set_clip_rect(visible);
        let zoom = self.camera.zoom();
        let hair = theme.border_width / zoom.max(1e-3);
        let mut clicked_node = None;
        for id in io.world.scene.stacked() {
            let Some(node) = io.world.scene.get(id).cloned() else {
                continue;
            };
            if !node.rect.intersects(visible) {
                continue;
            }
            let selected = self.selected == Some(id);
            let card = node.rect;
            let title = Rect::from_min_size(card.min, vec2(card.width(), NODE_TITLE));
            let content = Rect::from_min_max(pos2(card.left(), title.bottom()), card.max);
            let scene_painter = scene_ui.painter().clone();
            let radius = egui::CornerRadius::same(theme.pill_radius.clamp(0.0, 64.0) as u8);
            scene_painter.rect_filled(card, radius, theme.pill);
            scene_painter.hline(
                card.x_range(),
                title.bottom(),
                Stroke::new(hair, theme.border),
            );
            let name = io
                .world
                .arena
                .get(node.entity)
                .map_or_else(|_| "Missing".to_owned(), |entity| entity.name().to_owned());
            let kind = io
                .applets
                .manifest(&node.applet)
                .map_or_else(String::new, |manifest| manifest.name.clone());
            let title_painter = scene_painter.with_clip_rect(title.intersect(visible));
            title_painter.text(
                title.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                &name,
                theme.font(theme.small_size),
                if selected { theme.text } else { theme.muted },
            );
            let kind_rect = title_painter.text(
                title.right_center() - vec2(10.0, 0.0),
                Align2::RIGHT_CENTER,
                &kind,
                theme.font(theme.small_size),
                theme.muted,
            );
            if io
                .world
                .arena
                .get(node.entity)
                .is_ok_and(|entity| entity.projections().contains(Projection::Timeline))
            {
                // On the timeline: a dot before the kind.
                title_painter.circle_filled(
                    pos2(kind_rect.left() - 8.0, title.center().y),
                    2.5,
                    theme.text,
                );
            }

            // Title bar: select, focus, move.
            let title_response = scene_ui.interact(
                title,
                Id::unique(("sue-node-title", id.0)),
                Sense::click_and_drag(),
            );
            if title_response.clicked() || title_response.drag_started() {
                clicked_node = Some(id);
            }
            if title_response.double_clicked() {
                self.handle(ShellRequest::Focus(id), io, now);
            }
            if title_response.drag_started() {
                self.node_drag = Some(NodeDrag {
                    node: id,
                    from: card,
                    resize: false,
                });
            }
            if title_response.dragged()
                && let Some(drag) = self
                    .node_drag
                    .filter(|drag| drag.node == id && !drag.resize)
                && let Some(live) = io.world.scene.get_mut(drag.node)
            {
                live.rect = live.rect.translate(title_response.drag_delta());
            }
            if title_response.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }

            // Content, drawn by the applet.
            let mut content_ui = scene_ui.new_child(
                UiBuilder::new()
                    .max_rect(content)
                    .id_salt(("sue-node", id.0))
                    .layout(Layout::top_down(Align::Min)),
            );
            content_ui.set_clip_rect(content.intersect(visible));
            let view = self.view(now, dt);
            {
                let (applets, mut cx) = split(io, theme, view);
                if let Some(applet) = applets.get_mut(&node.applet) {
                    applet.node_ui(&mut content_ui, id, &mut cx);
                }
            }
            let pressed_inside = ctx.input(|input| input.pointer.any_pressed())
                && content_ui
                    .ctx()
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|at| {
                        let at = transform.inverse() * at;
                        content.contains(at) && visible.contains(at)
                    })
                && over_canvas;
            if pressed_inside {
                clicked_node = Some(id);
            }

            // Resize handle.
            let grip = Rect::from_min_max(card.max - vec2(14.0, 14.0), card.max);
            let grip_response =
                scene_ui.interact(grip, Id::unique(("sue-node-grip", id.0)), Sense::drag());
            if grip_response.hovered() || grip_response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            if grip_response.drag_started() {
                self.node_drag = Some(NodeDrag {
                    node: id,
                    from: card,
                    resize: true,
                });
            }
            if grip_response.dragged()
                && let Some(live) = io.world.scene.get_mut(id)
            {
                let size = (live.rect.size() + grip_response.drag_delta()).max(vec2(180.0, 110.0));
                live.rect = Rect::from_min_size(live.rect.min, size);
            }
            // Border on top of the content.
            let border = if selected {
                Stroke::new(hair * 1.5, theme.border_active)
            } else {
                Stroke::new(hair, theme.border)
            };
            scene_ui
                .painter()
                .rect_stroke(card, radius, border, StrokeKind::Inside);
            for step in [4.0, 8.0] {
                scene_ui.painter().line_segment(
                    [
                        pos2(card.right() - step - 3.0, card.bottom() - 3.0),
                        pos2(card.right() - 3.0, card.bottom() - step - 3.0),
                    ],
                    Stroke::new(hair, theme.muted),
                );
            }
        }
        // A finished move or resize becomes one undo step.
        if let Some(drag) = self.node_drag
            && !ctx.input(|input| input.pointer.primary_down())
        {
            self.node_drag = None;
            if let Some(node) = io.world.scene.get(drag.node)
                && node.rect != drag.from
            {
                io.typing.flush(io.history);
                io.history
                    .record(SetNodeRect::new(drag.node, drag.from, node.rect));
            }
        }
        if let Some(node) = clicked_node {
            self.select(Some(node), io.world);
        } else if background.clicked() && !modal {
            self.select(None, io.world);
        }
    }

    // ----- Rails, drawers, status ----------------------------------------------

    fn rail_ui(
        &mut self,
        root: &mut Ui,
        rect: Rect,
        left: bool,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        let role = if left {
            AppletRole::LeftDrawer
        } else {
            AppletRole::RightDrawer
        };
        let entries: Vec<(AppletId, String, String)> = io
            .applets
            .with_role(role)
            .map(|manifest| {
                (
                    manifest.id.clone(),
                    manifest.icon.clone(),
                    manifest.name.clone(),
                )
            })
            .collect();
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Center)),
        );
        ui.spacing_mut().item_spacing.y = 2.0;
        let drawer = if left { &self.left } else { &self.right };
        let mut clicked = None;
        for (id, icon, name) in entries {
            let chosen = drawer.open && drawer.shown.as_ref() == Some(&id);
            if widgets::rail_button(&mut ui, &icon, &name, chosen, left, theme).clicked() {
                clicked = Some(id);
            }
        }
        if let Some(id) = clicked {
            self.toggle_drawer_applet(&id, io.applets);
        }
        if left {
            // Settings at the foot of the left rail.
            let gear =
                Rect::from_min_max(pos2(rect.left(), rect.bottom() - rect.width()), rect.max);
            let mut foot = root.new_child(UiBuilder::new().max_rect(gear));
            let open = self.window.is_some() && self.window_appear.target() > 0.0;
            if widgets::rail_button(
                &mut foot,
                "\u{2699}",
                "Settings (Ctrl+,)",
                open,
                true,
                theme,
            )
            .clicked()
            {
                if open {
                    self.close_window();
                } else {
                    io.requests.push(ShellRequest::OpenSettings(None));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments, reason = "a drawer and where it is")]
    fn drawer_ui(
        &mut self,
        root: &mut Ui,
        visible: Rect,
        full: Rect,
        left: bool,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        if visible.width() < 0.5 {
            return;
        }
        let drawer = if left { &self.left } else { &self.right };
        let Some(id) = drawer.shown.clone() else {
            return;
        };
        let painter = root.painter().with_clip_rect(visible);
        painter.rect_filled(full, 0.0, theme.panel);
        let edge = if left {
            visible.right()
        } else {
            visible.left()
        };
        root.painter()
            .vline(edge, visible.y_range(), theme.hairline());
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(full.shrink(theme.padding))
                .id_salt(("sue-drawer", id.as_str()))
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(visible);
        let name = io
            .applets
            .manifest(&id)
            .map_or_else(String::new, |manifest| manifest.name.clone());
        let mut close = false;
        ui.horizontal(|ui| {
            widgets::heading(ui, &name, theme);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("\u{00D7}")
                                .size(theme.ui_size * 1.3)
                                .color(theme.muted),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Close")
                    .clicked()
                {
                    close = true;
                }
            });
        });
        ui.add_space(theme.padding * 0.5);
        let view = self.view(now, dt);
        ScrollArea::vertical()
            .id_salt(("sue-drawer-scroll", id.as_str()))
            .auto_shrink([false, false])
            .show(&mut ui, |ui| {
                let (applets, mut cx) = split(io, theme, view);
                if let Some(applet) = applets.get_mut(&id) {
                    applet.drawer_ui(ui, &mut cx);
                }
            });
        if close {
            self.toggle_drawer_applet(&id, io.applets);
        }
    }

    fn status_ui(&mut self, root: &mut Ui, rect: Rect, io: &mut ShellIo<'_>, theme: &Theme) {
        let painter = root.painter().with_clip_rect(rect);
        let font = theme.font(theme.small_size);
        let mode_label = Self::mode_buttons(io.applets)
            .into_iter()
            .find(|mode| mode.id == self.mode)
            .map_or_else(|| "Canvas".to_owned(), |mode| mode.label);
        let selection = self
            .selected
            .and_then(|node| io.world.scene.get(node))
            .and_then(|node| io.world.arena.get(node.entity).ok())
            .map(|entity| entity.name().to_owned());
        let mut left = mode_label;
        if let Some(name) = selection {
            left.push_str("    ");
            left.push_str(&name);
        }
        if let Some((text, error)) = self.message() {
            left.push_str("    ");
            if error {
                left.push_str("! ");
            }
            left.push_str(text);
        }
        painter.text(
            rect.left_center() + vec2(theme.padding, 0.0),
            Align2::LEFT_CENTER,
            left,
            font.clone(),
            theme.muted,
        );
        let items = [
            format!("{} entities", io.world.arena.len()),
            format!("Undo {}", io.history.depth()),
            profile_label(self.profile),
            format!("{:.0}%", self.camera.zoom() * 100.0),
            match io.fps {
                Some(fps) if self.animating => format!("{fps} FPS"),
                _ => "Idle".to_owned(),
            },
        ];
        let mut x = rect.right() - theme.padding;
        for (index, item) in items.iter().enumerate().rev() {
            let placed = painter.text(
                pos2(x, rect.center().y),
                Align2::RIGHT_CENTER,
                item,
                font.clone(),
                theme.muted,
            );
            x = placed.left() - theme.padding;
            if index > 0 {
                painter.vline(x, rect.shrink2(vec2(0.0, 5.0)).y_range(), theme.hairline());
                x -= theme.padding;
            }
        }
    }

    // ----- Timeline dock ----------------------------------------------------------

    fn timeline_dock(
        &mut self,
        root: &mut Ui,
        visible: Rect,
        full: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        // Lanes by title, in applet order; applets may share one.
        let mut titles: Vec<String> = Vec::new();
        let mut lanes: Vec<(AppletId, usize)> = Vec::new();
        for manifest in io.applets.manifests() {
            if let Some(title) = &manifest.lane {
                let index = titles
                    .iter()
                    .position(|seen| seen == title)
                    .unwrap_or_else(|| {
                        titles.push(title.clone());
                        titles.len() - 1
                    });
                lanes.push((manifest.id.clone(), index));
            }
        }
        let blocks: Vec<Block> = io
            .world
            .scene
            .iter()
            .filter_map(|(id, node)| {
                let entity = io.world.arena.get(node.entity).ok()?;
                if !entity.projections().contains(Projection::Timeline) {
                    return None;
                }
                let lane = lanes
                    .iter()
                    .find(|(applet, _)| applet == &node.applet)
                    .map(|(_, lane)| *lane)?;
                let duration = io
                    .applets
                    .get(&node.applet)
                    .map_or(4.0, |applet| applet.duration(node, io.world));
                Some(Block {
                    node: id,
                    lane,
                    start: node.prop("t0", 0.0),
                    duration,
                    label: entity.name().to_owned(),
                    selected: self.selected == Some(id),
                })
            })
            .collect();
        let mut ui = root.new_child(UiBuilder::new().max_rect(full).id_salt("sue-timeline"));
        ui.set_clip_rect(visible);
        let events = timeline_ui(&mut ui, full, &mut self.timeline, &titles, &blocks, theme);
        for event in events {
            match event {
                TimelineEvent::Seek(at) => {
                    self.timeline.playhead = at;
                    self.timeline_clip = None;
                    if self.timeline.playing {
                        self.sync_timeline_audio(io);
                    }
                }
                TimelineEvent::TogglePlay => self.toggle_play(io),
                TimelineEvent::Select(node) => self.select(Some(node), io.world),
                TimelineEvent::Moved { node, to, .. } => {
                    io.typing.flush(io.history);
                    let txn = SetNodeProp::new(node, "t0", Some(to), "Move on timeline");
                    if let Err(error) = io.history.apply(io.world, txn) {
                        self.notify(format!("Could not move it: {error}"), true);
                    }
                }
            }
            self.touch();
        }
        let _ = now;
    }

    // ----- Floating pills -----------------------------------------------------------

    fn top_pill_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        let appear = self.top_pill.position().clamp(0.0, 1.0);
        let at = pos2(
            self.canvas.center().x,
            self.canvas.top() + theme.padding * 1.5 - (1.0 - appear) * 16.0,
        );
        let modes = Self::mode_buttons(io.applets);
        let spawns: Vec<(AppletId, String)> = io
            .applets
            .with_role(AppletRole::Node)
            .filter_map(|manifest| {
                Some((manifest.id.clone(), manifest.spawn.as_ref()?.button.clone()))
            })
            .collect();
        let mut picked_mode = None;
        let mut picked_spawn = None;
        let mut palette = false;
        egui::Area::new(Id::unique("sue-top-pill"))
            .order(Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(at)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::pill_frame(theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for mode in &modes {
                            let chosen = self.mode == mode.id
                                || (mode.id == TIMELINE_MODE
                                    && self.timeline_open
                                    && self.mode == TIMELINE_MODE);
                            if widgets::pill_button(ui, &mode.label, chosen, theme).clicked() {
                                picked_mode = Some(mode.id.clone());
                            }
                        }
                        widgets::pill_divider(ui, theme);
                        for (id, label) in &spawns {
                            if widgets::pill_button(ui, label, false, theme)
                                .on_hover_text("Create a new one in view")
                                .clicked()
                            {
                                picked_spawn = Some(id.clone());
                            }
                        }
                        widgets::pill_divider(ui, theme);
                        let hint = format!("Command ({})", self.config.keymap.command_palette);
                        if widgets::pill_button(ui, &hint, self.palette.is_some(), theme).clicked()
                        {
                            palette = true;
                        }
                    });
                });
            });
        if let Some(mode) = picked_mode {
            if mode == TIMELINE_MODE && self.mode == TIMELINE_MODE {
                self.set_timeline(!self.timeline_open);
            } else {
                self.set_mode(&mode, io, now);
            }
        }
        if let Some(id) = picked_spawn
            && let Some(node) = self.spawn(&id, None, io, now)
        {
            self.frame_node(node, io.world, None);
        }
        if palette {
            self.open_palette();
        }
    }

    fn selection_pill_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let Some(node) = self.pill.node else {
            return;
        };
        let Some(applet) = io.world.scene.get(node).map(|n| n.applet.clone()) else {
            self.pill.node = None;
            return;
        };
        let Some(anchor) = self.pill_anchor(node, io.world) else {
            return;
        };
        self.pill.anchor = Some(anchor);
        let appear = self.pill.appear.position().clamp(0.0, 1.0);
        let at = anchor
            + vec2(self.pill.glide_x.position(), self.pill.glide_y.position())
            + vec2(0.0, (1.0 - appear) * 10.0);
        let view = self.view(now, dt);
        egui::Area::new(Id::unique("sue-selection-pill"))
            .order(Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(at)
            .interactable(appear > 0.5)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::pill_frame(theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let (applets, mut cx) = split(io, theme, view);
                        if let Some(applet) = applets.get_mut(&applet) {
                            applet.pill_ui(ui, node, &mut cx);
                        }
                    });
                });
            });
    }

    // ----- Marking menu, palette, window --------------------------------------------

    fn radial_items(applets: &Applets) -> Vec<(String, ShellRequest)> {
        let mut items: Vec<(String, ShellRequest)> = applets
            .with_role(AppletRole::Node)
            .filter_map(|manifest| {
                Some((
                    manifest.spawn.as_ref()?.label.clone(),
                    ShellRequest::Spawn(manifest.id.clone()),
                ))
            })
            .collect();
        items.push(("Toggle Timeline".to_owned(), ShellRequest::Timeline(true)));
        items.push(("Command Palette".to_owned(), ShellRequest::Palette));
        items.push(("Reset Camera".to_owned(), ShellRequest::ResetCamera));
        items
    }

    fn radial_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        let Some(radial) = self.radial.as_mut() else {
            return;
        };
        let items = Self::radial_items(io.applets);
        let labels: Vec<String> = items.iter().map(|(label, _)| label.clone()).collect();
        let canvas_point = radial.canvas_point;
        let outcome = radial.ui(ctx, screen, &labels, self.config.radial_radius, theme);
        self.touch();
        if let RadialOutcome::Picked(index) = outcome
            && let Some((_, request)) = items.into_iter().nth(index)
        {
            match request {
                ShellRequest::Spawn(id) => {
                    if let Some(node) = self.spawn(&id, Some(canvas_point), io, now) {
                        self.select(Some(node), io.world);
                    }
                }
                ShellRequest::Timeline(_) => {
                    let open = !self.timeline_open;
                    self.set_timeline(open);
                }
                other => {
                    self.handle(other, io, now);
                }
            }
        }
    }

    /// Every palette entry: shell actions, modes, spawns, drawers and the
    /// applets' own commands.
    fn palette_entries(&self, applets: &Applets) -> Vec<(PaletteEntry, PaletteAction)> {
        let keymap = &self.config.keymap;
        let mut entries = vec![
            (
                entry("Toggle Left Drawer", keymap.toggle_left_drawer.to_string()),
                PaletteAction::Drawer(true),
            ),
            (
                entry(
                    "Toggle Right Drawer",
                    keymap.toggle_right_drawer.to_string(),
                ),
                PaletteAction::Drawer(false),
            ),
            (
                entry("Toggle Timeline", keymap.toggle_timeline.to_string()),
                PaletteAction::Request(ShellRequest::Timeline(!self.timeline_open)),
            ),
            (
                entry("Open Settings", keymap.open_settings.to_string()),
                PaletteAction::Request(ShellRequest::OpenSettings(None)),
            ),
            (
                entry("Reset Camera", keymap.reset_camera.to_string()),
                PaletteAction::Request(ShellRequest::ResetCamera),
            ),
            (
                entry("Undo", keymap.undo.to_string()),
                PaletteAction::Undo(false),
            ),
            (
                entry("Redo", keymap.redo.to_string()),
                PaletteAction::Undo(true),
            ),
            (
                entry("Open File", keymap.open_file.to_string()),
                PaletteAction::Request(ShellRequest::Command("file.open".to_owned())),
            ),
            (
                entry("Quit Sue", keymap.quit.to_string()),
                PaletteAction::Request(ShellRequest::Command("app.quit".to_owned())),
            ),
        ];
        for mode in Self::mode_buttons(applets) {
            entries.push((
                entry(&format!("Mode: {}", mode.label), String::new()),
                PaletteAction::Request(ShellRequest::Mode(mode.id)),
            ));
        }
        for manifest in applets.manifests() {
            if let Some(mode) = manifest.mode.as_ref().filter(|mode| !mode.in_pill) {
                entries.push((
                    entry(&format!("Mode: {}", mode.label), String::new()),
                    PaletteAction::Request(ShellRequest::Mode(manifest.id.as_str().to_owned())),
                ));
            }
            if let Some(spawn) = &manifest.spawn {
                entries.push((
                    entry(&spawn.label, String::new()),
                    PaletteAction::Request(ShellRequest::Spawn(manifest.id.clone())),
                ));
            }
            if matches!(
                manifest.role,
                AppletRole::LeftDrawer | AppletRole::RightDrawer
            ) {
                entries.push((
                    entry(&format!("Show {}", manifest.name), String::new()),
                    PaletteAction::Request(ShellRequest::ShowDrawer(manifest.id.clone())),
                ));
            }
        }
        if self.selected.is_some() {
            entries.push((
                entry("Remove Selection from Canvas", keymap.delete.to_string()),
                PaletteAction::RemoveSelection,
            ));
        }
        for manifest in applets.manifests() {
            let commands = applets
                .get(&manifest.id)
                .map_or_else(Vec::new, |applet| applet.commands());
            for command in commands {
                entries.push((
                    PaletteEntry {
                        label: format!("{}: {}", manifest.name, command.label),
                        command: command.command.clone(),
                        hint: command.hint.clone(),
                    },
                    PaletteAction::Applet(manifest.id.clone(), command.command),
                ));
            }
        }
        entries
    }

    fn palette_ui(
        &mut self,
        ctx: &egui::Context,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
    ) {
        if self.palette.is_none() {
            return;
        }
        let entries = self.palette_entries(io.applets);
        let shown: Vec<PaletteEntry> = entries.iter().map(|(entry, _)| entry.clone()).collect();
        let canvas = self.canvas;
        let outcome = self
            .palette
            .as_mut()
            .map_or(PaletteOutcome::Open, |palette| {
                palette.ui(ctx, canvas, &shown, theme)
            });
        self.touch();
        if let PaletteOutcome::Run(index) = outcome
            && let Some((_, action)) = entries.into_iter().nth(index)
        {
            match action {
                PaletteAction::Request(request) => {
                    if let Some(request) = self.handle(request, io, now) {
                        io.requests.push(request);
                    }
                }
                PaletteAction::Drawer(left) => self.toggle_drawer(left, io.applets),
                PaletteAction::Undo(redo) => self.undo(io, redo),
                PaletteAction::RemoveSelection => {
                    if let Some(node) = self.selected {
                        io.typing.flush(io.history);
                        let _ = io.history.apply(io.world, RemoveNode::new(node));
                        self.select(None, io.world);
                    }
                }
                PaletteAction::Applet(id, command) => {
                    let view = self.view(now, 0.0);
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id)
                        && !applet.command(&command, &mut cx)
                    {
                        cx.fail(format!("{command} is not available"));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments, reason = "a window over the screen")]
    fn window_ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let Some(id) = self.window.clone() else {
            return;
        };
        let appear = self.window_appear.position().clamp(0.0, 1.0);
        // A scrim that closes the window when clicked.
        let scrim = egui::Area::new(Id::unique("sue-window-scrim"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.45 * appear));
                response.clicked()
            })
            .inner;
        if scrim {
            self.close_window();
        }
        let size = vec2(
            (screen.width() - 48.0).clamp(320.0, 980.0),
            (screen.height() - 64.0).clamp(320.0, 700.0),
        );
        let at = screen.center() - size / 2.0 + vec2(0.0, (1.0 - appear) * 18.0);
        let name = io
            .applets
            .manifest(&id)
            .map_or_else(String::new, |manifest| manifest.name.clone());
        let view = self.view(now, dt);
        let mut close = false;
        egui::Area::new(Id::unique("sue-window"))
            .order(Order::Foreground)
            .fixed_pos(at)
            .interactable(appear > 0.3)
            .show(ctx, |ui| {
                ui.set_opacity(appear);
                widgets::window_frame(theme).show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    let bar = Rect::from_min_size(ui.min_rect().min, vec2(size.x, 36.0));
                    let mut bar_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(bar.shrink2(vec2(theme.padding * 1.5, 0.0)))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    bar_ui.label(RichText::new(&name).size(theme.ui_size).color(theme.text));
                    bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{00D7}")
                                        .size(theme.ui_size * 1.3)
                                        .color(theme.muted),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Close (Esc)")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                    ui.painter()
                        .hline(bar.x_range(), bar.bottom(), theme.hairline());
                    let body =
                        Rect::from_min_max(pos2(bar.left(), bar.bottom()), bar.left_top() + size);
                    let mut body_ui = ui.new_child(
                        UiBuilder::new()
                            .max_rect(body)
                            .id_salt(("sue-window-body", id.as_str()))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    body_ui.set_clip_rect(body);
                    let (applets, mut cx) = split(io, theme, view);
                    if let Some(applet) = applets.get_mut(&id) {
                        applet.window_ui(&mut body_ui, &mut cx);
                    }
                    ui.allocate_rect(Rect::from_min_size(bar.min, size), Sense::hover());
                });
            });
        if close {
            self.close_window();
        }
    }

    /// Runs every applet's tick. Returns what they asked for.
    pub fn tick(&mut self, io: &mut ShellIo<'_>, now: Instant) -> crate::Tick {
        let view = self.view(now, 0.0);
        let theme = self.theme.clone();
        let (applets, mut cx) = split(io, &theme, view);
        let mut tick = crate::Tick::default();
        for (_, applet) in applets.iter_mut() {
            tick = tick.merge(applet.tick(&mut cx));
        }
        tick
    }

    /// Runs `work` with an applet context over `io`, as an applet would see
    /// it this frame (scripted runs, snapshots and tests).
    pub fn with_context<R>(
        &self,
        io: &mut ShellIo<'_>,
        now: Instant,
        work: impl FnOnce(&mut AppletContext<'_>) -> R,
    ) -> R {
        let view = self.view(now, 0.0);
        let (_, mut cx) = split(io, &self.theme, view);
        work(&mut cx)
    }

    /// Moves the timeline playhead.
    pub fn seek(&mut self, seconds: f64) {
        self.timeline.playhead = seconds.max(0.0);
        self.timeline_clip = None;
        self.touch();
    }

    /// Steps the shell's springs without drawing (headless scripted runs).
    pub fn settle(&mut self, dt: f32) -> bool {
        let mut moving = false;
        for spring in [
            &mut self.left.reveal,
            &mut self.right.reveal,
            &mut self.timeline_reveal,
            &mut self.pill.appear,
            &mut self.pill.glide_x,
            &mut self.pill.glide_y,
            &mut self.top_pill,
            &mut self.window_appear,
        ] {
            moving |= spring.step(dt);
        }
        moving |= self.camera.step(dt, self.canvas);
        moving
    }

    /// Sets the canvas area (headless runs that never draw).
    pub fn set_canvas(&mut self, canvas: Rect) {
        self.canvas = canvas;
    }

    /// The timeline's reveal, 0 to 1.
    #[must_use]
    pub fn timeline_reveal(&self) -> f32 {
        self.timeline_reveal.position()
    }

    /// The left drawer's reveal, 0 to 1.
    #[must_use]
    pub fn left_reveal(&self) -> f32 {
        self.left.reveal.position()
    }

    /// Duration of the grace period.
    #[must_use]
    pub fn settle_grace(&self) -> Duration {
        self.config.settle_grace
    }
}

fn entry(label: &str, hint: String) -> PaletteEntry {
    PaletteEntry {
        label: label.to_owned(),
        command: label.to_owned(),
        hint,
    }
}

/// What a palette entry does.
#[derive(Clone, Debug, PartialEq)]
enum PaletteAction {
    Request(ShellRequest),
    Drawer(bool),
    Undo(bool),
    RemoveSelection,
    Applet(AppletId, String),
}

/// A non-destructive Timeline projection of `node`'s entity, placed after
/// the last block in its lane, as one undo step; opens the timeline.
pub fn project_to_timeline(cx: &mut AppletContext<'_>, node: NodeId, seconds: f64) {
    let Some(scene_node) = cx.world.scene.get(node).cloned() else {
        return;
    };
    let already = cx
        .world
        .arena
        .get(scene_node.entity)
        .is_ok_and(|entity| entity.projections().contains(Projection::Timeline));
    let lane_end = cx
        .world
        .scene
        .iter()
        .filter(|(id, other)| *id != node && other.applet == scene_node.applet)
        .filter(|(_, other)| {
            cx.world
                .arena
                .get(other.entity)
                .is_ok_and(|entity| entity.projections().contains(Projection::Timeline))
        })
        .map(|(_, other)| other.prop("t0", 0.0) + other.prop("dur", 4.0))
        .fold(0.0_f64, f64::max);
    let start = if already {
        scene_node.prop("t0", 0.0)
    } else {
        lane_end
    };
    let mut batch = Batch::new("Project to Timeline")
        .then(SetNodeProp::new(node, "t0", Some(start), "Timeline start"))
        .then(SetNodeProp::new(
            node,
            "dur",
            Some(seconds),
            "Timeline length",
        ));
    if !already {
        match TimeSpan::from_seconds(start, seconds.max(0.01)) {
            Ok(span) => {
                batch = batch.then(ArenaEdit(sue_core::txn::AttachProjection::new(
                    scene_node.entity,
                    Projection::Timeline,
                    span,
                )));
            }
            Err(error) => {
                cx.fail(error.to_string());
                return;
            }
        }
    }
    match cx.apply(batch) {
        Ok(_) => cx.request(ShellRequest::Timeline(true)),
        Err(error) => cx.fail(format!("Could not project it: {error}")),
    }
}
