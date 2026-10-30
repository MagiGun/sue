//! Sue Paint's menu bar, as data. Every item names a command; the studio
//! grays out the ones it cannot run yet, so the menus show the whole plan.

use sue_wm::{Menu, MenuItem};

/// The Open Recent items for `files` (newest first), then Clear Recent
/// Files.
#[must_use]
pub fn recent_items(files: &[std::path::PathBuf]) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = files
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            act(&name, &format!("file.recent.open.{index}"))
        })
        .collect();
    if !items.is_empty() {
        items.push(SEP);
    }
    items.push(act(&tr!("clear-recent-files"), "file.recent.clear"));
    items
}

/// Every command of `menus` with its label, in menu order.
#[must_use]
pub fn commands(menus: &[Menu]) -> Vec<(String, String)> {
    fn walk(items: &[MenuItem], out: &mut Vec<(String, String)>) {
        for item in items {
            match item {
                MenuItem::Action { label, command, .. } => {
                    out.push((label.trim_end_matches("...").to_owned(), command.clone()))
                }
                MenuItem::Sub { items, .. } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in menus {
        walk(&menu.items, &mut out);
    }
    out
}

/// Shows the registered workspaces `names` in Window, Workspace.
pub fn set_workspaces(menus: &mut [Menu], names: &[String]) {
    let mut items: Vec<MenuItem> = names
        .iter()
        .enumerate()
        .map(|(index, name)| act(name, &format!("window.workspace.use.{index}")))
        .collect();
    if !items.is_empty() {
        items.push(SEP);
    }
    items.push(act(
        &tr!("register-workspace-ellipsis"),
        "window.workspace.save",
    ));
    items.push(act(
        &tr!("manage-workspaces-ellipsis"),
        "window.workspace.manage",
    ));
    for menu in menus.iter_mut().filter(|menu| menu.title == tr!("window")) {
        for item in &mut menu.items {
            if let MenuItem::Sub {
                label,
                items: inside,
            } = item
                && *label == tr!("workspace")
            {
                *inside = items;
                return;
            }
        }
    }
}

/// Puts `items` in the Open Recent submenu of `menus`.
pub fn set_recent(menus: &mut [Menu], items: Vec<MenuItem>) {
    for menu in menus.iter_mut().filter(|menu| menu.title == tr!("file")) {
        for item in &mut menu.items {
            if let MenuItem::Sub {
                label,
                items: inside,
            } = item
                && *label == tr!("open-recent")
            {
                *inside = items;
                return;
            }
        }
    }
}

fn act(label: &str, command: &str) -> MenuItem {
    MenuItem::action(label, command)
}

/// What the menus call `command`, if a menu has it.
#[must_use]
pub fn label_of<'m>(menus: &'m [Menu], command: &str) -> Option<&'m str> {
    fn find<'m>(items: &'m [MenuItem], command: &str) -> Option<&'m str> {
        items.iter().find_map(|item| match item {
            MenuItem::Action {
                label,
                command: found,
                ..
            } if found == command => Some(label.as_str()),
            MenuItem::Sub { items, .. } => find(items, command),
            _ => None,
        })
    }
    menus.iter().find_map(|menu| find(&menu.items, command))
}

fn sub(label: &str, items: Vec<MenuItem>) -> MenuItem {
    MenuItem::sub(label, items)
}

const SEP: MenuItem = MenuItem::Separator;

/// The menus, left to right.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the whole menu bar as one table")]
pub fn menus() -> Vec<Menu> {
    vec![
        Menu::new(
            tr!("file"),
            vec![
                act(&tr!("start-page"), "file.start_page"),
                act(&tr!("new-ellipsis"), "file.new").key("Ctrl+N"),
                act(&tr!("create-new-from-clipboard"), "file.new_clipboard"),
                act(&tr!("open-ellipsis"), "file.open").key("Ctrl+O"),
                sub(
                    &tr!("open-recent"),
                    vec![act(&tr!("clear-recent-files"), "file.recent.clear")],
                ),
                act(&tr!("close"), "file.close").key("Ctrl+W"),
                SEP,
                act(&tr!("save"), "file.save").key("Ctrl+S"),
                act(&tr!("save-as-ellipsis"), "file.save_as").key("Shift+Alt+S"),
                sub(
                    &tr!("save-duplicate"),
                    vec![
                        act(
                            &tr!("clip-clip-studio-format-ellipsis"),
                            "file.duplicate.clip",
                        ),
                        act(&tr!("bmp-bmp-ellipsis"), "file.export.bmp"),
                        act(&tr!("jpg-jpeg-ellipsis"), "file.export.jpg"),
                        act(&tr!("png-png-ellipsis"), "file.export.png"),
                        act(&tr!("tif-tiff-ellipsis"), "file.export.tif"),
                        act(&tr!("tga-targa-ellipsis"), "file.export.tga"),
                        act(&tr!("webp-webp-ellipsis"), "file.export.webp"),
                        act(&tr!("psb-photoshop-big-ellipsis"), "file.export.psb"),
                        act(&tr!("psd-photoshop-document-ellipsis"), "file.export.psd"),
                        act(&tr!("jxl-jpeg-xl-ellipsis"), "file.export.jxl"),
                    ],
                ),
                act(&tr!("revert"), "file.revert"),
                SEP,
                sub(
                    &tr!("import"),
                    vec![
                        act(&tr!("image-ellipsis"), "file.import.image"),
                        act(
                            &tr!("image-as-separate-layers-ellipsis"),
                            "file.import.layers",
                        ),
                        act(&tr!("pattern-from-image-ellipsis"), "file.import.pattern"),
                        act(&tr!("vector-ellipsis"), "file.import.vector"),
                        act(&tr!("audio-ellipsis"), "file.import.audio"),
                        act(&tr!("movie-ellipsis"), "file.import.movie"),
                        act(&tr!("n-3d-data-ellipsis"), "file.import.3d"),
                    ],
                ),
                sub(
                    &tr!("export-single-layer"),
                    vec![
                        act(&tr!("bmp-bmp-ellipsis"), "file.export.bmp"),
                        act(&tr!("jpg-jpeg-ellipsis"), "file.export.jpg"),
                        act(&tr!("png-png-ellipsis"), "file.export.png"),
                        act(&tr!("tif-tiff-ellipsis"), "file.export.tif"),
                        act(&tr!("tga-targa-ellipsis"), "file.export.tga"),
                        act(&tr!("webp-webp-ellipsis"), "file.export.webp"),
                        act(&tr!("psb-photoshop-big-ellipsis"), "file.export.psb"),
                        act(&tr!("psd-photoshop-document-ellipsis"), "file.export.psd"),
                        act(&tr!("jxl-jpeg-xl-ellipsis"), "file.export.jxl"),
                    ],
                ),
                sub(
                    &tr!("export-animation"),
                    vec![
                        act(&tr!("animated-gif-ellipsis"), "file.export.gif"),
                        act(&tr!("movie-ellipsis"), "file.export.movie"),
                        act(&tr!("image-sequence-ellipsis"), "file.export.sequence"),
                        act(&tr!("animation-cels-ellipsis"), "file.export.cels"),
                    ],
                ),
                sub(
                    &tr!("export-multiple-pages"),
                    vec![
                        act(&tr!("batch-export-ellipsis"), "file.export.batch"),
                        act(&tr!("export-as-pdf-ellipsis"), "file.export.pdf"),
                        act(&tr!("export-as-epub-ellipsis"), "file.export.epub"),
                    ],
                ),
                sub(
                    &tr!("special-export"),
                    vec![act(&tr!("export-webtoon-ellipsis"), "file.export.webtoon")],
                ),
                SEP,
                sub(
                    &tr!("timelapse"),
                    vec![
                        act(&tr!("record-timelapse-2"), "file.timelapse.record"),
                        act(&tr!("export-timelapse-ellipsis"), "file.timelapse.export"),
                    ],
                ),
                SEP,
                act(&tr!("print-settings-ellipsis"), "file.print_settings"),
                act(&tr!("print-ellipsis"), "file.print").key("Ctrl+P"),
                SEP,
                act(&tr!("preferences-ellipsis"), "file.preferences").key("Ctrl+,"),
                act(&tr!("shortcut-settings-ellipsis"), "file.shortcuts"),
                act(&tr!("modifier-key-settings-ellipsis"), "file.modifiers"),
                act(&tr!("command-bar-settings-ellipsis"), "file.command_bar"),
                act(&tr!("pen-pressure-settings-ellipsis"), "file.pressure"),
                SEP,
                act(&tr!("back-to-the-hub"), "file.leave"),
                act(&tr!("quit-sue"), "app.quit").key("Ctrl+Q"),
            ],
        ),
        Menu::new(
            tr!("edit"),
            vec![
                act(&tr!("undo"), "edit.undo").key("Ctrl+Z"),
                act(&tr!("redo"), "edit.redo").key("Ctrl+Y"),
                SEP,
                act(&tr!("cut"), "edit.cut").key("Ctrl+X"),
                act(&tr!("copy-2"), "edit.copy").key("Ctrl+C"),
                act(&tr!("paste"), "edit.paste").key("Ctrl+V"),
                act(&tr!("paste-to-shown-position"), "edit.paste_here"),
                act(&tr!("delete"), "edit.clear").key("Del"),
                act(&tr!("delete-outside-selection"), "edit.clear_outside").key("Shift+Del"),
                act(&tr!("fill"), "edit.fill").key("Alt+Del"),
                act(&tr!("convert-to-drawing-color"), "edit.recolor"),
                act(&tr!("outline-selection-ellipsis"), "edit.outline_selection"),
                act(&tr!("convert-brightness-to-opacity"), "edit.luma_to_alpha"),
                SEP,
                sub(
                    &tr!("tonal-correction"),
                    vec![
                        act(&tr!("brightness-contrast-ellipsis"), "filter.brightness"),
                        act(&tr!("level-correction-ellipsis"), "filter.levels"),
                        act(&tr!("tone-curve-ellipsis"), "filter.curve"),
                        act(&tr!("hue-saturation-luminosity-ellipsis"), "filter.hue").key("Ctrl+U"),
                        act(&tr!("color-balance-ellipsis"), "filter.balance"),
                        act(&tr!("reverse-gradient"), "filter.invert").key("Ctrl+I"),
                        act(&tr!("posterization-ellipsis"), "filter.posterize"),
                        act(&tr!("binarization-ellipsis"), "filter.binarize"),
                        act(&tr!("gradient-map-ellipsis"), "filter.gradient_map"),
                    ],
                ),
                sub(
                    &tr!("transform"),
                    vec![
                        act(&tr!("scale-rotate-ellipsis"), "edit.transform.scale").key("Ctrl+T"),
                        act(&tr!("scale-ellipsis"), "edit.transform.scale_only"),
                        act(&tr!("rotate-ellipsis"), "edit.transform.rotate"),
                        SEP,
                        act(&tr!("free-transform"), "edit.transform.free").key("Ctrl+Shift+T"),
                        act(&tr!("distort"), "edit.transform.distort"),
                        act(&tr!("skew"), "edit.transform.skew"),
                        act(&tr!("perspective"), "edit.transform.perspective"),
                        SEP,
                        act(&tr!("flip-horizontal"), "edit.flip_h"),
                        act(&tr!("flip-vertical"), "edit.flip_v"),
                        act(&tr!("rotate-180-degrees"), "edit.rotate180"),
                        SEP,
                        act(&tr!("mesh-transformation"), "edit.transform.mesh"),
                        act(&tr!("puppet-warp-ellipsis"), "edit.transform.puppet"),
                    ],
                ),
                sub(
                    &tr!("align-distribute"),
                    vec![
                        act(&tr!("align-left"), "edit.align.left"),
                        act(&tr!("align-horizontal-center"), "edit.align.hcenter"),
                        act(&tr!("align-right"), "edit.align.right"),
                        SEP,
                        act(&tr!("align-top"), "edit.align.top"),
                        act(&tr!("align-vertical-center"), "edit.align.vcenter"),
                        act(&tr!("align-bottom"), "edit.align.bottom"),
                    ],
                ),
                SEP,
                act(&tr!("change-image-resolution-ellipsis"), "edit.resolution"),
                act(&tr!("change-canvas-size-ellipsis"), "edit.canvas_size"),
                act(&tr!("change-canvas-height-ellipsis"), "edit.canvas_height"),
                act(&tr!("crop"), "edit.crop"),
                sub(
                    &tr!("rotate-flip-canvas"),
                    vec![
                        act(&tr!("rotate-90-degrees-left"), "canvas.rotate_left"),
                        act(&tr!("rotate-90-degrees-right"), "canvas.rotate_right"),
                        act(&tr!("rotate-180-degrees"), "canvas.rotate180"),
                        act(&tr!("flip-horizontal"), "canvas.flip_h"),
                        act(&tr!("flip-vertical"), "canvas.flip_v"),
                    ],
                ),
                act(&tr!("canvas-properties-ellipsis"), "edit.canvas_properties"),
                sub(
                    &tr!("clear-work-history"),
                    vec![act(&tr!("this-canvas"), "edit.clear_history")],
                ),
                SEP,
                act(
                    &tr!("create-brush-tip-from-image-ellipsis"),
                    "edit.brush_tip",
                ),
                sub(
                    &tr!("register-material"),
                    vec![
                        act(&tr!("image-ellipsis"), "edit.material.image"),
                        act(&tr!("brush-tip-ellipsis"), "edit.material.tip"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("story"),
            vec![
                act(&tr!("page-manager-ellipsis"), "story.manager"),
                SEP,
                act(&tr!("first-page"), "story.first"),
                act(&tr!("previous-page"), "story.previous"),
                act(&tr!("next-page"), "story.next"),
                act(&tr!("last-page"), "story.last"),
                SEP,
                act(&tr!("add-page-ellipsis"), "story.add"),
                act(&tr!("add-page-advanced-ellipsis"), "story.insert"),
                act(&tr!("delete-page"), "story.delete"),
                act(&tr!("page-reordering-ellipsis"), "story.order"),
                SEP,
                act(&tr!("combine-pages"), "story.combine"),
                act(&tr!("split-pages"), "story.separate"),
                SEP,
                act(
                    &tr!("change-project-settings-ellipsis"),
                    "story.work_settings",
                ),
                act(&tr!("change-page-settings-ellipsis"), "story.page_settings"),
                act(&tr!("change-story-information-ellipsis"), "story.info"),
                SEP,
                act(&tr!("story-editor"), "story.editor"),
                act(&tr!("batch-import-text-ellipsis"), "story.import_text"),
                sub(
                    &tr!("page-management"),
                    vec![
                        act(&tr!("open-page"), "story.open_page"),
                        act(&tr!("duplicate-page"), "story.duplicate_page"),
                        act(&tr!("export-page-as-file-ellipsis"), "story.export_page"),
                        act(&tr!("export-all-pages-ellipsis"), "story.export_pages"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("animation"),
            vec![
                sub(
                    &tr!("timeline"),
                    vec![
                        act(&tr!("new-timeline-ellipsis"), "anim.timeline.new"),
                        act(&tr!("edit-timeline-ellipsis"), "anim.timeline.edit"),
                        act(&tr!("enable-timeline"), "anim.timeline.enable"),
                    ],
                ),
                sub(
                    &tr!("new-animation-layer"),
                    vec![
                        act(&tr!("animation-folder-2"), "anim.layer.folder"),
                        act(&tr!("camera-folder"), "anim.layer.camera"),
                    ],
                ),
                act(&tr!("new-animation-cel"), "anim.cel.new"),
                act(&tr!("assign-animation-cel"), "anim.cel.assign"),
                SEP,
                sub(
                    &tr!("edit-track"),
                    vec![
                        act(&tr!("add-keyframe"), "anim.key.add"),
                        act(&tr!("delete-keyframe"), "anim.key.delete"),
                        act(&tr!("insert-frame"), "anim.frame.insert"),
                        act(&tr!("delete-frame"), "anim.frame.delete"),
                    ],
                ),
                sub(
                    &tr!("show-animation-cels"),
                    vec![
                        act(&tr!("enable-onion-skin"), "anim.onion"),
                        act(&tr!("onion-skin-settings-ellipsis"), "anim.onion_settings"),
                    ],
                ),
                sub(
                    &tr!("light-table"),
                    vec![
                        act(&tr!("enable-light-table"), "anim.light_table"),
                        SEP,
                        act(&tr!("register-selected-layer"), "anim.light_table.register"),
                        act(
                            &tr!("deregister-all-images-light-table"),
                            "anim.light_table.clear",
                        ),
                    ],
                ),
                SEP,
                act(&tr!("play-stop"), "anim.play"),
                sub(
                    &tr!("move-frame"),
                    vec![
                        act(&tr!("go-to-start"), "anim.first"),
                        act(&tr!("go-to-end"), "anim.last"),
                        act(&tr!("go-to-previous-frame"), "anim.prev"),
                        act(&tr!("go-to-next-frame"), "anim.next"),
                    ],
                ),
                sub(
                    &tr!("playback"),
                    vec![
                        act(&tr!("play-stop"), "anim.play"),
                        act(&tr!("loop"), "anim.loop"),
                        act(&tr!("next-frame-3"), "anim.next"),
                        act(&tr!("previous-frame-3"), "anim.prev"),
                        act(&tr!("first-frame-2"), "anim.first"),
                        act(&tr!("last-frame-2"), "anim.last"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("layer-2"),
            vec![
                act(&tr!("new-raster-layer"), "layer.new.raster_top"),
                sub(
                    &tr!("new-layer"),
                    vec![
                        act(&tr!("raster-layer-2"), "layer.new").key("Ctrl+Shift+N"),
                        act(&tr!("vector-layer-2"), "layer.new.vector"),
                        act(&tr!("text"), "layer.new.text"),
                        act(&tr!("balloon"), "layer.new.balloon"),
                        act(&tr!("gradient-ellipsis"), "layer.new.gradient"),
                        act(&tr!("fill"), "layer.new.fill"),
                        act(&tr!("tone-ellipsis"), "layer.new.tone"),
                        act(&tr!("paper-2"), "layer.new.paper"),
                        SEP,
                        act(&tr!("n-3d-layer"), "layer.new.3d"),
                        act(&tr!("frame-border-folder-ellipsis"), "layer.new.frame"),
                    ],
                ),
                sub(
                    &tr!("new-correction-layer"),
                    vec![
                        act(
                            &tr!("brightness-contrast-ellipsis"),
                            "layer.correction.brightness",
                        ),
                        act(&tr!("level-correction-ellipsis"), "layer.correction.levels"),
                        act(&tr!("tone-curve-ellipsis"), "layer.correction.curve"),
                        act(
                            &tr!("hue-saturation-luminosity-ellipsis"),
                            "layer.correction.hue",
                        ),
                        act(&tr!("color-balance-ellipsis"), "layer.correction.balance"),
                        act(&tr!("reverse-gradient"), "layer.correction.invert"),
                        act(&tr!("posterization-ellipsis"), "layer.correction.posterize"),
                        act(&tr!("binarization-ellipsis"), "layer.correction.binarize"),
                        act(
                            &tr!("gradient-map-ellipsis"),
                            "layer.correction.gradient_map",
                        ),
                    ],
                ),
                act(
                    &tr!("create-folder-and-insert-layer"),
                    "layer.folder_insert",
                )
                .key("Ctrl+G"),
                act(&tr!("new-layer-folder"), "layer.new.folder"),
                act(&tr!("ungroup-layer-folder"), "layer.ungroup").key("Ctrl+Shift+G"),
                act(&tr!("new-layer-from-selection"), "layer.from_selection"),
                SEP,
                act(&tr!("duplicate-layer"), "layer.duplicate"),
                act(&tr!("delete-layer"), "layer.delete"),
                sub(
                    &tr!("delete-layers"),
                    vec![
                        act(&tr!("empty-layers"), "layer.delete_empty"),
                        act(&tr!("hidden-layers"), "layer.delete_hidden"),
                    ],
                ),
                act(&tr!("hide-all-visible-draft-layers"), "layer.hide_drafts"),
                SEP,
                sub(
                    &tr!("layer-mask"),
                    vec![
                        act(&tr!("mask-outside-selection"), "layer.mask.outside"),
                        act(&tr!("mask-selection"), "layer.mask.inside"),
                        act(&tr!("delete-mask"), "layer.mask.delete"),
                        act(&tr!("apply-mask-to-layer"), "layer.mask.apply"),
                        SEP,
                        act(&tr!("enable-mask"), "layer.mask.toggle"),
                        act(&tr!("edit-mask"), "layer.mask.edit"),
                    ],
                ),
                sub(
                    &tr!("ruler-frame"),
                    vec![
                        act(&tr!("delete-ruler"), "layer.ruler.delete"),
                        act(&tr!("show-ruler"), "layer.ruler.show"),
                        SEP,
                        act(
                            &tr!("create-perspective-ruler-ellipsis"),
                            "layer.ruler.perspective",
                        ),
                        act(&tr!("create-symmetrical-ruler"), "layer.ruler.symmetry"),
                        act(&tr!("create-concentric-circle-ruler"), "layer.ruler.circle"),
                    ],
                ),
                SEP,
                sub(
                    &tr!("layer-settings"),
                    vec![
                        act(&tr!("show-layer"), "layer.toggle.visible"),
                        act(&tr!("change-layer-name-ellipsis"), "layer.rename"),
                        act(&tr!("clip-to-layer-below"), "layer.toggle.clip").key("Ctrl+Alt+G"),
                        act(&tr!("set-as-reference-layer"), "layer.toggle.reference"),
                        act(&tr!("set-as-draft-layer"), "layer.toggle.draft"),
                        act(&tr!("lock-layer"), "layer.toggle.lock"),
                        act(&tr!("lock-transparent-pixels"), "layer.toggle.lock_alpha"),
                    ],
                ),
                act(&tr!("layer-property"), "layer.properties"),
                SEP,
                sub(
                    &tr!("change-selected-layer"),
                    vec![
                        act(&tr!("select-layer-above"), "layer.select_above").key("Alt+]"),
                        act(&tr!("select-layer-below"), "layer.select_below").key("Alt+["),
                    ],
                ),
                sub(
                    &tr!("layer-order"),
                    vec![
                        act(&tr!("bring-to-front"), "layer.order.front"),
                        act(&tr!("move-up"), "layer.order.up").key("Ctrl+]"),
                        act(&tr!("move-down"), "layer.order.down").key("Ctrl+["),
                        act(&tr!("send-to-back"), "layer.order.back"),
                    ],
                ),
                SEP,
                act(&tr!("transfer-to-lower-layer"), "layer.transfer_down"),
                act(&tr!("merge-with-layer-below"), "layer.merge_down").key("Ctrl+E"),
                act(&tr!("merge-selected-layers"), "layer.merge_selected").key("Shift+Alt+E"),
                act(&tr!("merge-visible-layers"), "layer.merge_visible_here").key("Ctrl+Shift+E"),
                act(&tr!("merge-visible-to-new-layer"), "layer.merge_visible"),
                act(&tr!("flatten-image"), "layer.flatten"),
                SEP,
                act(&tr!("rasterize"), "layer.rasterize"),
                act(&tr!("convert-layer-ellipsis"), "layer.convert"),
                act(
                    &tr!("convert-to-lines-and-tones-ellipsis"),
                    "filter.lines_tones",
                ),
                act(&tr!("create-selection-from-layer"), "select.from_layer"),
            ],
        ),
        Menu::new(
            tr!("select"),
            vec![
                act(&tr!("select-all"), "select.all").key("Ctrl+A"),
                act(&tr!("deselect"), "select.none").key("Ctrl+D"),
                act(&tr!("reselect"), "select.again").key("Ctrl+Shift+D"),
                act(&tr!("invert-selected-area-2"), "select.invert").key("Ctrl+Shift+I"),
                SEP,
                act(&tr!("expand-selected-area-ellipsis"), "select.expand"),
                act(&tr!("shrink-selected-area-ellipsis"), "select.shrink"),
                act(&tr!("blur-border-ellipsis"), "select.feather"),
                act(&tr!("select-color-gamut-ellipsis"), "select.gamut"),
                SEP,
                sub(
                    &tr!("selection-from-layer"),
                    vec![
                        act(&tr!("create-selection"), "select.from_layer"),
                        act(&tr!("add-selection"), "select.add_layer"),
                        act(&tr!("remove-selection"), "select.subtract_layer"),
                    ],
                ),
                act(&tr!("convert-to-selection-layer"), "select.to_layer"),
                act(
                    &tr!("convert-selection-layer-to-selection"),
                    "select.from_selection_layer",
                ),
                act(&tr!("quick-mask"), "select.quick_mask"),
            ],
        ),
        Menu::new(
            tr!("view"),
            vec![
                sub(
                    &tr!("rotate-invert"),
                    vec![
                        act(&tr!("reset-rotation-inversion"), "view.reset_turn"),
                        act(&tr!("reset-rotation-2"), "view.rotate_reset"),
                        act(&tr!("rotate-90"), "view.rotate_90"),
                        act(&tr!("rotate-180"), "view.rotate_180"),
                        act(&tr!("rotate-270"), "view.rotate_270"),
                        SEP,
                        act(&tr!("flip-horizontal"), "view.flip_h"),
                        act(&tr!("flip-vertical"), "view.flip_v"),
                        SEP,
                        act(&tr!("rotate-left-2"), "view.rotate_left").key("-"),
                        act(&tr!("rotate-right-2"), "view.rotate_right").key("^"),
                    ],
                ),
                SEP,
                act(&tr!("zoom-in-2"), "view.zoom_in").key("Ctrl++"),
                act(&tr!("zoom-out-2"), "view.zoom_out").key("Ctrl+-"),
                act("100%", "view.actual").key("Ctrl+Alt+0"),
                act("200%", "view.double"),
                act(&tr!("fit-to-screen"), "view.fit").key("Ctrl+0"),
                act(&tr!("reset-display"), "view.reset_display"),
                act(&tr!("fit-to-navigator"), "view.fit_navigator"),
                act(&tr!("print-size-2"), "view.print_size"),
                SEP,
                act(&tr!("scroll-bar"), "view.scroll_bar"),
                act(&tr!("selection-border"), "view.selection"),
                act(&tr!("selection-launcher"), "select.launcher"),
                SEP,
                act(&tr!("grid"), "view.grid"),
                act(&tr!("ruler-bar"), "view.ruler_bar").key("Ctrl+R"),
                act(&tr!("pixel-grid"), "view.pixel_grid"),
                act(&tr!("crop-marks-inner-border"), "view.crop_frame"),
                act(&tr!("bleed-2"), "view.bleed"),
                act(&tr!("show-page-image"), "view.page_image"),
                act(&tr!("paper-2"), "view.paper"),
                act(&tr!("show-tone-area"), "view.tone_area"),
                SEP,
                sub(
                    &tr!("snap"),
                    vec![
                        act(&tr!("snap-to-ruler"), "view.snap_ruler").key("Ctrl+1"),
                        act(&tr!("snap-to-special-ruler"), "view.snap_special").key("Ctrl+2"),
                        act(&tr!("snap-to-grid"), "view.snap_grid").key("Ctrl+3"),
                    ],
                ),
                sub(
                    &tr!("color-profile-2"),
                    vec![
                        act(&tr!("preview"), "view.profile_preview"),
                        act(&tr!("preview-settings-ellipsis"), "view.profile_settings"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("filter"),
            vec![
                sub(
                    &tr!("blur"),
                    vec![
                        act(&tr!("blur"), "filter.blur"),
                        act(&tr!("blur-strong"), "filter.blur_strong"),
                        act(&tr!("gaussian-blur-ellipsis"), "filter.gaussian"),
                        act(&tr!("motion-blur-ellipsis"), "filter.motion_blur"),
                        act(&tr!("radial-blur-ellipsis"), "filter.radial_blur"),
                        act(&tr!("zoom-blur-ellipsis"), "filter.zoom_blur"),
                        act(&tr!("lens-blur-ellipsis"), "filter.lens_blur"),
                        act(&tr!("smooth"), "filter.smooth"),
                    ],
                ),
                sub(
                    &tr!("correction"),
                    vec![
                        act(&tr!("remove-dust-ellipsis"), "filter.dust"),
                        act(&tr!("remove-jpeg-noise-ellipsis"), "filter.jpeg_noise"),
                        act(&tr!("line-width-ellipsis"), "filter.line_width"),
                    ],
                ),
                sub(
                    &tr!("distort"),
                    vec![
                        act(&tr!("wave-ellipsis"), "filter.wave"),
                        act(&tr!("pinch-ellipsis"), "filter.pinch"),
                        act(&tr!("polar-coordinates-ellipsis"), "filter.polar"),
                        act(&tr!("ripple-ellipsis"), "filter.ripple"),
                        act(&tr!("fisheye-ellipsis"), "filter.fisheye"),
                        act(&tr!("twirl-ellipsis"), "filter.twirl"),
                        act(&tr!("zigzag-ellipsis"), "filter.zigzag"),
                    ],
                ),
                sub(
                    &tr!("effect"),
                    vec![
                        act(&tr!("mosaic-ellipsis"), "filter.mosaic"),
                        act(&tr!("crystallize-ellipsis"), "filter.crystallize"),
                        act(&tr!("add-noise-ellipsis"), "filter.noise"),
                        act(&tr!("line-extraction-ellipsis"), "filter.line_extraction"),
                        act(&tr!("chromatic-aberration-ellipsis"), "filter.aberration"),
                        act(&tr!("retro-ellipsis"), "filter.retro"),
                    ],
                ),
                sub(
                    &tr!("render"),
                    vec![
                        act(&tr!("focus-lines-ellipsis"), "filter.focus_lines"),
                        act(&tr!("speed-lines-ellipsis"), "filter.speed_lines"),
                        act(&tr!("screentone-ellipsis"), "filter.tone"),
                        SEP,
                        act(&tr!("perlin-noise-ellipsis"), "filter.perlin"),
                        act(&tr!("clouds"), "filter.clouds"),
                    ],
                ),
                sub(
                    &tr!("sharpen"),
                    vec![
                        act(&tr!("sharpen"), "filter.sharpen"),
                        act(&tr!("sharpen-strong"), "filter.sharpen_strong"),
                        act(&tr!("unsharp-mask-ellipsis"), "filter.unsharp"),
                    ],
                ),
                SEP,
                act(
                    &tr!("convert-to-lines-and-tones-ellipsis"),
                    "filter.lines_tones",
                ),
            ],
        ),
        Menu::new(
            tr!("window"),
            vec![
                sub(
                    &tr!("canvas"),
                    vec![
                        act(&tr!("next-canvas"), "window.next_tab").key("Ctrl+Tab"),
                        act(&tr!("previous-canvas"), "window.prev_tab"),
                        act(&tr!("new-window"), "window.new"),
                        act(&tr!("cascade"), "window.cascade"),
                        act(&tr!("tile"), "window.tile"),
                    ],
                ),
                sub(
                    &tr!("workspace"),
                    vec![
                        act(&tr!("register-workspace-ellipsis"), "window.workspace.save"),
                        act(
                            &tr!("manage-workspaces-ellipsis"),
                            "window.workspace.manage",
                        ),
                    ],
                ),
                sub(
                    &tr!("interface"),
                    vec![
                        act(
                            &tr!("automatic-follows-the-device"),
                            "window.interface.auto",
                        ),
                        act(&tr!("desktop"), "window.interface.desktop"),
                        act(&tr!("tablet"), "window.interface.tablet"),
                        act(&tr!("phone"), "window.interface.phone"),
                    ],
                ),
                act(&tr!("hide-all-palettes"), "window.hide_all"),
                act(&tr!("hide-menu-bar"), "window.hide_menus"),
                act(&tr!("reset-palette-layout"), "window.workspace.reset"),
                SEP,
                act(&tr!("command-bar"), "window.command_bar"),
                act(&tr!("tool"), "window.tool"),
                act(&tr!("tool-group"), "window.sub_tool"),
                act(&tr!("tool-settings"), "window.tool_property"),
                act(&tr!("tool-sliders"), "window.brush_size"),
                act(&tr!("brush-size"), "window.brush_grid"),
                act(&tr!("color-wheel"), "window.color_wheel"),
                act(&tr!("color-slider"), "window.color_slider"),
                act(&tr!("color-set"), "window.color_set"),
                act(&tr!("color-history"), "window.color_history"),
                act(&tr!("intermediate-color"), "window.intermediate"),
                act(&tr!("approximate-color"), "window.approximate"),
                act(&tr!("navigator"), "window.navigator"),
                act(&tr!("sub-view"), "window.sub_view"),
                act(&tr!("layer-2"), "window.layer"),
                act(&tr!("layer-properties"), "window.layer_property"),
                act(&tr!("history"), "window.history"),
                act(&tr!("timeline"), "window.timeline"),
                act(&tr!("animation-cels"), "window.cels"),
                act(&tr!("material"), "window.material"),
                act(&tr!("auto-action-palette"), "window.auto_actions"),
                act(&tr!("item-bank"), "window.item_bank"),
                act(&tr!("information"), "window.info"),
                SEP,
                act(&tr!("back-to-the-hub"), "file.leave"),
            ],
        ),
        Menu::new(
            tr!("help"),
            vec![
                act(&tr!("sue-paint-guide"), "help.guide"),
                act(&tr!("keyboard-shortcuts"), "help.shortcuts"),
                act(&tr!("check-for-updates"), "help.updates"),
                SEP,
                act(&tr!("about-sue-paint"), "help.about"),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_has_every_menu_and_commands_are_namespaced() {
        let menus = menus();
        let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "File",
                "Edit",
                "Story",
                "Animation",
                "Layer",
                "Select",
                "View",
                "Filter",
                "Window",
                "Help"
            ]
        );
        for menu in &menus {
            for item in &menu.items {
                for command in item.commands() {
                    assert!(command.contains('.'), "{command} has no area");
                }
            }
        }
    }
}
