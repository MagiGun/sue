//! Sue Paint's menu bar, as data. Every item names a command; the studio
//! grays out the ones it cannot run yet, so the menus show the whole plan.

use sue_wm::{Menu, MenuItem};

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
            "File",
            vec![
                act("New...", "file.new").key("Ctrl+N"),
                act("Open...", "file.open").key("Ctrl+O"),
                sub(
                    "Open Recent",
                    vec![act("Clear Recent Files", "file.recent.clear")],
                ),
                act("Close", "file.close").key("Ctrl+W"),
                SEP,
                act("Save Project", "file.save").key("Ctrl+S"),
                act("Save Project As...", "file.save_as"),
                sub(
                    "Save Duplicate",
                    vec![
                        act(".png (PNG)...", "file.export.png"),
                        act(".jpg (JPEG)...", "file.export.jpg"),
                        act(".jxl (JPEG XL)...", "file.export.jxl"),
                        act(".psd (Photoshop Document)...", "file.export.psd"),
                    ],
                ),
                act("Revert", "file.revert"),
                SEP,
                sub(
                    "Import",
                    vec![
                        act("Image...", "file.import.image"),
                        act("Image as Separate Layers...", "file.import.layers"),
                        act("Pattern from Image...", "file.import.pattern"),
                        act("Vector...", "file.import.vector"),
                        act("Audio...", "file.import.audio"),
                        act("Movie...", "file.import.movie"),
                        act("3D Data...", "file.import.3d"),
                    ],
                ),
                sub(
                    "Export Single Layer",
                    vec![
                        act(".png (PNG)...", "file.export.png"),
                        act(".jpg (JPEG)...", "file.export.jpg"),
                        act(".jxl (JPEG XL)...", "file.export.jxl"),
                        act(".bmp (BMP)...", "file.export.bmp"),
                        act(".tif (TIFF)...", "file.export.tif"),
                        act(".tga (Targa)...", "file.export.tga"),
                        act(".psd (Photoshop Document)...", "file.export.psd"),
                        act(".psb (Photoshop Big)...", "file.export.psb"),
                    ],
                ),
                sub(
                    "Export Animation",
                    vec![
                        act("Animated GIF...", "file.export.gif"),
                        act("Movie...", "file.export.movie"),
                        act("Image Sequence...", "file.export.sequence"),
                        act("Animation Cels...", "file.export.cels"),
                    ],
                ),
                sub(
                    "Export Multiple Pages",
                    vec![
                        act("Batch Export...", "file.export.batch"),
                        act("Export as PDF...", "file.export.pdf"),
                        act("Export as EPUB...", "file.export.epub"),
                    ],
                ),
                act("Export Webtoon...", "file.export.webtoon"),
                SEP,
                sub(
                    "Timelapse",
                    vec![
                        act("Record Timelapse", "file.timelapse.record"),
                        act("Export Timelapse...", "file.timelapse.export"),
                    ],
                ),
                SEP,
                act("Print Settings...", "file.print_settings"),
                act("Print...", "file.print").key("Ctrl+P"),
                SEP,
                act("Preferences...", "file.preferences").key("Ctrl+,"),
                act("Shortcut Settings...", "file.shortcuts"),
                act("Modifier Key Settings...", "file.modifiers"),
                act("Command Bar Settings...", "file.command_bar"),
                act("Pen Pressure Settings...", "file.pressure"),
                SEP,
                act("Return to Canvas", "file.leave"),
                act("Quit Sue", "app.quit").key("Ctrl+Q"),
            ],
        ),
        Menu::new(
            "Edit",
            vec![
                act("Undo", "edit.undo").key("Ctrl+Z"),
                act("Redo", "edit.redo").key("Ctrl+Y"),
                SEP,
                act("Cut", "edit.cut").key("Ctrl+X"),
                act("Copy", "edit.copy").key("Ctrl+C"),
                act("Paste", "edit.paste").key("Ctrl+V"),
                act("Paste to Shown Position", "edit.paste_here"),
                act("Clear", "edit.clear").key("Del"),
                act("Clear Outside Selection", "edit.clear_outside"),
                act("Fill", "edit.fill").key("Alt+Del"),
                act("Change Color to Drawing Color", "edit.recolor"),
                act("Convert Brightness to Opacity", "edit.luma_to_alpha"),
                SEP,
                sub(
                    "Tonal Correction",
                    vec![
                        act("Brightness/Contrast...", "filter.brightness"),
                        act("Level Correction...", "filter.levels"),
                        act("Tone Curve...", "filter.curve"),
                        act("Hue/Saturation/Luminosity...", "filter.hue"),
                        act("Color Balance...", "filter.balance"),
                        act("Reverse Gradient", "filter.invert").key("Ctrl+I"),
                        act("Posterization...", "filter.posterize"),
                        act("Binarization...", "filter.binarize"),
                        act("Gradient Map...", "filter.gradient_map"),
                    ],
                ),
                sub(
                    "Transform",
                    vec![
                        act("Scale/Rotate...", "edit.transform.scale").key("Ctrl+T"),
                        act("Free Transform", "edit.transform.free"),
                        act("Distort", "edit.transform.distort"),
                        act("Perspective", "edit.transform.perspective"),
                        act("Mesh Transformation", "edit.transform.mesh"),
                        act("Puppet Warp...", "edit.transform.puppet"),
                        SEP,
                        act("Flip Horizontal", "edit.flip_h"),
                        act("Flip Vertical", "edit.flip_v"),
                        act("Rotate 180 Degrees", "edit.rotate180"),
                    ],
                ),
                SEP,
                act("Change Image Resolution...", "edit.resolution"),
                act("Change Canvas Size...", "edit.canvas_size"),
                act("Crop", "edit.crop"),
                sub(
                    "Rotate/Flip Canvas",
                    vec![
                        act("Rotate 90 Degrees Left", "canvas.rotate_left"),
                        act("Rotate 90 Degrees Right", "canvas.rotate_right"),
                        act("Rotate 180 Degrees", "canvas.rotate180"),
                        act("Flip Horizontal", "canvas.flip_h"),
                        act("Flip Vertical", "canvas.flip_v"),
                    ],
                ),
                act("Canvas Properties...", "edit.canvas_properties"),
                SEP,
                act("Create Brush Tip from Image...", "edit.brush_tip"),
                sub(
                    "Register Material",
                    vec![
                        act("Image...", "edit.material.image"),
                        act("Brush Tip...", "edit.material.tip"),
                    ],
                ),
            ],
        ),
        Menu::new(
            "Story",
            vec![
                act("Page Manager...", "story.manager"),
                SEP,
                act("First Page", "story.first"),
                act("Previous Page", "story.previous"),
                act("Next Page", "story.next"),
                act("Last Page", "story.last"),
                SEP,
                act("Add Page...", "story.add"),
                act("Insert Page...", "story.insert"),
                act("Delete Page", "story.delete"),
                act("Change Page Order...", "story.order"),
                SEP,
                act("Combine Pages", "story.combine"),
                act("Separate Pages", "story.separate"),
                SEP,
                act("Change Basic Work Settings...", "story.work_settings"),
                act("Change Page Settings...", "story.page_settings"),
                act("Change Story Information...", "story.info"),
                SEP,
                act("Story Editor", "story.editor"),
                act("Batch Import Text...", "story.import_text"),
                sub(
                    "Page Management",
                    vec![
                        act("Open Page", "story.open_page"),
                        act("Duplicate Page", "story.duplicate_page"),
                        act("Export Page as File...", "story.export_page"),
                        act("Export All Pages...", "story.export_pages"),
                    ],
                ),
            ],
        ),
        Menu::new(
            "Animation",
            vec![
                sub(
                    "Timeline",
                    vec![
                        act("New Timeline...", "anim.timeline.new"),
                        act("Edit Timeline...", "anim.timeline.edit"),
                        act("Enable Timeline", "anim.timeline.enable"),
                    ],
                ),
                sub(
                    "New Animation Layer",
                    vec![
                        act("Animation Folder", "anim.layer.folder"),
                        act("Camera Folder", "anim.layer.camera"),
                    ],
                ),
                act("New Animation Cel", "anim.cel.new"),
                act("Assign Animation Cel", "anim.cel.assign"),
                SEP,
                sub(
                    "Track Editing",
                    vec![
                        act("Add Keyframe", "anim.key.add"),
                        act("Delete Keyframe", "anim.key.delete"),
                        act("Insert Frame", "anim.frame.insert"),
                        act("Delete Frame", "anim.frame.delete"),
                    ],
                ),
                sub(
                    "Show Animation Cels",
                    vec![
                        act("Enable Onion Skin", "anim.onion"),
                        act("Onion Skin Settings...", "anim.onion_settings"),
                        act("Light Table", "anim.light_table"),
                    ],
                ),
                SEP,
                sub(
                    "Playback",
                    vec![
                        act("Play/Stop", "anim.play"),
                        act("Loop", "anim.loop"),
                        act("Next Frame", "anim.next"),
                        act("Previous Frame", "anim.prev"),
                        act("First Frame", "anim.first"),
                        act("Last Frame", "anim.last"),
                    ],
                ),
            ],
        ),
        Menu::new(
            "Layer",
            vec![
                sub(
                    "New Layer",
                    vec![
                        act("Raster Layer", "layer.new").key("Ctrl+Shift+N"),
                        act("Vector Layer", "layer.new.vector"),
                        act("Text", "layer.new.text"),
                        act("Balloon", "layer.new.balloon"),
                        act("Gradient...", "layer.new.gradient"),
                        act("Fill", "layer.new.fill"),
                        act("Tone...", "layer.new.tone"),
                        act("Paper", "layer.new.paper"),
                        SEP,
                        act("3D Layer", "layer.new.3d"),
                        act("Frame Border Folder...", "layer.new.frame"),
                    ],
                ),
                sub(
                    "New Correction Layer",
                    vec![
                        act("Brightness/Contrast...", "layer.correction.brightness"),
                        act("Level Correction...", "layer.correction.levels"),
                        act("Tone Curve...", "layer.correction.curve"),
                        act("Hue/Saturation/Luminosity...", "layer.correction.hue"),
                        act("Color Balance...", "layer.correction.balance"),
                        act("Reverse Gradient", "layer.correction.invert"),
                        act("Posterization...", "layer.correction.posterize"),
                        act("Binarization...", "layer.correction.binarize"),
                        act("Gradient Map...", "layer.correction.gradient_map"),
                    ],
                ),
                act("New Layer Folder", "layer.new.folder"),
                act("New Layer from Selection", "layer.from_selection"),
                SEP,
                act("Duplicate Layer", "layer.duplicate"),
                act("Delete Layer", "layer.delete"),
                SEP,
                sub(
                    "Layer Mask",
                    vec![
                        act("Mask Outside Selection", "layer.mask.outside"),
                        act("Mask Selection", "layer.mask.inside"),
                        act("Delete Mask", "layer.mask.delete"),
                        act("Apply Mask to Layer", "layer.mask.apply"),
                    ],
                ),
                sub(
                    "Ruler/Frame",
                    vec![
                        act("Create Perspective Ruler...", "layer.ruler.perspective"),
                        act("Create Symmetrical Ruler...", "layer.ruler.symmetry"),
                        act("Create Concentric Circle Ruler", "layer.ruler.circle"),
                    ],
                ),
                SEP,
                sub(
                    "Layer Settings",
                    vec![
                        act("Show Layer", "layer.toggle.visible"),
                        act("Clip to Layer Below", "layer.toggle.clip"),
                        act("Set as Reference Layer", "layer.toggle.reference"),
                        act("Set as Draft Layer", "layer.toggle.draft"),
                        act("Lock Layer", "layer.toggle.lock"),
                        act("Lock Transparent Pixels", "layer.toggle.lock_alpha"),
                    ],
                ),
                act("Layer Property", "layer.properties"),
                SEP,
                sub(
                    "Change Selected Layer",
                    vec![
                        act("Select Layer Above", "layer.select_above").key("Alt+]"),
                        act("Select Layer Below", "layer.select_below").key("Alt+["),
                    ],
                ),
                sub(
                    "Layer Order",
                    vec![
                        act("Bring to Front", "layer.order.front"),
                        act("Move Up", "layer.order.up").key("Ctrl+]"),
                        act("Move Down", "layer.order.down").key("Ctrl+["),
                        act("Send to Back", "layer.order.back"),
                    ],
                ),
                SEP,
                act("Merge with Layer Below", "layer.merge_down").key("Ctrl+E"),
                act("Merge Selected Layers", "layer.merge_selected"),
                act("Merge Visible to New Layer", "layer.merge_visible"),
                act("Flatten Image", "layer.flatten"),
                SEP,
                act("Convert Layer...", "layer.convert"),
                act("Rasterize", "layer.rasterize"),
                act("Create Selection from Layer", "select.from_layer"),
            ],
        ),
        Menu::new(
            "Select",
            vec![
                act("All", "select.all").key("Ctrl+A"),
                act("Deselect", "select.none").key("Ctrl+D"),
                act("Reselect", "select.again").key("Ctrl+Shift+D"),
                act("Invert Selected Area", "select.invert").key("Ctrl+Shift+I"),
                SEP,
                act("Expand Selected Area...", "select.expand"),
                act("Shrink Selected Area...", "select.shrink"),
                act("Blur Border...", "select.feather"),
                act("Select Color Gamut...", "select.gamut"),
                SEP,
                sub(
                    "Selection from Layer",
                    vec![
                        act("Create Selection", "select.from_layer"),
                        act("Add Selection", "select.add_layer"),
                        act("Remove Selection", "select.subtract_layer"),
                    ],
                ),
                act("Convert to Selection Layer", "select.to_layer"),
                act(
                    "Convert Selection Layer to Selection",
                    "select.from_selection_layer",
                ),
                act("Quick Mask", "select.quick_mask"),
                SEP,
                act("Show Selection Launcher", "select.launcher"),
            ],
        ),
        Menu::new(
            "View",
            vec![
                sub(
                    "Rotate/Invert",
                    vec![
                        act("Rotate Left", "view.rotate_left").key("-"),
                        act("Rotate Right", "view.rotate_right").key("^"),
                        act("Reset Rotation", "view.rotate_reset"),
                        SEP,
                        act("Flip Horizontal", "view.flip_h"),
                        act("Flip Vertical", "view.flip_v"),
                    ],
                ),
                SEP,
                act("Zoom In", "view.zoom_in").key("Ctrl++"),
                act("Zoom Out", "view.zoom_out").key("Ctrl+-"),
                act("100%", "view.actual").key("Ctrl+Alt+0"),
                act("200%", "view.double"),
                act("Fit to Screen", "view.fit").key("Ctrl+0"),
                act("Fit to Navigator", "view.fit_navigator"),
                act("Print Size", "view.print_size"),
                SEP,
                act("Grid", "view.grid"),
                act("Pixel Grid", "view.pixel_grid"),
                act("Ruler Bar", "view.ruler_bar"),
                SEP,
                act("Snap to Ruler", "view.snap_ruler"),
                act("Snap to Special Ruler", "view.snap_special"),
                act("Snap to Grid", "view.snap_grid"),
                SEP,
                act("Crop Frame", "view.crop_frame"),
                act("Bleed", "view.bleed"),
                act("Show Page Image", "view.page_image"),
                act("Selection Area", "view.selection"),
                act("Tone Area", "view.tone_area"),
                sub(
                    "Color Profile",
                    vec![
                        act("Preview", "view.profile_preview"),
                        act("Preview Settings...", "view.profile_settings"),
                    ],
                ),
            ],
        ),
        Menu::new(
            "Filter",
            vec![
                sub(
                    "Blur",
                    vec![
                        act("Blur", "filter.blur"),
                        act("Blur (Strong)", "filter.blur_strong"),
                        act("Gaussian Blur...", "filter.gaussian"),
                        act("Motion Blur...", "filter.motion_blur"),
                        act("Radial Blur...", "filter.radial_blur"),
                        act("Zoom Blur...", "filter.zoom_blur"),
                        act("Smooth", "filter.smooth"),
                    ],
                ),
                sub(
                    "Correct Line",
                    vec![
                        act("Remove Dust...", "filter.dust"),
                        act("Line Width...", "filter.line_width"),
                    ],
                ),
                sub(
                    "Distort",
                    vec![
                        act("Wave...", "filter.wave"),
                        act("Pinch...", "filter.pinch"),
                        act("Polar Coordinates...", "filter.polar"),
                        act("Ripple...", "filter.ripple"),
                        act("Fisheye...", "filter.fisheye"),
                    ],
                ),
                sub(
                    "Effect",
                    vec![
                        act("Mosaic...", "filter.mosaic"),
                        act("Add Noise...", "filter.noise"),
                        act("Line Extraction...", "filter.line_extraction"),
                        act("Chromatic Aberration...", "filter.aberration"),
                        act("Retro...", "filter.retro"),
                    ],
                ),
                sub(
                    "Render",
                    vec![
                        act("Focus Lines...", "filter.focus_lines"),
                        act("Speed Lines...", "filter.speed_lines"),
                        act("Screentone...", "filter.tone"),
                        SEP,
                        act("Perlin Noise...", "filter.perlin"),
                        act("Clouds", "filter.clouds"),
                    ],
                ),
                sub(
                    "Sharpen",
                    vec![
                        act("Sharpen", "filter.sharpen"),
                        act("Sharpen (Strong)", "filter.sharpen_strong"),
                        act("Unsharp Mask...", "filter.unsharp"),
                    ],
                ),
                SEP,
                act("Convert to Lines and Tones...", "filter.lines_tones"),
            ],
        ),
        Menu::new(
            "Window",
            vec![
                sub(
                    "Canvas",
                    vec![
                        act("Next Canvas", "window.next_tab").key("Ctrl+Tab"),
                        act("Previous Canvas", "window.prev_tab"),
                        act("New Window", "window.new"),
                        act("Cascade", "window.cascade"),
                        act("Tile", "window.tile"),
                    ],
                ),
                sub(
                    "Workspace",
                    vec![
                        act("Register Workspace...", "window.workspace.save"),
                        act("Manage Workspaces...", "window.workspace.manage"),
                    ],
                ),
                act("Hide All Palettes", "window.hide_all"),
                act("Hide Menu Bar", "window.hide_menus"),
                act("Reset Palette Layout", "window.workspace.reset"),
                SEP,
                act("Command Bar", "window.command_bar"),
                act("Tool", "window.tool"),
                act("Sub Tool", "window.sub_tool"),
                act("Tool Property", "window.tool_property"),
                act("Brush Size", "window.brush_size"),
                act("Color Wheel", "window.color_wheel"),
                act("Color Slider", "window.color_slider"),
                act("Color Set", "window.color_set"),
                act("Color History", "window.color_history"),
                act("Intermediate Color", "window.intermediate"),
                act("Approximate Color", "window.approximate"),
                act("Navigator", "window.navigator"),
                act("Sub View", "window.sub_view"),
                act("Layer", "window.layer"),
                act("Layer Property", "window.layer_property"),
                act("History", "window.history"),
                act("Timeline", "window.timeline"),
                act("Animation Cels", "window.cels"),
                act("Material", "window.material"),
                act("Auto Actions", "window.auto_actions"),
                act("Item Bank", "window.item_bank"),
                act("Info", "window.info"),
                SEP,
                act("Return to Canvas", "file.leave"),
            ],
        ),
        Menu::new(
            "Help",
            vec![
                act("Sue Paint Guide", "help.guide"),
                act("Keyboard Shortcuts", "help.shortcuts"),
                act("Check for Updates", "help.updates"),
                SEP,
                act("About Sue Paint", "help.about"),
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
