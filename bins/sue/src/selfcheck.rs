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
use sue_wm::{AppletRole, NodeId, RiceEdit, ShellRequest, Theme};

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

fn node_of(runtime: &Runtime, applet: &str) -> Option<NodeId> {
    runtime
        .engine
        .world
        .scene
        .iter()
        .find(|(_, node)| node.applet.as_str() == applet)
        .map(|(id, _)| id)
}

fn text_of(runtime: &Runtime, node: NodeId) -> String {
    runtime
        .engine
        .world
        .scene
        .get(node)
        .and_then(|node| runtime.engine.world.arena.get(node.entity).ok())
        .and_then(|entity| entity.string_buffer())
        .map_or_else(String::new, |text| text.as_str().to_owned())
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

#[allow(
    clippy::too_many_lines,
    reason = "one scripted session through the shell"
)]
fn check_window_manager(subject: &mut Subject) -> Result<String, String> {
    let runtime = &mut subject.runtime;
    let ctx = egui::Context::default();
    let mut clock = (*ORIGIN).max(Instant::now());
    let mut tex = NoTextures;

    // The canvas Sue boots into.
    let kinds: Vec<String> = runtime
        .engine
        .world
        .scene
        .iter()
        .map(|(_, node)| node.applet.as_str().to_owned())
        .collect();
    ensure(
        kinds == ["document", "paint", "audio"],
        "Sue did not boot into a document, a paint layer and an audio waveform",
    )?;
    let roles = |role: AppletRole| runtime.applets.with_role(role).count();
    let (nodes, left, right, windows) = (
        roles(AppletRole::Node),
        roles(AppletRole::LeftDrawer),
        roles(AppletRole::RightDrawer),
        roles(AppletRole::Window),
    );
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.mode() == "canvas",
        "boot did not open the canvas",
    )?;

    // A drawer glides on its spring, and snaps on legacy hardware.
    runtime.request(
        ShellRequest::ShowDrawer(sue_wm::AppletId::new(crate::files::ID).map_err(text)?),
        &mut tex,
        clock,
    );
    let glide = run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.left_reveal() > 0.999,
        "the drawer did not open all the way",
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
    runtime.request(
        ShellRequest::ShowDrawer(sue_wm::AppletId::new(crate::player::ID).map_err(text)?),
        &mut tex,
        clock,
    );
    let snap = run_frames(runtime, &ctx, &mut clock, Vec::new());
    runtime
        .engine
        .history
        .undo(&mut runtime.engine.world)
        .map_err(text)?;
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        glide > snap,
        "springs did not glide, or legacy did not snap",
    )?;

    // A mode switch frames the document on the camera spring.
    let before = runtime.shell.camera().center();
    runtime.request(ShellRequest::Mode("document".to_owned()), &mut tex, clock);
    let framed = run_frames(runtime, &ctx, &mut clock, Vec::new());
    let document = node_of(runtime, crate::document::ID).ok_or("no document")?;
    ensure(
        runtime.shell.camera().center().distance(before) > 1.0 && framed > 10,
        "the camera teleported instead of gliding",
    )?;
    ensure(
        runtime.shell.selected() == Some(document) && runtime.shell.selection_pill_visible(),
        "the document's selection pill did not appear",
    )?;

    // Typing goes into the arena; Ctrl+Z and Ctrl+Y go through the UndoStack.
    let original = text_of(runtime, document);
    crate::document::select_words(&ctx, document, 0, 0);
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![egui::Event::Text("Sue ".to_owned())],
    );
    let typed = text_of(runtime, document);
    ensure(
        typed == format!("Sue {original}"),
        "typing did not reach the arena",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    ensure(
        text_of(runtime, document) == original,
        "Ctrl+Z did not undo the typing",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Y, egui::Modifiers::COMMAND)],
    );
    ensure(
        text_of(runtime, document) == typed,
        "Ctrl+Y did not redo the typing",
    )?;

    // A paint stroke is one undo step that stores only its tiles.
    let paint = node_of(runtime, crate::paint::ID).ok_or("no paint layer")?;
    let tiles = runtime.with_context(&mut tex, clock, |cx| crate::paint::scribble(cx, paint));
    ensure(tiles > 0, "the stroke touched no tiles")?;
    runtime
        .engine
        .history
        .undo(&mut runtime.engine.world)
        .map_err(text)?;
    runtime
        .engine
        .history
        .redo(&mut runtime.engine.world)
        .map_err(text)?;

    // Projection onto the timeline, which springs open.
    let audio = node_of(runtime, crate::audio_node::ID).ok_or("no audio")?;
    runtime.with_context(&mut tex, clock, |cx| {
        sue_wm::project_to_timeline(cx, audio, 3.0)
    });
    let timeline = run_frames(runtime, &ctx, &mut clock, Vec::new());
    let entity = runtime
        .engine
        .world
        .scene
        .get(audio)
        .map(|node| node.entity)
        .ok_or("audio vanished")?;
    ensure(
        runtime
            .engine
            .world
            .arena
            .get(entity)
            .map_err(text)?
            .projections()
            .contains(Projection::Timeline),
        "Project to Timeline did not attach the projection",
    )?;
    ensure(
        runtime.shell.timeline_open() && runtime.shell.timeline_reveal() > 0.999 && timeline > 5,
        "the timeline did not spring open",
    )?;

    // The marking menu grows on its spring; the palette filters.
    runtime.request(ShellRequest::Timeline(false), &mut tex, clock);
    runtime
        .shell
        .open_radial(runtime.shell.canvas_rect().center());
    let radial = run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(
        runtime.shell.radial_open() && radial > 5,
        "the marking menu did not open on its spring",
    )?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    runtime.shell.open_palette();
    runtime.shell.palette_query("paint layer");
    run_frames(runtime, &ctx, &mut clock, Vec::new());
    ensure(runtime.shell.palette_open(), "the palette did not open")?;
    run_frames(
        runtime,
        &ctx,
        &mut clock,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    let paints = runtime
        .engine
        .world
        .scene
        .iter()
        .filter(|(_, node)| node.applet.as_str() == crate::paint::ID)
        .count();
    ensure(paints == 2, "the palette's New Paint Layer did not run")?;

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
        "booted into canvas with document, paint, audio; {nodes} node kinds, {left}+{right} drawer applets, {windows} window; drawer glided {glide} frames ({}) vs {snap} (legacy snap); camera framed the document in {framed} frames; typed, Ctrl+Z, Ctrl+Y through the UndoStack; stroke touched {tiles} tiles, undone and redone; timeline projected and sprang open; marking menu and palette; then 0 FPS",
        sue_wm::profile_label(profile)
    );
    match render_shell(runtime, clock)? {
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
