//! Sue Paint's menu bar, as data. Every item names a command; the studio
//! grays out the ones it cannot run yet, so the menus show the whole plan.
//!
//! The menus hold the tree of command names. A command's label comes from
//! the registry ([`crate::commands`]), its key from the settings (the
//! studio writes them in each frame), its state from the registry too.
//! Items whose submenu says the rest (".png (PNG)..." under Save
//! Duplicate) keep a shorter label of their own, and the Window menu's
//! palettes read their palette's title. The menus follow Clip Studio
//! Paint's order; every label is Title Case.

use sue_wm::{Menu, MenuItem};

use crate::commands::registry;
use crate::dock::Palette;

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
            named(&name, &format!("file.recent.open.{index}"))
        })
        .collect();
    if !items.is_empty() {
        items.push(SEP);
    }
    items.push(item("file.recent.clear"));
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

/// The items of Window, Workspace in Clip Studio Paint's order: Reset to
/// Default, Register Workspace, Manage Workspaces, Sue's Use Current
/// Workspace as Default, Reload with the name of the workspace in use
/// (`current`), then the workspaces Sue Paint ships and the registered
/// ones (`names`), the one in use checked by its state.
#[must_use]
pub fn workspace_items(names: &[String], current: Option<&str>) -> Vec<MenuItem> {
    let mut items = vec![
        item("window.workspace.reset"),
        item("window.workspace.save"),
        item("window.workspace.manage"),
        item("window.workspace.default"),
        match current {
            Some(name) => named(
                &tr!("reload-workspace", name = name),
                "window.workspace.reload",
            ),
            None => item("window.workspace.reload"),
        },
        SEP,
    ];
    items.extend(crate::extras::PRESETS.iter().map(|preset| {
        named(
            &sue_lang::text_or("paint", preset.label, preset.id),
            &format!("window.workspace.preset.{}", preset.id),
        )
    }));
    if !names.is_empty() {
        items.push(SEP);
    }
    items.extend(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| named(name, &format!("window.workspace.use.{index}"))),
    );
    items
}

/// Shows the registered workspaces `names` in Window, Workspace.
#[cfg(test)]
pub fn set_workspaces(menus: &mut [Menu], names: &[String]) {
    set_workspaces_in_use(menus, names, None);
}

/// Shows the registered workspaces `names` in Window, Workspace, with
/// `current` the one in use.
pub fn set_workspaces_in_use(menus: &mut [Menu], names: &[String], current: Option<&str>) {
    let items = workspace_items(names, current);
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

/// The items of Window, Layout Style: every style of the registry, the
/// one in use checked and the ones not built yet grayed by their state.
#[must_use]
pub fn layout_style_items() -> Vec<MenuItem> {
    crate::layout_style::STYLES
        .iter()
        .map(|style| {
            named(
                &crate::layout_style::label(style),
                &format!("{}{}", crate::layout_style::COMMAND_PREFIX, style.id),
            )
        })
        .collect()
}

/// Gives the item for `command` the label `label` (Hide All Palettes
/// reads Show All Palettes while they are hidden); `false` when it
/// already had it.
pub fn set_label(menus: &mut [Menu], command: &str, label: &str) -> bool {
    fn walk(items: &mut [MenuItem], command: &str, label: &str) -> Option<bool> {
        items.iter_mut().find_map(|item| match item {
            MenuItem::Action {
                label: now,
                command: found,
                ..
            } if found == command => {
                let changed = now != label;
                label.clone_into(now);
                Some(changed)
            }
            MenuItem::Sub { items, .. } => walk(items, command, label),
            _ => None,
        })
    }
    menus
        .iter_mut()
        .find_map(|menu| walk(&mut menu.items, command, label))
        .unwrap_or(false)
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

/// An item for `command`, labeled as the registry labels it.
fn item(command: &str) -> MenuItem {
    let label = registry()
        .label(command)
        .unwrap_or_else(|| command.to_owned());
    MenuItem::action(label, command)
}

/// Why Pick Screen Color is grayed where the system lets no program
/// read the screen; nothing where it works.
fn screen_note() -> String {
    if sue_platform::screen::available() {
        String::new()
    } else {
        tr!("pick-screen-color-unavailable")
    }
}

/// An item for `command` with a label of its own: a recent file, a
/// workspace, or a submenu entry whose title says the rest.
fn named(label: &str, command: &str) -> MenuItem {
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

/// The palettes of the Window menu, in Clip Studio Paint's order, one
/// group between each two lines: the command that shows each and the
/// palette it shows. The item reads the palette's own title, so the menu,
/// the palette's title, its tab and its tooltip say the same.
const WINDOW_PALETTES: [&[(&str, Palette)]; 5] = [
    &[
        ("window.command_bar", Palette::CommandBar),
        ("window.quick_access", Palette::QuickAccess),
        ("window.tool", Palette::Tools),
        ("window.sub_tool", Palette::SubTool),
        ("window.tool_property", Palette::ToolProperty),
        ("window.brush_grid", Palette::BrushGrid),
        ("window.brush_size", Palette::BrushSize),
        ("window.sub_tool_detail", Palette::SubToolDetail),
    ],
    &[
        ("window.color_wheel", Palette::Color),
        ("window.color_slider", Palette::ColorSlider),
        ("window.color_set", Palette::ColorSet),
        ("window.intermediate", Palette::Intermediate),
        ("window.approximate", Palette::Approximate),
        ("window.color_history", Palette::ColorHistory),
    ],
    &[
        ("window.layer", Palette::Layer),
        ("window.layer_property", Palette::LayerProperty),
        ("window.cels", Palette::Cels),
        ("window.timeline", Palette::Timeline),
    ],
    &[
        ("window.navigator", Palette::Navigator),
        ("window.sub_view", Palette::SubView),
    ],
    &[
        ("window.history", Palette::History),
        ("window.auto_actions", Palette::AutoAction),
        ("window.info", Palette::Info),
        ("window.item_bank", Palette::ItemBank),
        ("window.material", Palette::Material),
    ],
];

/// Every palette command of the Window menu with the palette it shows.
#[cfg(test)]
fn window_palettes() -> impl Iterator<Item = (&'static str, Palette)> {
    WINDOW_PALETTES
        .iter()
        .flat_map(|group| group.iter().copied())
}

/// The Window menu's palette items, a line between each two groups.
fn palette_items() -> Vec<MenuItem> {
    let mut items = Vec::new();
    for group in WINDOW_PALETTES {
        items.push(SEP);
        items.extend(
            group
                .iter()
                .map(|(command, palette)| named(&palette.title(), command)),
        );
    }
    items
}

/// The items of Window, Canvas: the commands, then a numbered item for
/// each of the open canvases `names` (Clip Studio Paint's list, the
/// current one checked by its state).
#[must_use]
pub fn canvas_items(names: &[String]) -> Vec<MenuItem> {
    let mut items = vec![
        item("window.new"),
        item("window.cascade"),
        item("window.tile"),
        item("window.float"),
        SEP,
        item("window.next_tab"),
        item("window.prev_tab"),
    ];
    if !names.is_empty() {
        items.push(SEP);
    }
    items.extend(names.iter().enumerate().map(|(index, name)| {
        named(
            &tr!("numbered-canvas", number = index + 1, name = name.as_str()),
            &format!("window.canvas.{index}"),
        )
    }));
    items
}

/// Shows the open canvases `names` in Window, Canvas; `false` when the
/// menu already showed them.
pub fn set_canvases(menus: &mut [Menu], names: &[String]) -> bool {
    let items = canvas_items(names);
    for menu in menus.iter_mut().filter(|menu| menu.title == tr!("window")) {
        for item in &mut menu.items {
            if let MenuItem::Sub {
                label,
                items: inside,
            } = item
                && *label == tr!("canvas")
            {
                if *inside == items {
                    return false;
                }
                *inside = items;
                return true;
            }
        }
    }
    false
}

/// The menus, left to right.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the whole menu bar as one table")]
pub fn menus() -> Vec<Menu> {
    let mut window = vec![
        sub(&tr!("canvas"), canvas_items(&[])),
        sub(&tr!("layout-style"), layout_style_items()),
        sub(&tr!("workspace"), workspace_items(&[], None)),
    ];
    window.extend(palette_items());
    // The interface (desktop, tablet, phone) is Sue's, in the Sue menu
    // and Settings: one "Layout" menu here would sit beside another.
    window.extend([SEP, item("window.hide_all"), item("window.hide_menus")]);
    vec![
        Menu::new(
            tr!("file"),
            vec![
                item("file.new"),
                item("file.new_clipboard"),
                named(&tr!("open-ellipsis"), "file.open"),
                sub(&tr!("open-recent"), vec![item("file.recent.clear")]),
                SEP,
                item("file.close"),
                item("file.save"),
                item("file.save_as"),
                item("file.export.clip_back"),
                sub(
                    &tr!("save-duplicate"),
                    vec![
                        item("file.duplicate.clip"),
                        item("file.export.bmp"),
                        item("file.export.jpg"),
                        named(&tr!("png-png-ellipsis"), "file.export.png"),
                        item("file.export.tif"),
                        item("file.export.tga"),
                        item("file.export.webp"),
                        item("file.export.psb"),
                        item("file.export.psd"),
                        item("file.export.jxl"),
                        item("file.duplicate.gif"),
                        item("file.duplicate.qoi"),
                        item("file.duplicate.ico"),
                        item("file.duplicate.ppm"),
                    ],
                ),
                item("file.revert"),
                item("file.show_in_folder"),
                SEP,
                sub(
                    &tr!("export-single-layer"),
                    vec![
                        item("file.export.bmp"),
                        item("file.export.jpg"),
                        named(&tr!("png-png-ellipsis"), "file.export.png"),
                        item("file.export.tif"),
                        item("file.export.tga"),
                        item("file.export.webp"),
                        item("file.export.psb"),
                        item("file.export.psd"),
                        item("file.export.jxl"),
                        item("file.duplicate.gif"),
                        item("file.duplicate.qoi"),
                        item("file.duplicate.ico"),
                        item("file.duplicate.ppm"),
                    ],
                ),
                sub(
                    &tr!("export-animation"),
                    vec![
                        item("file.export.sequence"),
                        item("file.export.gif"),
                        item("file.export.apng"),
                        item("file.export.webp_animation"),
                        item("file.export.movie"),
                        item("file.export.audio"),
                        item("file.export.cels"),
                        item("file.export.exposure_sheet"),
                    ],
                ),
                sub(
                    &tr!("export-multiple-pages"),
                    vec![
                        item("file.export.batch"),
                        item("file.export.pdf"),
                        item("file.export.epub"),
                        SEP,
                        item("story.preview_binding"),
                    ],
                ),
                sub(
                    &tr!("special-export"),
                    vec![
                        item("file.export.webtoon"),
                        item("file.export.vectors"),
                        item("file.export.layer_comps"),
                    ],
                ),
                item("file.batch_convert"),
                SEP,
                sub(
                    &tr!("timelapse"),
                    vec![item("file.timelapse.record"), item("file.timelapse.export")],
                ),
                SEP,
                sub(
                    &tr!("import"),
                    vec![
                        item("file.import.image"),
                        item("file.import.layers"),
                        item("file.import.batch"),
                        item("file.import.exposure_sheet"),
                        item("file.import.pattern"),
                        item("file.import.vector"),
                        item("file.import.audio"),
                        item("file.import.movie"),
                        item("file.import.3d"),
                        SEP,
                        item("file.import.pose_scanner"),
                    ],
                ),
                SEP,
                item("file.print_settings"),
                item("file.print"),
                SEP,
                item("file.preferences"),
                item("file.command_bar"),
                item("file.shortcuts"),
                item("file.modifiers"),
                SEP,
                item("file.fonts"),
                SEP,
                item("file.pressure"),
                SEP,
                item("file.leave"),
                item("app.quit"),
            ],
        ),
        Menu::new(
            tr!("edit"),
            vec![
                item("edit.undo"),
                item("edit.redo"),
                SEP,
                item("edit.cut"),
                item("edit.copy"),
                item("edit.copy_svg"),
                item("edit.paste"),
                item("edit.paste_here"),
                item("edit.cut_paste"),
                item("edit.copy_paste"),
                item("edit.clear"),
                item("edit.clear_outside"),
                item("edit.smart_smoothing"),
                item("edit.fill"),
                item("edit.advanced_fill"),
                item("edit.colorize"),
                item("edit.recolor"),
                item("edit.outline_selection"),
                item("edit.luma_to_alpha"),
                SEP,
                sub(
                    &tr!("tonal-correction"),
                    vec![
                        item("filter.brightness"),
                        item("filter.levels"),
                        item("filter.curve"),
                        item("filter.hue"),
                        item("filter.balance"),
                        item("filter.invert"),
                        item("filter.posterize"),
                        item("filter.binarize"),
                        item("filter.gradient_map"),
                        item("filter.grayscale"),
                    ],
                ),
                sub(
                    &tr!("transform"),
                    vec![
                        item("edit.transform.scale"),
                        item("edit.transform.scale_only"),
                        item("edit.transform.rotate"),
                        SEP,
                        item("edit.transform.free"),
                        item("edit.transform.distort"),
                        item("edit.transform.skew"),
                        item("edit.transform.perspective"),
                        SEP,
                        item("edit.flip_h"),
                        item("edit.flip_v"),
                        item("edit.rotate180"),
                        SEP,
                        item("edit.transform.mesh"),
                        item("edit.transform.puppet"),
                    ],
                ),
                sub(
                    &tr!("align-distribute"),
                    vec![
                        item("edit.align.left"),
                        item("edit.align.hcenter"),
                        item("edit.align.right"),
                        SEP,
                        item("edit.align.top"),
                        item("edit.align.vcenter"),
                        item("edit.align.bottom"),
                    ],
                ),
                SEP,
                item("edit.resolution"),
                item("edit.canvas_size"),
                item("edit.canvas_height"),
                item("edit.crop"),
                sub(
                    &tr!("rotate-flip-canvas"),
                    vec![
                        item("canvas.rotate_left"),
                        item("canvas.rotate_right"),
                        item("canvas.rotate180"),
                        item("canvas.flip_h"),
                        item("canvas.flip_v"),
                    ],
                ),
                item("edit.canvas_properties"),
                sub(&tr!("clear-work-history"), vec![item("edit.clear_history")]),
                SEP,
                item("edit.brush_tip"),
                sub(
                    &tr!("register-material"),
                    vec![item("edit.material.image"), item("edit.material.tip")],
                ),
                SEP,
                item("edit.screen_color").note(screen_note()),
                item("edit.screen_color_hidden").note(screen_note()),
            ],
        ),
        Menu::new(
            tr!("story"),
            vec![
                item("story.manager"),
                SEP,
                item("story.first"),
                item("story.previous"),
                item("story.next"),
                item("story.last"),
                SEP,
                item("story.add"),
                item("story.insert"),
                item("story.delete"),
                item("story.order"),
                SEP,
                item("story.combine"),
                item("story.separate"),
                SEP,
                item("story.work_settings"),
                item("story.page_settings"),
                item("story.info"),
                sub(
                    &tr!("webtoon-reading-direction"),
                    vec![
                        item("story.webtoon.right_to_left"),
                        item("story.webtoon.left_to_right"),
                    ],
                ),
                SEP,
                item("story.editor"),
                item("story.import_text"),
                sub(
                    &tr!("page-management"),
                    vec![
                        item("story.open_page"),
                        item("story.duplicate_page"),
                        item("story.export_page"),
                        item("story.export_pages"),
                        SEP,
                        item("story.preview_binding"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("animation"),
            vec![
                sub(
                    &tr!("new-animation-layer"),
                    vec![
                        item("anim.layer.folder"),
                        item("anim.layer.camera"),
                        item("file.import.audio"),
                    ],
                ),
                item("anim.cel.new"),
                sub(
                    &tr!("edit-track"),
                    vec![
                        item("anim.edit.delete"),
                        item("anim.edit.cut"),
                        item("anim.edit.copy"),
                        item("anim.edit.paste"),
                        SEP,
                        item("anim.keyframes"),
                        item("anim.key.layer"),
                        item("anim.key.ease.hold"),
                        item("anim.key.ease.linear"),
                        item("anim.key.ease.smooth"),
                        item("anim.key.edit_keyed"),
                        item("anim.key.delete_all"),
                        SEP,
                        item("anim.cel.assign"),
                        item("anim.cel.assign_many"),
                        item("anim.cel.create_supported"),
                        item("anim.cel.rename_timeline"),
                        item("anim.cel.rename_layer"),
                        SEP,
                        item("anim.cel.prev"),
                        item("anim.cel.next"),
                        SEP,
                        item("anim.key.add"),
                        item("anim.key.delete"),
                        item("anim.key.layer_delete"),
                        SEP,
                        item("anim.clip.merge"),
                        item("anim.clip.split"),
                        item("anim.clip.first"),
                        item("anim.clip.last"),
                        SEP,
                        item("anim.frame.insert"),
                        item("anim.frame.delete"),
                    ],
                ),
                SEP,
                item("anim.play"),
                sub(
                    &tr!("playback-settings"),
                    vec![
                        item("anim.loop"),
                        item("anim.audio.scrub"),
                        item("anim.playback.camera"),
                    ],
                ),
                sub(
                    &tr!("move-frame"),
                    vec![
                        item("anim.first"),
                        item("anim.prev"),
                        item("anim.next"),
                        item("anim.last"),
                    ],
                ),
                sub(
                    &tr!("timeline-menu-label"),
                    vec![
                        item("anim.label"),
                        SEP,
                        item("anim.track_note"),
                        item("anim.track_label.inbetween"),
                        item("anim.track_label.inbetween-done"),
                        item("anim.track_label.reverse-sheet"),
                        item("anim.track_label.none"),
                    ],
                ),
                SEP,
                sub(
                    &tr!("timeline"),
                    vec![
                        item("anim.timeline.enable"),
                        item("anim.timeline.new"),
                        item("anim.timeline.prev"),
                        item("anim.timeline.next"),
                        item("anim.timeline.edit"),
                        item("anim.exposure_sheet"),
                    ],
                ),
                sub(
                    &tr!("show-animation-cels"),
                    vec![
                        item("anim.cel.edited_only"),
                        item("anim.onion"),
                        item("anim.onion_settings"),
                        item("anim.cel.check_motion"),
                        item("anim.cel.check_surrounding"),
                    ],
                ),
                sub(
                    &tr!("light-table"),
                    vec![
                        item("anim.light_table"),
                        SEP,
                        item("anim.light_table.register"),
                        item("anim.light_table.clear"),
                    ],
                ),
            ],
        ),
        Menu::new(
            tr!("layer-2"),
            vec![
                item("layer.new.raster_top"),
                sub(
                    &tr!("new-layer"),
                    vec![
                        item("layer.new"),
                        item("layer.new.vector"),
                        item("layer.new.text"),
                        item("layer.new.balloon"),
                        item("layer.new.gradient"),
                        item("layer.new.fill"),
                        item("layer.new.tone"),
                        item("layer.new.paper"),
                        SEP,
                        item("layer.new.3d"),
                        item("layer.new.figure"),
                        item("layer.new.hand"),
                        item("layer.new.head"),
                        item("layer.new.frame"),
                    ],
                ),
                sub(
                    &tr!("new-correction-layer"),
                    vec![
                        item("layer.correction.brightness"),
                        item("layer.correction.levels"),
                        item("layer.correction.curve"),
                        item("layer.correction.hue"),
                        item("layer.correction.balance"),
                        item("layer.correction.invert"),
                        item("layer.correction.posterize"),
                        item("layer.correction.binarize"),
                        item("layer.correction.gradient_map"),
                    ],
                ),
                item("layer.folder_insert"),
                item("layer.new.folder"),
                item("layer.ungroup"),
                item("layer.from_selection"),
                SEP,
                item("layer.duplicate"),
                item("layer.delete"),
                sub(
                    &tr!("delete-layers"),
                    vec![item("layer.delete_empty"), item("layer.delete_hidden")],
                ),
                item("layer.hide_drafts"),
                SEP,
                sub(
                    &tr!("layer-mask"),
                    vec![
                        item("layer.mask.outside"),
                        item("layer.mask.inside"),
                        item("layer.mask.delete"),
                        item("layer.mask.apply"),
                        SEP,
                        item("layer.mask.toggle"),
                        item("layer.mask.edit"),
                    ],
                ),
                sub(
                    &tr!("ruler-frame"),
                    vec![
                        item("layer.ruler.delete"),
                        item("layer.ruler.show"),
                        SEP,
                        item("layer.ruler.perspective"),
                        item("layer.ruler.symmetry"),
                        item("layer.ruler.circle"),
                        SEP,
                        item("layer.frame.divide_equally"),
                        item("layer.frame.combine"),
                    ],
                ),
                SEP,
                sub(
                    &tr!("layer-settings"),
                    vec![
                        item("layer.toggle.visible"),
                        item("layer.rename"),
                        item("layer.toggle.clip"),
                        item("layer.toggle.reference"),
                        item("layer.toggle.draft"),
                        item("layer.toggle.lock"),
                        item("layer.toggle.lock_alpha"),
                        SEP,
                        item("layer.toggle.layer_color"),
                    ],
                ),
                sub(
                    &tr!("change-palette-color"),
                    vec![
                        item("layer.palette_color.none"),
                        SEP,
                        item("layer.palette_color.red"),
                        item("layer.palette_color.orange"),
                        item("layer.palette_color.yellow"),
                        item("layer.palette_color.green"),
                        item("layer.palette_color.blue"),
                        item("layer.palette_color.purple"),
                    ],
                ),
                item("layer.properties"),
                item("layer.comps"),
                SEP,
                sub(
                    &tr!("change-selected-layer"),
                    vec![
                        item("layer.select_above"),
                        item("layer.select_below"),
                        item("layer.select_all"),
                        item("layer.select_none"),
                    ],
                ),
                sub(
                    &tr!("layer-order"),
                    vec![
                        item("layer.order.front"),
                        item("layer.order.up"),
                        item("layer.order.down"),
                        item("layer.order.back"),
                    ],
                ),
                SEP,
                item("layer.transfer_down"),
                item("layer.merge_down"),
                item("layer.merge_selected"),
                item("layer.merge_visible_here"),
                item("layer.merge_visible"),
                item("layer.flatten"),
                SEP,
                item("layer.rasterize"),
                item("layer.convert"),
                item("filter.lines_tones"),
                item("layer.lines_tones_vector"),
                item("layer.update_lines"),
                item("select.from_layer"),
            ],
        ),
        Menu::new(
            tr!("select"),
            vec![
                item("select.all"),
                item("select.none"),
                item("select.again"),
                item("select.invert"),
                SEP,
                item("select.expand"),
                item("select.shrink"),
                item("select.feather"),
                item("select.gamut"),
                SEP,
                sub(
                    &tr!("selection-from-layer"),
                    vec![
                        named(&tr!("create-selection"), "select.from_layer"),
                        item("select.add_layer"),
                        item("select.subtract_layer"),
                        item("layer.selection.overlap"),
                    ],
                ),
                item("select.to_layer"),
                item("select.from_selection_layer"),
                item("select.vectors_overlapping"),
                item("select.vectors_within"),
                item("select.quick_mask"),
            ],
        ),
        Menu::new(
            tr!("view"),
            vec![
                sub(
                    &tr!("rotate-flip"),
                    vec![
                        item("view.reset_turn"),
                        item("view.rotate_reset"),
                        item("view.rotate_90"),
                        item("view.rotate_180"),
                        item("view.rotate_270"),
                        SEP,
                        item("view.flip_h"),
                        item("view.flip_v"),
                        SEP,
                        item("view.rotate_left"),
                        item("view.rotate_right"),
                    ],
                ),
                SEP,
                item("view.zoom_in"),
                item("view.zoom_out"),
                item("view.actual"),
                item("view.double"),
                item("view.fit"),
                item("view.reset_display"),
                item("view.fit_navigator"),
                item("view.print_size"),
                SEP,
                item("view.scroll_bar"),
                item("view.selection"),
                item("select.launcher"),
                item("view.transform_launcher"),
                item("view.text_launcher"),
                item("view.object_launcher"),
                SEP,
                item("view.grid"),
                item("view.grid_settings"),
                item("view.ruler_bar"),
                item("view.pixel_grid"),
                item("view.crop_frame"),
                item("view.bleed"),
                item("view.page_image"),
                item("view.safety_margin"),
                item("view.paper"),
                item("view.tone_area"),
                sub(
                    &tr!("show-vector-paths"),
                    vec![
                        item("view.vector_paths.selected"),
                        item("view.vector_paths.all"),
                        SEP,
                        item("view.vector_paths.ends"),
                        item("view.vector_paths.points"),
                    ],
                ),
                SEP,
                item("view.playback_fps"),
                SEP,
                sub(
                    &tr!("snap"),
                    vec![
                        item("view.snap_ruler"),
                        item("view.snap_special"),
                        item("view.snap_grid"),
                        item("view.special_ruler_snapping"),
                    ],
                ),
                sub(
                    &tr!("color-profile-2"),
                    vec![item("view.profile_preview"), item("view.profile_settings")],
                ),
            ],
        ),
        Menu::new(
            tr!("filter"),
            vec![
                sub(
                    &tr!("quick-filters"),
                    vec![item("filter.quick.layer"), item("filter.quick.all")],
                ),
                sub(
                    &tr!("quick-effects"),
                    vec![item("filter.effects.layer"), item("filter.effects.all")],
                ),
                SEP,
                sub(
                    &tr!("blur"),
                    vec![
                        item("filter.blur"),
                        item("filter.blur_strong"),
                        item("filter.gaussian"),
                        item("filter.motion_blur"),
                        item("filter.radial_blur"),
                        item("filter.zoom_blur"),
                        item("filter.lens_blur"),
                        item("filter.smooth"),
                    ],
                ),
                sub(
                    &tr!("correction"),
                    vec![
                        item("filter.dust"),
                        item("filter.jpeg_noise"),
                        item("filter.line_width"),
                    ],
                ),
                sub(
                    &tr!("distort"),
                    vec![
                        item("filter.curved_surface"),
                        item("filter.wave"),
                        item("filter.pinch"),
                        item("filter.polar"),
                        item("filter.ripple"),
                        item("filter.fisheye"),
                        item("filter.twirl"),
                        item("filter.zigzag"),
                    ],
                ),
                sub(
                    &tr!("effect"),
                    vec![
                        item("filter.artistic"),
                        item("filter.mosaic"),
                        item("filter.crystallize"),
                        item("filter.noise"),
                        item("filter.line_extraction"),
                        item("filter.aberration"),
                        item("filter.glow"),
                        item("filter.inflate"),
                        item("filter.drop_shadow"),
                        item("filter.retro"),
                    ],
                ),
                sub(
                    &tr!("render"),
                    vec![
                        item("filter.focus_lines"),
                        item("filter.speed_lines"),
                        item("filter.tone"),
                        SEP,
                        item("filter.perlin"),
                        item("filter.clouds"),
                    ],
                ),
                sub(
                    &tr!("sharpen"),
                    vec![
                        item("filter.sharpen"),
                        item("filter.sharpen_strong"),
                        item("filter.unsharp"),
                    ],
                ),
                SEP,
                item("filter.lines_tones"),
            ],
        ),
        Menu::new(tr!("window"), window),
        Menu::new(
            tr!("help"),
            vec![
                item("help.guide"),
                item("help.online_guide"),
                item("help.tips"),
                item("help.faq"),
                SEP,
                item("help.shortcuts"),
                SEP,
                item("help.contact"),
                item("help.feedback"),
                SEP,
                item("help.updates"),
                SEP,
                item("help.tips"),
                item("help.online_guide"),
                item("help.faq"),
                item("help.contact"),
                item("help.feedback"),
                SEP,
                item("help.about"),
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

    /// Every label of `items` and of the menus inside them, with where it
    /// sits.
    fn labels(items: &[MenuItem], path: &str, out: &mut Vec<(String, String)>) {
        for item in items {
            match item {
                MenuItem::Action { label, command, .. } => {
                    out.push((format!("{path} > {command}"), label.clone()));
                }
                MenuItem::Sub { label, items } => {
                    out.push((format!("{path} > {label}"), label.clone()));
                    labels(items, &format!("{path} > {label}"), out);
                }
                MenuItem::Separator => {}
            }
        }
    }

    fn every_label() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for menu in menus() {
            out.push((menu.title.clone(), menu.title.clone()));
            labels(&menu.items, &menu.title, &mut out);
        }
        out
    }

    /// Words Title Case keeps small inside a label.
    const SMALL: [&str; 18] = [
        "a", "an", "and", "as", "at", "by", "for", "from", "in", "into", "of", "on", "or", "per",
        "the", "to", "with", "vs",
    ];

    #[test]
    fn every_label_is_title_case() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let mut wrong = Vec::new();
        for (place, label) in every_label() {
            let words: Vec<&str> = label
                .split(|c: char| c.is_whitespace() || c == '/' || c == '(')
                .filter(|word| !word.is_empty())
                .collect();
            for (at, word) in words.iter().enumerate() {
                let Some(first) = word.chars().next() else {
                    continue;
                };
                let small = SMALL.contains(&word.trim_end_matches("..."));
                if first.is_lowercase() && (at == 0 || !small) {
                    wrong.push(format!("{place}: {label:?}"));
                    break;
                }
            }
        }
        assert!(wrong.is_empty(), "not Title Case: {wrong:#?}");
    }

    #[test]
    fn the_window_menu_names_each_palette_by_its_title() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let mut titles = std::collections::BTreeSet::new();
        for (command, palette) in window_palettes() {
            let title = palette.title();
            assert_eq!(label_of(&menus, command), Some(title.as_str()), "{command}");
            assert_eq!(
                registry().label(command).as_deref(),
                Some(title.as_str()),
                "{command}: the registry's label is the palette's title"
            );
            assert!(
                titles.insert(title.clone()),
                "two palettes are called {title}"
            );
        }
        // Clip Studio Paint's names, not the older Sue ones.
        for (command, title) in [
            ("window.sub_tool", "Sub Tool"),
            ("window.tool_property", "Tool Property"),
            ("window.brush_grid", "Brush Size"),
            ("window.layer_property", "Layer Property"),
            ("window.auto_actions", "Auto Action"),
            ("window.info", "Information"),
        ] {
            assert_eq!(label_of(&menus, command), Some(title), "{command}");
        }
    }

    #[test]
    fn the_canvas_list_follows_the_open_canvases() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let mut menus = menus();
        let two = ["Illustration".to_owned(), "Comic".to_owned()];
        assert!(set_canvases(&mut menus, &two));
        // Numbered, so two windows of one canvas read apart.
        assert_eq!(label_of(&menus, "window.canvas.0"), Some("1 Illustration"));
        assert_eq!(label_of(&menus, "window.canvas.1"), Some("2 Comic"));
        assert!(
            !set_canvases(&mut menus, &two),
            "the same canvases change nothing"
        );
        assert!(set_canvases(&mut menus, &[]));
        assert_eq!(label_of(&menus, "window.canvas.0"), None);
        // Clip Studio Paint's order: New Window, Cascade, Tile, then Sue's
        // canvas in a window of its own, then the next and previous
        // canvas.
        let items = canvas_items(&[]);
        let names: Vec<&str> = items.iter().flat_map(MenuItem::commands).collect();
        assert_eq!(
            names,
            [
                "window.new",
                "window.cascade",
                "window.tile",
                "window.float",
                "window.next_tab",
                "window.prev_tab"
            ]
        );
    }

    /// The skeleton every studio shares (D11, [`sue_wm::Skeleton`]) is in
    /// Sue Paint's menus, each item in its menu. Sue Paint keeps Clip
    /// Studio Paint's bar (its own menus before View) and names three
    /// items as Clip Studio does: Export is the export submenus,
    /// Settings is Preferences and Select All lives in Select.
    #[test]
    fn the_menus_hold_the_skeleton_every_studio_shares() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let in_menu = |title: &str, command: &str| {
            let menu = menus
                .iter()
                .find(|menu| menu.title == title)
                .unwrap_or_else(|| panic!("no {title} menu"));
            menu.items
                .iter()
                .any(|item| item.commands().contains(&command))
        };
        for slot in sue_wm::Slot::ALL {
            let (title, command) = match (slot, slot.command()) {
                (sue_wm::Slot::Recent, _) => {
                    assert!(label_of(&menus, "file.recent.clear").is_some());
                    continue;
                }
                (sue_wm::Slot::Export, _) => ("File", "file.export.png"),
                (sue_wm::Slot::Settings, _) => ("File", "file.preferences"),
                (sue_wm::Slot::SelectAll, _) => ("Select", "select.all"),
                (_, Some(command)) => (
                    match command.split('.').next() {
                        Some("file" | "app") => "File",
                        Some("edit") => "Edit",
                        _ => "Help",
                    },
                    command,
                ),
                (_, None) => unreachable!("only Open Recent runs no command"),
            };
            assert!(in_menu(title, command), "{slot:?}: {command} in {title}");
        }
    }

    #[test]
    fn the_file_menu_keeps_clip_studio_paints_order() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let order: Vec<String> = commands(&menus[..1])
            .into_iter()
            .map(|(_, command)| command)
            .collect();
        let at = |command: &str| {
            order
                .iter()
                .position(|found| found == command)
                .unwrap_or_else(|| panic!("{command} is not in File"))
        };
        let chain = [
            "file.new",
            "file.open",
            "file.close",
            "file.save",
            "file.save_as",
            "file.revert",
            "file.export.sequence",
            "file.export.webtoon",
            "file.timelapse.record",
            "file.import.image",
            "file.import.pose_scanner",
            "file.print",
            "file.preferences",
            "file.command_bar",
            "file.shortcuts",
            "file.modifiers",
            "file.fonts",
            "file.pressure",
            "app.quit",
        ];
        for pair in chain.windows(2) {
            assert!(at(pair[0]) < at(pair[1]), "{} before {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn the_animation_menu_has_each_transport_command_once() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let animation = menus
            .iter()
            .find(|menu| menu.title == "Animation")
            .expect("an Animation menu");
        let names: Vec<&str> = animation
            .items
            .iter()
            .flat_map(MenuItem::commands)
            .collect();
        for command in [
            "anim.play",
            "anim.first",
            "anim.prev",
            "anim.next",
            "anim.last",
            "anim.cel.prev",
            "anim.cel.next",
            "anim.cel.rename_timeline",
            "anim.cel.rename_layer",
        ] {
            assert_eq!(
                names.iter().filter(|name| **name == command).count(),
                1,
                "{command}"
            );
        }
        // Clip Studio Paint's order: New Animation Layer first, Timeline
        // after the transport.
        let top: Vec<&str> = animation.items.iter().map(MenuItem::label).collect();
        assert_eq!(top.first(), Some(&"New Animation Layer"));
        let play = top.iter().position(|label| *label == "Play/Stop");
        let timeline = top.iter().position(|label| *label == "Timeline");
        assert!(play.is_some() && play < timeline, "{top:?}");
    }

    #[test]
    fn the_labels_come_from_the_language_in_use() {
        let english: Vec<String> = {
            let _english = sue_lang::scope(sue_lang::ENGLISH);
            every_label().into_iter().map(|(_, label)| label).collect()
        };
        let german: Vec<String> = {
            let _german = sue_lang::scope("de");
            every_label().into_iter().map(|(_, label)| label).collect()
        };
        assert_eq!(
            english.len(),
            german.len(),
            "the same tree in every language"
        );
        let _german = sue_lang::scope("de");
        let menus = menus();
        let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(titles[0], "Datei");
        assert_eq!(titles[1], "Bearbeiten");
        assert_eq!(
            label_of(&menus, "edit.undo"),
            registry().label("edit.undo").as_deref()
        );
        assert_ne!(label_of(&menus, "edit.undo"), Some("Undo"));
        // The Window menu reads the palettes' titles in German too.
        for (command, palette) in window_palettes() {
            assert_eq!(
                label_of(&menus, command),
                Some(palette.title().as_str()),
                "{command}"
            );
        }
        let translated = english
            .iter()
            .zip(&german)
            .filter(|(english, german)| english != german)
            .count();
        assert!(
            translated * 2 > english.len(),
            "most labels are German: {translated} of {}",
            english.len()
        );
    }
}
