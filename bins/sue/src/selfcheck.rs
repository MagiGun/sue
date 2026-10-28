//! The headless self-check: exercises every subsystem through the shared
//! engine state, proving they work and that they talk to each other.

use std::f32::consts::TAU;
use std::fmt;
use std::time::{Duration, Instant};

use sue_ai::{
    AiTask, ComputeBackend, GgufHeader, LocalModelRegistry, MidSideStemSplit, ZeroCopyTensorTask,
};
use sue_core::txn::{AttachProjection, InsertText};
use sue_core::{Arena, ComponentKind, PCMFloat1D, Projection, StringBuffer, TimeSpan, UndoStack};
use sue_io::{
    IngestOptions, IngestRoute, IngestView, JxlPipeline, NativeFormat, NativeJxlPipeline,
    ProgressiveTarget, StructuredFormat, TileCache, TileKey, ingest_bytes, sniff_stream,
};
use sue_pkg::{
    AtomicUpdater, Dependency, PackageIndex, PackageKind, PackageManifest, SignedManifest,
    SigningKey, TrustStore, UpdateOutcome, Version, resolve,
};
use sue_platform::{
    FileDrop, FormFactor, FramePacing, GpuContext, HeadlessHost, InputEvent, Key, KeyEvent,
    Modifiers, MouseButton, MouseEvent, NamedKey, PlatformHost, Point, SafeAreaInsets,
    SurfaceDescriptor,
};
use sue_rice::{
    DEFAULT_RICE, HardwareProfile, Keybind, RiceSchema, RiceValue, SetRiceValue, SettingsTree,
};
use sue_wm::{
    AppletContext, Canvas, Fonts, FrameTarget, Placement, Rect, RenderDemand, RiceEdit, ShellFrame,
    Tab, Theme, Ui, UiInput, UiState, WmConfig, Workspace, Zone, draw_shell, render_offscreen,
    widget_id,
};

use crate::engine::NOTE_NAME;
use crate::runtime::Runtime;
use crate::{image_viewer, quick_notes, settings, snapshot};

/// Seed of the throwaway publisher key used to sign the self-check's sample
/// packages. It is trusted only by the self-check's own `TrustStore`.
const SELF_CHECK_SEED: [u8; 32] = [0x5E; 32];

/// A sample package: id, version, kind, payload and `(id, requirement)` deps.
type SamplePackage = (
    &'static str,
    &'static str,
    PackageKind,
    &'static [u8],
    &'static [(&'static str, &'static str)],
);

/// What the checks run against.
pub struct Subject {
    /// The headless platform host.
    pub host: HeadlessHost,
    /// The booted engine inside the shell runtime.
    pub runtime: Runtime,
}

/// The outcome of one subsystem check.
pub struct Check {
    /// Subsystem name.
    pub name: &'static str,
    /// Details on success, the reason on failure.
    pub outcome: Result<String, String>,
}

/// Every check, in order.
pub struct Report {
    /// The checks.
    pub checks: Vec<Check>,
}

impl Report {
    /// `true` if every check passed.
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.outcome.is_ok())
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Self-check")?;
        for check in &self.checks {
            let (status, detail) = match &check.outcome {
                Ok(detail) => (" OK ", detail),
                Err(reason) => ("FAIL", reason),
            };
            writeln!(f, "  [{status}] {:<15} {detail}", check.name)?;
        }
        let passed = self
            .checks
            .iter()
            .filter(|check| check.outcome.is_ok())
            .count();
        writeln!(f, "{passed}/{} subsystems passed.", self.checks.len())
    }
}

type CheckFn = fn(&mut Subject) -> Result<String, String>;

/// Runs every check against `subject`, in dependency order.
pub fn run(subject: &mut Subject) -> Report {
    let checks: [(&'static str, CheckFn); 7] = [
        ("platform", check_platform),
        ("rice", check_rice),
        ("arena", check_arena),
        ("sniffer", check_sniffer),
        ("packages", check_packages),
        ("ai", check_ai),
        ("window manager", check_window_manager),
    ];
    Report {
        checks: checks
            .into_iter()
            .map(|(name, check)| Check {
                name,
                outcome: check(subject),
            })
            .collect(),
    }
}

fn ensure(condition: bool, failure: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(failure.to_owned())
    }
}

fn text<E: fmt::Display>(error: E) -> String {
    error.to_string()
}

/// `1 utility`, `2 utilities`.
fn count(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

fn check_platform(subject: &mut Subject) -> Result<String, String> {
    let host = &mut subject.host;
    let surface = host
        .create_surface(&SurfaceDescriptor {
            title: format!("Sue {}", sue_core::SUE_VERSION),
            width: 1280.0,
            height: 800.0,
            scale_factor: 1.0,
        })
        .map_err(text)?;
    let resized = host
        .resize_surface(surface.id, 1440.0, 900.0)
        .map_err(text)?;
    host.destroy_surface(surface.id).map_err(text)?;
    ensure(
        host.surface(surface.id).is_none(),
        "destroyed surface is still alive",
    )?;
    Ok(format!(
        "{}, {}, {} cores, surface {}x{}, config {}",
        host.os(),
        host.form_factor(),
        host.capabilities().logical_cores,
        resized.width,
        resized.height,
        host.paths().config_dir.display()
    ))
}

fn check_rice(subject: &mut Subject) -> Result<String, String> {
    let engine = &mut subject.runtime.engine;
    let tree = SettingsTree::build(&engine.world.rice);
    ensure(
        tree.widget_count() == engine.world.rice.len(),
        "a parameter has no Settings widget",
    )?;
    let gap = engine
        .world
        .rice
        .get_i64("layout.gap")
        .ok_or("layout.gap is missing")?;
    let edited = (gap + 4).min(32);

    engine
        .history
        .apply(
            &mut engine.world,
            RiceEdit(SetRiceValue::new("layout.gap", RiceValue::Int(edited))),
        )
        .map_err(text)?;
    ensure(
        tree.is_stale(&engine.world.rice),
        "the Settings tree missed the edit",
    )?;
    let written = engine.world.rice.to_suerice();
    let reparsed = RiceSchema::parse(&written).map_err(text)?;
    ensure(
        reparsed.get_i64("layout.gap") == Some(edited),
        "the edit was not written back",
    )?;
    ensure(
        Theme::from_rice(&engine.world.rice).gap == edited as f32,
        "the live theme did not follow the edit",
    )?;
    engine.history.undo(&mut engine.world).map_err(text)?;
    ensure(
        engine.world.rice.get_i64("layout.gap") == Some(gap),
        "undo did not restore the gap",
    )?;
    ensure(
        RiceSchema::parse(DEFAULT_RICE).map_err(text)?.to_suerice()
            == DEFAULT_RICE.replace("\r\n", "\n"),
        "the default rice does not round-trip byte for byte",
    )?;

    Ok(format!(
        "{} params → {} widgets in {} sections; gap {gap}→{edited}→{gap} via global undo; {}; profile {}",
        engine.world.rice.len(),
        tree.widget_count(),
        tree.sections.len(),
        engine.rice_source,
        engine.workspace.profile()
    ))
}

fn check_arena(subject: &mut Subject) -> Result<String, String> {
    let arena = &mut subject.runtime.engine.world.arena;
    let mut history: UndoStack<Arena> = UndoStack::new();
    let doc = arena.spawn(
        "Faustian Enigma.md",
        StringBuffer::from("# Faustian Enigma\n"),
    );
    let length = |arena: &Arena| -> Result<usize, String> {
        Ok(arena
            .get(doc)
            .map_err(text)?
            .string_buffer()
            .map_or(0, StringBuffer::len))
    };
    let before_edit = history.cursor();

    let end = length(arena)?;
    history
        .apply(arena, InsertText::new(doc, end, "A jack of all trades.\n"))
        .map_err(text)?;
    let buffer_address = |arena: &Arena| {
        arena
            .get(doc)
            .ok()
            .and_then(|entity| entity.string_buffer())
            .map(|buffer| buffer.as_str().as_ptr())
    };
    let address = buffer_address(arena);
    let span = TimeSpan::from_seconds(0.0, 12.0).map_err(text)?;
    history
        .apply(
            arena,
            AttachProjection::new(doc, Projection::Timeline, span),
        )
        .map_err(text)?;
    ensure(
        buffer_address(arena) == address,
        "attaching a projection moved the text buffer",
    )?;
    let projections = arena.get(doc).map_err(text)?.projections();
    ensure(
        projections.contains(Projection::Timeline),
        "Timeline projection missing",
    )?;

    history.undo(arena).map_err(text)?;
    ensure(
        !arena
            .get(doc)
            .map_err(text)?
            .projections()
            .contains(Projection::Timeline),
        "undo kept the projection",
    )?;
    history.redo(arena).map_err(text)?;

    history.checkout(arena, before_edit).map_err(text)?;
    let end = length(arena)?;
    history
        .apply(arena, InsertText::new(doc, end, "Master of none.\n"))
        .map_err(text)?;
    let branches = history.children(before_edit).len();
    ensure(branches == 2, "the old future was not kept as a branch")?;

    Ok(format!(
        "{doc}, {} txns, {branches} branches; projections {projections}; buffer address stable; {}",
        history.len(),
        count(arena.len(), "entity", "entities")
    ))
}

fn check_sniffer(subject: &mut Subject) -> Result<String, String> {
    let pipeline = NativeJxlPipeline;
    let card = snapshot::test_card();
    let jxl = pipeline.encode_layers(&[&card], 3).map_err(text)?;
    let decoded = pipeline
        .decode(&jxl, ProgressiveTarget::Full)
        .map_err(text)?;
    ensure(
        decoded.tile == card,
        "lossless JPEG XL round trip changed pixels",
    )?;
    let preview = pipeline
        .decode(&jxl, ProgressiveTarget::DcPreview)
        .map_err(text)?;

    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".as_slice();
    let json = br#"{"motto": "jack of all trades"}"#.as_slice();
    let routes = [
        ("jxl", sniff_stream(&jxl)),
        ("png", sniff_stream(png)),
        ("rice", sniff_stream(DEFAULT_RICE.as_bytes())),
        ("json", sniff_stream(json)),
    ];
    ensure(
        routes[0].1 == IngestRoute::Native(NativeFormat::JpegXl { container: false }),
        "JXL misrouted",
    )?;
    ensure(
        routes[1].1 == IngestRoute::Native(NativeFormat::Png),
        "PNG misrouted",
    )?;
    ensure(
        routes[2].1 == IngestRoute::Structured(StructuredFormat::Toml),
        "rice misrouted",
    )?;
    ensure(
        routes[3].1 == IngestRoute::Structured(StructuredFormat::Json),
        "JSON misrouted",
    )?;
    let summary: Vec<String> = routes
        .iter()
        .map(|(label, route)| format!("{label}→T{}", route.tier() as u8))
        .collect();

    let arena = &mut subject.runtime.engine.world.arena;
    let options = IngestOptions { byte_map_width: 64 };
    let image = ingest_bytes(arena, "card.jxl", jxl.clone(), &pipeline, options).map_err(text)?;
    ensure(
        image.view == IngestView::Raster,
        "JXL did not ingest as a raster",
    )?;
    let bytes = ingest_bytes(
        arena,
        "noise.bin",
        b"\x7fELF\x02\x01\x01\0".repeat(64),
        &pipeline,
        options,
    )
    .map_err(text)?;
    ensure(
        bytes.view == IngestView::Bytes,
        "binary did not ingest as a byte map",
    )?;

    let mut cache = TileCache::new(1 << 20);
    let key = TileKey {
        layer: 0,
        level: 0,
        x: 0,
        y: 0,
    };
    cache.insert(key, jxl.clone());
    ensure(
        cache.get(key) == Some(jxl.as_slice()),
        "tile cache lost the tile",
    )?;

    Ok(format!(
        "{}; JXL {}x{} decoded bit-exact ({} B lossless, {}x{} preview); ingest → raster {} + byte map {}; tile cache hit",
        summary.join(" "),
        decoded.info.width,
        decoded.info.height,
        jxl.len(),
        preview.tile.width(),
        preview.tile.height(),
        image.handle,
        bytes.handle,
    ))
}

fn check_packages(_subject: &mut Subject) -> Result<String, String> {
    let publisher = SigningKey::from_bytes(&SELF_CHECK_SEED);
    let mut trust = TrustStore::new();
    trust
        .trust("sue-self-check", publisher.verifying_key().as_bytes())
        .map_err(text)?;
    let mut store = sue_pkg::ContentStore::new();
    let mut index = PackageIndex::new();
    let catalogue: [SamplePackage; 3] = [
        (
            "sue.codec.flac",
            "1.0.0",
            PackageKind::Codec,
            b"flac codec 1.0",
            &[],
        ),
        (
            "sue.codec.flac",
            "1.1.0",
            PackageKind::Codec,
            b"flac codec 1.1",
            &[],
        ),
        (
            "sue.studio.daw",
            "1.0.0",
            PackageKind::Applet,
            b"daw studio",
            &[("sue.codec.flac", "^1")],
        ),
    ];
    for (id, version, kind, payload, dependencies) in catalogue {
        let mut manifest = PackageManifest::new(id, version, kind, payload).map_err(text)?;
        for (dependency, requirement) in dependencies {
            manifest = manifest
                .with_dependency(dependency, requirement)
                .map_err(text)?;
        }
        store.put(payload.to_vec());
        index
            .add_verified(
                SignedManifest::sign(manifest, "sue-self-check", &publisher),
                &trust,
            )
            .map_err(text)?;
    }

    let evil = PackageManifest::new("sue.codec.evil", "6.6.6", PackageKind::Codec, b"evil")
        .map_err(text)?;
    let mut forged = SignedManifest::sign(evil, "sue-self-check", &publisher);
    forged.manifest.version = Version::new(6, 6, 7);
    ensure(
        index.add_verified(forged, &trust).is_err(),
        "a tampered manifest was accepted",
    )?;

    let plan = resolve(
        &index,
        &[Dependency::parse("sue.studio.daw", "^1").map_err(text)?],
    )
    .map_err(text)?;
    let order: Vec<String> = plan
        .install_order
        .iter()
        .map(|manifest| format!("{}@{}", manifest.id, manifest.version))
        .collect();
    let mut updater = AtomicUpdater::new();
    let first = updater
        .apply_with_health_check(&plan, &store, |generation| generation.len() == plan.len())
        .map_err(text)?;
    let downgrade = resolve(
        &index,
        &[Dependency::parse("sue.codec.flac", "=1.0.0").map_err(text)?],
    )
    .map_err(text)?;
    let second = updater
        .apply_with_health_check(&downgrade, &store, |_| false)
        .map_err(text)?;
    let (UpdateOutcome::Committed { generation }, UpdateOutcome::RolledBack { failed, restored }) =
        (first, second)
    else {
        return Err(format!("unexpected update outcomes {first:?} / {second:?}"));
    };
    let flac = updater
        .active()
        .get("sue.codec.flac")
        .map(|manifest| manifest.version.to_string());
    ensure(
        flac.as_deref() == Some("1.1.0"),
        "rollback did not restore the previous generation",
    )?;

    Ok(format!(
        "{} signed manifests verified, tampered one rejected; plan {}; gen {generation} committed; gen {failed} failed its health check → gen {restored} restored",
        index.len(),
        order.join(" → ")
    ))
}

fn check_ai(subject: &mut Subject) -> Result<String, String> {
    let engine = &mut subject.runtime.engine;
    let available = ComputeBackend::detect_available(engine.os, &engine.capabilities);

    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3_u32.to_le_bytes());
    header.extend_from_slice(&291_u64.to_le_bytes());
    header.extend_from_slice(&24_u64.to_le_bytes());
    let gguf = GgufHeader::parse(&header).map_err(text)?;

    let mut registry = LocalModelRegistry::new();
    registry
        .register(MidSideStemSplit::descriptor())
        .map_err(text)?;
    let budget = engine.capabilities.memory_bytes.unwrap_or(u64::MAX);
    let selection = registry
        .select(AiTask::StemSeparation, &available, budget)
        .ok_or("no stem separator fits this device")?;
    ensure(
        selection.model.id == MidSideStemSplit::ID,
        "unexpected model selected",
    )?;

    let frames = 4_800;
    let samples: Vec<f32> = (0..frames)
        .flat_map(|frame| {
            let phase = TAU * 440.0 * frame as f32 / 48_000.0;
            [phase.sin(), 0.5 * phase.sin() + 0.25 * (3.0 * phase).sin()]
        })
        .collect();
    let arena = &mut engine.world.arena;
    let mix = arena.spawn(
        "mix.wav",
        PCMFloat1D::new(48_000, 2, samples).map_err(text)?,
    );
    let stems = arena.spawn(
        "stems.wav",
        PCMFloat1D::silence(48_000, 2, frames).map_err(text)?,
    );
    let report = MidSideStemSplit
        .run(arena, mix, stems, &available)
        .map_err(text)?;

    let read = |handle| -> Result<Vec<f32>, String> {
        let entity = arena.get(handle).map_err(text)?;
        let pcm = entity
            .component(ComponentKind::PCMFloat1D)
            .and_then(|data| data.as_pcm());
        Ok(pcm.ok_or("missing audio")?.samples().to_vec())
    };
    let (mix_samples, stem_samples) = (read(mix)?, read(stems)?);
    let exact = mix_samples
        .chunks(2)
        .zip(stem_samples.chunks(2))
        .all(|(lr, ms)| {
            (lr[0] - (ms[0] + ms[1])).abs() < 1e-6 && (lr[1] - (ms[0] - ms[1])).abs() < 1e-6
        });
    ensure(exact, "stems do not reconstruct the mix")?;
    ensure(report.bytes_copied == 0, "the kernel copied buffers")?;

    let names: Vec<&str> = available.iter().map(|backend| backend.name()).collect();
    Ok(format!(
        "backends [{}]; GGUF v{} header ({} tensors) parsed; {} on {}; {} samples in place, {} bytes copied",
        names.join(", "),
        gguf.version,
        gguf.tensor_count,
        selection.model.id,
        selection.backend,
        report.elements_written,
        report.bytes_copied
    ))
}

fn press(keybind: Keybind) -> InputEvent {
    InputEvent::Keyboard(KeyEvent::press(keybind.key, keybind.modifiers))
}

/// Renders frames at 60 Hz until the workspace asks to sleep.
fn frames_to_settle(workspace: &mut Workspace, ctx: &mut AppletContext<'_>) -> usize {
    let mut frames = 0;
    while workspace.update(1.0 / 60.0, ctx) != RenderDemand::Sleep && frames < 10_000 {
        workspace.begin_frame();
        frames += 1;
    }
    frames
}

/// Draws the shell offscreen and checks the header's color. `None` without a
/// GPU adapter.
fn render_shell(runtime: &mut Runtime) -> Result<Option<String>, String> {
    let Ok(gpu) = GpuContext::headless() else {
        return Ok(None);
    };
    let fonts = Fonts::new()?;
    let engine = &mut runtime.engine;
    let theme = Theme::from_rice(&engine.world.rice);
    let (width, height) = (960, 600);
    let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
    let (input, mut state) = (UiInput::default(), UiState::default());
    let mut canvas = Canvas::new(viewport);
    {
        let (workspace, mut ctx) = engine.shell();
        let mut ui = Ui {
            canvas: &mut canvas,
            fonts: &fonts,
            theme: &theme,
            input: &input,
            state: &mut state,
        };
        let frame = ShellFrame {
            viewport,
            safe_area: SafeAreaInsets::ZERO,
            form_factor: FormFactor::Desktop,
            fps: None,
            message: None,
            dialog: None,
        };
        draw_shell(workspace, &mut ui, &mut ctx, &frame);
    }
    let target = FrameTarget {
        width,
        height,
        scale: 1.0,
    };
    let pixels = render_offscreen(
        &gpu.device,
        &gpu.queue,
        target,
        &canvas,
        &fonts,
        &engine.world.arena,
        theme.base,
    )?;
    let chrome = engine
        .world
        .rice
        .get_color("colors.chrome")
        .ok_or("colors.chrome is missing")?;
    // Row 1 lies inside the header, between the menus and the tabs' text.
    let probe = &pixels[(width as usize + width as usize - 120) * 4..][..3];
    ensure(
        probe
            .iter()
            .zip([chrome.r, chrome.g, chrome.b])
            .all(|(&got, want)| got.abs_diff(want) <= 2),
        "the header did not render in colors.chrome",
    )?;
    Ok(Some(format!(
        "shell rendered on {} ({} draw commands)",
        gpu.adapter_label(),
        canvas.len()
    )))
}

fn check_window_manager(subject: &mut Subject) -> Result<String, String> {
    let runtime = &mut subject.runtime;
    let detected = runtime.engine.workspace.profile();
    let keymap = WmConfig::from_rice(&runtime.engine.world.rice)
        .map_err(text)?
        .keymap;
    let samples = std::env::temp_dir().join(format!("sue-self-check-{}", std::process::id()));
    std::fs::create_dir_all(&samples).map_err(text)?;
    let dropped = samples.join("card.jxl");
    let card = NativeJxlPipeline
        .encode_layers(&[&snapshot::test_card()], 1)
        .map_err(text)?;
    std::fs::write(&dropped, card).map_err(text)?;
    let viewport = Rect::new(0.0, 0.0, 1280.0, 800.0);

    let result = (|| -> Result<String, String> {
        let (studios, inspectors, left, right) = {
            let workspace = &runtime.engine.workspace;
            let hub = workspace.launchpad().hub();
            let rail = |zone| {
                workspace
                    .dock_applets(zone)
                    .iter()
                    .map(|(_, manifest)| manifest.id.to_string())
                    .collect::<Vec<_>>()
                    .join("+")
            };
            (
                hub.studios.len(),
                hub.inspectors.len(),
                rail(Zone::LeftDock),
                rail(Zone::RightDock),
            )
        };
        let boot = runtime.engine.workspace.launchpad().boot_target();

        // Quick Notes: typing, backspace, undo and the alternative redo.
        runtime.show_dock(quick_notes::ID);
        for ch in "Sue!".chars() {
            runtime.handle(&InputEvent::Keyboard(KeyEvent::typed(ch)));
        }
        runtime.handle(&InputEvent::Keyboard(KeyEvent::press(
            Key::Named(NamedKey::Backspace),
            Modifiers::NONE,
        )));
        let note_text = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .find(|(_, entity)| entity.name() == NOTE_NAME)
                .and_then(|(_, entity)| {
                    entity
                        .string_buffer()
                        .map(|buffer| buffer.as_str().to_owned())
                })
                .unwrap_or_default()
        };
        ensure(
            note_text(runtime).ends_with("Sue"),
            "typing or backspace failed",
        )?;
        runtime.handle(&press(keymap.undo));
        ensure(
            note_text(runtime).ends_with("Sue!"),
            "undo did not restore the deleted character",
        )?;
        runtime.handle(&press(keymap.redo_alt));
        ensure(note_text(runtime).ends_with("Sue"), "Ctrl+Y redo failed")?;

        // A file dropped anywhere opens in the applet its content fits.
        runtime.handle(&InputEvent::FileDrop(FileDrop::Dropped(dropped.clone())));
        let workspace = &runtime.engine.workspace;
        let opened = matches!(workspace.active_tab(), Tab::Applet { id, .. } if id.as_str() == image_viewer::ID);
        ensure(
            opened,
            "the dropped JPEG XL did not open in the Image Viewer",
        )?;
        let title = workspace.tab_title(workspace.active_index(), &runtime.engine.world);

        // Settings is one tab, however often it is asked for.
        runtime.handle(&press(keymap.open_settings));
        runtime.handle(&press(keymap.home));
        runtime.handle(&press(keymap.open_settings));
        let settings_tabs = runtime
            .engine
            .workspace
            .tabs()
            .iter()
            .filter(|tab| {
                tab.applet_id()
                    .is_some_and(|id| id.as_str() == settings::ID)
            })
            .count();
        ensure(settings_tabs == 1, "Settings opened more than one tab")?;

        // The pacing fix: a clicked dock toggle animates to the end with no
        // further input, then the scene goes back to sleep.
        runtime
            .engine
            .workspace
            .set_profile(HardwareProfile::Balanced);
        let mut now = Instant::now();
        runtime.frame(viewport, SafeAreaInsets::ZERO, now);
        let toggle = runtime
            .widget_rect(widget_id("shell", "toggle-left"))
            .ok_or("the left dock toggle was not drawn")?;
        let center = Point::new(
            toggle.x + toggle.width / 2.0,
            toggle.y + toggle.height / 2.0,
        );
        runtime.handle(&InputEvent::Mouse(MouseEvent::Moved { position: center }));
        for pressed in [true, false] {
            runtime.handle(&InputEvent::Mouse(MouseEvent::Button {
                button: MouseButton::Left,
                pressed,
                position: center,
            }));
            now += Duration::from_millis(16);
            runtime.frame(viewport, SafeAreaInsets::ZERO, now);
        }
        ensure(
            !runtime.engine.workspace.docks_open().0,
            "the click missed the toggle",
        )?;
        let mut animated = 0;
        while matches!(runtime.pacing(now), FramePacing::Animate { .. }) && animated < 2_000 {
            now += Duration::from_millis(16);
            runtime.frame(viewport, SafeAreaInsets::ZERO, now);
            if runtime.engine.workspace.is_animating() {
                animated += 1;
            }
        }
        ensure(
            runtime.engine.workspace.dock_reveal().0 == 0.0,
            "the dock animation stopped before the end",
        )?;
        ensure(
            !matches!(
                runtime.pacing(now + Duration::from_secs(1)),
                FramePacing::Animate { .. }
            ),
            "frames kept coming after the animation",
        )?;

        // Legacy hardware snaps in one frame.
        let (workspace, mut ctx) = runtime.engine.shell();
        workspace.set_profile(HardwareProfile::LegacyLowSpec);
        workspace
            .dispatch(&press(keymap.toggle_left_dock), &mut ctx)
            .map_err(text)?;
        let snapped = frames_to_settle(workspace, &mut ctx);
        workspace.set_profile(detected);
        // Switching profiles repaints once; then the scene must go idle.
        frames_to_settle(workspace, &mut ctx);
        ensure(
            animated > snapped && snapped <= 1,
            "springs or instant snap misbehaved",
        )?;
        ensure(workspace.docks_open().0, "left dock should be open again")?;

        let desktop = workspace.layout(
            Rect::new(0.0, 0.0, 1280.0, 800.0),
            SafeAreaInsets::ZERO,
            FormFactor::Desktop,
        );
        let insets = SafeAreaInsets {
            top: 47.0,
            right: 0.0,
            bottom: 34.0,
            left: 0.0,
        };
        let phone = workspace.layout(
            Rect::new(0.0, 0.0, 390.0, 844.0),
            insets,
            FormFactor::Mobile,
        );
        ensure(
            matches!(desktop.left, Placement::Tiled(_)) && desktop.left_rail.is_some(),
            "desktop left dock should tile beside its rail",
        )?;
        ensure(
            matches!(phone.left, Placement::Drawer { .. }) && phone.left_rail.is_none(),
            "phone left dock should be a drawer",
        )?;
        ensure(
            phone.center.width == 390.0,
            "phone Center Stage must fill the safe width",
        )?;
        ensure(
            workspace.begin_frame().is_none(),
            "an idle workspace still renders",
        )?;
        Ok(format!(
            "home: {}, {}; rails {left} | {right}; boot {boot:?}; notes typed, undone, redone; drop → {title}; Settings opens once; clicked dock animated {animated} frames to the end (balanced) vs {snapped} (legacy snap), then 0 FPS; center {}px desktop / {}px phone",
            count(studios, "studio", "studios"),
            count(inspectors, "inspector", "inspectors"),
            desktop.center.width,
            phone.center.width
        ))
    })();
    let _ = std::fs::remove_dir_all(&samples);
    let mut detail = result?;
    match render_shell(runtime)? {
        Some(rendered) => detail = format!("{detail}; {rendered}"),
        None => detail = format!("{detail}; GPU render skipped (no adapter)"),
    }
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::engine::tests::temp_host;

    #[test]
    fn every_subsystem_passes() {
        let (host, root) = temp_host("self-check");
        let runtime = Runtime::new(Engine::boot(&host).expect("engine boots")).unwrap();
        let mut subject = Subject { host, runtime };
        let report = run(&mut subject);
        assert!(report.passed(), "{report}");
        assert_eq!(report.checks.len(), 7);
        let _ = std::fs::remove_dir_all(&root);
    }
}
