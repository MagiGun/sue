//! The boot self-check: exercises every subsystem through the shared engine
//! state, proving they work and that they talk to each other.

use std::f32::consts::TAU;
use std::fmt;

use sue_ai::{
    AiTask, ComputeBackend, GgufHeader, LocalModelRegistry, MidSideStemSplit, ZeroCopyTensorTask,
};
use sue_core::txn::{AttachProjection, InsertText};
use sue_core::{ComponentKind, PCMFloat1D, Projection, StringBuffer, TimeSpan};
use sue_io::{
    ContainerOnlyPipeline, IngestRoute, JXL_CONTAINER_SIGNATURE, JxlError, JxlPipeline,
    NativeFormat, StructuredFormat, TileCache, TileKey, byte_map_tile, sniff_stream,
};
use sue_pkg::{
    AtomicUpdater, Dependency, PackageIndex, PackageKind, PackageManifest, SignedManifest,
    SigningKey, TrustStore, UpdateOutcome, Version, resolve,
};
use sue_platform::{
    FormFactor, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PlatformHost, SafeAreaInsets,
    SurfaceDescriptor,
};
use sue_rice::{
    DEFAULT_RICE, HardwareProfile, Keybind, RiceSchema, RiceValue, SetRiceValue, SettingsTree,
};
use sue_wm::{AppletContext, AppletId, Placement, Rect, RenderDemand, WmConfig, Workspace};

use crate::engine::Engine;
use crate::quick_notes;

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

type CheckFn = fn(&mut Engine) -> Result<String, String>;

/// Runs every check against `engine`, in dependency order.
pub fn run(engine: &mut Engine) -> Report {
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
                outcome: check(engine),
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

fn check_platform(engine: &mut Engine) -> Result<String, String> {
    let host = &mut engine.host;
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
        "{} · {} · {} cores · surface {}x{} · config {}",
        host.os(),
        host.form_factor(),
        host.capabilities().logical_cores,
        resized.width,
        resized.height,
        host.paths().config_dir.display()
    ))
}

fn check_rice(engine: &mut Engine) -> Result<String, String> {
    let schema = &mut engine.rice;
    let tree = SettingsTree::build(schema);
    ensure(
        tree.widget_count() == schema.len(),
        "a parameter has no Settings widget",
    )?;
    let gap = schema
        .get_i64("layout.gap")
        .ok_or("layout.gap is missing")?;
    let edited = (gap + 4).min(64);

    engine
        .rice_history
        .apply(
            schema,
            SetRiceValue::new("layout.gap", RiceValue::Int(edited)),
        )
        .map_err(text)?;
    ensure(tree.is_stale(schema), "the Settings tree missed the edit")?;
    let written = schema.to_suerice();
    let reparsed = RiceSchema::parse(&written).map_err(text)?;
    ensure(
        reparsed.get_i64("layout.gap") == Some(edited),
        "the edit was not written back",
    )?;
    engine.rice_history.undo(schema).map_err(text)?;
    ensure(
        schema.get_i64("layout.gap") == Some(gap),
        "undo did not restore the gap",
    )?;
    ensure(
        RiceSchema::parse(DEFAULT_RICE).map_err(text)?.to_suerice()
            == DEFAULT_RICE.replace("\r\n", "\n"),
        "the default rice does not round-trip byte for byte",
    )?;

    Ok(format!(
        "{} params → {} widgets in {} sections · gap {gap}→{edited}→{gap} via undo · {} · profile {}",
        schema.len(),
        tree.widget_count(),
        tree.sections.len(),
        engine.rice_source,
        engine.profile
    ))
}

fn check_arena(engine: &mut Engine) -> Result<String, String> {
    let (arena, history) = (&mut engine.arena, &mut engine.history);
    let doc = arena.spawn(
        "Faustian Enigma.md",
        StringBuffer::from("# Faustian Enigma\n"),
    );
    let length = |arena: &sue_core::Arena| -> Result<usize, String> {
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
    let buffer_address = |arena: &sue_core::Arena| {
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
        "{doc} · {} txns, {branches} branches · projections {projections} · buffer address stable · {}",
        history.len(),
        count(arena.len(), "entity", "entities")
    ))
}

/// A minimal JPEG XL container: signature, `ftyp`, an 8×8 codestream and a
/// JPEG reconstruction box.
fn sample_jxl() -> Vec<u8> {
    let boxed = |kind: &[u8; 4], payload: &[u8]| {
        let size = u32::try_from(payload.len() + 8).unwrap_or(u32::MAX);
        [&size.to_be_bytes()[..], kind, payload].concat()
    };
    [
        JXL_CONTAINER_SIGNATURE.to_vec(),
        boxed(b"ftyp", b"jxl \0\0\0\0jxl "),
        boxed(b"jxlc", &[0xFF, 0x0A, 0x41, 0x00]),
        boxed(b"jbrd", &[0; 8]),
    ]
    .concat()
}

/// Deterministic high-entropy bytes (xorshift64).
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[7]
        })
        .collect()
}

fn check_sniffer(engine: &mut Engine) -> Result<String, String> {
    let jxl = sample_jxl();
    let noise = noise(4096);
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".as_slice();
    let json = br#"{"motto": "jack of all trades"}"#.as_slice();
    let routes = [
        ("jxl", sniff_stream(&jxl)),
        ("png", sniff_stream(png)),
        ("rice", sniff_stream(DEFAULT_RICE.as_bytes())),
        ("json", sniff_stream(json)),
        ("noise", sniff_stream(&noise)),
    ];
    ensure(
        routes[0].1 == IngestRoute::Native(NativeFormat::JpegXl { container: true }),
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
    let IngestRoute::ByteMap(hint) = routes[4].1 else {
        return Err("noise was not sent to the byte map".to_owned());
    };
    let summary: Vec<String> = routes
        .iter()
        .map(|(label, route)| format!("{label}→T{}", route.tier() as u8))
        .collect();

    let pipeline = ContainerOnlyPipeline;
    let probe = pipeline.probe(&jxl).map_err(text)?;
    ensure(
        probe.width == Some(8) && probe.height == Some(8),
        "JXL size header misread",
    )?;
    ensure(probe.has_jpeg_reconstruction, "jbrd box not found")?;
    ensure(
        matches!(
            pipeline.reconstruct_jpeg(&jxl),
            Err(JxlError::CodecUnavailable { .. })
        ),
        "the container-only pipeline claimed to decode",
    )?;

    let width = engine
        .rice
        .get_i64("io.byte_map_width")
        .and_then(|width| u32::try_from(width).ok())
        .ok_or("io.byte_map_width is missing")?;
    let tile = byte_map_tile(noise, width).map_err(text)?;
    let (columns, rows) = (tile.width(), tile.height());
    let handle = engine.arena.spawn("noise.bin", tile);
    ensure(
        engine.arena.get(handle).map_err(text)?.base_kind() == ComponentKind::RasterTile2D,
        "byte map is not a raster",
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
        "{} ({:.2} bits/B) · JXL {}x{} + jbrd, pixels await codec package · byte map {columns}x{rows} → {handle} · tile cache hit",
        summary.join(" "),
        hint.entropy_bits_per_byte,
        probe.width.unwrap_or(0),
        probe.height.unwrap_or(0),
    ))
}

fn check_packages(_engine: &mut Engine) -> Result<String, String> {
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
        "{} signed manifests verified, tampered one rejected · plan {} · gen {generation} committed · gen {failed} failed its health check → gen {restored} restored",
        index.len(),
        order.join(" → ")
    ))
}

fn check_ai(engine: &mut Engine) -> Result<String, String> {
    let os = engine.host.os();
    let capabilities = engine.host.capabilities().clone();
    let available = ComputeBackend::detect_available(os, &capabilities);

    let mut header = b"GGUF".to_vec();
    header.extend_from_slice(&3_u32.to_le_bytes());
    header.extend_from_slice(&291_u64.to_le_bytes());
    header.extend_from_slice(&24_u64.to_le_bytes());
    let gguf = GgufHeader::parse(&header).map_err(text)?;

    let mut registry = LocalModelRegistry::new();
    registry
        .register(MidSideStemSplit::descriptor())
        .map_err(text)?;
    let budget = capabilities.memory_bytes.unwrap_or(u64::MAX);
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
    let mix = engine.arena.spawn(
        "mix.wav",
        PCMFloat1D::new(48_000, 2, samples).map_err(text)?,
    );
    let stems = engine.arena.spawn(
        "stems.wav",
        PCMFloat1D::silence(48_000, 2, frames).map_err(text)?,
    );
    let report = MidSideStemSplit
        .run(&mut engine.arena, mix, stems, &available)
        .map_err(text)?;

    let read = |handle| -> Result<Vec<f32>, String> {
        let entity = engine.arena.get(handle).map_err(text)?;
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
        "backends [{}] · GGUF v{} header ({} tensors) parsed · {} on {} · {} samples in place, {} bytes copied",
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

fn check_window_manager(engine: &mut Engine) -> Result<String, String> {
    let detected = engine.profile;
    let keymap = WmConfig::from_rice(&engine.rice).map_err(text)?.keymap;
    let (workspace, mut ctx) = engine.shell();

    let hub = workspace.launchpad().hub();
    let (studios, utilities) = (hub.studios.len(), hub.utilities.len());
    let boot = workspace.launchpad().boot_target();

    let notes = AppletId::new(quick_notes::ID).map_err(text)?;
    workspace.open_applet(&notes, &mut ctx).map_err(text)?;
    let typed = "Sue!";
    for ch in typed.chars() {
        workspace
            .dispatch(&InputEvent::Keyboard(KeyEvent::typed(ch)), &mut ctx)
            .map_err(text)?;
    }
    let backspace = InputEvent::Keyboard(KeyEvent::press(
        Key::Named(NamedKey::Backspace),
        Modifiers::NONE,
    ));
    workspace.dispatch(&backspace, &mut ctx).map_err(text)?;
    let note_text = |ctx: &AppletContext<'_>| {
        ctx.arena
            .iter()
            .find(|(_, entity)| entity.name() == quick_notes::NOTE_NAME)
            .and_then(|(_, entity)| {
                entity
                    .string_buffer()
                    .map(|buffer| buffer.as_str().to_owned())
            })
            .unwrap_or_default()
    };
    ensure(note_text(&ctx) == "Sue", "typing or backspace failed")?;
    workspace
        .dispatch(&press(keymap.undo), &mut ctx)
        .map_err(text)?;
    ensure(
        note_text(&ctx) == "Sue!",
        "undo did not restore the deleted character",
    )?;
    workspace
        .dispatch(&press(keymap.redo), &mut ctx)
        .map_err(text)?;
    ensure(note_text(&ctx) == "Sue", "redo failed")?;

    workspace.set_profile(HardwareProfile::Balanced);
    workspace
        .dispatch(&press(keymap.toggle_left_dock), &mut ctx)
        .map_err(text)?;
    let animated = frames_to_settle(workspace, &mut ctx);
    workspace.set_profile(HardwareProfile::LegacyLowSpec);
    workspace
        .dispatch(&press(keymap.toggle_left_dock), &mut ctx)
        .map_err(text)?;
    let snapped = frames_to_settle(workspace, &mut ctx);
    workspace.set_profile(detected);
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
        matches!(desktop.left, Placement::Tiled(_)),
        "desktop left dock should tile",
    )?;
    ensure(
        matches!(phone.left, Placement::Drawer { .. }),
        "phone left dock should be a drawer",
    )?;
    ensure(
        phone.center == Rect::new(0.0, 47.0, 390.0, 763.0),
        "phone Center Stage must fill the safe area",
    )?;
    ensure(
        workspace.begin_frame().is_none(),
        "an idle workspace still renders",
    )?;

    Ok(format!(
        "hub: {}, {} · boot {boot:?} · quick-notes typed \"{typed}\" ⌫ undo redo → \"Sue\" · dock {animated} frames (balanced) vs {snapped} (legacy snap) · center {}px desktop / {}px phone · idle → 0 FPS",
        count(studios, "studio", "studios"),
        count(utilities, "utility", "utilities"),
        desktop.center.width,
        phone.center.width
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subsystem_passes() {
        let mut engine = Engine::boot().expect("engine boots");
        let report = run(&mut engine);
        assert!(report.passed(), "{report}");
        assert_eq!(report.checks.len(), 7);
    }
}
