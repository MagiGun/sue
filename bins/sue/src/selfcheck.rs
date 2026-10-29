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
    EguiPainter, FramePacing, GpuContext, HeadlessHost, NoTextures, PlatformHost, SurfaceDescriptor,
};
use sue_rice::{DEFAULT_RICE, RiceSchema, RiceValue, SetRiceValue, SettingsTree};
use sue_wm::{AppletRole, RiceEdit, ShellRequest, Theme};

use crate::runtime::Runtime;
use crate::snapshot;

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
    let checks: [(&'static str, CheckFn); 10] = [
        ("platform", check_platform),
        ("rice", check_rice),
        ("arena", check_arena),
        ("sniffer", check_sniffer),
        ("packages", check_packages),
        ("ai", check_ai),
        ("window manager", check_window_manager),
        ("backbone", check_backbone),
        ("sue paint", check_paint_studio),
        ("sue writer", check_writer_studio),
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
        .get_i64("layout.padding")
        .ok_or("layout.padding is missing")?;
    let edited = (gap + 4).min(24);

    engine
        .history
        .apply(
            &mut engine.world,
            RiceEdit(SetRiceValue::new("layout.padding", RiceValue::Int(edited))),
        )
        .map_err(text)?;
    ensure(
        tree.is_stale(&engine.world.rice),
        "the Settings tree missed the edit",
    )?;
    let written = engine.world.rice.to_suerice();
    let reparsed = RiceSchema::parse(&written).map_err(text)?;
    ensure(
        reparsed.get_i64("layout.padding") == Some(edited),
        "the edit was not written back",
    )?;
    ensure(
        Theme::from_rice(&engine.world.rice).padding == edited as f32,
        "the live theme did not follow the edit",
    )?;
    engine.history.undo(&mut engine.world).map_err(text)?;
    ensure(
        engine.world.rice.get_i64("layout.padding") == Some(gap),
        "undo did not restore the padding",
    )?;
    ensure(
        RiceSchema::parse(DEFAULT_RICE).map_err(text)?.to_suerice()
            == DEFAULT_RICE.replace("\r\n", "\n"),
        "the default rice does not round-trip byte for byte",
    )?;

    Ok(format!(
        "{} params, {} widgets in {} sections; padding {gap} to {edited} and back via global undo; {}; profile {}",
        engine.world.rice.len(),
        tree.widget_count(),
        tree.sections.len(),
        engine.rice_source,
        engine.profile
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

/// The headless clock's zero, so egui sees one monotonic time across runs.
static ORIGIN: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// One headless frame's input at `clock`.
fn raw_input(clock: Instant, events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        events,
        time: Some(clock.saturating_duration_since(*ORIGIN).as_secs_f64()),
        ..egui::RawInput::default()
    }
}

/// Runs headless egui frames of the whole shell every 8 ms until it sleeps.
/// `events` go into the first frame. Returns the number of frames.
fn run_frames(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    clock: &mut Instant,
    events: Vec<egui::Event>,
) -> usize {
    let mut events = Some(events);
    for count in 1..=1_500 {
        let raw = raw_input(*clock, events.take().unwrap_or_default());
        let now = *clock;
        ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
        *clock += Duration::from_millis(8);
        if count > 2 && !matches!(runtime.pacing(*clock), FramePacing::Animate { .. }) {
            return count;
        }
    }
    1_500
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Paints one frame of the shell with the GPU and counts pixels that are
/// not gray. `None` without an adapter.
fn render_shell(runtime: &mut Runtime, clock: Instant) -> Result<Option<String>, String> {
    let Ok(gpu) = GpuContext::headless() else {
        return Ok(None);
    };
    let mut painter = EguiPainter::new(&gpu, sue_platform::wgpu::TextureFormat::Rgba8UnormSrgb);
    let ctx = egui::Context::default();
    let size = [960_u32, 600_u32];
    let mut output = None;
    for frame in 0..3 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            time: Some(f64::from(frame) * 0.008),
            ..egui::RawInput::default()
        };
        let full = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut painter, clock));
        if frame < 2 {
            painter.apply_textures(full.textures_delta);
        } else {
            output = Some(full);
        }
    }
    let output = output.ok_or("no frame")?;
    let primitives = output.shapes.len();
    let pixels = painter.paint_offscreen(
        &ctx,
        size,
        runtime.theme().void,
        output.shapes,
        output.textures_delta,
        output.pixels_per_point,
    )?;
    let chromatic = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| {
            let (low, high) = (
                pixel[0].min(pixel[1]).min(pixel[2]),
                pixel[0].max(pixel[1]).max(pixel[2]),
            );
            high - low > 3
        })
        .count();
    let lit = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 40)
        .count();
    ensure(lit > 1_000, "the GPU frame came out blank")?;
    ensure(chromatic == 0, &format!("{chromatic} pixels are not gray"))?;
    Ok(Some(format!(
        "rendered on {} ({primitives} shapes, 0 tinted pixels)",
        gpu.adapter_label()
    )))
}

/// A click at `at`, a frame each for moving there, pressing and letting
/// go, then frames until the shell sleeps. Returns the number of frames.
fn click_at(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    clock: &mut Instant,
    at: egui::Pos2,
) -> usize {
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ] {
        let now = *clock;
        ctx.run_ui(raw_input(now, events), |ui| {
            runtime.ui(ui, &mut NoTextures, now);
        })
        .drop_without_applying_deltas();
        *clock += Duration::from_millis(8);
    }
    3 + run_frames(runtime, ctx, clock, Vec::new())
}

/// Where the Hub drew `applet`'s box and its pin in the last frame.
fn hub_box(runtime: &Runtime, applet: &str) -> Option<(egui::Rect, egui::Rect)> {
    runtime
        .shell
        .hub_boxes()
        .iter()
        .find(|(id, _, _)| id.as_str() == applet)
        .map(|(_, rect, pin)| (*rect, *pin))
}

/// The pin in the live rice.
fn pin_of(runtime: &Runtime) -> String {
    runtime
        .engine
        .world
        .rice
        .get_str(sue_wm::PIN_KEY)
        .unwrap_or_default()
        .to_owned()
}

/// Pins Sue Writer with a click on its pin in the Hub, then walks the pin
/// through the global undo keys. The pin is set back afterwards whatever
/// happens, so the self-check never leaves one in the rice.
fn check_pin(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    clock: &mut Instant,
) -> Result<(), String> {
    let before = pin_of(runtime);
    let outcome = (|| {
        let (_, pin) = hub_box(runtime, sue_writer::ID).ok_or("the Hub has no Sue Writer")?;
        click_at(runtime, ctx, clock, pin.center());
        ensure(
            pin_of(runtime) == sue_writer::ID,
            "the pin did not pin Sue Writer",
        )?;
        ensure(
            runtime.shell.hub_open(),
            "the pin opened the studio instead of pinning it",
        )?;
        ensure(
            sue_wm::pinned(&runtime.engine.world.rice, &runtime.applets).as_ref()
                == Some(&sue_wm::AppletId::new(sue_writer::ID).map_err(text)?),
            "the pinned studio is not the one Sue would open into",
        )?;
        run_frames(
            runtime,
            ctx,
            clock,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        ensure(
            pin_of(runtime) == before,
            "Ctrl+Z did not take the pin away",
        )?;
        run_frames(
            runtime,
            ctx,
            clock,
            vec![key(egui::Key::Y, egui::Modifiers::COMMAND)],
        );
        ensure(
            pin_of(runtime) == sue_writer::ID,
            "Ctrl+Y did not pin it again",
        )?;
        // One pin at a time: pinning the Converter moves it.
        let (_, pin) = hub_box(runtime, crate::convert::ID).ok_or("the Hub has no Converter")?;
        click_at(runtime, ctx, clock, pin.center());
        ensure(
            pin_of(runtime) == crate::convert::ID,
            "pinning a tool did not move the pin to it",
        )?;
        // A second click on the same pin takes it away.
        click_at(runtime, ctx, clock, pin.center());
        ensure(pin_of(runtime).is_empty(), "the pin did not come off")
    })();
    if pin_of(runtime) != before {
        runtime
            .engine
            .history
            .apply(
                &mut runtime.engine.world,
                RiceEdit(SetRiceValue::new(sue_wm::PIN_KEY, RiceValue::Text(before))),
            )
            .map_err(text)?;
        run_frames(runtime, ctx, clock, Vec::new());
    }
    outcome
}

#[allow(
    clippy::too_many_lines,
    reason = "one scripted session through the shell"
)]
fn check_window_manager(subject: &mut Subject) -> Result<String, String> {
    let runtime = &mut subject.runtime;
    let ctx = egui::Context::default();
    let mut clock = (*ORIGIN).max(Instant::now());
    let mut tex = NoTextures;
    let paint = sue_wm::AppletId::new(sue_paint::ID).map_err(text)?;
    let viewer = sue_wm::AppletId::new(crate::viewer::ID).map_err(text)?;
    let convert = sue_wm::AppletId::new(crate::convert::ID).map_err(text)?;

    // Sue opens on the Hub: a box for every studio and every tool.
    let roles = |role: AppletRole| runtime.applets.with_role(role).count();
    let (studios, tools, windows) = (
        roles(AppletRole::Studio),
        roles(AppletRole::Tool),
        roles(AppletRole::Window),
    );
    ensure(
        studios >= 2 && tools >= 2 && windows == 1,
        "the studios, tools and Settings are not all installed",
    )?;
    // Sue opens on the Hub, or straight into what is pinned there (the
    // check runs against this machine's own settings).
    let pinned_at_boot = sue_wm::pinned(&runtime.engine.world.rice, &runtime.applets);
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    let opened = match &pinned_at_boot {
        None => {
            ensure(
                runtime.shell.hub_open() && runtime.shell.page().is_none(),
                "with nothing pinned, Sue did not open on the Hub",
            )?;
            "booted on the Hub".to_owned()
        }
        Some(id) => {
            ensure(
                runtime.shell.page() == Some(id) && runtime.shell.reveal() > 0.999,
                "Sue did not open straight into the pinned studio or tool",
            )?;
            runtime.request(ShellRequest::Hub, &mut tex, clock);
            run_frames(runtime, &ctx, &mut clock, Vec::new());
            format!("booted straight into the pinned {id}, then the Hub")
        }
    };
    ensure(
        runtime.shell.hub_boxes().len() == studios + tools,
        "the Hub does not show a box for every studio and tool",
    )?;
    let sue = runtime.shell.sue_button();
    ensure(
        sue.min == egui::Pos2::ZERO,
        "the Sue button is not in the window's top left corner",
    )?;

    // The pin, one at a time, through the undo keys.
    check_pin(runtime, &ctx, &mut clock)?;

    // A click on a box opens its studio, gliding in over the Hub; on
    // legacy hardware it snaps.
    let (paint_box, _) = hub_box(runtime, sue_paint::ID).ok_or("the Hub has no Sue Paint")?;
    let glide = click_at(
        runtime,
        &ctx,
        &mut clock,
        paint_box.left_top() + egui::vec2(24.0, 24.0),
    );
    ensure(
        runtime.shell.page() == Some(&paint) && runtime.shell.reveal() > 0.999,
        "a click on Sue Paint's box did not open it",
    )?;
    ensure(
        runtime.shell.hub_boxes().is_empty(),
        "the Hub still shows under an open studio",
    )?;
    runtime.request(ShellRequest::Hub, &mut tex, clock);
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.hub_open() && runtime.shell.reveal() == 0.0,
        "Sue Paint did not go back to the Hub",
    )?;
    let profile = runtime.engine.profile;
    runtime
        .engine
        .history
        .apply(
            &mut runtime.engine.world,
            RiceEdit(SetRiceValue::new(
                "profile.hardware",
                RiceValue::Text("legacy-low-spec".to_owned()),
            )),
        )
        .map_err(text)?;
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    runtime.request(ShellRequest::Open(paint.clone()), &mut tex, clock);
    let snap = run_frames(runtime, &ctx, &mut clock, Vec::new());
    runtime
        .engine
        .history
        .undo(&mut runtime.engine.world)
        .map_err(text)?;
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        glide > snap,
        "the page did not glide in, or legacy did not snap",
    )?;

    // The Sue button opens the Sue menu over the studio: the way back to
    // the Hub, every studio and tool, files, projects and Settings.
    click_at(runtime, &ctx, &mut clock, sue.center());
    ensure(
        runtime.shell.sue_menu_open(),
        "the Sue button did not open the Sue menu",
    )?;
    let labels = runtime.shell.sue_menu_labels(&[]);
    for wanted in [
        "Hub", "Studios", "Tools", "New", "File", "Project", "Settings", "Quit Sue",
    ] {
        ensure(
            labels.iter().any(|label| label == wanted),
            &format!("the Sue menu has no {wanted}"),
        )?;
    }
    ensure(
        runtime.shell.sue_menu_labels(&["Tools"]) == ["Image Viewer", "Converter"],
        "the Sue menu's Tools are not the tools",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    ensure(
        !runtime.shell.sue_menu_open(),
        "Esc did not close the Sue menu",
    )?;

    // A picture opens in the Image Viewer, a tool with a page of its own,
    // and goes on from there by the Sue menu's Send To.
    let picture = runtime
        .engine
        .world
        .arena
        .iter()
        .find(|(_, entity)| crate::viewer::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .ok_or("no picture to view")?;
    runtime.request(
        ShellRequest::OpenIn {
            entity: picture,
            applet: viewer.clone(),
        },
        &mut tex,
        clock,
    );
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.page() == Some(&viewer)
            && runtime
                .applets
                .get(&viewer)
                .and_then(|applet| applet.focus())
                == Some(picture),
        "the picture did not open in the Image Viewer",
    )?;
    runtime.request(ShellRequest::SueMenu, &mut tex, clock);
    let send_to = runtime.shell.sue_menu_labels(&["Send To"]);
    ensure(
        send_to.iter().any(|label| label == "Converter"),
        "the Sue menu cannot send the picture to the Converter",
    )?;
    let sent = runtime.with_shell(&mut tex, clock, |shell, io, now| {
        shell.sue_menu_pick(&["Send To", "Converter"], io, now)
    });
    ensure(sent, "Send To, Converter is not in the Sue menu")?;
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.page() == Some(&convert),
        "Send To did not open the Converter",
    )?;

    // The palette finds a tool; Settings floats over it and Esc closes it.
    runtime.shell.open_palette();
    runtime.shell.palette_query("open image viewer");
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(runtime.shell.palette_open(), "the palette did not open")?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    ensure(
        runtime.shell.page() == Some(&viewer),
        "the palette's Open Image Viewer did not run",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Comma, egui::Modifiers::COMMAND)],
    );
    ensure(
        runtime.shell.window().is_some(),
        "Ctrl+, did not open Settings",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    ensure(
        runtime.shell.window().is_none(),
        "Esc did not close Settings",
    )?;
    runtime.request(ShellRequest::Hub, &mut tex, clock);
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(runtime.shell.hub_open(), "the Hub did not come back")?;

    // Then the shell sleeps, and egui asks for no frame either (its own hover
    // fades finish first, a second or so of headless time).
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    clock += Duration::from_secs(2);
    for _ in 0..2 {
        let now = clock;
        ctx.run_ui(raw_input(now, Vec::new()), |ui| {
            runtime.ui(ui, &mut NoTextures, now);
        })
        .drop_without_applying_deltas();
        clock += Duration::from_millis(8);
    }
    let raw = raw_input(clock, Vec::new());
    let now = clock;
    let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let delay = output
        .viewport_output
        .values()
        .next()
        .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
    output.drop_without_applying_deltas();
    ensure(
        delay > Duration::from_secs(60),
        &format!(
            "egui keeps asking for frames while idle (every {delay:?}, from {:?})",
            ctx.repaint_causes()
        ),
    )?;
    ensure(
        matches!(
            runtime.pacing(clock + Duration::from_secs(5)),
            FramePacing::Sleep | FramePacing::WakeAt(_)
        ),
        "the idle shell keeps drawing",
    )?;
    let mut detail = format!(
        "{opened} ({}, {}, {}); pinned with a click, one pin at a time, Ctrl+Z and Ctrl+Y; a box opened Sue Paint, gliding in {glide} frames ({}) vs {snap} (legacy snap); the Sue button's menu; a picture into the Image Viewer, Send To the Converter; palette, Settings, back to the Hub; then 0 FPS",
        count(studios, "studio", "studios"),
        count(tools, "tool", "tools"),
        count(windows, "window", "windows"),
        sue_wm::profile_label(profile)
    );
    match render_shell(runtime, clock)? {
        Some(rendered) => detail = format!("{detail}; {rendered}"),
        None => detail = format!("{detail}; GPU render skipped (no adapter)"),
    }
    Ok(detail)
}

/// Sue Writer: a novel made and written, saved in the project and opened
/// again, compiled to every publishing format, and its Word file read back.
fn check_writer_studio(subject: &mut Subject) -> Result<String, String> {
    use sue_model::manuscript::{self, role};

    let runtime = &mut subject.runtime;
    let studio = sue_wm::AppletId::new(sue_writer::ID).map_err(text)?;
    let now = Instant::now();
    let mut textures = sue_platform::NoTextures;
    runtime.request(ShellRequest::Open(studio.clone()), &mut textures, now);
    ensure(
        runtime.shell.page() == Some(&studio),
        "Sue Writer did not open",
    )?;
    let depth = runtime.engine.history.depth();
    let novel = runtime
        .with_context(&mut textures, now, sue_writer::sample_project)
        .ok_or("the sample novel was not made")?;
    let steps = runtime.engine.history.depth() - depth;
    let arena = &runtime.engine.world.arena;
    let draft = manuscript::folder(arena, novel, role::DRAFT).ok_or("the novel has no draft")?;
    let words = manuscript::total_words(arena, draft);
    ensure(words > 100, "the scenes were not written")?;

    // Every step undoes, and comes back.
    for _ in 0..steps {
        runtime
            .engine
            .history
            .undo(&mut runtime.engine.world)
            .map_err(text)?;
    }
    ensure(
        runtime.engine.world.arena.get_by_id(novel).is_err(),
        "undo left the novel behind",
    )?;
    for _ in 0..steps {
        runtime
            .engine
            .history
            .redo(&mut runtime.engine.world)
            .map_err(text)?;
    }
    ensure(
        manuscript::total_words(&runtime.engine.world.arena, draft) == words,
        "redo did not bring every word back",
    )?;

    // The novel lives in the project.
    let dir = runtime.engine.paths.cache_dir.join("self-check-writer");
    std::fs::create_dir_all(&dir).map_err(text)?;
    let path = dir.join("writer.sue");
    runtime.save_project_to(&path)?;
    let bytes = std::fs::read(&path).map_err(text)?;
    runtime.open_project(&path, &bytes)?;
    let arena = &runtime.engine.world.arena;
    ensure(
        manuscript::total_words(arena, draft) == words,
        "the project changed the novel",
    )?;

    // Every format compiles; the Word file reads back to the same words.
    let fonts = sue_type::Fonts::installed(12.0);
    let look = |sheet: sue_type::StyleSheet| sue_writer::Look {
        sheet,
        page: sue_type::PageSetup::default(),
        title: "The Lighthouse Keeper".to_owned(),
        author: "A. Writer".to_owned(),
    };
    let mut sizes = Vec::new();
    for (format, _, _) in sue_writer::FORMATS {
        let options = sue_writer::Options {
            format: (*format).to_owned(),
            from: draft,
            titles: sue_writer::Titles::Folders,
            separator: sue_writer::Separator::SceneBreak,
            title_page: true,
            chapter_pages: true,
        };
        let bytes = sue_writer::compile(
            arena,
            &options,
            &look(sue_type::StyleSheet::manuscript(&fonts)),
        )?;
        if *format == "docx" {
            let (text, _) = sue_io::publish::from_docx(&bytes)?;
            ensure(
                text.contains("Mara had kept the light"),
                "the Word file lost the text",
            )?;
        }
        if *format == "pdf" {
            ensure(bytes.starts_with(b"%PDF-"), "the PDF is not a PDF")?;
        }
        sizes.push((*format, bytes.len()));
    }
    let _ = std::fs::remove_dir_all(&dir);
    runtime.request(ShellRequest::Hub, &mut textures, now);
    ensure(
        runtime.shell.hub_open(),
        "Sue Writer did not go back to the Hub",
    )?;
    let pdf = sizes
        .iter()
        .find(|(format, _)| *format == "pdf")
        .map_or(0, |(_, size)| *size);
    Ok(format!(
        "novel of {words} words written in {steps} undo steps, undone, redone, saved in the project, compiled to {} formats (PDF {} KiB with its fonts), Word read back",
        sizes.len(),
        pdf.div_ceil(1024)
    ))
}

/// Sue Paint: a layered illustration painted with the real brushes, laid
/// over its paper, undone and redone, saved in the project and exported.
fn check_paint_studio(subject: &mut Subject) -> Result<String, String> {
    use sue_paint::composite::{RenderOptions, flatten};
    use sue_paint::doc;

    let runtime = &mut subject.runtime;
    let studio = sue_wm::AppletId::new(sue_paint::ID).map_err(text)?;
    let now = Instant::now();
    let mut textures = sue_platform::NoTextures;
    runtime.request(ShellRequest::Open(studio.clone()), &mut textures, now);
    ensure(
        runtime.shell.page() == Some(&studio),
        "Sue Paint did not open",
    )?;
    let depth = runtime.engine.history.depth();
    let picture = runtime
        .with_context(&mut textures, now, |cx| {
            sue_paint::sample_picture(cx, 320, 240)
        })
        .ok_or("the sample illustration was not made")?;
    let steps = runtime.engine.history.depth() - depth;
    let arena = &runtime.engine.world.arena;
    let layers = doc::layers(arena, picture);
    ensure(
        layers.len() == 3,
        "the illustration does not have paper, color and ink",
    )?;
    let flat = flatten(arena, picture, RenderOptions::default()).ok_or("nothing to flatten")?;
    let pixels = flat.bytes().ok_or("the flattened picture is not 8-bit")?;
    let inked = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] < 80 && pixel[1] < 80)
        .count();
    let colored = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0].abs_diff(pixel[2]) > 60)
        .count();
    ensure(
        inked > 200 && colored > 200,
        "the strokes did not land on the picture",
    )?;
    ensure(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255),
        "the paper does not show under the layers",
    )?;

    // Every stroke undoes, and comes back.
    let before = flat.clone();
    for _ in 0..steps {
        runtime
            .engine
            .history
            .undo(&mut runtime.engine.world)
            .map_err(text)?;
    }
    ensure(
        runtime.engine.world.arena.get_by_id(picture).is_err(),
        "undo left the illustration behind",
    )?;
    for _ in 0..steps {
        runtime
            .engine
            .history
            .redo(&mut runtime.engine.world)
            .map_err(text)?;
    }
    let again = flatten(
        &runtime.engine.world.arena,
        picture,
        RenderOptions::default(),
    )
    .ok_or("redo did not bring the illustration back")?;
    ensure(again == before, "redo did not restore every pixel")?;

    // The illustration lives in the project.
    let dir = runtime.engine.paths.cache_dir.join("self-check-paint");
    std::fs::create_dir_all(&dir).map_err(text)?;
    let path = dir.join("paint.sue");
    runtime.save_project_to(&path)?;
    let bytes = std::fs::read(&path).map_err(text)?;
    runtime.open_project(&path, &bytes)?;
    let _ = std::fs::remove_dir_all(&dir);
    let reopened = flatten(
        &runtime.engine.world.arena,
        picture,
        RenderOptions::default(),
    )
    .ok_or("the illustration did not come back from the project")?;
    ensure(reopened == before, "the project changed the illustration")?;

    // And exports through the format registry.
    let png = sue_io::encode_png(&reopened)?;
    let formats = sue_io::formats(sue_model::registry());
    let back = formats
        .import(&png, "sample.png", &sue_core::Properties::new())
        .map_err(text)?;
    let raster = back
        .entities
        .first()
        .and_then(|entity| entity.component(ComponentKind::RasterTile2D))
        .and_then(|data| data.as_raster())
        .ok_or("the exported PNG has no pixels")?;
    ensure(
        (raster.width(), raster.height()) == (320, 240),
        "the PNG has the wrong size",
    )?;
    runtime.request(ShellRequest::Hub, &mut textures, now);
    ensure(
        runtime.shell.hub_open(),
        "Sue Paint did not go back to the Hub",
    )?;
    Ok(format!(
        "illustration of {} painted in {} undo steps, composited, undone, redone, saved in the project and exported as PNG ({} KiB)",
        count(layers.len(), "layer", "layers"),
        steps,
        png.len().div_ceil(1024),
    ))
}

/// The shared backbone: the project file, the format registry, operators,
/// presets and background jobs, run against the live engine.
fn check_backbone(subject: &mut Subject) -> Result<String, String> {
    use sue_core::txn::SetProperty;
    use sue_core::{
        Entity, EntityId, FnOperator, JobPool, OperatorRegistry, Plan, Preset, PresetLibrary,
        Properties, PropertyDef, Shared, SueFile, TxnError,
    };

    let runtime = &mut subject.runtime;
    let document = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .map(|(_, entity)| (entity.id(), entity.name().to_owned()))
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = document(runtime);
    let dir = runtime.engine.paths.cache_dir.join("self-check");
    std::fs::create_dir_all(&dir).map_err(text)?;
    let path = dir.join("backbone.sue");
    runtime.save_project_to(&path)?;
    let bytes = std::fs::read(&path).map_err(text)?;
    ensure(SueFile::sniff(&bytes), "the project file has no Sue header")?;
    runtime.open_project(&path, &bytes)?;
    let _ = std::fs::remove_dir_all(&dir);
    ensure(
        document(runtime) == before,
        "the project lost or renamed entities",
    )?;
    ensure(
        runtime.project() == Some(path.as_path()),
        "the project path was not kept",
    )?;

    // Every format through one registry: export the sketch, read it back.
    let formats = sue_io::formats(sue_model::registry());
    let arena = &runtime.engine.world.arena;
    let sketch = arena
        .iter()
        .find(|(_, entity)| entity.component(ComponentKind::RasterTile2D).is_some())
        .map(|(_, entity)| entity.id())
        .ok_or("no picture to export")?;
    let png = formats
        .get("png")
        .ok_or("no PNG handler")?
        .export(arena, &[sketch], &Properties::new())
        .map_err(text)?;
    let back = formats
        .import(&png, "sketch.png", &Properties::new())
        .map_err(text)?;
    ensure(
        back.entities.len() == 1,
        "the PNG did not come back as one entity",
    )?;
    let csv = formats
        .import(b"a,b\n1,2\n", "t.csv", &Properties::new())
        .map_err(text)?;
    ensure(
        csv.entities[0].ext::<sue_model::Table>().is_some(),
        "a CSV did not become a table",
    )?;

    // One operator, run with a preset, undone through the history.
    let mut operators: OperatorRegistry<Arena> = OperatorRegistry::new();
    operators.register(
        FnOperator::new(
            "check.tag",
            "Tag",
            |_: &Arena, selection: &[EntityId], params: &Properties| {
                let id = *selection
                    .first()
                    .ok_or_else(|| TxnError::new("nothing selected"))?;
                let tag = params.str("tag").unwrap_or_default().to_owned();
                Ok(Box::new(SetProperty::new(id, "check.tag", tag)) as Plan<Arena>)
            },
        )
        .param(PropertyDef::new("tag", "Tag", "draft")),
    );
    let mut presets = PresetLibrary::new();
    presets.add(Preset::new(
        "Final",
        "check.tag",
        Properties::new().with("tag", "final"),
    ));
    let mut arena = Arena::new();
    let handle = arena.insert(Entity::empty("probe", "check.probe"));
    let id = arena.get(handle).map_err(text)?.id();
    let mut history = UndoStack::new();
    let preset = presets.get("check.tag", "Final").ok_or("preset missing")?;
    operators
        .run("check.tag", &mut arena, &mut history, &[id], &preset.values)
        .map_err(text)?;
    ensure(
        arena
            .get(handle)
            .map_err(text)?
            .properties()
            .str("check.tag")
            == Some("final"),
        "the operator did not apply the preset",
    )?;
    history.undo(&mut arena).map_err(text)?;
    ensure(
        arena
            .get(handle)
            .map_err(text)?
            .property("check.tag")
            .is_none(),
        "undo kept the operator's change",
    )?;

    // A background job reads a shared buffer without copying it.
    let pool = JobPool::new(1);
    let samples = Shared::new(vec![0.5_f32; 48_000]);
    let input = samples.clone();
    let peak = pool
        .spawn("peak", move |_| {
            Ok(input.iter().copied().fold(0.0_f32, f32::max))
        })
        .wait()
        .map_err(text)?;
    ensure(
        peak == 0.5 && samples.holders() == 1,
        "the job did not run cleanly",
    )?;

    Ok(format!(
        "project saved and reopened ({}, {} KiB); {} formats; PNG and CSV through the registry; operator with preset undone; background job",
        count(before.len(), "entity", "entities"),
        bytes.len().div_ceil(1024),
        formats.iter().count(),
    ))
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
        assert_eq!(report.checks.len(), 10);
        let _ = std::fs::remove_dir_all(&root);
    }
}
