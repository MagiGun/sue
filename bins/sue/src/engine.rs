//! Boot: wires a platform host, the live rice (migrated when it is old), the
//! undoable world, the catalog the applets read, and the window manager.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use sue_ai::{AiTask, ComputeBackend, LocalModelRegistry, MidSideStemSplit, ModelFormat};
use sue_core::{StringBuffer, UndoStack};
use sue_io::meta;
use sue_platform::{
    AudioConfig, AudioSink, FormFactor, HardwareCapabilities, OsFamily, PlatformHost,
    PlatformPaths, RealtimeStatus, write_atomic,
};
use sue_rice::{
    DEFAULT_RICE, HardwareProfile, RiceDeclaration, RiceParameter, RiceSchema, builtin_presets,
    migrate,
};
use sue_wm::{
    AppletCategory, AppletContext, Catalog, Choice, CommandButton, InfoTable, SettingsGroup,
    SettingsItem, SettingsPage, ShellRequest, WmConfig, Workspace, World, Zone,
};

use crate::applets;
use crate::util::human_bytes;

/// The live rice file in the platform rice directory. It starts as a byte-for-
/// byte copy of the built-in `default.suerice` and is written back
/// format-preservingly after every edit.
pub const RICE_FILE: &str = "default.suerice";

/// Where Quick Notes are saved, in the notes directory.
pub const NOTE_FILE: &str = "quick-note.txt";

/// The entity Quick Notes edits; every instance shares it.
pub const NOTE_NAME: &str = "Quick Note";

/// Where the live rice came from.
#[derive(Debug)]
pub enum RiceSource {
    /// The rice embedded in the binary.
    Default,
    /// A file on disk.
    File(PathBuf),
    /// A file from an older release, migrated on this boot.
    Migrated {
        /// The file.
        path: PathBuf,
        /// The format it had.
        from: u32,
        /// Customized values that were carried over.
        kept: usize,
    },
    /// A file that did not parse; it was set aside and the defaults used.
    Recovered {
        /// Where the unreadable file was moved.
        backup: PathBuf,
        /// The parse error.
        error: String,
    },
}

impl fmt::Display for RiceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("built-in default.suerice"),
            Self::File(path) => write!(f, "{}", path.display()),
            Self::Migrated { path, from, kept } => write!(
                f,
                "{} (migrated from format {from}, {kept} custom values kept)",
                path.display()
            ),
            Self::Recovered { backup, error } => write!(
                f,
                "built-in defaults ({error}; the old file is at {})",
                backup.display()
            ),
        }
    }
}

/// Riceables the shell itself reads, beyond the window manager's (LAW 3).
pub fn shell_declarations() -> Vec<RiceDeclaration> {
    let int = |value, min, max| RiceParameter::IntRange {
        value,
        min: Some(min),
        max: Some(max),
    };
    let text = |value: &str| RiceParameter::Text {
        value: value.to_owned(),
    };
    let toggle = |value| RiceParameter::Bool { value };
    let choice = |value: &str, options: &[&str]| RiceParameter::EnumChoice {
        value: value.to_owned(),
        options: options.iter().map(|&option| option.to_owned()).collect(),
    };
    let declare = RiceDeclaration::new;
    vec![
        declare(
            "general.workspace_dir",
            text(""),
            "Folder the Files panel opens in (empty for your home folder)",
        ),
        declare(
            "general.autosave_seconds",
            int(30, 0, 600),
            "Save Quick Notes this often, in seconds (0 turns autosave off)",
        ),
        declare(
            "general.confirm_on_exit",
            toggle(true),
            "Ask before closing the window",
        ),
        declare(
            "files.show_hidden",
            toggle(false),
            "List files and folders whose names start with a dot",
        ),
        declare(
            "io.jxl_effort",
            int(7, 1, 9),
            "JPEG XL effort for conversions (higher is smaller and slower)",
        ),
        declare(
            "io.jpeg_lossless_transcode",
            toggle(true),
            "Recompress JPEG to JPEG XL bit for bit when a recompression codec is installed",
        ),
        declare(
            "io.byte_map_width",
            int(64, 16, 1024),
            "Columns in the byte map of unknown files",
        ),
        declare(
            "io.max_ingest_mib",
            int(512, 1, 4096),
            "Largest file Sue opens (MiB)",
        ),
        declare(
            "io.convert_dir",
            text(""),
            "Folder for converted files (empty saves next to the source)",
        ),
        declare(
            "audio.output_device",
            text(""),
            "Output device (empty for the system default)",
        ),
        declare(
            "audio.sample_rate",
            choice("48000", &["44100", "48000", "96000"]),
            "Sample rate (Hz)",
        ),
        declare(
            "audio.buffer_frames",
            choice("512", &["128", "256", "512", "1024", "2048"]),
            "Buffer size (frames per callback)",
        ),
        declare(
            "audio.realtime_priority",
            toggle(true),
            "Ask the OS to schedule the audio thread in real time",
        ),
        declare(
            "audio.volume",
            RiceParameter::FloatRange {
                value: 0.8,
                min: Some(0.0),
                max: Some(1.0),
            },
            "Player volume",
        ),
        declare(
            "ai.backend",
            choice("auto", &["auto", "cpu-simd", "vulkan", "cuda", "metal"]),
            "Where AI kernels run",
        ),
        declare(
            "ai.model_dir",
            text(""),
            "Folder with offline models (empty for Sue's data folder)",
        ),
        declare(
            "ai.vram_budget_mib",
            int(2048, 256, 65_536),
            "Most GPU memory AI models may use (MiB)",
        ),
        declare(
            "packages.require_signatures",
            toggle(true),
            "Only accept packages with a valid Ed25519 signature",
        ),
        declare(
            "browser.bookmarks",
            text("https://www.rust-lang.org, https://docs.rs, https://jpeg.org/jpegxl"),
            "Bookmarks, separated by commas",
        ),
    ]
}

/// The backend `ai.backend` names.
fn backend_from_rice(name: &str) -> Option<ComputeBackend> {
    Some(match name {
        "cpu-simd" => ComputeBackend::CpuSimd,
        "vulkan" => ComputeBackend::VulkanWgpu,
        "cuda" => ComputeBackend::Cuda,
        "metal" => ComputeBackend::Metal,
        _ => return None,
    })
}

/// One model file found in the model folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelFile {
    /// File name.
    pub name: String,
    /// Format from the extension.
    pub format: String,
    /// Size in bytes.
    pub size: u64,
}

/// Lists model files (GGUF, ONNX, safetensors) in `dir`.
fn scan_models(dir: &Path) -> Vec<ModelFile> {
    sue_platform::list_dir(dir, false)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !entry.is_dir)
        .filter_map(|entry| {
            let format = ModelFormat::from_extension(&entry.name)?;
            Some(ModelFile {
                format: format!("{format:?}"),
                name: entry.name,
                size: entry.size,
            })
        })
        .collect()
}

/// What the engine was given beyond the platform host.
#[derive(Clone, Debug, Default)]
pub struct BootOptions {
    /// Output devices to offer in Settings (empty in headless runs).
    pub audio_devices: Vec<String>,
    /// The GPU adapter, for the Display page.
    pub gpu_label: Option<String>,
}

/// Every subsystem, booted and connected.
pub struct Engine {
    /// Operating system.
    pub os: OsFamily,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Hardware capabilities.
    pub capabilities: HardwareCapabilities,
    /// Platform directories.
    pub paths: PlatformPaths,
    /// The arena and the live rice.
    pub world: World,
    /// The global undo tree.
    pub history: UndoStack<World>,
    /// Applet requests waiting for the shell.
    pub requests: Vec<ShellRequest>,
    /// Where the rice came from.
    pub rice_source: RiceSource,
    /// The shell.
    pub workspace: Workspace,
    /// Settings pages, live choices and system tables.
    pub catalog: Catalog,
    /// The audio output, once something played.
    pub audio: Option<Box<dyn AudioSink>>,
    /// Why audio is unavailable, if it is.
    pub audio_error: Option<String>,
    options: BootOptions,
}

impl Engine {
    /// Boots against `host` with no audio devices or GPU to report.
    ///
    /// # Errors
    /// A message naming the subsystem that failed.
    pub fn boot(host: &dyn PlatformHost) -> Result<Self, String> {
        Self::boot_with(host, BootOptions::default())
    }

    /// Boots against `host`: loads the live rice file (migrating one from an
    /// older release, setting aside one that does not parse), registers every
    /// riceable, loads the Quick Note, installs the built-in applets and boots
    /// the workspace.
    ///
    /// # Errors
    /// A message naming the subsystem that failed.
    pub fn boot_with(host: &dyn PlatformHost, options: BootOptions) -> Result<Self, String> {
        let paths = host.paths().clone();
        let (mut rice, rice_source) = load_rice(&paths.rice_dir().join(RICE_FILE))?;
        let launchpad = applets::launchpad().map_err(|error| format!("launchpad: {error}"))?;
        sue_wm::register_rice(&mut rice).map_err(|error| format!("rice: {error}"))?;
        rice.register_all(&shell_declarations())
            .map_err(|error| format!("rice: {error}"))?;
        rice.register_all(&launchpad.rice_declarations())
            .map_err(|error| format!("rice: {error}"))?;

        let config =
            WmConfig::from_rice(&rice).map_err(|error| format!("window manager: {error}"))?;
        let profile = HardwareProfile::resolve(&config.hardware_profile, host.capabilities());
        let workspace = Workspace::new(
            launchpad,
            config,
            profile,
            host.capabilities().display_refresh_hz,
            host.form_factor(),
        );
        let mut world = World {
            rice,
            ..World::default()
        };
        let note_path = paths.notes_dir().join(NOTE_FILE);
        let note = match std::fs::read_to_string(&note_path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(format!(
                    "notes: cannot read {}: {error}",
                    note_path.display()
                ));
            }
        };
        let handle = world.arena.spawn(NOTE_NAME, StringBuffer::from(note));
        if let Ok(entity) = world.arena.get_mut(handle) {
            entity.set_meta(meta::SOURCE, note_path.display().to_string());
        }
        let mut engine = Self {
            os: host.os(),
            form_factor: host.form_factor(),
            capabilities: host.capabilities().clone(),
            paths,
            world,
            history: UndoStack::new(),
            requests: Vec::new(),
            rice_source,
            workspace,
            catalog: Catalog::default(),
            audio: None,
            audio_error: None,
            options,
        };
        engine.rebuild_catalog();
        let (workspace, mut ctx) = engine.shell();
        workspace
            .boot(&mut ctx)
            .map_err(|error| format!("window manager: {error}"))?;
        Ok(engine)
    }

    /// The live rice file.
    pub fn rice_path(&self) -> PathBuf {
        self.paths.rice_dir().join(RICE_FILE)
    }

    /// Where the Quick Note is saved.
    pub fn note_path(&self) -> PathBuf {
        self.paths.notes_dir().join(NOTE_FILE)
    }

    /// The folder model files are read from.
    pub fn model_dir(&self) -> PathBuf {
        match self.world.rice.get_str("ai.model_dir") {
            Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir.trim()),
            _ => self.paths.models_dir(),
        }
    }

    /// The workspace plus an applet context over the rest of the engine.
    pub fn shell(&mut self) -> (&mut Workspace, AppletContext<'_>) {
        let audio: Option<&mut dyn AudioSink> = match self.audio.as_mut() {
            Some(sink) => Some(sink.as_mut()),
            None => None,
        };
        let ctx = AppletContext {
            world: &mut self.world,
            history: &mut self.history,
            form_factor: self.form_factor,
            profile: self.workspace.profile(),
            requests: &mut self.requests,
            focused: false,
            catalog: &self.catalog,
            audio,
            inspection: Vec::new(),
        };
        (&mut self.workspace, ctx)
    }

    /// Re-reads the window-manager config and hardware profile from the rice,
    /// after an edit, undo or redo.
    ///
    /// # Errors
    /// If a window-manager parameter is missing.
    pub fn refresh_config(&mut self) -> Result<(), String> {
        let config = WmConfig::from_rice(&self.world.rice).map_err(|error| error.to_string())?;
        let profile = HardwareProfile::resolve(&config.hardware_profile, &self.capabilities);
        self.workspace.apply_config(config);
        if profile != self.workspace.profile() {
            self.workspace.set_profile(profile);
        }
        if let Some(sink) = self.audio.as_mut() {
            let volume = self.world.rice.get_f64("audio.volume").unwrap_or(0.8) as f32;
            sink.set_volume(volume);
        }
        Ok(())
    }

    /// The audio output the rice asks for.
    pub fn audio_config(&self) -> AudioConfig {
        let rice = &self.world.rice;
        let number = |path: &str, fallback: u32| {
            rice.get_str(path)
                .and_then(|value| value.parse().ok())
                .unwrap_or(fallback)
        };
        AudioConfig {
            device: rice
                .get_str("audio.output_device")
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            sample_rate: number("audio.sample_rate", 48_000),
            buffer_frames: number("audio.buffer_frames", 512),
            realtime: rice.get_bool("audio.realtime_priority").unwrap_or(true),
            queue_ms: 500,
        }
    }

    /// Rebuilds the Settings pages, live choices and tables from the current
    /// state (after a rescan, an audio change or an edit that affects them).
    pub fn rebuild_catalog(&mut self) {
        self.catalog = self.build_catalog();
    }

    fn build_catalog(&self) -> Catalog {
        let rice = &self.world.rice;
        let param = |path: &str| SettingsItem::Param(path.to_owned());
        let section = |name: &str| SettingsItem::Section(name.to_owned());
        let fact = |id: &str| SettingsItem::Fact(id.to_owned());
        let table = |id: &str| SettingsItem::Table(id.to_owned());
        let group = SettingsGroup::new;
        let launchpad = self.workspace.launchpad();

        let mut catalog = Catalog {
            presets: builtin_presets(),
            ..Catalog::default()
        };
        catalog.pages = vec![
            SettingsPage::new(
                "general",
                "General",
                vec![
                    group("Startup", vec![param("launchpad.boot_into")]),
                    group(
                        "Workspace",
                        vec![param("general.workspace_dir"), param("files.show_hidden")],
                    ),
                    group(
                        "Saving",
                        vec![
                            param("general.autosave_seconds"),
                            param("general.confirm_on_exit"),
                            fact("notes.path"),
                        ],
                    ),
                    group("Quick Notes", vec![section("quick_notes")]),
                    group("Browser", vec![param("browser.bookmarks")]),
                ],
            ),
            SettingsPage::new(
                "appearance",
                "Appearance & Rice",
                vec![
                    group("Theme", vec![SettingsItem::Presets]),
                    group("Colors", vec![section("colors")]),
                    group(
                        "Geometry",
                        [
                            "layout.gap",
                            "layout.padding",
                            "layout.border_width",
                            "layout.panel_radius",
                            "layout.control_radius",
                            "layout.header_height",
                            "layout.status_bar_height",
                            "layout.rail_width",
                            "layout.left_dock_width",
                            "layout.right_dock_width",
                            "layout.timeline_height",
                            "layout.tab_accent",
                            "layout.slider_track",
                            "layout.settings_sidebar_width",
                            "launchpad.card_width",
                            "layout.min_center_width",
                            "layout.drawer_max_fraction",
                            "layout.sheet_max_fraction",
                        ]
                        .into_iter()
                        .map(param)
                        .collect(),
                    ),
                    group("Typography", vec![section("typography")]),
                    group(
                        "Rice File",
                        vec![
                            fact("rice.path"),
                            SettingsItem::Actions(vec![
                                CommandButton::new("Reload from Disk", "rice.reload"),
                                CommandButton::new("Export...", "rice.export"),
                                CommandButton::new("Open Folder", "rice.reveal"),
                            ]),
                        ],
                    ),
                ],
            ),
            SettingsPage::new(
                "display",
                "Display & Graphics",
                vec![
                    group(
                        "Hardware",
                        vec![
                            param("profile.hardware"),
                            fact("display.profile"),
                            param("display.gpu_preference"),
                            fact("display.gpu"),
                        ],
                    ),
                    group(
                        "Frame Pacing",
                        vec![
                            param("display.frame_rate_cap"),
                            param("motion.settle_grace_ms"),
                        ],
                    ),
                    group(
                        "Scale",
                        vec![param("display.ui_scale"), param("input.wheel_line_px")],
                    ),
                    group(
                        "Motion",
                        [
                            "motion.instant_transitions",
                            "motion.animation_speed",
                            "motion.spring_stiffness",
                            "motion.spring_damping",
                            "motion.caret_blink_ms",
                            "motion.message_seconds",
                        ]
                        .into_iter()
                        .map(param)
                        .collect(),
                    ),
                ],
            ),
            SettingsPage::new(
                "audio",
                "Audio & DSP",
                vec![
                    group(
                        "Output",
                        vec![
                            param("audio.output_device"),
                            param("audio.sample_rate"),
                            param("audio.buffer_frames"),
                            param("audio.realtime_priority"),
                            param("audio.volume"),
                        ],
                    ),
                    group("Status", vec![fact("audio.stream"), fact("audio.realtime")]),
                ],
            ),
            SettingsPage::new(
                "files",
                "Files & Codecs",
                vec![
                    group(
                        "JPEG XL",
                        vec![param("io.jxl_effort"), param("io.jpeg_lossless_transcode")],
                    ),
                    group("Conversion", vec![param("io.convert_dir")]),
                    group(
                        "Opening Files",
                        vec![param("io.max_ingest_mib"), param("io.byte_map_width")],
                    ),
                    group("Built-in Codecs", vec![table("codecs")]),
                ],
            ),
            SettingsPage::new(
                "ai",
                "Local AI",
                vec![
                    group(
                        "Compute",
                        vec![
                            param("ai.backend"),
                            param("ai.vram_budget_mib"),
                            fact("ai.selection"),
                        ],
                    ),
                    group(
                        "Models",
                        vec![
                            param("ai.model_dir"),
                            table("models"),
                            SettingsItem::Actions(vec![
                                CommandButton::new("Rescan", "ai.rescan"),
                                CommandButton::new("Open Folder", "ai.reveal"),
                            ]),
                        ],
                    ),
                ],
            ),
            SettingsPage::new(
                "packages",
                "Packages & Updates",
                vec![
                    group(
                        "Trust",
                        vec![
                            param("packages.require_signatures"),
                            fact("packages.policy"),
                        ],
                    ),
                    group("Installed Applets", vec![table("applets")]),
                    group("Packages", vec![table("packages")]),
                    group(
                        "Updates",
                        vec![fact("packages.generation"), fact("packages.health")],
                    ),
                ],
            ),
            SettingsPage::new(
                "keymaps",
                "Keymaps",
                vec![group("Shortcuts", vec![section("keys")])],
            ),
        ];

        // Live choices.
        let mut boot = vec![Choice::new("hub", "Home")];
        boot.extend(
            launchpad
                .manifests()
                .filter(|manifest| {
                    manifest.home_zone == Zone::CenterStage
                        && manifest.category != AppletCategory::System
                })
                .map(|manifest| Choice::new(manifest.id.as_str(), manifest.name.clone())),
        );
        catalog
            .choices
            .insert("launchpad.boot_into".to_owned(), boot);
        let mut devices = vec![Choice::new("", "System default")];
        devices.extend(
            self.options
                .audio_devices
                .iter()
                .map(|name| Choice::new(name.clone(), name.clone())),
        );
        catalog
            .choices
            .insert("audio.output_device".to_owned(), devices);
        let available = ComputeBackend::detect_available(self.os, &self.capabilities);
        let mut backends = vec![Choice::new("auto", "Auto")];
        backends.extend(
            [
                ("cpu-simd", "CPU SIMD", ComputeBackend::CpuSimd),
                ("vulkan", "Vulkan", ComputeBackend::VulkanWgpu),
                ("cuda", "CUDA", ComputeBackend::Cuda),
                ("metal", "Metal", ComputeBackend::Metal),
            ]
            .into_iter()
            .map(|(value, label, backend)| {
                Choice::new(value, label).enabled(available.contains(&backend))
            }),
        );
        catalog.choices.insert("ai.backend".to_owned(), backends);
        for (name, folder) in [
            ("home", self.paths.home_dir.clone()),
            ("models", self.model_dir()),
            ("rice", self.paths.rice_dir()),
            ("notes", self.paths.notes_dir()),
        ] {
            catalog.folders.insert(name.to_owned(), folder);
        }

        // Tables.
        catalog.tables.insert(
            "codecs".to_owned(),
            InfoTable {
                columns: vec!["Format".into(), "Abilities".into(), "Library".into()],
                rows: sue_io::builtin_codecs()
                    .into_iter()
                    .map(|codec| {
                        vec![
                            codec.format.to_owned(),
                            codec.abilities.to_owned(),
                            codec.library.to_owned(),
                        ]
                    })
                    .collect(),
                empty: String::new(),
            },
        );
        let model_dir = self.model_dir();
        catalog.tables.insert(
            "models".to_owned(),
            InfoTable {
                columns: vec!["Model".into(), "Format".into(), "Size".into()],
                rows: scan_models(&model_dir)
                    .into_iter()
                    .map(|model| vec![model.name, model.format, human_bytes(model.size)])
                    .collect(),
                empty: format!(
                    "No model files in {}. Put .gguf, .onnx or .safetensors files there and rescan.",
                    model_dir.display()
                ),
            },
        );
        catalog.tables.insert(
            "applets".to_owned(),
            InfoTable {
                columns: vec!["Applet".into(), "Kind".into(), "Lives in".into()],
                rows: launchpad
                    .manifests()
                    .map(|manifest| {
                        let place = match manifest.home_zone {
                            Zone::LeftDock => "Left dock",
                            Zone::RightDock => "Right dock",
                            Zone::BottomTimeline => "Timeline",
                            Zone::CenterStage => "Center Stage",
                        };
                        vec![
                            manifest.name.clone(),
                            format!("{:?}", manifest.category),
                            place.to_owned(),
                        ]
                    })
                    .collect(),
                empty: String::new(),
            },
        );
        catalog.tables.insert(
            "packages".to_owned(),
            InfoTable {
                columns: vec!["Package".into(), "Version".into(), "Signature".into()],
                rows: Vec::new(),
                empty: "No packages are installed yet. Everything above is built in.".to_owned(),
            },
        );

        // Facts.
        let mut facts = |id: &str, label: &str, value: String| {
            catalog
                .facts
                .insert(id.to_owned(), (label.to_owned(), value));
        };
        facts("rice.path", "File", self.rice_source.to_string());
        facts(
            "notes.path",
            "Quick Note file",
            self.note_path().display().to_string(),
        );
        facts(
            "display.profile",
            "Active profile",
            format!(
                "{} ({} Hz display, {} FPS animation target)",
                sue_wm::profile_label(self.workspace.profile()),
                self.capabilities.display_refresh_hz,
                self.workspace.target_hz()
            ),
        );
        facts(
            "display.gpu",
            "Graphics",
            self.options.gpu_label.clone().unwrap_or_else(|| {
                format!(
                    "{:?} through {:?}",
                    self.capabilities.gpu_vendor, self.capabilities.graphics_api
                )
            }),
        );
        let (stream, realtime) = match (&self.audio, &self.audio_error) {
            (Some(sink), _) => {
                let info = sink.info();
                (
                    format!(
                        "{}, {} Hz, {} channels{}",
                        info.device,
                        info.sample_rate,
                        info.channels,
                        info.buffer_frames
                            .map_or_else(String::new, |frames| format!(", {frames} frames"))
                    ),
                    match sink.realtime() {
                        RealtimeStatus::Off => "Off".to_owned(),
                        RealtimeStatus::Pending => {
                            "Requested; granted when playback starts".to_owned()
                        }
                        RealtimeStatus::Granted => "Granted by the OS".to_owned(),
                        RealtimeStatus::Refused => {
                            "Refused by the OS; the default priority applies".to_owned()
                        }
                    },
                )
            }
            (None, Some(error)) => (format!("Unavailable: {error}"), "Not running".to_owned()),
            (None, None) => (
                "Closed. It opens when the Player starts.".to_owned(),
                "Not running".to_owned(),
            ),
        };
        facts("audio.stream", "Stream", stream);
        facts("audio.realtime", "Real-time priority", realtime);
        let preferred = rice.get_str("ai.backend").and_then(backend_from_rice);
        let usable: Vec<ComputeBackend> = match preferred {
            Some(backend) if available.contains(&backend) => vec![backend],
            _ => available.clone(),
        };
        let budget = rice
            .get_i64("ai.vram_budget_mib")
            .and_then(|mib| u64::try_from(mib).ok())
            .unwrap_or(2048)
            .saturating_mul(1024 * 1024);
        let mut registry = LocalModelRegistry::new();
        let _ = registry.register(MidSideStemSplit::descriptor());
        let selection = registry
            .select(AiTask::StemSeparation, &usable, budget)
            .map_or_else(
                || "No installed model fits the chosen backend and budget".to_owned(),
                |selected| format!("{} on {}", selected.model.id, selected.backend.name()),
            );
        facts("ai.selection", "Stem separation runs", selection);
        facts(
            "packages.policy",
            "Policy",
            if rice.get_bool("packages.require_signatures").unwrap_or(true) {
                "Unsigned or tampered packages are refused".to_owned()
            } else {
                "Unsigned packages are accepted and marked unverified".to_owned()
            },
        );
        let generation = sue_pkg::read_active_pointer(&self.paths.packages_dir())
            .ok()
            .flatten();
        facts(
            "packages.generation",
            "Active generation",
            generation.map_or_else(
                || "0, the factory install".to_owned(),
                |generation| generation.to_string(),
            ),
        );
        facts(
            "packages.health",
            "Rollback",
            "Healthy: no update has run, so there is nothing to roll back".to_owned(),
        );
        catalog
    }
}

/// Reads the live rice file: missing means the defaults, an older format is
/// migrated in place (keeping a backup), and an unreadable file is set aside.
fn load_rice(path: &Path) -> Result<(RiceSchema, RiceSource), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let schema =
                RiceSchema::parse(DEFAULT_RICE).map_err(|error| format!("rice: {error}"))?;
            return Ok((schema, RiceSource::Default));
        }
        Err(error) => return Err(format!("rice: cannot read {}: {error}", path.display())),
    };
    match migrate(&text) {
        Ok(None) => match RiceSchema::parse(&text) {
            Ok(schema) => Ok((schema, RiceSource::File(path.to_path_buf()))),
            Err(error) => recover(path, &text, &error.to_string()),
        },
        Ok(Some(migration)) => {
            let backup = path.with_extension(format!("suerice.v{}.bak", migration.from));
            write_atomic(&backup, text.as_bytes())
                .and_then(|()| write_atomic(path, migration.text.as_bytes()))
                .map_err(|error| format!("rice: cannot migrate {}: {error}", path.display()))?;
            let schema =
                RiceSchema::parse(&migration.text).map_err(|error| format!("rice: {error}"))?;
            Ok((
                schema,
                RiceSource::Migrated {
                    path: path.to_path_buf(),
                    from: migration.from,
                    kept: migration.kept.len(),
                },
            ))
        }
        Err(error) => recover(path, &text, &error.to_string()),
    }
}

fn recover(path: &Path, text: &str, error: &str) -> Result<(RiceSchema, RiceSource), String> {
    let backup = path.with_extension("suerice.broken");
    write_atomic(&backup, text.as_bytes())
        .and_then(|()| std::fs::remove_file(path))
        .map_err(|io| format!("rice: {error}, and the file cannot be set aside: {io}"))?;
    let schema = RiceSchema::parse(DEFAULT_RICE).map_err(|error| format!("rice: {error}"))?;
    Ok((
        schema,
        RiceSource::Recovered {
            backup,
            error: error.to_owned(),
        },
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sue_platform::HeadlessHost;

    fn every_declaration() -> Vec<RiceDeclaration> {
        let mut declarations = sue_wm::rice_declarations();
        declarations.extend(shell_declarations());
        declarations.extend(applets::launchpad().unwrap().rice_declarations());
        declarations
    }

    #[test]
    fn shipped_rice_and_declarations_match_key_for_key() {
        let shipped = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let declarations = every_declaration();
        for declaration in &declarations {
            assert_eq!(
                shipped.parameter(&declaration.path),
                Some(&declaration.default),
                "{} differs from its declaration",
                declaration.path
            );
            assert_eq!(
                shipped
                    .entry(&declaration.path)
                    .and_then(|entry| entry.description()),
                Some(declaration.description.as_str()),
                "{} has a different description",
                declaration.path
            );
        }
        for entry in shipped.entries() {
            assert!(
                declarations
                    .iter()
                    .any(|declaration| declaration.path == entry.path()),
                "{} is in default.suerice but nothing declares it",
                entry.path()
            );
        }
    }

    pub(crate) fn temp_host(tag: &str) -> (HeadlessHost, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let root = std::env::temp_dir().join(format!("sue-{tag}-{}-{unique}", std::process::id()));
        let paths = PlatformPaths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            home_dir: root.join("home"),
        };
        let host = HeadlessHost::new(
            OsFamily::current(),
            FormFactor::Desktop,
            HardwareCapabilities::probe_portable(),
            paths,
        );
        (host, root)
    }

    #[test]
    fn old_rice_files_are_migrated_on_boot_with_a_backup() {
        let (host, root) = temp_host("migrate");
        let path = host.paths().rice_dir().join(RICE_FILE);
        let old = include_str!("../../../crates/sue-rice/rice/history/v1.suerice");
        write_atomic(&path, old.as_bytes()).unwrap();
        let engine = Engine::boot(&host).unwrap();
        assert!(matches!(
            engine.rice_source,
            RiceSource::Migrated { from: 1, .. }
        ));
        let migrated = std::fs::read_to_string(&path).unwrap();
        assert!(!migrated.contains("#c9a227") && migrated.contains("#3b82f6"));
        assert_eq!(engine.world.rice.get_i64("layout.gap"), Some(0));
        let backup = path.with_extension("suerice.v1.bak");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), old);
        // A second boot finds the file current.
        assert!(matches!(
            Engine::boot(&host).unwrap().rice_source,
            RiceSource::File(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unreadable_rice_is_set_aside_not_fatal() {
        let (host, root) = temp_host("broken");
        let path = host.paths().rice_dir().join(RICE_FILE);
        write_atomic(&path, b"# sue-rice format 2\n[broken\n").unwrap();
        let engine = Engine::boot(&host).unwrap();
        assert!(matches!(engine.rice_source, RiceSource::Recovered { .. }));
        assert!(path.with_extension("suerice.broken").exists());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_catalog_reaches_every_rice_section() {
        let (host, root) = temp_host("catalog");
        let engine = Engine::boot(&host).unwrap();
        assert_eq!(engine.catalog.pages.len(), 8);
        let titles: Vec<&str> = engine
            .catalog
            .pages
            .iter()
            .map(|page| page.title.as_str())
            .collect();
        assert_eq!(
            titles,
            [
                "General",
                "Appearance & Rice",
                "Display & Graphics",
                "Audio & DSP",
                "Files & Codecs",
                "Local AI",
                "Packages & Updates",
                "Keymaps"
            ]
        );
        let claimed = |path: &str| {
            engine
                .catalog
                .pages
                .iter()
                .flat_map(|page| page.claims())
                .any(|item| match item {
                    SettingsItem::Param(param) => param == path,
                    SettingsItem::Section(section) => path.starts_with(&format!("{section}.")),
                    _ => false,
                })
        };
        for entry in engine.world.rice.entries() {
            assert!(
                claimed(entry.path()),
                "{} is on no Settings page",
                entry.path()
            );
        }
        assert!(
            engine.catalog.choices["ai.backend"]
                .iter()
                .any(|choice| choice.value == "cpu-simd" && choice.enabled)
        );
        assert!(engine.catalog.facts["ai.selection"].1.contains("mid-side"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
