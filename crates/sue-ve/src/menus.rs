//! Sue VE's menus, built on the skeleton every studio shares (File, Edit,
//! View, the studio's own, Help), with Premiere Pro's own menus between:
//! Clip, Sequence, Markers and Playback.

use sue_wm::{Menu, MenuItem, Skeleton, Slot};

use crate::commands::registry;

/// An item for `command`, labeled as the registry labels it.
fn item(command: &str) -> MenuItem {
    let label = registry()
        .label(command)
        .unwrap_or_else(|| command.to_owned());
    MenuItem::action(label, command)
}

fn items(commands: &[&str]) -> Vec<MenuItem> {
    commands
        .iter()
        .map(|command| {
            if command.is_empty() {
                MenuItem::Separator
            } else {
                item(command)
            }
        })
        .collect()
}

/// Every menu of the bar.
#[must_use]
pub fn menus() -> Vec<Menu> {
    Skeleton::new(tr!("applet-name"))
        .set(Slot::New, item("file.new"))
        .after(
            Slot::New,
            items(&[
                "file.new_sequence",
                "file.new_title",
                "file.new_solid",
                "file.new_adjustment",
                "file.new_shape",
                "file.new_bin",
            ]),
        )
        .set(Slot::Open, item("file.import"))
        .after(
            Slot::Open,
            items(&[
                "file.import_sequence",
                "file.import_lut",
                "file.relink",
                "file.proxies",
            ]),
        )
        .without(Slot::Recent)
        .without(Slot::Close)
        .without(Slot::Print)
        .set(Slot::Export, item("file.export"))
        .edit(items(&[
            "edit.delete",
            "edit.ripple_delete",
            "",
            "edit.deselect",
        ]))
        .view(items(&[
            "view.page_media",
            "view.page_edit",
            "view.page_effects",
            "view.page_color",
            "view.page_audio",
            "view.page_deliver",
            "",
            "view.zoom_in",
            "view.zoom_out",
            "view.zoom_fit",
            "",
            "view.quality_full",
            "view.quality_half",
            "view.quality_quarter",
            "view.proxies",
            "",
            "view.reset_layout",
            "",
            "view.workspace_editing",
            "view.workspace_effects",
            "view.workspace_color",
            "view.workspace_audio",
        ]))
        .menu(Menu::new(
            tr!("menu-clip"),
            items(&[
                "clip.insert",
                "clip.overwrite",
                "",
                "clip.enable",
                "clip.link",
                "clip.unlink",
                "",
                "clip.speed_double",
                "clip.speed_half",
                "clip.speed_normal",
                "clip.reverse",
                "clip.freeze",
                "clip.time_remap",
                "",
                "clip.fade",
                "audio.normalize",
                "",
                "clip.nest",
            ]),
        ))
        .menu(Menu::new(
            tr!("menu-sequence"),
            items(&[
                "sequence.razor",
                "sequence.lift",
                "sequence.extract",
                "sequence.close_gap",
                "",
                "sequence.dissolve",
                "sequence.dip_black",
                "sequence.dip_white",
                "sequence.wipe",
                "sequence.slide",
                "sequence.push",
                "sequence.zoom",
                "sequence.blur_transition",
                "sequence.audio_dissolve",
                "",
                "sequence.render",
                "",
                "sequence.snap",
                "sequence.linked",
                "",
                "sequence.add_video_track",
                "sequence.add_audio_track",
                "",
                "tool.select",
                "tool.ripple",
                "tool.roll",
                "tool.rate_stretch",
                "tool.slip",
                "tool.slide",
                "tool.razor",
            ]),
        ))
        .menu(Menu::new(
            tr!("menu-markers"),
            items(&[
                "marker.in",
                "marker.out",
                "marker.clip",
                "marker.clear",
                "marker.go_in",
                "marker.go_out",
                "",
                "marker.add",
                "marker.remove",
                "marker.clip_marker",
            ]),
        ))
        .menu(Menu::new(
            tr!("menu-effects"),
            items(&[
                "fx.copy",
                "fx.paste",
                "fx.remove_all",
                "",
                "fx.save_preset",
                "fx.import_preset",
                "fx.export_preset",
                "",
                "fx.mask_ellipse",
                "fx.mask_rect",
                "fx.mask_path",
                "fx.track_mask",
                "",
                "fx.ease",
                "fx.ease_in",
                "fx.ease_out",
                "fx.linear",
                "fx.hold",
                "",
                "fx.text_typewriter",
                "fx.text_drop",
                "fx.text_spin",
                "fx.text_grow",
            ]),
        ))
        .menu(Menu::new(
            tr!("menu-color"),
            items(&[
                "color.add_corrector",
                "color.reset_grade",
                "",
                "color.copy_grade",
                "color.paste_grade",
                "",
                "color.split",
            ]),
        ))
        .menu(Menu::new(
            tr!("menu-playback"),
            items(&[
                "playback.play",
                "playback.reverse",
                "playback.stop",
                "playback.forward",
                "",
                "playback.prev_frame",
                "playback.next_frame",
                "playback.prev_edit",
                "playback.next_edit",
                "playback.start",
                "playback.end",
                "",
                "playback.loop",
                "playback.source",
                "playback.program",
                "",
                "audio.record",
            ]),
        ))
        .without(Slot::Guide)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_command_is_a_command() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let menus = menus();
        let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "File", "Edit", "View", "Clip", "Sequence", "Markers", "Effects", "Color",
                "Playback", "Help"
            ]
        );
        for menu in &menus {
            for item in &menu.items {
                for command in item.commands() {
                    assert!(
                        registry().has(command),
                        "{command} is in a menu but no command"
                    );
                }
            }
        }
    }
}
