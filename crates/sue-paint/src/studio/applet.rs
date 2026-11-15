//! Sue Paint as an applet: what it tells the shell and answers it.

use std::path::Path;

use egui::{Ui, vec2};
use sue_core::txn::{Batch, SpawnEntity};
use sue_core::{CHILD, ComponentKind, Entity, EntityId, PixelFormat, Relation, Value};
use sue_model::class;
use sue_model::picture;
use sue_rice::{RiceDeclaration, RiceParameter};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, Choice, Facts, FilePick, PaletteEntry,
    SettingsPage, ShellRequest, SueApplet,
};

use super::{
    BRUSHES, ID, KEYS_SECTION, SOUNDS, SuePaint, has_extension, me_id, opens, shortcut_table,
    to_rgba,
};
use crate::dialogs::{Dialog, NewDialog, PressureDialog};
use crate::doc::{self, NewPicture};

impl SuePaint {
    /// Sue Paint's own colors, used when `sue_paint.theme` is Custom; they
    /// start as the Gray theme.
    pub(super) fn custom_colors() -> Vec<RiceDeclaration> {
        let color = |key: &str, hex: &str, description: &str| {
            RiceDeclaration::new(
                format!("sue_paint_colors.{key}"),
                RiceParameter::ColorHex {
                    value: hex
                        .parse()
                        .unwrap_or_else(|error| panic!("built-in color {hex}: {error}")),
                },
                description,
            )
        };
        vec![
            color("void", "#2b2b2b", "Around the picture"),
            color("panel", "#3a3a3a", "Palettes and menus"),
            color("pill", "#444444", "Fields, chips and cards"),
            color("hover", "#505050", "Hovered controls"),
            color("active", "#5e5e5e", "Pressed and chosen controls"),
            color("border", "#2a2a2a", "Hairline borders"),
            color(
                "border_active",
                "#d8d8d8",
                "Borders of chosen and focused controls",
            ),
            color("text", "#e6e6e6", "Primary text and active icons"),
            color("muted", "#c4c4c4", "Secondary text"),
            color("selection", "#6a6a6a", "Text selection highlight"),
            color("selection_text", "#ffffff", "Text inside a selection"),
            color("grid", "#474747", "Grids and rulers"),
            color("shadow", "#141414", "Shadow under floating surfaces"),
        ]
    }
}

impl SueApplet for SuePaint {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(
            ID,
            "Sue Paint",
            AppletRole::Studio,
            sue_wm::icons::Icon::AppPaint,
        )
        .localized("paint");
        manifest.summary = tr!("applet-summary").to_owned();
        manifest.order = 25;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let int = |value, min, max| RiceParameter::IntRange {
            value,
            min: Some(min),
            max: Some(max),
        };
        let mut declarations = vec![
            RiceDeclaration::new(
                "sue_paint.width",
                int(1600, 1, 10_000),
                "Width of new illustrations (px)",
            ),
            RiceDeclaration::new(
                "sue_paint.height",
                int(1200, 1, 10_000),
                "Height of new illustrations (px)",
            ),
            RiceDeclaration::new(
                "sue_paint.dpi",
                int(72, 1, 2400),
                "Resolution of new illustrations (dpi)",
            ),
            RiceDeclaration::new(
                crate::materials::LOCATION_KEY,
                RiceParameter::Text {
                    value: String::new(),
                },
                "Where Sue Paint keeps its materials and brush tips (empty: its own folder)",
            ),
            RiceDeclaration::new(
                crate::clip_io::SAVE_FORMAT_KEY,
                RiceParameter::EnumChoice {
                    value: "sue".to_owned(),
                    options: vec!["clip".to_owned(), "sue".to_owned()],
                },
                "How Save keeps a picture: a Clip Studio Paint .clip file, or a Sue project",
            ),
            RiceDeclaration::new(
                crate::simple::MODE_KEY,
                RiceParameter::EnumChoice {
                    value: "simple".to_owned(),
                    options: vec!["simple".to_owned(), "studio".to_owned()],
                },
                "Sue Paint on phones and tablets: Simple Mode or Studio Mode",
            ),
            RiceDeclaration::new(
                "sue_paint.theme",
                RiceParameter::EnumChoice {
                    value: "gray".to_owned(),
                    options: [
                        "gray",
                        sue_wm::STUDIO_THEME_SUE,
                        "void",
                        "graphite",
                        "paper",
                        sue_wm::STUDIO_THEME_CUSTOM,
                    ]
                    .map(String::from)
                    .to_vec(),
                },
                "Sue Paint's colors: Gray, Sue's own theme (follows it), a preset, or Custom (below)",
            ),
            RiceDeclaration::new(
                crate::dialogs::PEN_PRESSURE,
                RiceParameter::Text {
                    value: crate::dialogs::DEFAULT_PEN_PRESSURE.to_owned(),
                },
                "Pen pressure curve for every tool, as pressure,output points (File, Pen Pressure Settings draws it)",
            ),
        ];
        // File, Preferences: what Sue Paint honors of Clip Studio's.
        let choice = |value: &str, options: &[&str]| RiceParameter::EnumChoice {
            value: value.to_owned(),
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        };
        declarations.extend([
            RiceDeclaration::new(
                "sue_paint.shift_line",
                RiceParameter::Bool { value: true },
                "Shift and drag draws a straight line, shown as it is drawn (Preferences, Tools)",
            )
            .labeled("Shift for straight lines"),
            RiceDeclaration::new(
                "sue_paint.two_finger_tap",
                choice("undo", &["undo", "none"]),
                "What a quick tap of two fingers on the canvas does (Preferences, Touch gestures)",
            )
            .labeled("Two-finger tap"),
            RiceDeclaration::new(
                "sue_paint.three_finger_tap",
                choice("redo", &["redo", "none"]),
                "What a quick tap of three fingers on the canvas does (Preferences, Touch gestures)",
            )
            .labeled("Three-finger tap"),
            RiceDeclaration::new(
                "sue_paint.brush_cursor",
                choice("brush", &["brush", "crosshair", "dot"]),
                "The pointer over the canvas while painting: the brush's outline, a crosshair or a dot (Preferences, Cursor)",
            )
            .labeled("Brush cursor"),
            RiceDeclaration::new(
                "sue_paint.selection_layer_opacity",
                int(50, 0, 100),
                "Opacity of a new selection layer (%; Preferences, Layer/Frame)",
            )
            .labeled("Opacity of selection layers"),
            RiceDeclaration::new(
                "sue_paint.rotate_step",
                int(15, 1, 90),
                "How far Rotate Left and Rotate Right turn the view (degrees; Preferences, Canvas)",
            )
            .labeled("Rotation step"),
            RiceDeclaration::new(
                crate::layout_style::KEY,
                RiceParameter::EnumChoice {
                    value: crate::layout_style::CLASSIC.to_owned(),
                    options: crate::layout_style::STYLES
                        .iter()
                        .map(|style| style.id.to_owned())
                        .collect(),
                },
                "How Sue Paint's workspace is designed: which palettes there are and where they sit (Window, Layout Style)",
            )
            .labeled("Layout Style"),
            RiceDeclaration::new(
                crate::extras::REMEMBER_LAYOUT,
                RiceParameter::Bool { value: true },
                "Sue Paint opens with its palettes where they were left (Window, Workspace)",
            )
            .labeled("Remember the palette layout"),
            RiceDeclaration::new(
                crate::panels::TOOL_COLOR_ICON,
                RiceParameter::Bool { value: true },
                "The Tool palette shows the main and sub colors under its tools (Tool palette menu, Show Color Icon)",
            )
            .labeled("Tool color icon"),
            RiceDeclaration::new(
                crate::studio::ASK_DELETE_FOLDERS,
                RiceParameter::Bool { value: true },
                "Ask before deleting picked layer folders (the question's \"Do not show again\" turns it off)",
            )
            .labeled("Ask before deleting layer folders"),
        ]);
        for (key, name, choices) in crate::dialogs::MODIFIERS {
            declarations.push(RiceDeclaration::new(
                key,
                RiceParameter::EnumChoice {
                    value: choices[0].to_owned(),
                    options: choices.iter().map(|choice| (*choice).to_owned()).collect(),
                },
                tr!("what-does-canvas-file-modifier-key", name = name),
            ));
        }
        declarations.extend(crate::studio_mode::CUSTOM_DEFAULTS.iter().enumerate().map(
            |(index, command)| {
                RiceDeclaration::new(
                    crate::studio_mode::custom_key(index),
                    RiceParameter::Text {
                        value: (*command).to_owned(),
                    },
                    format!("The command Studio Mode's key T{} runs", index + 1),
                )
            },
        ));
        declarations.extend(crate::studio_mode::declarations());
        declarations.extend(Self::custom_colors());
        declarations.extend(sue_wm::shortcuts::declarations(
            KEYS_SECTION,
            shortcut_table(),
        ));
        declarations
    }

    fn settings_page(&self) -> Option<SettingsPage> {
        let mut page = crate::prefs::page(&self.manifest().title());
        // Each Auto Action can be given a key too.
        if let Some(keys) = page.keys.as_mut() {
            keys.groups.push(self.auto_key_group());
        }
        Some(page)
    }

    fn setting_choices(&self) -> Vec<(String, Vec<Choice>)> {
        let mut choices = crate::prefs::choices();
        choices.push((
            crate::layout_style::KEY.to_owned(),
            crate::layout_style::choices(),
        ));
        choices
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.load_recent(cx);
        self.load_extras(cx);
        self.frame(ui, cx);
    }

    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        if cx.view.form_factor != sue_platform::FormFactor::Desktop
            && !crate::simple::studio_mode(cx)
        {
            if let Some(command) = self.simple_bar(ui, rect, cx, &theme) {
                self.run(&command, cx);
            }
            return;
        }
        if cx.view.form_factor != sue_platform::FormFactor::Desktop {
            self.studio_bar(ui, rect, cx, &theme);
            return;
        }
        if self.menus_hidden {
            return;
        }
        let mut menubar = std::mem::take(&mut self.menubar);
        let picked = {
            let facts = Facts::of(cx);
            let state = |command: &str| self.state(command, &facts);
            menubar.ui(ui, rect, &self.menus, &state, &theme)
        };
        self.menubar = menubar;
        if let Some(command) = picked {
            self.run(&command, cx);
        }
    }

    /// The Materials place, beside the Hub's Home and Files.
    fn hub_places(&self) -> Vec<sue_wm::HubPlace> {
        crate::hub::places()
    }

    fn hub_ui(&mut self, place: &str, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.place_ui(place, ui, cx);
    }

    /// The pictures of Sue Paint's projects, `.clip` and `.psd` files on
    /// the Hub's cards.
    fn thumbnail(&self) -> Option<sue_wm::files::Thumbnailer> {
        Some(crate::hub::thumbnailer())
    }

    /// A layered picture is a canvas as it is; a flat picture becomes one
    /// when it is handed over (see `take_entity`).
    fn accepts(&self, entity: &Entity) -> bool {
        picture::is_picture(entity) || sue_io::is_flat_picture(entity)
    }

    fn focus(&self) -> Option<EntityId> {
        self.picture()
    }

    /// Each canvas has its own history; the pages of a story share the
    /// story's, as a comic's pages share their Page Manager.
    fn history_scope(&self, arena: &sue_core::Arena) -> Option<EntityId> {
        let picture = self.picture()?;
        Some(crate::story::story_of(arena, picture).unwrap_or(picture))
    }

    /// Picture files come to Sue Paint as they are, wherever they are
    /// opened (the Sue menu, a drop, the Hub): it reads
    /// their layers where they have any, and opens each as a canvas. So
    /// do brush files, which become sub tools. A sound file is Sue Paint's
    /// only while a canvas is in front, where it goes on the timeline.
    fn opens_file(&self, path: &Path) -> bool {
        opens(path)
            || has_extension(path, BRUSHES)
            || (has_extension(path, SOUNDS) && self.present && self.picture().is_some())
    }

    fn reserved_keys(&self) -> Vec<sue_rice::Keybind> {
        // Every chord of the studio's shortcuts wins while it is open (Tab
        // hides the palettes, / is the Zoom tool, Ctrl+0 fits the picture).
        self.bound.chords(shortcut_table())
    }

    fn unsaved(&self, arena: &sue_core::Arena) -> Vec<(String, String)> {
        self.docs
            .iter()
            .filter(|doc| {
                let (_, _, version) = doc.compositor.display();
                self.clean
                    .get(&doc.picture)
                    .is_some_and(|clean| *clean != version)
            })
            .filter_map(|doc| {
                let name = arena.get_by_id(doc.picture).ok()?.name().to_owned();
                Some((doc.picture.to_string(), name))
            })
            .collect()
    }

    fn save_unsaved(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        // That picture to the front, then File, Save as for its tab.
        if let Some(index) = self
            .docs
            .iter()
            .position(|doc| doc.picture.to_string() == key)
        {
            self.active = index;
        }
        self.run("file.save", cx);
    }

    fn window_title(&self, arena: &sue_core::Arena) -> Option<String> {
        let Some(doc) = self.docs.get(self.active) else {
            return Some(tr!("paint-window-title-empty"));
        };
        let picture = arena.get_by_id(doc.picture).ok()?;
        let [width, height] = sue_model::picture::size(picture)?;
        let dpi = picture
            .property(sue_model::picture::DPI)
            .and_then(Value::as_f64)
            .unwrap_or(72.0);
        let (_, _, version) = doc.compositor.display();
        let unsaved = self
            .clean
            .get(&doc.picture)
            .is_some_and(|clean| *clean != version);
        // Clip Studio names the color profile in use; Sue Paint's one is
        // View, Color Profile, Preview (the picture as printed).
        let profile = if self.proof_on {
            tr!(
                "paint-window-title-profile",
                profile = crate::localize(
                    crate::proof::PROFILES
                        [self.proof_profile.min(crate::proof::PROFILES.len() - 1)]
                    .0
                )
            )
        } else {
            String::new()
        };
        Some(tr!(
            "paint-window-title",
            name = self
                .save_title(doc.picture)
                .unwrap_or_else(|| picture.name().to_owned()),
            unsaved = if unsaved { "*" } else { "" },
            width = width.to_string(),
            height = height.to_string(),
            dpi = format!("{dpi:.0}"),
            zoom = format!("{:.1}", doc.view.zoom * 100.0),
            profile = profile
        ))
    }

    fn take_entity(&mut self, purpose: &str, entity: EntityId, cx: &mut AppletContext<'_>) {
        if !matches!(purpose, "open" | "adopt") {
            return;
        }
        if cx.arena().get_by_id(entity).is_ok_and(picture::is_picture) {
            // A layered picture opens as it is.
            self.activate(&cx.world.arena, entity);
            cx.request(ShellRequest::Open(me_id()));
            return;
        }
        // Pixels that are a canvas's layer already (a flat picture Sue
        // Paint took before still reads as one) open that canvas.
        let canvas = cx
            .arena()
            .relations()
            .parent(entity, CHILD)
            .filter(|&parent| cx.arena().get_by_id(parent).is_ok_and(picture::is_picture));
        if let Some(canvas) = canvas {
            self.activate(&cx.world.arena, canvas);
            cx.request(ShellRequest::Open(me_id()));
            return;
        }
        // Any other picture becomes a layered picture whose layer is that
        // very entity, and Sue Paint opens on it.
        let name = cx.arena().get_by_id(entity).map_or_else(
            |_| tr!("picture"),
            |found| {
                Path::new(found.name()).file_stem().map_or_else(
                    || found.name().to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                )
            },
        );
        // Gray and float pixels cannot be painted on as they are. The
        // canvas gets an 8-bit copy, and the picture itself stays as it
        // was: nothing of it is lost, and Save asks where to write.
        let copy = cx.arena().get_by_id(entity).ok().and_then(|found| {
            let raster = found.component(ComponentKind::RasterTile2D)?.as_raster()?;
            (raster.format() != PixelFormat::Rgba8).then(|| {
                let source = found.meta(sue_io::meta::SOURCE).map(str::to_owned);
                (to_rgba(raster), source)
            })
        });
        if let Some((rgba, source)) = copy {
            let (width, height) = (rgba.width(), rgba.height());
            if width > doc::MAX_SIDE || height > doc::MAX_SIDE {
                cx.fail(tr!(
                    "picture-must-most-pixels-side",
                    MAX_SIDE = doc::MAX_SIDE
                ));
                return;
            }
            let mut image = Entity::empty(name.clone(), class::IMAGE)
                .with_property(picture::WIDTH, i64::from(width))
                .with_property(picture::HEIGHT, i64::from(height))
                .with_property(picture::DPI, 72.0);
            if let Some(source) = source {
                image = image.with_meta(sue_io::meta::SOURCE, source);
            }
            let picture_id = image.id();
            let layer = Entity::new(name.clone(), rgba).with_class(class::LAYER);
            let layer_id = layer.id();
            let edit = Batch::new(tr!("open-in-sue-paint", name = name.as_str()))
                .then(SpawnEntity::new(image))
                .then(
                    SpawnEntity::new(layer).with_relation(
                        Relation::new(picture_id, CHILD, layer_id)
                            .ordered(sue_core::OrderKey::default()),
                    ),
                );
            if Self::apply(cx, edit) {
                self.activate(&cx.world.arena, picture_id);
                self.saved_at.insert(picture_id, cx.history.cursor());
                cx.request(ShellRequest::Open(me_id()));
            }
            return;
        }
        match doc::adopt(cx.arena(), entity, &name) {
            Ok((edit, picture)) => {
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture);
                    self.came_from_picture(cx, entity, picture);
                    cx.request(ShellRequest::Open(me_id()));
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if matches!(pick.purpose.as_str(), "open" | "import" | "layer")
            && cx.view.page.as_ref().map(AppletId::as_str) != Some(ID)
        {
            cx.request(ShellRequest::Open(me_id()));
        }
        if self.place_picked(&pick.purpose, path, cx)
            || self.bar_picked(&pick.purpose, path, cx)
            || self.auto_picked(&pick.purpose, path, cx)
            || self.bank_picked(&pick.purpose, path, cx)
        {
            return;
        }
        // Batch Convert Pictures' files and output folder.
        if self.batch_picked(cx, &pick.purpose, path) {
            return;
        }
        match pick.purpose.as_str() {
            "new-from-movie" => self.new_from_movie(path, cx),
            "open" => self.open_picked(cx, path, false),
            // A picture dropped on a canvas becomes its layer.
            "import" if self.take_canvas_drop(path) => self.open_picked(cx, path, true),
            // A file opened anywhere in Sue that Sue Paint takes itself
            // (see `opens_file`).
            "import" => self.take_file(cx, path),
            "layer" => self.open_picked(cx, path, true),
            "import-layers" => self.open_picked(cx, path, true),
            "batch-import" => self.batch_import(cx, path),
            "story-text" => self.import_text(cx, path),
            "brush-import" => self.import_brushes(cx, path),
            "tool-layout" => self.import_tool_layout(cx, path),
            "batch" => self.export_canvases(cx, path),
            "import-vector" | "import-movie" | "import-pattern" => {
                self.import_file(cx, pick.purpose.as_str(), path)
            }
            "export" => {
                if let Some(bytes) = self.pending_export.take() {
                    self.write_export(cx, path, &bytes);
                }
            }
            "save-as" => self.save_as_to(cx, path),
            "story-pages" => self.export_pages(cx, path),
            "book-pdf" => self.export_book(cx, path, false),
            "book-epub" => self.export_book(cx, path, true),
            "anim-gif" => self.export_animation(cx, path, true),
            "anim-movie" => self.export_movie(cx, path),
            "webtoon" => self.export_webtoon(cx, path),
            "vectors" => self.export_vectors(cx, path),
            "layer-comps" => self.export_comps(cx, path),
            "exposure-sheet-out" => self.export_exposure_sheet(cx, path),
            "light-table-file" => self.light_table_file(cx, path),
            "pose-scanner" => self.scan(cx, path, false),
            "hand-scanner" => self.scan(cx, path, true),
            "exposure-sheet-in" => self.import_exposure_sheet(cx, path),
            "anim-cels" => self.export_cels(cx, path),
            "timelapse" => self.export_timelapse(cx, path),
            "anim-sequence" => self.export_animation(cx, path, false),
            "subview" => self.add_reference(path, cx),
            "model3d" => self.import_3d(cx, path),
            "anim-audio" => self.import_audio(cx, path),
            "anim-sound" => self.write_audio(cx, path),
            "colorset-import" => self.import_color_set(cx, path),
            "colorset-export" => self.export_color_set(cx, path),
            _ => {}
        }
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        let mut templates: Vec<sue_wm::FileTemplate> = crate::dialogs::new_presets()
            .for_target(crate::dialogs::NEW_PRESET_TARGET)
            .map(|preset| {
                // Short labels fit the wheel's wedges.
                let label = match preset.name.as_str() {
                    "Print (A4, 350 dpi)" => tr!("print-a4"),
                    "Screen (Full HD)" => tr!("full-hd"),
                    "Transparent Sprite" => tr!("sprite"),
                    name => name.to_owned(),
                };
                sue_wm::FileTemplate::new(tr!("picture"), label, &preset.name)
            })
            .collect();
        templates.push(sue_wm::FileTemplate::new(
            tr!("picture"),
            tr!("custom-ellipsis"),
            "custom",
        ));
        templates
    }

    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) {
        if key == "custom" {
            self.dialog = Some(Dialog::New(Self::new_dialog(cx)));
        } else {
            let library = crate::dialogs::new_presets();
            let Some(preset) = library.get(crate::dialogs::NEW_PRESET_TARGET, key) else {
                return;
            };
            let mut dialog = NewDialog::new(&NewPicture::default());
            dialog.apply_preset(preset);
            let mut spec = dialog.spec();
            spec.name = preset.name.clone();
            self.make_picture(cx, &spec);
        }
        // Then into Sue Paint.
        cx.request(ShellRequest::Open(me_id()));
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> sue_wm::Tick {
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            self.shown = false;
            self.settle(cx);
        }
        // The command palette shows the keys Settings has, read again
        // only when the settings changed.
        let revision = cx.rice().revision();
        if self.palette_keys.0 != Some(revision) {
            let keys = crate::commands::registry()
                .palette(cx.rice())
                .into_iter()
                .map(|entry| (entry.command, entry.hint))
                .collect();
            self.palette_keys = (Some(revision), keys);
        }
        // Batch Convert Pictures goes on while its dialog is closed and
        // Sue Paint is not shown.
        self.poll_batch(cx);
        // So do the exports running in the background: each says how it
        // went when it ends.
        self.poll_exports(cx);
        sue_wm::Tick {
            wake: (self
                .files
                .batch
                .as_ref()
                .is_some_and(sue_wm::job_list::JobList::busy)
                || self.exports.busy())
            .then(|| cx.view.now + std::time::Duration::from_millis(100)),
            ..sue_wm::Tick::default()
        }
    }

    fn captures_keys(&self) -> bool {
        // A dialog, a stroke or words being typed own the keys, so the
        // global undo cannot pull pixels from under a preview, a pen or a
        // caret.
        self.dialog.is_some()
            || self.gesture.is_some()
            || self.typing.is_some()
            || self.files.dialog_open()
    }

    fn commands(&self) -> Vec<PaletteEntry> {
        // The palette says whose command it is ("Sue Paint: ..."), in the
        // language in use now, with the keys Settings has (the default
        // keys until the first tick has read them).
        let mut entries = crate::commands::registry().palette(&sue_rice::RiceSchema::new());
        for entry in &mut entries {
            if let Some((_, hint)) = self
                .palette_keys
                .1
                .iter()
                .find(|(command, _)| *command == entry.command)
            {
                entry.hint.clone_from(hint);
            }
        }
        entries
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        if !crate::commands::registry().has(name) {
            return false;
        }
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) {
            cx.request(ShellRequest::Open(me_id()));
        }
        self.sync_docs(&cx.world.arena);
        self.run(name, cx);
        true
    }

    fn navigate(&mut self, target: &str) -> bool {
        if let Some(title) = target.strip_prefix("menu/") {
            return self.menubar.open(&self.menus, title);
        }
        match target {
            "dialog/new" => {
                self.dialog = Some(Dialog::New(NewDialog::new(&NewPicture::default())));
                true
            }
            "dialog/pages" => {
                // The Page Manager is a view of the canvas area now.
                self.dialog = None;
                self.page_view = Some(crate::pages::PageView::default());
                true
            }
            "dialog/binding" => {
                self.dialog = Some(Dialog::Pages(crate::dialogs::PagesDialog::at(0)));
                true
            }
            "dialog/pressure" => {
                // A sample of someone's drawing, so the pad shows lines.
                let mut pressure = PressureDialog::new(self.pen_curve);
                let line: Vec<(egui::Vec2, f32)> = (0..120)
                    .map(|step| {
                        let t = step as f32 / 119.0;
                        let force = (t * std::f32::consts::PI).sin() * 0.55 + 0.05;
                        (vec2(20.0 + t * 480.0, 75.0 + (t * 9.0).sin() * 40.0), force)
                    })
                    .collect();
                pressure.raw = line.iter().map(|(_, force)| *force).collect();
                pressure.lines.push(line);
                self.dialog = Some(Dialog::Pressure(pressure));
                true
            }
            place if place.starts_with("materials/") => self.hub.navigate(place),
            bar if bar.starts_with("bar/") => self.bar_navigate(bar),
            extras if extras.starts_with("extras/") => self.extras_navigate(extras),
            page if page.starts_with("new-canvas") => self.new_canvas.navigate(page),
            // A palette's menu: `dock/menu/<palette key>`; `dock/menu` alone
            // closes it.
            menu if menu.starts_with("dock/menu") => {
                use sue_wm::dock::DockPanel;
                let key = menu.strip_prefix("dock/menu/").unwrap_or("");
                self.dock.menu = crate::dock::Palette::named(key);
                self.dock.popped = None;
                if let Some(palette) = self.dock.menu {
                    self.dock.choose(palette);
                }
                true
            }
            // A palette floating over the canvas, `dock/float/<palette
            // key>`, and rolled up to its header, `dock/roll/<key>`.
            float if float.starts_with("dock/float/") => {
                use sue_wm::dock::DockPanel;
                let Some(palette) = crate::dock::Palette::named(&float["dock/float/".len()..])
                else {
                    return false;
                };
                self.dock.float(palette, egui::pos2(480.0, 160.0));
                true
            }
            roll if roll.starts_with("dock/roll/") => {
                use sue_wm::dock::DockPanel;
                let Some(palette) = crate::dock::Palette::named(&roll["dock/roll/".len()..]) else {
                    return false;
                };
                self.roll(palette, true);
                true
            }
            // A palette's tab: `tab/<palette>/<index>`.
            tab if tab.starts_with("tab/") => {
                let mut parts = tab["tab/".len()..].split('/');
                let (Some(palette), Some(Ok(index))) =
                    (parts.next(), parts.next().map(str::parse::<u8>))
                else {
                    return false;
                };
                // In the dock every tab is a palette of its own: it shows.
                use crate::dock::Palette;
                let own = match (palette, index) {
                    ("tool", 1) => Some(Palette::ToolProperty),
                    ("tool", 2) => Some(Palette::BrushGrid),
                    ("layer", 1) => Some(Palette::LayerProperty),
                    ("layer", 2) => Some(Palette::History),
                    ("layer", 3) => Some(Palette::Cels),
                    ("navigator", 1) => Some(Palette::SubView),
                    ("color", 1) => Some(Palette::ColorSlider),
                    ("color", 2) => Some(Palette::Intermediate),
                    ("color", 3) => Some(Palette::Approximate),
                    _ => None,
                };
                if let Some(own) = own.filter(|_| self.sheet.is_none()) {
                    self.show_palette(own);
                    return true;
                }
                match palette {
                    "tool" => self.tool_tab = index,
                    "layer" => self.layer_tab = index,
                    "navigator" => self.nav_tab = index,
                    "color" => self.color_tab = index,
                    _ => return false,
                }
                true
            }
            "sheet/more" => {
                self.sheet = Some(crate::phone::Sheet::More);
                true
            }
            "sheet/pens" => {
                self.sheet = Some(crate::phone::Sheet::Pens);
                true
            }
            "sheet/close" => {
                self.sheet = None;
                self.touch.bar = crate::studio_mode::KeyBar::Hidden;
                true
            }
            "sheet/menu" => {
                self.sheet = Some(crate::phone::Sheet::Menu);
                true
            }
            "keys/open" => {
                self.touch.bar = crate::studio_mode::KeyBar::Pinned;
                true
            }
            "keys/peek" => {
                self.touch.bar = crate::studio_mode::KeyBar::Peek;
                true
            }
            "keys/left" => {
                self.touch.edge = crate::studio_mode::Edge::Left;
                true
            }
            "keys/right" => {
                self.touch.edge = crate::studio_mode::Edge::Right;
                true
            }
            "sheet/tools" => {
                self.sheet = Some(crate::phone::Sheet::Tools);
                true
            }
            "sheet/layers" => {
                self.sheet = Some(crate::phone::Sheet::Layers);
                true
            }
            "dialog/close" => {
                self.dialog = None;
                self.save_ask = None;
                self.page_view = None;
                self.tool_palettes.naming = None;
                self.files.close();
                self.menubar.close();
                true
            }
            _ => false,
        }
    }

    /// A canvas in a window of its own (see `canvas/window.rs`).
    fn view_ui(&mut self, view: &str, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.canvas_window_ui(view, ui, cx);
    }

    /// The window of a canvas closed: the canvas goes back in the tabs.
    fn view_closed(&mut self, view: &str, cx: &mut AppletContext<'_>) {
        self.canvas_window_closed(view, cx);
    }
}
