//! Sue Paint's menu bar, as data. Every item names a command; the studio
//! grays out the ones it cannot run yet, so the menus show the whole plan.
//!
//! The menus hold the tree of command names. A command's label comes from
//! the registry ([`crate::commands`]), its key from the settings (the
//! studio writes them in each frame), its state from the registry too.
//! Items whose submenu says the rest (".png (PNG)..." under Save
//! Duplicate) keep a shorter label of their own.

use sue_wm::{Menu, MenuItem};

use crate::commands::registry;

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

/// Shows the registered workspaces `names` in Window, Workspace.
pub fn set_workspaces(menus: &mut [Menu], names: &[String]) {
    let mut items: Vec<MenuItem> = names
        .iter()
        .enumerate()
        .map(|(index, name)| named(name, &format!("window.workspace.use.{index}")))
        .collect();
    if !items.is_empty() {
        items.push(SEP);
    }
    items.push(item("window.workspace.save"));
    items.push(item("window.workspace.manage"));
    items.push(SEP);
    items.push(item("window.workspace.default"));
    items.push(item("window.workspace.reset"));
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

/// An item for `command`, labeled as the registry labels it.
fn item(command: &str) -> MenuItem {
    let label = registry()
        .label(command)
        .unwrap_or_else(|| command.to_owned());
    MenuItem::action(label, command)
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

/// The menus, left to right.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the whole menu bar as one table")]
pub fn menus() -> Vec<Menu> {
    vec![
        Menu::new(
            tr!("file"),
            vec![
                item("file.start_page"),
                item("file.new"),
                item("file.new_clipboard"),
                named(&tr!("open-ellipsis"), "file.open").key("Ctrl+O"),
                sub(&tr!("open-recent"), vec![item("file.recent.clear")]),
                item("file.close"),
                SEP,
                item("file.save").key("Ctrl+S"),
                item("file.save_as"),
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
                    ],
                ),
                item("file.revert"),
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
                    ],
                ),
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
                    ],
                ),
                sub(
                    &tr!("export-animation"),
                    vec![
                        item("file.export.gif"),
                        item("file.export.apng"),
                        item("file.export.webp_animation"),
                        item("file.export.exposure_sheet"),
                        item("file.export.movie"),
                        item("file.export.sequence"),
                        item("file.export.cels"),
                    ],
                ),
                sub(
                    &tr!("export-multiple-pages"),
                    vec![
                        item("file.export.batch"),
                        item("file.export.pdf"),
                        item("file.export.epub"),
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
                SEP,
                sub(
                    &tr!("timelapse"),
                    vec![item("file.timelapse.record"), item("file.timelapse.export")],
                ),
                SEP,
                item("file.print_settings"),
                item("file.print"),
                SEP,
                item("file.preferences"),
                item("file.shortcuts"),
                item("file.modifiers"),
                item("file.command_bar"),
                item("file.pressure"),
                SEP,
                item("file.leave"),
                item("app.quit").key("Ctrl+Q"),
            ],
        ),
        Menu::new(
            tr!("edit"),
            vec![
                item("edit.undo").key("Ctrl+Z"),
                item("edit.redo").key("Ctrl+Y"),
                SEP,
                item("edit.cut"),
                item("edit.copy"),
                item("edit.paste"),
                item("edit.paste_here"),
                item("edit.clear"),
                item("edit.clear_outside"),
                item("edit.smart_smoothing"),
                item("edit.fill"),
                item("edit.advanced_fill"),
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
                        item("anim.timeline.new"),
                        item("anim.timeline.edit"),
                        item("anim.exposure_sheet"),
                        item("anim.timeline.enable"),
                    ],
                ),
                sub(
                    &tr!("new-animation-layer"),
                    vec![item("anim.layer.folder"), item("anim.layer.camera")],
                ),
                item("anim.cel.new"),
                item("anim.cel.assign"),
                SEP,
                sub(
                    &tr!("edit-track"),
                    vec![
                        item("anim.key.add"),
                        item("anim.key.delete"),
                        item("anim.frame.insert"),
                        item("anim.frame.delete"),
                    ],
                ),
                sub(
                    &tr!("show-animation-cels"),
                    vec![item("anim.onion"), item("anim.onion_settings")],
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
                SEP,
                item("anim.play"),
                sub(
                    &tr!("move-frame"),
                    vec![
                        item("anim.first"),
                        item("anim.last"),
                        item("anim.prev"),
                        item("anim.next"),
                    ],
                ),
                sub(
                    &tr!("playback"),
                    vec![
                        item("anim.play"),
                        item("anim.loop"),
                        named(&tr!("next-frame-3"), "anim.next"),
                        named(&tr!("previous-frame-3"), "anim.prev"),
                        named(&tr!("first-frame-2"), "anim.first"),
                        named(&tr!("last-frame-2"), "anim.last"),
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
                    ],
                ),
                item("select.to_layer"),
                item("select.from_selection_layer"),
                item("select.quick_mask"),
            ],
        ),
        Menu::new(
            tr!("view"),
            vec![
                sub(
                    &tr!("rotate-invert"),
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
                item("view.ruler_bar"),
                item("view.pixel_grid"),
                item("view.crop_frame"),
                item("view.bleed"),
                item("view.page_image"),
                item("view.paper"),
                item("view.tone_area"),
                SEP,
                sub(
                    &tr!("snap"),
                    vec![
                        item("view.snap_ruler"),
                        item("view.snap_special"),
                        item("view.snap_grid"),
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
        Menu::new(
            tr!("window"),
            vec![
                sub(
                    &tr!("canvas"),
                    vec![
                        item("window.next_tab"),
                        item("window.prev_tab"),
                        item("window.new"),
                        item("window.cascade"),
                        item("window.tile"),
                    ],
                ),
                sub(
                    &tr!("workspace"),
                    vec![
                        item("window.workspace.save"),
                        item("window.workspace.manage"),
                        SEP,
                        item("window.workspace.default"),
                        item("window.workspace.reset"),
                    ],
                ),
                sub(
                    &tr!("interface"),
                    vec![
                        item("window.interface.auto"),
                        item("window.interface.desktop"),
                        item("window.interface.tablet"),
                        item("window.interface.phone"),
                    ],
                ),
                item("window.hide_all"),
                item("window.hide_menus"),
                SEP,
                item("window.command_bar"),
                item("window.tool"),
                item("window.sub_tool"),
                item("window.tool_property"),
                item("window.brush_size"),
                item("window.brush_grid"),
                item("window.color_wheel"),
                item("window.color_slider"),
                item("window.color_set"),
                item("window.color_history"),
                item("window.intermediate"),
                item("window.approximate"),
                item("window.navigator"),
                item("window.sub_view"),
                item("window.layer"),
                item("window.layer_property"),
                item("window.history"),
                item("window.timeline"),
                item("window.cels"),
                item("window.material"),
                item("window.auto_actions"),
                item("window.item_bank"),
                item("window.info"),
                SEP,
                item("file.leave"),
            ],
        ),
        Menu::new(
            tr!("help"),
            vec![
                item("help.guide"),
                item("help.shortcuts"),
                item("help.updates"),
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
}
