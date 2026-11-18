//! Sue VE: what it holds and what its commands do. Drawing is in `ui`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use std::time::Instant;

use sue_core::{Entity, EntityId, Rate, Time, Value};
use sue_model::{Clip, Sequence, SequenceEdit};
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, ArenaEdit, FilePick, Menu, MenuBar,
    PaletteEntry, ShellRequest, SueAction, SueApplet, TransferKind, TransferState, sue_menu,
};

use crate::edit::{self, EditCtx, EditError};
use crate::export::{self, Target};
use crate::media::{MediaHub, Quality, import};
use crate::model::{self, SourceKind, key};
use crate::playback::Clock;
use crate::ui::Panel;
use sue_wm::dock::DockLayout;

/// The applet id.
pub const ID: &str = "sue-ve";
/// The settings section the shortcuts live in.
pub const KEYS_SECTION: &str = "sue_ve_keys";
/// Rice keys.
pub mod rice {
    /// Decode on the graphics card.
    pub const HARDWARE: &str = "sue_ve.hardware_decode";
    /// The frame cache's budget in megabytes.
    pub const CACHE_MB: &str = "sue_ve.cache_mb";
    /// The monitors' decode quality.
    pub const QUALITY: &str = "sue_ve.preview_quality";
    /// Keep imported files inside the project.
    pub const EMBED: &str = "sue_ve.embed_media";
    /// Sue VE's colors.
    pub const THEME: &str = "sue_ve.theme";
    /// Draw what needs the processor ahead of the playhead while paused.
    pub const BACKGROUND_RENDER: &str = "sue_ve.background_render";
    /// The render cache's budget in megabytes.
    pub const RENDER_CACHE_MB: &str = "sue_ve.render_cache_mb";
}

pub(crate) fn me() -> AppletId {
    AppletId::new(ID).expect("a valid id")
}

/// The pages along the bottom, as DaVinci Resolve has them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// The media pool, large, with the source monitor.
    Media,
    /// Editing: pool, monitors, timeline, inspector.
    Edit,
    /// Effects and compositing (stage 2).
    Effects,
    /// Color grading (stage 2).
    Color,
    /// The mixer (stage 2).
    Audio,
    /// Export.
    Deliver,
}

impl Page {
    /// Every page, left to right.
    pub const ALL: [Self; 6] = [
        Self::Media,
        Self::Edit,
        Self::Effects,
        Self::Color,
        Self::Audio,
        Self::Deliver,
    ];

    /// Its name.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Media => tr!("page-media"),
            Self::Edit => tr!("page-edit"),
            Self::Effects => tr!("page-effects"),
            Self::Color => tr!("page-color"),
            Self::Audio => tr!("page-audio"),
            Self::Deliver => tr!("page-deliver"),
        }
    }

    /// Its icon.
    #[must_use]
    pub fn icon(self) -> sue_wm::icons::Icon {
        use sue_wm::icons::Icon;
        match self {
            Self::Media => Icon::Files,
            Self::Edit => Icon::Film,
            Self::Effects => Icon::Effects,
            Self::Color => Icon::PaletteColorWheel,
            Self::Audio => Icon::Audio,
            Self::Deliver => Icon::Export,
        }
    }

    /// Its stable name (`navigate` targets, tests).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Edit => "edit",
            Self::Effects => "effects",
            Self::Color => "color",
            Self::Audio => "audio",
            Self::Deliver => "deliver",
        }
    }
}

/// The editing tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Select, move, trim edges.
    Select,
    /// Trim an edge and move what follows.
    Ripple,
    /// Move a cut.
    Roll,
    /// Other frames, same place.
    Slip,
    /// Move a clip between its neighbours.
    Slide,
    /// Cut clips in two.
    Razor,
    /// Change a clip's speed by dragging its edge (the same frames fill
    /// the new length).
    RateStretch,
}

impl Tool {
    /// Every tool, as the tool strip shows them.
    pub const ALL: [Self; 7] = [
        Self::Select,
        Self::Ripple,
        Self::Roll,
        Self::RateStretch,
        Self::Slip,
        Self::Slide,
        Self::Razor,
    ];

    /// Its command.
    #[must_use]
    pub fn command(self) -> &'static str {
        match self {
            Self::Select => "tool.select",
            Self::Ripple => "tool.ripple",
            Self::Roll => "tool.roll",
            Self::Slip => "tool.slip",
            Self::Slide => "tool.slide",
            Self::Razor => "tool.razor",
            Self::RateStretch => "tool.rate_stretch",
        }
    }

    /// Its icon.
    #[must_use]
    pub fn icon(self) -> sue_wm::icons::Icon {
        use sue_wm::icons::Icon;
        match self {
            Self::Select => Icon::ToolOperation,
            Self::Ripple => Icon::ToolRipple,
            Self::Roll => Icon::ToolRoll,
            Self::Slip => Icon::ToolSlip,
            Self::Slide => Icon::ToolSlide,
            Self::Razor => Icon::ToolRazor,
            Self::RateStretch => Icon::Speed,
        }
    }
}

/// Which monitor the keys drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Monitor {
    /// The source monitor (a pool item).
    Source,
    /// The program monitor (the sequence).
    Program,
}

/// The source monitor's state: the item, its clock and its marks.
#[derive(Debug, Default)]
pub struct SourceView {
    /// The pool item shown.
    pub item: Option<EntityId>,
    /// Its playhead and playing.
    pub clock: Clock,
    /// Its in mark.
    pub mark_in: Option<Time>,
    /// Its out mark.
    pub mark_out: Option<Time>,
}

/// A dialog over the studio.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    /// A new sequence: name, size and rate.
    NewSequence {
        /// Its name.
        name: String,
        /// Width.
        width: f64,
        /// Height.
        height: f64,
        /// The rate's index in [`RATES`].
        rate: usize,
    },
    /// The keyboard shortcuts.
    Shortcuts,
    /// What Sue VE is.
    About,
    /// Save the selected clip's effects as a preset: its name.
    SavePreset {
        /// The preset's name.
        name: String,
    },
}

/// The frame rates a new sequence offers.
pub const RATES: [Rate; 8] = [
    Rate::FPS_23_976,
    Rate::FPS_24,
    Rate::FPS_25,
    Rate::FPS_29_97,
    Rate::FPS_30,
    Rate::FPS_50,
    Rate::FPS_59_94,
    Rate::FPS_60,
];

/// The Deliver page's choices.
#[derive(Clone, Debug, PartialEq)]
pub struct DeliverForm {
    /// What to write.
    pub target: Target,
    /// The frame size.
    pub width: f64,
    /// See `width`.
    pub height: f64,
    /// The bit rate in megabits a second (0: by quality).
    pub megabits: f64,
    /// The whole sequence (else the in to out range).
    pub whole: bool,
    /// The MP4 carries sound.
    pub sound: bool,
    /// Frames a second (`None`: the sequence's own).
    pub rate: Option<u32>,
}

/// The Color page's state: which corrector is picked, the before and
/// after split, the scope shown, the copied grade and the open tab.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorState {
    /// The corrector being edited.
    pub corrector: usize,
    /// The viewer splits before (left) and after (right).
    pub split: bool,
    /// The scope shown.
    pub scope: crate::scopes::Scope,
    /// A copied grade.
    pub clipboard: Option<Vec<crate::grade::Corrector>>,
    /// The tab under the wheels: 0 primaries, 1 curves, 2 hue curves, 3
    /// qualifier.
    pub tab: usize,
    /// The viewer shows the qualifier's matte.
    pub matte: bool,
}

impl Default for ColorState {
    fn default() -> Self {
        Self {
            corrector: 0,
            split: false,
            scope: crate::scopes::Scope::Waveform,
            clipboard: None,
            tab: 0,
            matte: false,
        }
    }
}

/// The graph editor's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphState {
    /// The channel shown.
    pub channel: Option<String>,
    /// The speed graph (else the value graph).
    pub speed: bool,
    /// The keys picked: their source times.
    pub picked: Vec<Time>,
    /// A key being dragged: its time when the drag began, and which handle
    /// (0 the key, 1 its outgoing handle, 2 the incoming handle of the
    /// segment before it).
    pub dragging: Option<(Time, u8)>,
}

/// The phone layout's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MobileState {
    /// The tool whose sheet is open.
    pub sheet: Option<&'static str>,
    /// The export sheet is open.
    pub export: bool,
}

/// A voice over being recorded.
pub struct Recording {
    /// The microphone.
    pub input: sue_platform::AudioInput,
    /// What it heard so far, interleaved.
    pub samples: Vec<f32>,
    /// Where on the sequence it started.
    pub at: Time,
    /// The track it goes on.
    pub track: usize,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording")
            .field("at", &self.at)
            .field("samples", &self.samples.len())
            .finish_non_exhaustive()
    }
}

/// An export under way.
#[derive(Debug)]
pub struct Running {
    /// Its transfer in Sue's list.
    pub transfer: u64,
    /// Where it writes.
    pub path: PathBuf,
    /// What it reported when done.
    pub report: Arc<Mutex<Option<export::Report>>>,
}

/// A movie to make a proxy of: its id, file, size, rate and length.
type ToProxy = (EntityId, PathBuf, (u32, u32), (u32, u32), f64);

/// A proxy being made.
#[derive(Debug)]
pub struct Proxying {
    /// Its transfer in Sue's list.
    pub transfer: u64,
    /// The movie.
    pub media: EntityId,
    /// Where the proxy goes.
    pub path: PathBuf,
}

/// The video editing studio.
pub struct SueVe {
    /// The project open.
    pub project: Option<EntityId>,
    /// The sequence on the timeline.
    pub sequence: Option<EntityId>,
    /// The page showing.
    pub page: Page,
    /// The Edit page's palettes.
    pub dock: DockLayout<Panel>,
    /// Decoding, sound, thumbnails.
    pub hub: MediaHub,
    /// The program monitor's clock.
    pub clock: Clock,
    /// The source monitor.
    pub source: SourceView,
    /// Which monitor the keys drive.
    pub focus: Monitor,
    /// Selected clips.
    pub selection: Vec<u64>,
    /// The pool item selected.
    pub picked: Option<EntityId>,
    /// The tool.
    pub tool: Tool,
    /// Edges snap.
    pub snapping: bool,
    /// Linked clips select and trim together.
    pub linked: bool,
    /// Pixels a second on the timeline.
    pub zoom: f32,
    /// The timeline's first second shown.
    pub scroll: f32,
    /// The timeline's first pixel down.
    pub scroll_y: f32,
    /// The timeline's drag in progress.
    pub drag: Option<crate::ui::timeline::Drag>,
    /// The inspector's change being dragged: the sequence before it.
    pub live: Option<(EntityId, Sequence)>,
    /// The pool shows a list (else thumbnails).
    pub list: bool,
    /// The monitors' decode quality.
    pub quality: Quality,
    /// Copied clips with their tracks.
    pub clipboard: Vec<(usize, Clip)>,
    /// A dialog.
    pub dialog: Option<Dialog>,
    /// The Deliver page's choices.
    pub deliver: DeliverForm,
    /// Exports under way.
    pub exports: Vec<Running>,
    /// Proxies being made.
    pub proxying: Vec<Proxying>,
    /// The last export's report.
    pub last_report: Option<(PathBuf, export::Report)>,
    /// The menus.
    pub menubar: MenuBar,
    /// See `menubar`.
    pub menus: Vec<Menu>,
    /// The shortcuts as Settings has them.
    pub bound: sue_wm::shortcuts::Bound,
    /// When to build the menus again, in another language.
    pub language: sue_lang::Watch,
    /// Filled the page this frame, all in place.
    pub ready: bool,
    /// The program monitor's picture rect last frame (tests, notices).
    pub program_rect: Option<egui::Rect>,
    /// The timeline's rect last frame.
    pub timeline_rect: Option<egui::Rect>,
    /// The title being typed into in the inspector.
    pub title_draft: Option<(EntityId, String)>,
    /// Processed frames and the render cache.
    pub preview: crate::preview::Preview,
    /// The video scopes' worker.
    pub scopes: crate::scopes::Scopes,
    /// The frame the program monitor drew on the processor last (the
    /// scopes read it).
    pub shown: Option<Arc<crate::media::frames::VideoFrame>>,
    /// The Color page.
    pub color: ColorState,
    /// The graph editor.
    pub graph: GraphState,
    /// The effects browser's search.
    pub fx_search: String,
    /// Copied effects (Paste Attributes).
    pub fx_clipboard: Option<Vec<crate::fx::Effect>>,
    /// The effect whose parameters the graph and the controls show first.
    pub fx_picked: Option<u64>,
    /// The phone layout.
    pub mobile: MobileState,
    /// A voice over being recorded.
    pub recording: Option<Recording>,
    /// Where the timeline's zoom is going (it eases there).
    pub zoom_target: Option<f32>,
    /// The render bar's marks: what they were worked out for, and the
    /// spans (start, end, drawn).
    pub render_bar: Option<(u64, Vec<(Time, Time, bool)>)>,
    /// The mask being edited on the monitor.
    pub mask_picked: Option<usize>,
    /// The missing movie being relinked.
    pub relinking: Option<EntityId>,
    /// The audio track the Audio page's settings edit.
    pub audio_track: Option<usize>,
}

impl Default for SueVe {
    fn default() -> Self {
        Self {
            project: None,
            sequence: None,
            page: Page::Edit,
            dock: crate::ui::default_dock(),
            hub: MediaHub::new(1024),
            clock: Clock::default(),
            source: SourceView::default(),
            focus: Monitor::Program,
            selection: Vec::new(),
            picked: None,
            tool: Tool::Select,
            snapping: true,
            linked: true,
            zoom: 60.0,
            scroll: 0.0,
            scroll_y: 0.0,
            drag: None,
            live: None,
            list: false,
            quality: Quality::Full,
            clipboard: Vec::new(),
            dialog: None,
            deliver: DeliverForm {
                target: Target::Mp4,
                width: 1920.0,
                height: 1080.0,
                megabits: 0.0,
                whole: true,
                sound: true,
                rate: None,
            },
            exports: Vec::new(),
            proxying: Vec::new(),
            last_report: None,
            menubar: MenuBar::new(),
            menus: crate::menus::menus(),
            bound: sue_wm::shortcuts::Bound::default(),
            language: sue_lang::Watch::default(),
            ready: false,
            program_rect: None,
            timeline_rect: None,
            title_draft: None,
            preview: crate::preview::Preview::default(),
            scopes: crate::scopes::Scopes::default(),
            shown: None,
            color: ColorState::default(),
            graph: GraphState::default(),
            fx_search: String::new(),
            fx_clipboard: None,
            fx_picked: None,
            mobile: MobileState::default(),
            recording: None,
            zoom_target: None,
            render_bar: None,
            mask_picked: None,
            relinking: None,
            audio_track: None,
        }
    }
}

impl std::fmt::Debug for SueVe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SueVe")
            .field("project", &self.project)
            .field("sequence", &self.sequence)
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

impl SueVe {
    // ----- What is open -----------------------------------------------------

    /// The sequence on the timeline.
    #[must_use]
    pub fn current<'a>(&self, cx: &'a AppletContext<'_>) -> Option<&'a Sequence> {
        model::sequence(cx.arena(), self.sequence?)
    }

    /// The sequence's frame size, or 1920 by 1080.
    #[must_use]
    pub fn frame_size(&self, cx: &AppletContext<'_>) -> (u32, u32) {
        self.current(cx).map_or((1920, 1080), model::frame_size)
    }

    /// One frame of the sequence.
    #[must_use]
    pub fn frame_length(&self, cx: &AppletContext<'_>) -> Time {
        self.current(cx)
            .map_or(Time::from_frames(1, Rate::FPS_30), edit::frame)
    }

    /// Where the playback stops: the out mark or the end.
    #[must_use]
    pub fn range(&self, cx: &AppletContext<'_>) -> (Time, Time) {
        let Some(sequence) = self.current(cx) else {
            return (Time::ZERO, Time::ZERO);
        };
        let (mark_in, mark_out) = model::marks(sequence);
        let end = sequence.duration();
        if self.clock.looping {
            (
                mark_in.unwrap_or(Time::ZERO),
                mark_out.filter(|out| *out > Time::ZERO).unwrap_or(end),
            )
        } else {
            (Time::ZERO, end)
        }
    }

    /// Opens `project` (and its first sequence, or `sequence`).
    pub fn open_project(
        &mut self,
        cx: &mut AppletContext<'_>,
        project: EntityId,
        sequence: Option<EntityId>,
    ) {
        self.project = Some(project);
        self.sequence = sequence.or_else(|| model::sequences(cx.arena(), project).first().copied());
        self.selection.clear();
        self.clock = Clock::default();
        self.source = SourceView::default();
        self.picked = None;
        for (id, _) in model::pool(cx.arena(), project) {
            self.hub.prepare(cx.arena(), id);
            // The first movie is picked, as a pool opens with its first
            // clip chosen.
            if self.picked.is_none()
                && cx
                    .arena()
                    .get_by_id(id)
                    .is_ok_and(|entity| SourceKind::of(entity) == SourceKind::Movie)
            {
                self.picked = Some(id);
            }
        }
        let size = self.frame_size(cx);
        self.quality = rice_text(cx.rice(), rice::QUALITY)
            .and_then(|id| Quality::from_id(&id))
            .unwrap_or_else(|| export::default_quality(size));
        self.deliver.width = f64::from(size.0);
        self.deliver.height = f64::from(size.1);
    }

    /// Opens whatever `id` is: a project, a sequence, or a pool item.
    pub fn open_entity(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        let Some(project) = model::project_of(cx.arena(), id) else {
            // Something from another studio: a new project with it.
            self.new_project(cx);
            self.take_media(cx, id);
            return;
        };
        let sequence = model::sequence(cx.arena(), id).map(|_| id);
        if self.project != Some(project) {
            self.open_project(cx, project, sequence);
        } else if sequence.is_some() {
            self.sequence = sequence;
        }
        if sequence.is_none() && id != project {
            self.show_source(cx, id);
        }
    }

    /// Makes a new project with one sequence and opens it.
    pub fn new_project(&mut self, cx: &mut AppletContext<'_>) {
        self.new_project_sized(cx, 1920, 1080, Rate::FPS_30);
    }

    /// Makes a new project with one sequence of `width` by `height` at
    /// `rate`, and opens it.
    pub fn new_project_sized(
        &mut self,
        cx: &mut AppletContext<'_>,
        width: u32,
        height: u32,
        rate: Rate,
    ) {
        let made = model::new_project(
            &tr!("untitled-project"),
            &tr!("sequence-name", number = 1),
            width,
            height,
            rate,
        );
        if self.apply(cx, made.batch) {
            self.open_project(cx, made.project, Some(made.sequence));
        }
    }

    /// Applies an undo step, telling what went wrong if it did.
    pub fn apply<X>(&mut self, cx: &mut AppletContext<'_>, step: X) -> bool
    where
        X: sue_core::ReversibleTransaction<sue_core::Arena> + 'static,
    {
        match cx.apply(ArenaEdit(step)) {
            Ok(_) => true,
            Err(error) => {
                cx.fail(error.to_string());
                false
            }
        }
    }

    /// Changes the open sequence with `change` as one undo step named
    /// `label`.
    pub fn edit<T>(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        change: impl FnOnce(&mut Sequence, &EditCtx<'_>) -> Result<T, EditError>,
    ) -> Option<T> {
        let id = self.sequence?;
        let arena = cx.arena();
        let length = |source: EntityId| model::length_of(arena, source);
        let ctx = EditCtx { media_len: &length };
        match model::plan(arena, id, label, |sequence| change(sequence, &ctx)) {
            Ok((step, out)) => {
                if let Some(step) = step {
                    self.apply(cx, step);
                }
                Some(out)
            }
            Err(error) => {
                cx.notify(error.text());
                None
            }
        }
    }

    /// Changes the open sequence in place, without a step (a drag in the
    /// inspector); [`Self::commit_live`] records the step when it ends.
    pub fn edit_live(&mut self, cx: &mut AppletContext<'_>, change: impl FnOnce(&mut Sequence)) {
        let Some(id) = self.sequence else { return };
        if self.live.as_ref().is_none_or(|(live, _)| *live != id) {
            let Some(before) = model::sequence(cx.arena(), id).cloned() else {
                return;
            };
            self.live = Some((id, before));
        }
        if let Ok(entity) = cx.world.arena.get_by_id_mut(id)
            && let Some(sequence) = entity.ext_mut::<Sequence>()
        {
            change(sequence);
        }
    }

    /// Records the in place change [`Self::edit_live`] made as one step
    /// (rule 2: an edit already made is recorded with its delta).
    pub fn commit_live(&mut self, cx: &mut AppletContext<'_>, label: &str) {
        let Some((id, before)) = self.live.take() else {
            return;
        };
        let Some(after) = model::sequence(cx.arena(), id) else {
            return;
        };
        if let Some(step) = SequenceEdit::between(id, label, &before, after) {
            cx.record(ArenaEdit(step));
        }
    }

    /// The first selected clip, with its track.
    #[must_use]
    pub fn selected_clip(&self, cx: &AppletContext<'_>) -> Option<(usize, Clip)> {
        let sequence = self.current(cx)?;
        let id = self.selection.first()?;
        sequence
            .find(*id)
            .map(|(track, clip)| (track, clip.clone()))
    }

    /// Changes clip `id` with `change` as one undo step named `label`.
    pub fn change_clip(
        &mut self,
        cx: &mut AppletContext<'_>,
        label: &str,
        id: u64,
        change: impl FnOnce(&mut Clip) -> Result<(), EditError>,
    ) -> bool {
        self.edit(cx, label, |sequence, _| {
            let (_, clip) = sequence.find_mut(id).ok_or(EditError::NoClip(id))?;
            change(clip)
        })
        .is_some()
    }

    /// Changes clip `id` in place while a value is dragged; the step is
    /// made by [`Self::commit_live`].
    pub fn live_clip(
        &mut self,
        cx: &mut AppletContext<'_>,
        id: u64,
        change: impl FnOnce(&mut Clip),
    ) {
        self.edit_live(cx, |sequence| {
            if let Some((_, clip)) = sequence.find_mut(id) {
                change(clip);
            }
        });
    }

    /// The selection with the clips linked to it (when linked selection
    /// is on).
    #[must_use]
    pub fn selected_with_links(&self, cx: &AppletContext<'_>) -> Vec<u64> {
        let Some(sequence) = self.current(cx) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for id in &self.selection {
            let group = if self.linked {
                sequence.linked(*id)
            } else {
                vec![*id]
            };
            for clip in group {
                if !out.contains(&clip) && sequence.find(clip).is_some() {
                    out.push(clip);
                }
            }
        }
        out
    }

    /// The tracks edits go to: the targeted video and audio tracks.
    #[must_use]
    pub fn targets(&self, cx: &AppletContext<'_>) -> (Option<usize>, Option<usize>) {
        let Some(sequence) = self.current(cx) else {
            return (None, None);
        };
        let find = |video: bool| {
            let tracks = || {
                sequence
                    .tracks
                    .iter()
                    .enumerate()
                    .filter(move |(_, track)| model::is_video(track) == video)
            };
            tracks()
                .find(|(_, track)| model::flag(track, key::TARGET))
                .or_else(|| {
                    if video {
                        tracks().next_back()
                    } else {
                        tracks().next()
                    }
                })
                .map(|(index, _)| index)
        };
        (find(true), find(false))
    }

    // ----- Media --------------------------------------------------------------

    /// Asks for files to import.
    pub fn ask_import(&mut self, cx: &mut AppletContext<'_>) {
        let pick = FilePick::open(Some(me()), "import", &tr!("import-media"))
            .extensions(&import::extensions())
            .many();
        cx.request(ShellRequest::PickFile(pick));
    }

    /// Imports the files at `paths` into the pool, as one undo step.
    pub fn import(&mut self, cx: &mut AppletContext<'_>, paths: &[PathBuf]) -> Vec<EntityId> {
        if self.project.is_none() {
            self.new_project(cx);
        }
        let Some(project) = self.project else {
            return Vec::new();
        };
        let embed = rice_bool(cx.rice(), rice::EMBED, false);
        let mut entities = Vec::new();
        for path in paths {
            match import::entity_for(path, embed) {
                Ok(entity) => entities.push(entity),
                Err(why) => cx.fail(tr!("could-not-import", name = file_name(path), error = why)),
            }
        }
        let ids: Vec<EntityId> = entities.iter().map(Entity::id).collect();
        if entities.is_empty() {
            return ids;
        }
        let mut batch = sue_core::txn::Batch::new(tr!("import-media"));
        model::spawn_all_under(cx.arena(), project, entities, &mut batch);
        if self.apply(cx, batch) {
            for id in &ids {
                self.hub.prepare(cx.arena(), *id);
            }
            for path in paths {
                cx.request(ShellRequest::Remember {
                    path: path.clone(),
                    saved: false,
                });
            }
            self.picked = ids.first().copied();
        }
        ids
    }

    /// Imports the numbered run of pictures `path` is part of.
    pub fn import_image_sequence(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        if self.project.is_none() {
            self.new_project(cx);
        }
        let Some(project) = self.project else { return };
        let rate = self
            .current(cx)
            .map_or(Rate::FPS_24, |sequence| sequence.rate);
        match import::image_sequence_for(path, rate) {
            Ok(entity) => {
                let id = entity.id();
                let spawn = model::spawn_under(cx.arena(), project, entity);
                if self.apply(cx, spawn) {
                    self.picked = Some(id);
                }
            }
            Err(why) => cx.fail(why),
        }
    }

    /// Adds a generated source (a title, a solid, an adjustment clip) to
    /// the pool and puts it on the timeline at the playhead.
    pub fn add_generated(&mut self, cx: &mut AppletContext<'_>, entity: Entity) {
        if self.project.is_none() {
            self.new_project(cx);
        }
        let Some(project) = self.project else { return };
        let id = entity.id();
        let spawn = model::spawn_under(cx.arena(), project, entity);
        if self.apply(cx, spawn) {
            self.picked = Some(id);
            let at = self.clock.position;
            let (video, _) = self.targets(cx);
            if let Some(track) = video {
                self.place(cx, id, at, track, None, false);
            }
        }
    }

    /// Takes an entity another studio sent: into the pool, and onto the
    /// timeline at the playhead.
    pub fn take_media(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        if self.project.is_none() {
            self.new_project(cx);
        }
        let Some(project) = self.project else { return };
        // Already in the pool (or a child of it): use it as it is.
        if model::project_of(cx.arena(), id) != Some(project) {
            let order = cx.arena().relations().next_order(project, sue_core::CHILD);
            let relate = sue_core::txn::Relate::new(
                sue_core::Relation::new(project, sue_core::CHILD, id).ordered(order),
            );
            // A picture from Sue Paint has a parent already (its document):
            // it is linked into the pool by reference instead.
            if cx.arena().relations().parent(id, sue_core::CHILD).is_some() {
                let order = cx.arena().relations().next_order(project, sue_core::REF);
                let link = sue_core::txn::Relate::new(
                    sue_core::Relation::new(project, sue_core::REF, id).ordered(order),
                );
                self.apply(cx, link);
            } else {
                self.apply(cx, relate);
            }
        }
        self.picked = Some(id);
        let at = self.clock.position;
        let (video, _) = self.targets(cx);
        if let Some(track) = video {
            self.place(cx, id, at, track, None, false);
        }
    }

    /// Puts source `id` on the timeline at `at` on track `track` (with
    /// its sound linked on the targeted audio track when it has sound):
    /// `range` is the part of the source (else the whole), `insert` moves
    /// what follows instead of covering it.
    pub fn place(
        &mut self,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        at: Time,
        track: usize,
        range: Option<(Time, Time)>,
        insert: bool,
    ) {
        let Some(sequence) = self.current(cx) else {
            return;
        };
        let Ok(entity) = cx.arena().get_by_id(id) else {
            return;
        };
        let kind = SourceKind::of(entity);
        let length = model::length_of(cx.arena(), id).unwrap_or(Time::from_seconds(5.0));
        let (from, to) = range.unwrap_or((Time::ZERO, length));
        let duration = (to - from).max(edit::frame(sequence));
        let sound = model::has_sound(cx.arena(), id);
        let (_, audio_target) = self.targets(cx);
        let is_video_track = sequence.tracks.get(track).is_some_and(model::is_video);
        let mut placements = Vec::new();
        let mut clip = Clip::new(id, at, duration);
        clip.source_in = from;
        if kind == SourceKind::Sound {
            // A sound goes on an audio track only.
            let to_track = if is_video_track {
                audio_target
            } else {
                Some(track)
            };
            if let Some(to_track) = to_track {
                placements.push((to_track, clip));
            }
        } else if kind.has_pictures() {
            let to_track = if is_video_track {
                Some(track)
            } else {
                self.targets(cx).0
            };
            if let Some(to_track) = to_track {
                placements.push((to_track, clip.clone()));
            }
            if sound
                && let Some(audio) = if is_video_track {
                    audio_target
                } else {
                    Some(track)
                }
            {
                placements.push((audio, clip));
            }
        }
        if placements.is_empty() {
            return;
        }
        let label = if insert {
            tr!("insert")
        } else {
            tr!("overwrite")
        };
        let linked = placements.len() > 1;
        let placed = self.edit(cx, &label, |sequence, _| {
            let ids = if insert {
                edit::insert(sequence, placements)?
            } else {
                edit::overwrite(sequence, placements)?
            };
            if linked {
                edit::link(sequence, &ids)?;
            }
            Ok(ids)
        });
        if let Some(ids) = placed {
            self.selection = ids;
            self.hub.prepare(cx.arena(), id);
        }
    }

    /// Shows pool item `id` in the source monitor.
    pub fn show_source(&mut self, cx: &mut AppletContext<'_>, id: EntityId) {
        self.source = SourceView {
            item: Some(id),
            ..SourceView::default()
        };
        self.focus = Monitor::Source;
        self.picked = Some(id);
        self.hub.prepare(cx.arena(), id);
    }

    /// Insert or overwrite the source monitor's marked part at the
    /// program's playhead on the targeted tracks.
    pub fn edit_from_source(&mut self, cx: &mut AppletContext<'_>, insert: bool) {
        let Some(item) = self.source.item.or(self.picked) else {
            cx.notify(tr!("pick-a-source-first"));
            return;
        };
        let length = model::length_of(cx.arena(), item).unwrap_or(Time::from_seconds(5.0));
        let from = self.source.mark_in.unwrap_or(Time::ZERO);
        let to = self
            .source
            .mark_out
            .filter(|out| *out > from)
            .unwrap_or(length);
        let (video, audio) = self.targets(cx);
        let track = if model::has_sound(cx.arena(), item)
            && cx
                .arena()
                .get_by_id(item)
                .is_ok_and(|entity| SourceKind::of(entity) == SourceKind::Sound)
        {
            audio
        } else {
            video
        };
        if let Some(track) = track {
            let at = self.clock.position;
            self.place(cx, item, at, track, Some((from, to)), insert);
            self.clock.position = at + (to - from);
        }
    }

    // ----- Proxies and exports ----------------------------------------------

    /// Makes proxies of the movies in the pool that have none, in the
    /// background.
    pub fn make_proxies(&mut self, cx: &mut AppletContext<'_>) {
        let Some(project) = self.project else { return };
        let movies: Vec<ToProxy> = model::pool(cx.arena(), project)
            .into_iter()
            .filter_map(|(id, _)| {
                let entity = cx.arena().get_by_id(id).ok()?;
                if SourceKind::of(entity) != SourceKind::Movie
                    || model::text(entity, key::PROXY).is_some()
                {
                    return None;
                }
                let size = model::size_of(cx.arena(), id)?;
                let rate = model::rate_of(entity)?;
                let seconds = model::length_of(cx.arena(), id)?.seconds();
                Some((
                    id,
                    import::file_of(entity)?,
                    size,
                    (rate.numerator(), rate.denominator()),
                    seconds,
                ))
            })
            .collect();
        if movies.is_empty() {
            cx.notify(tr!("no-proxies-to-make"));
        }
        for (id, from, size, rate, seconds) in movies {
            if self.proxying.iter().any(|running| running.media == id) {
                continue;
            }
            let path = import::cache_dir()
                .join("proxies")
                .join(format!("{id}.mp4"));
            let to = path.clone();
            let label = tr!("proxy-of", name = file_name(&from));
            let transfer = cx.transfers.spawn(
                TransferKind::Export,
                &label,
                Some(path.clone()),
                move |job| {
                    crate::media::thumbs::make_proxy(&from, &to, size, rate, seconds, Some(job))
                },
            );
            self.proxying.push(Proxying {
                transfer,
                media: id,
                path,
            });
        }
    }

    /// Exports the open sequence to `path` as the Deliver page says.
    pub fn export_to(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(id) = self.sequence else {
            cx.notify(tr!("edit-no-sequence"));
            return;
        };
        let Some(sequence) = self.current(cx) else {
            return;
        };
        let mut settings = export::Settings::whole(sequence);
        settings.target = self.deliver.target;
        settings.size = (
            (self.deliver.width.round() as u32).max(16),
            (self.deliver.height.round() as u32).max(16),
        );
        if !self.deliver.whole {
            let (mark_in, mark_out) = model::marks(sequence);
            settings.range = (
                mark_in.unwrap_or(Time::ZERO),
                mark_out.unwrap_or(sequence.duration()),
            );
        }
        if matches!(settings.target, Target::Png | Target::Jpeg) {
            settings.range.0 = self.clock.position;
        }
        settings.bitrate =
            (self.deliver.megabits > 0.0).then_some((self.deliver.megabits * 1_000_000.0) as u32);
        settings.sound = self.deliver.sound;
        if let Some(rate) = self.deliver.rate {
            settings.rate = Rate::new(rate, 1);
        }
        let arena = cx.arena().clone();
        let hardware = self.hub.hardware;
        let report = Arc::new(Mutex::new(None));
        let into = Arc::clone(&report);
        let target = path.to_path_buf();
        let label = file_name(path);
        let transfer = cx.transfers.spawn_noticed(
            TransferKind::Export,
            &label,
            Some(target.clone()),
            move |job| {
                let made = export::run(arena, id, settings, &target, hardware, Some(job))?;
                *into.lock().unwrap_or_else(PoisonError::into_inner) = Some(made);
                Ok(())
            },
        );
        self.exports.push(Running {
            transfer,
            path: path.to_path_buf(),
            report,
        });
    }

    /// Takes in exports and proxies that finished.
    pub fn poll_jobs(&mut self, cx: &mut AppletContext<'_>) {
        let mut done = Vec::new();
        self.exports.retain(|running| {
            match cx.transfers.get(running.transfer).map(|t| t.state.clone()) {
                Some(TransferState::Running(_)) => true,
                Some(TransferState::Done) => {
                    done.push((
                        running.path.clone(),
                        running
                            .report
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .take(),
                    ));
                    false
                }
                _ => false,
            }
        });
        for (path, report) in done {
            if let Some(report) = report {
                cx.saved(&path, tr!("export-note", frames = report.frames as i64));
                self.last_report = Some((path, report));
            }
        }
        let mut made = Vec::new();
        self.proxying.retain(|running| {
            match cx.transfers.get(running.transfer).map(|t| t.state.clone()) {
                Some(TransferState::Running(_)) => true,
                Some(TransferState::Done) => {
                    made.push((running.media, running.path.clone()));
                    false
                }
                _ => false,
            }
        });
        for (media, path) in made {
            let set = sue_core::txn::SetProperty::new(
                media,
                key::PROXY,
                path.to_string_lossy().into_owned(),
            );
            self.apply(cx, set);
            self.hub.reset();
        }
    }

    /// Runs command `name`.
    pub fn run(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        crate::commands::registry().run(self, cx, name)
    }
}

/// A switch in the rice, or `default`.
#[must_use]
pub fn rice_bool(rice: &sue_rice::RiceSchema, path: &str, default: bool) -> bool {
    match rice.get(path) {
        Some(sue_rice::RiceValue::Bool(value)) => value,
        _ => default,
    }
}

/// A text (or choice) in the rice.
#[must_use]
pub fn rice_text(rice: &sue_rice::RiceSchema, path: &str) -> Option<String> {
    match rice.get(path) {
        Some(sue_rice::RiceValue::Text(value)) => Some(value),
        _ => None,
    }
}

/// A whole number in the rice, or `default`.
#[must_use]
pub fn rice_int(rice: &sue_rice::RiceSchema, path: &str, default: i64) -> i64 {
    match rice.get(path) {
        Some(sue_rice::RiceValue::Int(value)) => value,
        _ => default,
    }
}

/// A path's file name.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

impl SueApplet for SueVe {
    fn settings_page(&self) -> Option<sue_wm::SettingsPage> {
        Some(crate::settings_page::page(&self.manifest().title()))
    }

    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(
            ID,
            "Sue VE",
            AppletRole::Studio,
            sue_wm::icons::Icon::AppVideo,
        )
        .localized("ve");
        manifest.summary = "Edit video: cut, trim and arrange clips on a timeline, add titles and transitions, mix the sound and export MP4.".to_owned();
        manifest.order = 28;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        vec![
            RiceDeclaration::new(
                rice::HARDWARE,
                RiceParameter::Bool { value: false },
                "Decode movies on the graphics card (else on the processor, which measured faster for the monitors on this release's test machine)",
            ),
            RiceDeclaration::new(
                rice::CACHE_MB,
                RiceParameter::IntRange {
                    value: 1024,
                    min: Some(128),
                    max: Some(16_384),
                },
                "Memory Sue VE keeps decoded frames in, in megabytes",
            ),
            RiceDeclaration::new(
                rice::QUALITY,
                RiceParameter::EnumChoice {
                    value: "full".to_owned(),
                    options: ["full", "half", "quarter"].map(String::from).to_vec(),
                },
                "How large the monitors decode movies: full, half or a quarter of their size",
            ),
            RiceDeclaration::new(
                rice::BACKGROUND_RENDER,
                RiceParameter::Bool { value: true },
                "While paused, draw the seconds after the playhead that need effects, grades or masks into the render cache, so they play at once",
            ),
            RiceDeclaration::new(
                rice::RENDER_CACHE_MB,
                RiceParameter::IntRange {
                    value: 768,
                    min: Some(64),
                    max: Some(16_384),
                },
                "Memory Sue VE keeps rendered frames (effects, grades, masks) in, in megabytes",
            ),
            RiceDeclaration::new(
                rice::EMBED,
                RiceParameter::Bool { value: false },
                "Keep imported movies and sounds inside the project file (else it links to them)",
            ),
        ]
        .into_iter()
        .chain(sue_wm::shortcuts::declarations(
            KEYS_SECTION,
            crate::commands::shortcut_table(),
        ))
        .collect()
    }

    fn accepts(&self, entity: &Entity) -> bool {
        let class = entity.class();
        class == model::class::PROJECT
            || class == sue_model::class::VIDEO
            || (class == sue_model::class::SEQUENCE
                && entity
                    .ext::<Sequence>()
                    .is_some_and(|sequence| sequence.properties.get(key::SEQ_WIDTH).is_some()))
    }

    fn focus(&self) -> Option<EntityId> {
        self.project
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        let video = tr!("video");
        [
            ("hd", tr!("template-hd")),
            ("uhd", tr!("template-uhd")),
            ("vertical", tr!("template-vertical")),
            ("square", tr!("template-square")),
        ]
        .into_iter()
        .map(|(key, label)| sue_wm::FileTemplate::new(video.clone(), label, key))
        .collect()
    }

    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        let (width, height, rate) = match key {
            "uhd" => (3840, 2160, Rate::FPS_30),
            "vertical" => (1080, 1920, Rate::FPS_30),
            "square" => (1080, 1080, Rate::FPS_30),
            _ => (1920, 1080, Rate::FPS_30),
        };
        self.new_project_sized(cx, width, height, rate);
        self.page = Page::Edit;
        cx.request(ShellRequest::Open(me()));
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, cx: &mut AppletContext<'_>) {
        self.frame(ui, cx);
    }

    fn bar_ui(&mut self, ui: &mut egui::Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        self.menu_bar(ui, cx);
    }

    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        self.bound.chords(crate::commands::shortcut_table())
    }

    fn captures_keys(&self) -> bool {
        self.dialog.is_some()
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        let mut tick = sue_wm::Tick::default();
        let hardware = rice_bool(cx.rice(), rice::HARDWARE, false);
        if hardware != self.hub.hardware {
            self.hub.hardware = hardware;
            self.hub.reset();
        }
        if self.hub.poll() {
            tick.repaint = true;
        }
        let budget = rice_int(cx.rice(), rice::RENDER_CACHE_MB, 768).max(32) as usize;
        self.preview.set_budget_mb(budget);
        self.poll_jobs(cx);
        let here = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        if !here {
            if self.clock.playing {
                self.clock.pause(cx.sink());
            }
            return tick;
        }
        let now = cx.view.now;
        if let Some(sequence) = self.current(cx).cloned() {
            let range = self.range(cx);
            let sounds = &self.hub.sounds;
            let audio = cx.audio.as_deref_mut();
            if self
                .clock
                .advance(&sequence, range, now, audio, &|id| sounds.get(id))
            {
                tick.animating = true;
            }
        }
        if self.source.clock.playing {
            let length = self
                .source
                .item
                .and_then(|item| model::length_of(cx.arena(), item))
                .unwrap_or(Time::ZERO);
            let empty = Sequence::default();
            if self
                .source
                .clock
                .advance(&empty, (Time::ZERO, length), now, None, &|_| None)
            {
                tick.animating = true;
            }
        }
        if self.hub.busy()
            || !self.exports.is_empty()
            || !self.proxying.is_empty()
            || self.drag.is_some()
            || self.preview.busy()
            || self.scopes.busy()
            || self.recording.is_some()
            || self.zoom_target.is_some()
        {
            tick.animating = true;
        }
        if let Some(recording) = self.recording.as_mut() {
            let Recording { input, samples, .. } = recording;
            input.take(samples);
        }
        // Ticks run when the earliest applet asks: while something moves
        // here, that is the next frame.
        if tick.animating {
            tick.wake = Some(now + std::time::Duration::from_millis(8));
        }
        tick
    }

    fn commands(&self) -> Vec<PaletteEntry> {
        crate::commands::registry().palette(&sue_rice::RiceSchema::new())
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me()));
        }
        self.run(name, cx)
    }

    fn navigate(&mut self, target: &str) -> bool {
        if let Some(title) = target.strip_prefix("menu/") {
            return self.menubar.open(&self.menus, title);
        }
        if let Some(page) = target.strip_prefix("page/") {
            if let Some(page) = Page::ALL
                .into_iter()
                .find(|candidate| candidate.id() == page)
            {
                self.page = page;
                return true;
            }
            return false;
        }
        match target {
            "dialog/close" => {
                self.dialog = None;
                self.menubar.close();
                true
            }
            "dialog/new-sequence" => {
                self.dialog = Some(Dialog::NewSequence {
                    name: tr!("sequence-name", number = 2),
                    width: 1920.0,
                    height: 1080.0,
                    rate: 4,
                });
                true
            }
            "dialog/shortcuts" => {
                self.dialog = Some(Dialog::Shortcuts);
                true
            }
            "dialog/about" => {
                self.dialog = Some(Dialog::About);
                true
            }
            "pool/list" => {
                self.list = true;
                true
            }
            "pool/thumbs" => {
                self.list = false;
                true
            }
            _ => false,
        }
    }

    fn window_title(&self, arena: &sue_core::Arena) -> Option<String> {
        let project = arena.get_by_id(self.project?).ok()?;
        let sequence = self
            .sequence
            .and_then(|id| arena.get_by_id(id).ok())
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default();
        Some(format!("{} - {} - Sue VE", project.name(), sequence))
    }

    fn sue_actions(&self, focus: Option<&Entity>, current: bool) -> Vec<SueAction> {
        if current {
            return Vec::new();
        }
        let Some(entity) = focus else {
            return Vec::new();
        };
        let kind = SourceKind::of(entity);
        if matches!(
            kind,
            SourceKind::Picture | SourceKind::Still | SourceKind::Movie | SourceKind::Sound
        ) {
            return vec![SueAction::new(
                sue_menu::SEND_TO,
                "Sue VE",
                ShellRequest::SendEntity {
                    to: me(),
                    purpose: "clip".to_owned(),
                    entity: entity.id(),
                },
            )];
        }
        Vec::new()
    }

    fn take_entity(&mut self, purpose: &str, entity: EntityId, cx: &mut AppletContext<'_>) {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me()));
        }
        self.page = Page::Edit;
        if purpose == "open" {
            self.open_entity(cx, entity);
        } else {
            self.take_media(cx, entity);
        }
    }

    fn opens_file(&self, path: &Path) -> bool {
        matches!(import::kind_of(path), import::FileKind::Movie)
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me()));
        }
        match pick.purpose.as_str() {
            "import" => {
                let paths = pick.chosen(path);
                self.import(cx, &paths);
            }
            "image-sequence" => self.import_image_sequence(cx, path),
            "export" => self.export_to(cx, path),
            "relink" => {
                if let Some(id) = self.relinking.take() {
                    crate::actions::relink(self, cx, id, path);
                }
            }
            "lut" => crate::actions::import_lut(self, cx, path),
            "preset-import" => match sue_platform::fs::read(path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| crate::fx::preset::from_file(&bytes))
            {
                Ok(preset) => {
                    let _ = crate::fx::preset::save(&preset);
                    crate::ui::browser::apply_preset(self, cx, &preset);
                }
                Err(why) => cx.fail(why),
            },
            "preset-export" => {
                if let Some((_, clip)) = self.selected_clip(cx) {
                    let name = path.file_stem().map_or_else(
                        || tr!("preset-default-name"),
                        |stem| stem.to_string_lossy().into_owned(),
                    );
                    let preset = crate::fx::preset::capture(&clip, &name);
                    if let Err(error) =
                        sue_platform::fs::write(path, crate::fx::preset::to_file(&preset))
                    {
                        cx.fail(error.to_string());
                    } else {
                        cx.saved(path, tr!("preset-exported"));
                    }
                }
            }
            _ => {}
        }
    }
}

/// The value of a number property of a clip, or its default.
#[must_use]
pub fn clip_number(clip: &Clip, name: &str) -> f64 {
    clip.properties
        .get(name)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| model::default_of(name))
}

/// The time `now` of the clock, for tests.
#[must_use]
pub fn now() -> Instant {
    Instant::now()
}
