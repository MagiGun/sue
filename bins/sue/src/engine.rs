//! Boot: wires a platform host, the live rice (migrated when it is old or
//! tinted), the undoable world, the installed applets and the catalog
//! Settings reads.

/// Text from the language database's `settings` namespace (the Settings pages).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use sue_core::UndoStack;
use sue_platform::{
    AudioConfig, AudioSink, FormFactor, HardwareCapabilities, OsFamily, PlatformHost,
    PlatformPaths, RealtimeStatus, write_atomic,
};
use sue_rice::{
    DEFAULT_RICE, HardwareProfile, RiceDeclaration, RiceParameter, RiceSchema, builtin_presets,
    migrate,
};
use sue_wm::{
    AppletRole, Catalog, Choice, CommandButton, InfoTable, Launchpad, SettingsGroup, SettingsItem,
    SettingsPage, ShellRequest, WmConfig, World,
};

use crate::applets;

/// The live rice file in the platform rice directory. It starts as a byte-for-
/// byte copy of the built-in `default.suerice` and is written back
/// format-preservingly after every edit.
pub const RICE_FILE: &str = "default.suerice";

/// Where the live rice came from.
#[derive(Debug)]
pub enum RiceSource {
    /// The rice embedded in the binary.
    Default,
    /// A file on disk.
    File(PathBuf),
    /// A file from an older release or with tinted colors, migrated on this
    /// boot (the old file is kept next to it).
    Migrated {
        /// The file.
        path: PathBuf,
        /// The format it had.
        from: u32,
        /// Customized values that were carried over.
        kept: usize,
        /// Tinted colors reset to the monochrome defaults.
        purged: usize,
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
            Self::Migrated {
                path,
                from,
                kept,
                purged,
            } => write!(
                f,
                "{} (migrated from format {from}, {kept} custom values kept, {purged} tinted colors reset)",
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

/// Riceables the shell itself reads, beyond the window manager's.
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
            "general.confirm_on_exit",
            toggle(true),
            "Ask before closing the window",
        ),
        declare(
            "general.autosave_seconds",
            int(60, 0, 3600),
            "Keep unsaved work this often, so the Hub can bring it back (s; 0 turns autosave off)",
        )
        .labeled("Autosave Every"),
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
            "io.jpeg_quality",
            int(90, 10, 100),
            "JPEG quality for conversions to .jpg",
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
            "Volume of everything Sue plays",
        ),
        declare(
            "packages.require_signatures",
            toggle(true),
            "Only accept packages with a valid Ed25519 signature",
        ),
    ]
}

/// What the engine was given beyond the platform host.
#[derive(Clone, Debug, Default)]
pub struct BootOptions {
    /// Output devices to offer in Settings (empty in headless runs).
    pub audio_devices: Vec<String>,
    /// The GPU adapter, for the Display page.
    pub gpu_label: Option<String>,
    /// The kind of GPU drawing (`integrated`, `discrete`).
    pub gpu_kind: Option<String>,
    /// Every GPU the machine offers.
    pub gpus: Vec<String>,
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
    /// Requests waiting for the runtime.
    pub requests: Vec<ShellRequest>,
    /// Where the rice came from.
    pub rice_source: RiceSource,
    /// The installed applets.
    pub launchpad: Launchpad,
    /// The window-manager config.
    pub config: WmConfig,
    /// The active hardware profile.
    pub profile: HardwareProfile,
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
    /// older release or with tinted colors, setting aside one that does not
    /// parse), registers every riceable and installs the built-in applets.
    /// The project starts empty.
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

        let world = World {
            rice,
            ..World::default()
        };
        let mut engine = Self {
            os: host.os(),
            form_factor: host.form_factor(),
            capabilities: host.capabilities().clone(),
            paths,
            world,
            history: UndoStack::new(),
            requests: Vec::new(),
            rice_source,
            launchpad,
            config,
            profile,
            catalog: Catalog::default(),
            audio: None,
            audio_error: None,
            options,
        };
        engine.rebuild_catalog();
        Ok(engine)
    }

    /// The live rice file.
    pub fn rice_path(&self) -> PathBuf {
        self.paths.rice_dir().join(RICE_FILE)
    }

    /// Re-reads the window-manager config and hardware profile from the rice,
    /// after an edit, undo or redo.
    ///
    /// # Errors
    /// If a window-manager parameter is missing.
    pub fn refresh_config(&mut self) -> Result<(), String> {
        self.config = WmConfig::from_rice(&self.world.rice).map_err(|error| error.to_string())?;
        self.profile = HardwareProfile::resolve(&self.config.hardware_profile, &self.capabilities);
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

    #[allow(clippy::too_many_lines, reason = "the Settings pages, as data")]
    fn build_catalog(&self) -> Catalog {
        let rice = &self.world.rice;
        let _language = sue_lang::scope(&sue_wm::sue_language(rice));
        let param = |path: &str| SettingsItem::Param(path.to_owned());
        let section = |name: &str| SettingsItem::Section(name.to_owned());
        let fact = |id: &str| SettingsItem::Fact(id.to_owned());
        let table = |id: &str| SettingsItem::Table(id.to_owned());
        let group = SettingsGroup::new;
        let params = |paths: &[&str]| paths.iter().map(|path| param(path)).collect::<Vec<_>>();

        let mut catalog = Catalog {
            presets: builtin_presets(),
            ..Catalog::default()
        };
        catalog.pages = vec![
            SettingsPage::new(
                "general",
                tr!("general"),
                vec![
                    group(
                        tr!("startup"),
                        vec![
                            SettingsItem::Note(
                                tr!("sue-opens-hub-straight-into-studio").to_owned(),
                            ),
                            param("launchpad.pinned"),
                            param("launchpad.recent_count"),
                        ],
                    ),
                    group(
                        tr!("unsaved-work"),
                        vec![
                            SettingsItem::Note(tr!("sue-keeps-work-you-have-not").to_owned()),
                            param("general.autosave_seconds"),
                        ],
                    ),
                    group(tr!("quitting"), vec![param("general.confirm_on_exit")]),
                ],
            ),
            SettingsPage::new("language", tr!("language"), {
                let mut own: Vec<SettingsItem> = vec![SettingsItem::Note(
                    tr!("each-studio-tool-follows-sues-language").to_owned(),
                )];
                own.extend(
                    self.launchpad
                        .manifests()
                        .filter(|manifest| manifest.role.is_page())
                        .map(|manifest| param(&sue_wm::language_key(&manifest.id))),
                );
                vec![
                    group(
                        tr!("sue"),
                        vec![
                            SettingsItem::Note(
                                tr!("language-sue-every-studio-tool-follows").to_owned(),
                            ),
                            param(sue_wm::LANGUAGE_KEY),
                        ],
                    ),
                    group(tr!("studios-and-tools"), own),
                ]
            }),
            SettingsPage::new(
                "appearance",
                tr!("appearance-and-rice"),
                vec![
                    group(tr!("theme"), vec![SettingsItem::Presets]),
                    group(tr!("colors"), vec![section("colors")]),
                    group(tr!("geometry"), vec![section("layout")]),
                    group(tr!("typography"), vec![section("typography")]),
                    group(
                        tr!("rice-file"),
                        vec![
                            fact("rice.path"),
                            SettingsItem::Actions(vec![
                                CommandButton::new(tr!("reload-from-disk"), "rice.reload"),
                                CommandButton::new(tr!("export-ellipsis"), "rice.export"),
                                CommandButton::new(tr!("open-folder"), "rice.reveal"),
                            ]),
                        ],
                    ),
                ],
            ),
            SettingsPage::new(
                "display",
                tr!("display-and-graphics"),
                vec![
                    group(
                        tr!("performance"),
                        vec![param("profile.hardware"), fact("display.profile")],
                    ),
                    group(
                        tr!("graphics-card"),
                        vec![
                            param("display.gpu_preference"),
                            fact("display.gpu"),
                            fact("display.gpus"),
                            SettingsItem::Actions(vec![CommandButton::new(
                                tr!("restart-sue"),
                                "app.restart",
                            )]),
                        ],
                    ),
                    group(
                        tr!("frame-pacing"),
                        params(&["display.frame_rate_cap", "motion.settle_grace_ms"]),
                    ),
                    group(
                        tr!("interface-and-scale"),
                        params(&[
                            "display.interface",
                            "display.ui_scale",
                            "input.wheel_line_px",
                        ]),
                    ),
                    group(tr!("motion"), vec![section("motion")]),
                ],
            ),
            SettingsPage::new(
                "audio",
                tr!("audio-and-dsp"),
                vec![
                    group(
                        tr!("output"),
                        params(&[
                            "audio.output_device",
                            "audio.sample_rate",
                            "audio.buffer_frames",
                            "audio.realtime_priority",
                            "audio.volume",
                        ]),
                    ),
                    group(
                        tr!("status"),
                        vec![fact("audio.stream"), fact("audio.realtime")],
                    ),
                ],
            ),
            SettingsPage::new(
                "files",
                tr!("files-and-codecs"),
                vec![
                    group(
                        tr!("jpeg-xl"),
                        params(&["io.jxl_effort", "io.jpeg_lossless_transcode"]),
                    ),
                    group(
                        tr!("conversion"),
                        params(&["io.convert_dir", "io.jpeg_quality"]),
                    ),
                    group(
                        tr!("opening-files"),
                        params(&["io.max_ingest_mib", "io.byte_map_width"]),
                    ),
                    group(tr!("built-in-codecs"), vec![table("codecs")]),
                ],
            ),
            SettingsPage::new(
                "studios",
                tr!("studios"),
                vec![
                    group(
                        tr!("sue-paint"),
                        vec![
                            SettingsItem::Note(tr!("each-studio-can-wear-own-theme").to_owned()),
                            section("sue_paint"),
                        ],
                    ),
                    group(
                        tr!("sue-paint-custom-colors"),
                        vec![section("sue_paint_colors")],
                    ),
                    group(tr!("image-viewer"), vec![section("image_viewer")]),
                    group(tr!("converter"), vec![section("convert")]),
                    group(
                        tr!("sue-writer"),
                        vec![
                            SettingsItem::Note(
                                tr!("sue-writer-follows-sues-theme-unless").to_owned(),
                            ),
                            section("sue_writer"),
                        ],
                    ),
                ],
            ),
            SettingsPage::new(
                "packages",
                tr!("packages-and-updates"),
                vec![
                    group(
                        tr!("trust"),
                        vec![
                            param("packages.require_signatures"),
                            fact("packages.policy"),
                        ],
                    ),
                    group(tr!("installed-applets"), vec![table("applets")]),
                    group(tr!("packages"), vec![table("packages")]),
                    group(
                        tr!("updates"),
                        vec![fact("packages.generation"), fact("packages.health")],
                    ),
                ],
            ),
            SettingsPage::new(
                "keymaps",
                tr!("keymaps"),
                vec![
                    group(
                        tr!("shortcuts"),
                        vec![
                            SettingsItem::Note(tr!("every-action-can-have-as-many").to_owned()),
                            section("keys"),
                        ],
                    ),
                    group(tr!("sue-paint"), vec![section("sue_paint_keys")]),
                    group(tr!("sue-writer"), vec![section("sue_writer_keys")]),
                    group(tr!("image-viewer"), vec![section("image_viewer_keys")]),
                ],
            ),
            SettingsPage::new(
                "mcp",
                "MCP",
                vec![group(
                    tr!("model-context-protocol"),
                    vec![SettingsItem::Note(tr!("not-yet-available"))],
                )],
            ),
        ];

        // Live choices: the Hub, or any studio or tool to open straight into.
        let mut pins = vec![Choice::new("", tr!("the-hub"))];
        pins.extend(
            self.launchpad
                .manifests()
                .filter(|manifest| manifest.role.is_page())
                .map(|manifest| Choice::new(manifest.id.as_str(), manifest.title())),
        );
        catalog.choices.insert(sue_wm::PIN_KEY.to_owned(), pins);
        // Languages: Sue's, and each studio's and tool's own.
        catalog.choices.insert(
            sue_wm::LANGUAGE_KEY.to_owned(),
            sue_wm::sue_language_choices(),
        );
        let sue = sue_wm::sue_language(rice);
        for manifest in self.launchpad.manifests() {
            if manifest.role.is_page() {
                catalog.choices.insert(
                    sue_wm::language_key(&manifest.id),
                    sue_wm::applet_language_choices(&sue),
                );
            }
        }
        let mut devices = vec![Choice::new("", tr!("system-default"))];
        devices.extend(
            self.options
                .audio_devices
                .iter()
                .map(|name| Choice::new(name.clone(), name.clone())),
        );
        catalog
            .choices
            .insert("audio.output_device".to_owned(), devices);
        for (name, folder) in [
            ("home", self.paths.home_dir.clone()),
            ("rice", self.paths.rice_dir()),
            ("paint", self.paths.data_dir.join("paint")),
        ] {
            catalog.folders.insert(name.to_owned(), folder);
        }

        // Tables.
        catalog.tables.insert(
            "codecs".to_owned(),
            InfoTable {
                columns: vec![tr!("format"), tr!("abilities"), tr!("library")],
                rows: sue_io::builtin_codecs()
                    .into_iter()
                    .map(|codec| {
                        vec![
                            sue_lang::translate("io", codec.format),
                            sue_lang::translate("io", codec.abilities),
                            sue_lang::translate("io", codec.library),
                        ]
                    })
                    .collect(),
                empty: String::new(),
            },
        );
        catalog.tables.insert(
            "applets".to_owned(),
            InfoTable {
                columns: vec![tr!("applet"), tr!("lives-in"), tr!("what-it-does")],
                rows: self
                    .launchpad
                    .manifests()
                    .map(|manifest| {
                        let place = match manifest.role {
                            AppletRole::Studio => tr!("hub-studios"),
                            AppletRole::Tool => tr!("hub-tools"),
                            AppletRole::Window => tr!("window"),
                        };
                        vec![manifest.title(), place.to_owned(), manifest.blurb()]
                    })
                    .collect(),
                empty: String::new(),
            },
        );
        catalog.tables.insert(
            "packages".to_owned(),
            InfoTable {
                columns: vec![tr!("package"), tr!("version"), tr!("signature")],
                rows: Vec::new(),
                empty: tr!("no-packages-are-installed-yet-everything"),
            },
        );

        // Facts.
        let mut facts = |id: &str, label: &str, value: String| {
            catalog
                .facts
                .insert(id.to_owned(), (label.to_owned(), value));
        };
        facts("rice.path", &tr!("file"), self.rice_source.to_string());
        let target = {
            let mut scheduler =
                sue_wm::FrameScheduler::new(self.profile, self.capabilities.display_refresh_hz);
            scheduler.set_frame_cap(self.config.frame_rate_cap);
            scheduler.target_hz()
        };
        facts(
            "display.profile",
            &tr!("active-profile"),
            tr!(
                "hz-display-fps-animation-target",
                value = sue_wm::profile_label(self.profile),
                display_refresh_hz = self.capabilities.display_refresh_hz,
                target = target
            ),
        );
        let running = self.options.gpu_label.clone().unwrap_or_else(|| {
            format!(
                "{:?} through {:?}",
                self.capabilities.gpu_vendor, self.capabilities.graphics_api
            )
        });
        let wanted = rice.get_str("display.gpu_preference").unwrap_or("auto");
        let offered = |kind: &str| {
            self.options
                .gpus
                .iter()
                .any(|gpu| gpu.ends_with(&tr!("gpu", kind = kind)))
        };
        let drawing = match (wanted, self.options.gpu_kind.as_deref()) {
            ("auto", _) | (_, None) => running,
            (wanted, Some(kind)) if wanted == kind => running,
            (wanted, Some(_)) if !offered(wanted) => {
                tr!(
                    "this-machine-has-no-gpu",
                    running = running,
                    wanted = wanted
                )
            }
            (wanted, Some(_)) => {
                tr!("restart-sue-draw-gpu", running = running, wanted = wanted)
            }
        };
        facts("display.gpu", &tr!("drawing-on"), drawing);
        facts(
            "display.gpus",
            &tr!("gpus-here"),
            if self.options.gpus.is_empty() {
                tr!("not-probed-in-this-run")
            } else {
                self.options.gpus.join(", ")
            },
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
                            .map_or_else(String::new, |frames| tr!("frames", frames = frames))
                    ),
                    match sink.realtime() {
                        RealtimeStatus::Off => tr!("off"),
                        RealtimeStatus::Pending => {
                            tr!("requested-granted-when-playback-starts")
                        }
                        RealtimeStatus::Granted => tr!("granted-by-the-os"),
                        RealtimeStatus::Refused => {
                            tr!("refused-os-default-priority-applies")
                        }
                    },
                )
            }
            (None, Some(error)) => (tr!("unavailable", error = error), tr!("not-running")),
            (None, None) => (tr!("closed-opens-when-something-plays"), tr!("not-running")),
        };
        facts("audio.stream", &tr!("stream"), stream);
        facts("audio.realtime", &tr!("real-time-priority"), realtime);
        facts(
            "packages.policy",
            &tr!("policy"),
            if rice.get_bool("packages.require_signatures").unwrap_or(true) {
                tr!("unsigned-tampered-packages-are-refused")
            } else {
                tr!("unsigned-packages-are-accepted-marked-unverified")
            },
        );
        let generation = sue_pkg::read_active_pointer(&self.paths.packages_dir())
            .ok()
            .flatten();
        facts(
            "packages.generation",
            &tr!("active-generation"),
            generation.map_or_else(
                || tr!("n-0-the-factory-install"),
                |generation| generation.to_string(),
            ),
        );
        facts(
            "packages.health",
            &tr!("rollback"),
            tr!("healthy-no-update-has-run-so"),
        );
        catalog
    }
}

/// Reads the live rice file: missing means the defaults, an older or tinted
/// file is migrated in place (keeping a backup), and an unreadable file is
/// set aside.
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
                    purged: migration.purged.len(),
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
    fn boot_starts_an_empty_project_with_the_studios_and_tools_installed() {
        let (host, root) = temp_host("boot");
        let engine = Engine::boot(&host).unwrap();
        assert!(engine.world.arena.is_empty(), "nothing is made at boot");
        let pages = |role: AppletRole| -> Vec<String> {
            engine
                .launchpad
                .manifests()
                .filter(|manifest| manifest.role == role)
                .map(|manifest| manifest.name.clone())
                .collect()
        };
        assert_eq!(pages(AppletRole::Studio), ["Sue Paint", "Sue Writer"]);
        assert_eq!(pages(AppletRole::Tool), ["Image Viewer", "Converter"]);
        assert_eq!(pages(AppletRole::Window), ["Settings"]);
        // Settings offers every studio and tool to pin, and the Hub.
        let pins: Vec<&str> = engine.catalog.choices[sue_wm::PIN_KEY]
            .iter()
            .map(|choice| choice.value.as_str())
            .collect();
        assert_eq!(
            pins,
            ["", "sue-paint", "sue-writer", "image-viewer", "convert"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn old_and_tinted_rice_files_are_migrated_on_boot_with_a_backup() {
        let (host, root) = temp_host("migrate");
        let path = host.paths().rice_dir().join(RICE_FILE);
        let old = include_str!("../../../crates/sue-rice/rice/history/v2.suerice");
        write_atomic(&path, old.as_bytes()).unwrap();
        let engine = Engine::boot(&host).unwrap();
        assert!(matches!(
            engine.rice_source,
            RiceSource::Migrated { from: 2, .. }
        ));
        let migrated = std::fs::read_to_string(&path).unwrap();
        assert!(!migrated.contains("#3b82f6"));
        assert_eq!(
            engine.world.rice.get_i64("layout.status_bar_height"),
            Some(22)
        );
        assert_eq!(
            engine.world.rice.get_i64("layout.rail_width"),
            None,
            "keys that are gone are dropped"
        );
        let backup = path.with_extension("suerice.v2.bak");
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
        write_atomic(&path, b"# sue-rice format 3\n[broken\n").unwrap();
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
                "Language",
                "Appearance & Rice",
                "Display & Graphics",
                "Audio & DSP",
                "Files & Codecs",
                "Studios",
                "Packages & Updates",
                "Keymaps",
                "MCP"
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
        assert_eq!(
            engine.world.rice.get_str(sue_wm::PIN_KEY),
            Some(""),
            "nothing is pinned: Sue opens on the Hub"
        );
        assert!(
            engine
                .world
                .rice
                .entries()
                .iter()
                .any(|entry| entry.path() == "profile.hardware"
                    && entry.label() == "Performance Preference")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
