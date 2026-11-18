//! Sue VE's commands: one registry that the menus, the shortcuts, the
//! command palette and the tests read. A command's name is saved data
//! (shortcut settings are keyed by it), so it is never renamed without a
//! migration. The default keys follow Premiere Pro's and DaVinci
//! Resolve's where they agree.

use std::sync::OnceLock;

use sue_core::Time;
use sue_wm::{
    AppletContext, Call, Command, CommandRegistry, Facts, FilePick, Flags, ItemState, ShellRequest,
};

use crate::edit;
use crate::media::{Quality, import};
use crate::model::{self, key};
use crate::studio::{Dialog, KEYS_SECTION, Monitor, Page, SueVe, Tool, me};

/// Commands the shell's own keys run: they get no row in the studio's
/// shortcuts.
const SHELL_OWNED: [&str; 5] = [
    "edit.undo",
    "edit.redo",
    "file.open",
    "file.save",
    "app.quit",
];

const fn c(name: &'static str, label: &'static str, group: &'static str) -> Command<SueVe> {
    Command::new(name, label, group, run).state(state)
}

const FILE: &[Command<SueVe>] = &[
    c("file.new", "cmd-new-project", "file").with(Flags::PALETTE),
    c("file.new_sequence", "cmd-new-sequence", "file").keys("Ctrl+Alt+N"),
    c("file.open", "cmd-open", "file").with(Flags::LEAVES_CANVAS),
    c("file.import", "cmd-import", "file")
        .keys("Ctrl+I")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("file.import_sequence", "cmd-import-image-sequence", "file").with(Flags::LEAVES_CANVAS),
    c("file.new_title", "cmd-new-title", "file")
        .keys("Ctrl+T")
        .with(Flags::PALETTE),
    c("file.new_solid", "cmd-new-solid", "file"),
    c("file.new_adjustment", "cmd-new-adjustment", "file"),
    c("file.new_shape", "cmd-new-shape", "file"),
    c("file.new_bin", "cmd-new-bin", "file").keys("Ctrl+B"),
    c("file.relink", "cmd-relink", "file").with(Flags::LEAVES_CANVAS),
    c("file.import_lut", "cmd-import-lut", "file").with(Flags::LEAVES_CANVAS),
    c("file.proxies", "cmd-make-proxies", "file").with(Flags::PALETTE),
    c("file.save", "cmd-save", "file").with(Flags::LEAVES_CANVAS),
    c("file.save_as", "cmd-save-as", "file")
        .keys("Ctrl+Shift+S")
        .with(Flags::LEAVES_CANVAS),
    c("file.export", "cmd-export", "file")
        .keys("Ctrl+M")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("file.settings", "cmd-settings", "file").with(Flags::LEAVES_CANVAS),
    c("file.leave", "cmd-back-to-hub", "file").with(Flags::LEAVES_CANVAS),
    c("app.quit", "cmd-quit", "file").with(Flags::LEAVES_CANVAS),
];

const EDIT: &[Command<SueVe>] = &[
    c("edit.undo", "cmd-undo", "edit"),
    c("edit.redo", "cmd-redo", "edit"),
    c("edit.cut", "cmd-cut", "edit").keys("Ctrl+X"),
    c("edit.copy", "cmd-copy", "edit").keys("Ctrl+C"),
    c("edit.paste", "cmd-paste", "edit").keys("Ctrl+V"),
    c("edit.delete", "cmd-delete", "edit").keys("Delete or Backspace"),
    c("edit.ripple_delete", "cmd-ripple-delete", "edit").keys("Shift+Delete"),
    c("edit.select_all", "cmd-select-all", "edit").keys("Ctrl+A"),
    c("edit.deselect", "cmd-deselect", "edit").keys("Ctrl+Shift+A"),
];

const CLIP: &[Command<SueVe>] = &[
    c("clip.insert", "cmd-insert", "clip")
        .keys(",")
        .with(Flags::PALETTE),
    c("clip.overwrite", "cmd-overwrite", "clip")
        .keys(".")
        .with(Flags::PALETTE),
    c("clip.enable", "cmd-enable", "clip").keys("Shift+E"),
    c("clip.link", "cmd-link", "clip").keys("Ctrl+L"),
    c("clip.unlink", "cmd-unlink", "clip"),
    c("clip.freeze", "cmd-freeze-frame", "clip").keys("Shift+F"),
    c("clip.speed_double", "cmd-speed-double", "clip"),
    c("clip.speed_half", "cmd-speed-half", "clip"),
    c("clip.speed_normal", "cmd-speed-normal", "clip"),
    c("clip.reverse", "cmd-reverse", "clip"),
    c("clip.fade", "cmd-sound-fades", "clip"),
    c("clip.time_remap", "cmd-time-remapping", "clip"),
    c("clip.nest", "cmd-nest", "clip"),
];

const SEQUENCE: &[Command<SueVe>] = &[
    c("sequence.razor", "cmd-add-edit", "sequence")
        .keys("Ctrl+K")
        .with(Flags::PALETTE),
    c("sequence.lift", "cmd-lift", "sequence").keys(";"),
    c("sequence.extract", "cmd-extract", "sequence").keys("'"),
    c("sequence.dissolve", "cmd-add-dissolve", "sequence")
        .keys("Ctrl+D")
        .with(Flags::PALETTE),
    c("sequence.dip_black", "cmd-add-dip-black", "sequence"),
    c("sequence.dip_white", "cmd-add-dip-white", "sequence"),
    c("sequence.audio_dissolve", "cmd-add-crossfade", "sequence").keys("Ctrl+Shift+D"),
    c("sequence.wipe", "cmd-add-wipe", "sequence"),
    c("sequence.slide", "cmd-add-slide", "sequence"),
    c("sequence.push", "cmd-add-push", "sequence"),
    c("sequence.zoom", "cmd-add-zoom", "sequence"),
    c(
        "sequence.blur_transition",
        "cmd-add-blur-transition",
        "sequence",
    ),
    c("sequence.render", "cmd-render-in-to-out", "sequence")
        .keys("Enter")
        .with(Flags::PALETTE),
    c("sequence.close_gap", "cmd-close-gap", "sequence"),
    c("sequence.snap", "cmd-snapping", "sequence").keys("S"),
    c("sequence.linked", "cmd-linked-selection", "sequence").keys("Ctrl+Shift+L"),
    c(
        "sequence.add_video_track",
        "cmd-add-video-track",
        "sequence",
    ),
    c(
        "sequence.add_audio_track",
        "cmd-add-audio-track",
        "sequence",
    ),
];

const MARKER: &[Command<SueVe>] = &[
    c("marker.add", "cmd-add-marker", "marker").keys("M"),
    c("marker.remove", "cmd-remove-marker", "marker"),
    c("marker.clip_marker", "cmd-add-clip-marker", "marker").keys("Shift+M"),
    c("marker.in", "cmd-mark-in", "marker").keys("I"),
    c("marker.out", "cmd-mark-out", "marker").keys("O"),
    c("marker.clip", "cmd-mark-clip", "marker").keys("X"),
    c("marker.clear", "cmd-clear-marks", "marker").keys("Ctrl+Shift+X"),
    c("marker.go_in", "cmd-go-to-in", "marker").keys("Shift+I"),
    c("marker.go_out", "cmd-go-to-out", "marker").keys("Shift+O"),
];

const PLAYBACK: &[Command<SueVe>] = &[
    c("playback.play", "cmd-play", "playback").keys("Space"),
    c("playback.reverse", "cmd-shuttle-back", "playback").keys("J"),
    c("playback.stop", "cmd-stop", "playback").keys("K"),
    c("playback.forward", "cmd-shuttle-forward", "playback").keys("L"),
    c("playback.prev_frame", "cmd-previous-frame", "playback").keys("Left"),
    c("playback.next_frame", "cmd-next-frame", "playback").keys("Right"),
    c("playback.prev_edit", "cmd-previous-edit", "playback").keys("Up"),
    c("playback.next_edit", "cmd-next-edit", "playback").keys("Down"),
    c("playback.start", "cmd-go-to-start", "playback").keys("Home"),
    c("playback.end", "cmd-go-to-end", "playback").keys("End"),
    c("playback.loop", "cmd-loop", "playback").keys("Ctrl+Alt+L"),
    c("playback.source", "cmd-source-monitor", "playback").keys("Shift+1"),
    c("playback.program", "cmd-program-monitor", "playback").keys("Shift+2"),
];

const TOOLS: &[Command<SueVe>] = &[
    c("tool.select", "cmd-tool-select", "tool")
        .keys("V")
        .icon("tool-operation"),
    c("tool.ripple", "cmd-tool-ripple", "tool")
        .keys("B")
        .icon("tool-ripple"),
    c("tool.roll", "cmd-tool-roll", "tool")
        .keys("N")
        .icon("tool-roll"),
    c("tool.slip", "cmd-tool-slip", "tool")
        .keys("Y")
        .icon("tool-slip"),
    c("tool.slide", "cmd-tool-slide", "tool")
        .keys("U")
        .icon("tool-slide"),
    c("tool.razor", "cmd-tool-razor", "tool")
        .keys("C")
        .icon("tool-razor"),
    c("tool.rate_stretch", "cmd-tool-rate-stretch", "tool")
        .keys("R")
        .icon("speed"),
];

const VIEW: &[Command<SueVe>] = &[
    c("view.zoom_in", "cmd-zoom-in", "view").keys("="),
    c("view.zoom_out", "cmd-zoom-out", "view").keys("-"),
    c("view.zoom_fit", "cmd-zoom-fit", "view").keys("Shift+Z"),
    c("view.quality_full", "cmd-quality-full", "view"),
    c("view.quality_half", "cmd-quality-half", "view"),
    c("view.quality_quarter", "cmd-quality-quarter", "view"),
    c("view.proxies", "cmd-use-proxies", "view"),
    c("view.page_media", "cmd-page-media", "view").keys("Shift+3"),
    c("view.page_edit", "cmd-page-edit", "view").keys("Shift+4"),
    c("view.page_effects", "cmd-page-effects", "view").keys("Shift+5"),
    c("view.page_color", "cmd-page-color", "view").keys("Shift+6"),
    c("view.page_audio", "cmd-page-audio", "view").keys("Shift+7"),
    c("view.page_deliver", "cmd-page-deliver", "view").keys("Shift+8"),
    c("view.reset_layout", "cmd-reset-layout", "view"),
    c("view.workspace_editing", "cmd-workspace-editing", "view").keys("Alt+Shift+1"),
    c("view.workspace_effects", "cmd-workspace-effects", "view").keys("Alt+Shift+2"),
    c("view.workspace_color", "cmd-workspace-color", "view").keys("Alt+Shift+3"),
    c("view.workspace_audio", "cmd-workspace-audio", "view").keys("Alt+Shift+4"),
];

const FX: &[Command<SueVe>] = &[
    c("fx.copy", "cmd-copy-effects", "fx").keys("Ctrl+Alt+C"),
    c("fx.paste", "cmd-paste-effects", "fx").keys("Ctrl+Alt+V"),
    c("fx.remove_all", "cmd-remove-effects", "fx"),
    c("fx.save_preset", "cmd-save-effect-preset", "fx").with(Flags::PALETTE),
    c("fx.import_preset", "cmd-import-effect-preset", "fx").with(Flags::LEAVES_CANVAS),
    c("fx.export_preset", "cmd-export-effect-preset", "fx").with(Flags::LEAVES_CANVAS),
    c("fx.ease", "cmd-easy-ease", "fx").keys("F9"),
    c("fx.ease_in", "cmd-ease-in", "fx").keys("Shift+F9"),
    c("fx.ease_out", "cmd-ease-out", "fx").keys("Ctrl+Shift+F9"),
    c("fx.linear", "cmd-linear-keys", "fx"),
    c("fx.hold", "cmd-hold-keys", "fx"),
    c("fx.mask_ellipse", "cmd-mask-ellipse", "fx"),
    c("fx.mask_rect", "cmd-mask-rect", "fx"),
    c("fx.mask_path", "cmd-mask-path", "fx"),
    c("fx.track_mask", "cmd-track-mask", "fx"),
    c("fx.text_typewriter", "cmd-text-typewriter", "fx"),
    c("fx.text_drop", "cmd-text-drop", "fx"),
    c("fx.text_spin", "cmd-text-spin", "fx"),
    c("fx.text_grow", "cmd-text-grow", "fx"),
];

const COLOR: &[Command<SueVe>] = &[
    c("color.copy_grade", "cmd-copy-grade", "color").keys("Ctrl+Shift+C"),
    c("color.paste_grade", "cmd-paste-grade", "color").keys("Ctrl+Shift+V"),
    c("color.add_corrector", "cmd-add-corrector", "color").keys("Alt+S"),
    c("color.reset_grade", "cmd-reset-grade", "color"),
    c("color.split", "cmd-split-before-after", "color").keys("Ctrl+Alt+W"),
];

const AUDIO: &[Command<SueVe>] = &[
    c("audio.normalize", "cmd-normalize", "audio").with(Flags::PALETTE),
    c("audio.record", "cmd-record-voice-over", "audio").keys("Shift+R"),
];

const HELP: &[Command<SueVe>] = &[
    c("help.shortcuts", "cmd-keyboard-shortcuts", "help").with(Flags::PALETTE),
    c("help.updates", "cmd-check-for-updates", "help").with(Flags::LEAVES_CANVAS),
    c("help.about", "cmd-about", "help"),
];

/// Every command of Sue VE.
pub fn registry() -> &'static CommandRegistry<SueVe> {
    static REGISTRY: OnceLock<CommandRegistry<SueVe>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::new("ve", KEYS_SECTION);
        for table in [
            FILE, EDIT, CLIP, SEQUENCE, MARKER, PLAYBACK, TOOLS, VIEW, FX, COLOR, AUDIO, HELP,
        ] {
            registry.add(table);
        }
        registry
    })
}

/// Sue VE's shortcuts: a row for every command that can be given a key,
/// but for the ones the shell's own keys run.
pub fn shortcut_table() -> &'static [sue_wm::shortcuts::Shortcut] {
    static TABLE: OnceLock<Vec<sue_wm::shortcuts::Shortcut>> = OnceLock::new();
    TABLE.get_or_init(|| {
        registry()
            .shortcuts()
            .into_iter()
            .filter(|shortcut| !SHELL_OWNED.contains(&shortcut.command))
            .collect()
    })
}

/// The rows of the Keyboard Shortcuts list.
pub fn shortcut_rows(rice: &sue_rice::RiceSchema) -> Vec<(String, String)> {
    let table = shortcut_table();
    sue_wm::shortcuts::rows(rice, KEYS_SECTION, table)
        .into_iter()
        .zip(table)
        .map(|((label, keys), shortcut)| {
            let named = registry().label(shortcut.command).unwrap_or(label);
            (named, keys)
        })
        .collect()
}

/// Whether a command works now.
fn state(studio: &SueVe, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    let has_sequence = studio
        .sequence
        .is_some_and(|id| model::sequence(facts.arena, id).is_some());
    let has_selection = has_sequence && !studio.selection.is_empty();
    let on_off = |on: bool| {
        if on {
            ItemState::on()
        } else {
            ItemState::off()
        }
    };
    match call.name {
        "tool.select" => ItemState::check(studio.tool == Tool::Select),
        "tool.ripple" => ItemState::check(studio.tool == Tool::Ripple),
        "tool.roll" => ItemState::check(studio.tool == Tool::Roll),
        "tool.slip" => ItemState::check(studio.tool == Tool::Slip),
        "tool.slide" => ItemState::check(studio.tool == Tool::Slide),
        "tool.razor" => ItemState::check(studio.tool == Tool::Razor),
        "tool.rate_stretch" => ItemState::check(studio.tool == Tool::RateStretch),
        "sequence.snap" => ItemState::check(studio.snapping),
        "sequence.linked" => ItemState::check(studio.linked),
        "playback.loop" => ItemState::check(studio.clock.looping),
        "view.quality_full" => ItemState::check(studio.quality == Quality::Full),
        "view.quality_half" => ItemState::check(studio.quality == Quality::Half),
        "view.quality_quarter" => ItemState::check(studio.quality == Quality::Quarter),
        "view.proxies" => ItemState::check(studio.hub.proxies),
        "view.page_media" => ItemState::check(studio.page == Page::Media),
        "view.page_edit" => ItemState::check(studio.page == Page::Edit),
        "view.page_effects" => ItemState::check(studio.page == Page::Effects),
        "view.page_color" => ItemState::check(studio.page == Page::Color),
        "view.page_audio" => ItemState::check(studio.page == Page::Audio),
        "view.page_deliver" => ItemState::check(studio.page == Page::Deliver),
        "edit.cut" | "edit.copy" | "edit.delete" | "edit.ripple_delete" | "clip.enable"
        | "clip.link" | "clip.unlink" | "clip.freeze" | "clip.speed_double" | "clip.speed_half"
        | "clip.speed_normal" | "clip.reverse" | "clip.fade" | "marker.clip" => {
            on_off(has_selection)
        }
        "edit.paste" => on_off(has_sequence && !studio.clipboard.is_empty()),
        "fx.copy"
        | "fx.remove_all"
        | "fx.save_preset"
        | "fx.export_preset"
        | "fx.mask_ellipse"
        | "fx.mask_rect"
        | "fx.mask_path"
        | "fx.ease"
        | "fx.ease_in"
        | "fx.ease_out"
        | "fx.linear"
        | "fx.hold"
        | "fx.text_typewriter"
        | "fx.text_drop"
        | "fx.text_spin"
        | "fx.text_grow"
        | "clip.time_remap"
        | "clip.nest"
        | "marker.clip_marker"
        | "color.copy_grade"
        | "color.add_corrector"
        | "color.reset_grade"
        | "audio.normalize" => on_off(has_selection),
        "fx.paste" => on_off(has_selection && studio.fx_clipboard.is_some()),
        "color.paste_grade" => on_off(has_selection && studio.color.clipboard.is_some()),
        "fx.track_mask" => ItemState::off(),
        "color.split" => ItemState::check(studio.color.split),
        "audio.record" => ItemState::check(studio.recording.is_some()),
        "file.new_bin" | "file.relink" | "file.import_lut" => on_off(studio.project.is_some()),
        "edit.undo"
        | "edit.redo"
        | "file.new"
        | "file.open"
        | "file.import"
        | "file.import_sequence"
        | "file.settings"
        | "file.leave"
        | "app.quit"
        | "help.shortcuts"
        | "help.updates"
        | "help.about"
        | "view.reset_layout" => ItemState::on(),
        "file.proxies" => on_off(studio.project.is_some()),
        _ => on_off(has_sequence),
    }
}

/// Moves the playhead of the monitor the keys drive to `at`.
fn go(studio: &mut SueVe, cx: &mut AppletContext<'_>, at: Time) {
    let now = cx.view.now;
    match studio.focus {
        Monitor::Program => studio.clock.seek(at.max(Time::ZERO), now, cx.sink()),
        Monitor::Source => studio.source.clock.seek(at.max(Time::ZERO), now, None),
    }
}

/// The cuts of the sequence: every clip's start and end, in order.
fn cuts(studio: &SueVe, cx: &AppletContext<'_>) -> Vec<Time> {
    let Some(sequence) = studio.current(cx) else {
        return Vec::new();
    };
    let mut points: Vec<Time> = sequence
        .tracks
        .iter()
        .flat_map(|track| track.clips())
        .flat_map(|clip| [clip.start, clip.end()])
        .collect();
    points.push(Time::ZERO);
    points.sort();
    points.dedup();
    points
}

/// The targeted tracks, video first.
fn targeted(studio: &SueVe, cx: &AppletContext<'_>) -> Vec<usize> {
    let Some(sequence) = studio.current(cx) else {
        return Vec::new();
    };
    let mut out: Vec<usize> = sequence
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| model::flag(track, key::TARGET))
        .map(|(index, _)| index)
        .collect();
    if out.is_empty() {
        let (video, audio) = studio.targets(cx);
        out.extend(video);
        out.extend(audio);
    }
    out
}

/// Adds a video transition of `kind` at the cut nearest the playhead on
/// the targeted video tracks.
pub fn add_transition(studio: &mut SueVe, cx: &mut AppletContext<'_>, kind: &str) {
    transition(studio, cx, kind, false);
}

/// Adds a transition of `kind` at the cut nearest the playhead on the
/// targeted tracks of the right kind.
fn transition(studio: &mut SueVe, cx: &mut AppletContext<'_>, kind: &str, audio: bool) {
    let Some(sequence) = studio.current(cx) else {
        return;
    };
    let at = studio.clock.position;
    let rate = sequence.rate;
    let tracks: Vec<usize> = targeted(studio, cx)
        .into_iter()
        .filter(|index| {
            sequence
                .tracks
                .get(*index)
                .is_some_and(|track| model::is_video(track) != audio)
        })
        .collect();
    // The cut nearest the playhead on those tracks.
    let nearest = tracks
        .iter()
        .filter_map(|index| sequence.tracks.get(*index))
        .flat_map(|track| {
            track
                .clips()
                .iter()
                .flat_map(|clip| [clip.start, clip.end()])
        })
        .min_by_key(|cut| (*cut - at).flicks().abs());
    let Some(cut) = nearest else {
        cx.notify(tr!("no-cut-here"));
        return;
    };
    let length = Time::from_frames(i64::from(rate.timecode_base()), rate);
    studio.edit(cx, &tr!("cmd-add-dissolve"), |sequence, _| {
        let mut added = 0;
        for index in &tracks {
            if edit::add_transition(sequence, *index, kind, cut, length).is_ok() {
                added += 1;
            }
        }
        if added == 0 {
            return Err(edit::EditError::NotAdjacent);
        }
        Ok(())
    });
}

/// Does what a command does.
#[allow(
    clippy::too_many_lines,
    reason = "one match over every command, each a line or two, reads best in one place"
)]
fn run(studio: &mut SueVe, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let now = cx.view.now;
    let one = studio.frame_length(cx);
    match call.name {
        // File.
        "file.new" => studio.new_project(cx),
        "file.new_sequence" => {
            let size = studio.frame_size(cx);
            studio.dialog = Some(Dialog::NewSequence {
                name: tr!(
                    "sequence-name",
                    number = studio.project.map_or(1, |project| {
                        model::sequences(cx.arena(), project).len() as i64 + 1
                    })
                ),
                width: f64::from(size.0),
                height: f64::from(size.1),
                rate: 4,
            });
        }
        "file.open" | "file.import" => studio.ask_import(cx),
        "file.import_sequence" => {
            let pick = FilePick::open(
                Some(me()),
                "image-sequence",
                &tr!("cmd-import-image-sequence"),
            )
            .extensions(&import::STILL_EXTENSIONS);
            cx.request(ShellRequest::PickFile(pick));
        }
        "file.new_title" => {
            studio.add_generated(cx, import::title(&tr!("title-name"), &tr!("title-text")))
        }
        "file.new_solid" => {
            studio.add_generated(cx, import::solid(&tr!("solid-name"), [24, 24, 24]))
        }
        "file.new_adjustment" => {
            studio.add_generated(cx, import::adjustment(&tr!("adjustment-name")))
        }
        "file.proxies" => studio.make_proxies(cx),
        "file.save" | "file.save_as" | "app.quit" => {
            cx.request(ShellRequest::Command(call.name.to_owned()))
        }
        "file.export" => {
            studio.page = Page::Deliver;
        }
        "file.settings" => cx.request(ShellRequest::OpenSettings(Some("studios".to_owned()))),
        "file.leave" => cx.request(ShellRequest::Hub),
        // Edit.
        "edit.undo" => {
            let _ = cx.undo(false);
        }
        "edit.redo" => {
            let _ = cx.undo(true);
        }
        "edit.copy" | "edit.cut" => {
            let ids = studio.selected_with_links(cx);
            if let Some(sequence) = studio.current(cx) {
                studio.clipboard = ids
                    .iter()
                    .filter_map(|id| {
                        sequence
                            .find(*id)
                            .map(|(track, clip)| (track, clip.clone()))
                    })
                    .collect();
            }
            if call.name == "edit.cut" {
                studio.edit(cx, &tr!("cmd-cut"), |sequence, _| {
                    Ok(edit::delete(sequence, &ids))
                });
                studio.selection.clear();
            }
        }
        "edit.paste" => {
            let clips = studio.clipboard.clone();
            let at = studio.clock.position;
            if let Some(ids) = studio.edit(cx, &tr!("cmd-paste"), |sequence, _| {
                edit::paste(sequence, &clips, at)
            }) {
                studio.selection = ids;
            }
        }
        "edit.delete" => {
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-delete"), |sequence, _| {
                Ok(edit::delete(sequence, &ids))
            });
            studio.selection.clear();
        }
        "edit.ripple_delete" => {
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-ripple-delete"), |sequence, _| {
                edit::ripple_delete(sequence, &ids)
            });
            studio.selection.clear();
        }
        "edit.select_all" => {
            if let Some(sequence) = studio.current(cx) {
                studio.selection = sequence
                    .tracks
                    .iter()
                    .flat_map(|track| track.clips().iter().map(|clip| clip.id))
                    .collect();
            }
        }
        "edit.deselect" => studio.selection.clear(),
        // Clip.
        "clip.insert" => studio.edit_from_source(cx, true),
        "clip.overwrite" => studio.edit_from_source(cx, false),
        "clip.enable" => {
            let ids = studio.selected_with_links(cx);
            let on = studio
                .current(cx)
                .and_then(|sequence| ids.first().and_then(|id| sequence.find(*id)))
                .is_some_and(|(_, clip)| !clip.enabled);
            studio.edit(cx, &tr!("cmd-enable"), |sequence, _| {
                edit::set_enabled(sequence, &ids, on)
            });
        }
        "clip.link" => {
            let ids = studio.selection.clone();
            studio.edit(cx, &tr!("cmd-link"), |sequence, _| {
                edit::link(sequence, &ids)
            });
        }
        "clip.unlink" => {
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-unlink"), |sequence, _| {
                edit::unlink(sequence, &ids)
            });
        }
        "clip.freeze" => {
            let at = studio.clock.position;
            if let Some(id) = studio.selection.first().copied() {
                studio.edit(cx, &tr!("cmd-freeze-frame"), |sequence, _| {
                    edit::freeze_frame(sequence, id, at)
                });
            }
        }
        "clip.speed_double" | "clip.speed_half" | "clip.speed_normal" | "clip.reverse" => {
            let ids = studio.selected_with_links(cx);
            let factor = call.name;
            studio.edit(cx, &tr!("cmd-speed"), |sequence, _| {
                for id in &ids {
                    let Some((_, clip)) = sequence.find(*id) else {
                        continue;
                    };
                    let speed = match factor {
                        "clip.speed_double" => clip.speed * 2.0,
                        "clip.speed_half" => clip.speed / 2.0,
                        "clip.reverse" => -clip.speed,
                        _ => clip.speed.signum(),
                    };
                    edit::set_speed(sequence, *id, speed, true)?;
                }
                Ok(())
            });
        }
        "clip.fade" => {
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-sound-fades"), |sequence, _| {
                edit::set_property(sequence, &ids, key::FADE_IN, Some(0.5.into()))?;
                edit::set_property(sequence, &ids, key::FADE_OUT, Some(0.5.into()))
            });
        }
        // Sequence.
        "sequence.razor" => {
            let at = studio.clock.position;
            let tracks = targeted(studio, cx);
            let selected = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-add-edit"), |sequence, _| {
                let cut = if selected.is_empty() {
                    edit::razor(sequence, &tracks, at)
                } else {
                    edit::razor_clips(sequence, &selected, at)
                };
                if cut.is_empty() {
                    return Err(edit::EditError::Nothing);
                }
                Ok(())
            });
        }
        "sequence.lift" | "sequence.extract" => {
            let Some(sequence) = studio.current(cx) else {
                return;
            };
            let (mark_in, mark_out) = model::marks(sequence);
            let (Some(start), Some(end)) = (mark_in, mark_out) else {
                cx.notify(tr!("mark-in-and-out-first"));
                return;
            };
            let tracks = targeted(studio, cx);
            let extracting = call.name == "sequence.extract";
            let label = if extracting {
                tr!("cmd-extract")
            } else {
                tr!("cmd-lift")
            };
            studio.edit(cx, &label, |sequence, _| {
                if extracting {
                    edit::extract(sequence, &tracks, start, end);
                } else {
                    edit::lift(sequence, &tracks, start, end);
                }
                edit::set_mark(sequence, false, None);
                edit::set_mark(sequence, true, None);
                Ok(())
            });
            go(studio, cx, start);
        }
        "sequence.dissolve" => transition(studio, cx, "dissolve", false),
        "sequence.dip_black" => transition(studio, cx, "dip-black", false),
        "sequence.dip_white" => transition(studio, cx, "dip-white", false),
        "sequence.audio_dissolve" => transition(studio, cx, "dissolve", true),
        "sequence.close_gap" => {
            let at = studio.clock.position;
            let tracks = targeted(studio, cx);
            studio.edit(cx, &tr!("cmd-close-gap"), |sequence, _| {
                let mut closed = false;
                for index in tracks {
                    closed |= edit::close_gap(sequence, index, at).is_ok();
                }
                if closed {
                    Ok(())
                } else {
                    Err(edit::EditError::Nothing)
                }
            });
        }
        "sequence.snap" => studio.snapping = !studio.snapping,
        "sequence.linked" => studio.linked = !studio.linked,
        "sequence.add_video_track" => {
            studio.edit(cx, &tr!("cmd-add-video-track"), |sequence, _| {
                Ok(edit::add_track(sequence, model::VIDEO))
            });
        }
        "sequence.add_audio_track" => {
            studio.edit(cx, &tr!("cmd-add-audio-track"), |sequence, _| {
                Ok(edit::add_track(sequence, model::AUDIO))
            });
        }
        // Markers and marks.
        "marker.add" => {
            let at = studio.clock.position;
            let count = studio
                .current(cx)
                .map_or(0, |sequence| sequence.markers.len());
            let name = tr!("marker-name", number = count as i64 + 1);
            studio.edit(cx, &tr!("cmd-add-marker"), |sequence, _| {
                edit::add_marker(sequence, at, &name);
                Ok(())
            });
        }
        "marker.remove" => {
            let at = studio.clock.position;
            studio.edit(cx, &tr!("cmd-remove-marker"), |sequence, _| {
                if edit::remove_marker(sequence, at, one + one) {
                    Ok(())
                } else {
                    Err(edit::EditError::Nothing)
                }
            });
        }
        "marker.in" | "marker.out" => {
            let out = call.name == "marker.out";
            match studio.focus {
                Monitor::Source => {
                    let at = studio.source.clock.position;
                    if out {
                        studio.source.mark_out = Some(at);
                    } else {
                        studio.source.mark_in = Some(at);
                    }
                }
                Monitor::Program => {
                    let at = studio.clock.position;
                    let label = if out {
                        tr!("cmd-mark-out")
                    } else {
                        tr!("cmd-mark-in")
                    };
                    studio.edit(cx, &label, |sequence, _| {
                        edit::set_mark(sequence, out, Some(at));
                        Ok(())
                    });
                }
            }
        }
        "marker.clip" => {
            let Some(sequence) = studio.current(cx) else {
                return;
            };
            let Some((_, clip)) = studio.selection.first().and_then(|id| sequence.find(*id)) else {
                return;
            };
            let (start, end) = (clip.start, clip.end());
            studio.edit(cx, &tr!("cmd-mark-clip"), |sequence, _| {
                edit::set_mark(sequence, false, Some(start));
                edit::set_mark(sequence, true, Some(end));
                Ok(())
            });
        }
        "marker.clear" => match studio.focus {
            Monitor::Source => {
                studio.source.mark_in = None;
                studio.source.mark_out = None;
            }
            Monitor::Program => {
                studio.edit(cx, &tr!("cmd-clear-marks"), |sequence, _| {
                    edit::set_mark(sequence, false, None);
                    edit::set_mark(sequence, true, None);
                    Ok(())
                });
            }
        },
        "marker.go_in" | "marker.go_out" => {
            let marks = match studio.focus {
                Monitor::Source => (studio.source.mark_in, studio.source.mark_out),
                Monitor::Program => studio.current(cx).map_or((None, None), model::marks),
            };
            let at = if call.name == "marker.go_in" {
                marks.0
            } else {
                marks.1
            };
            if let Some(at) = at {
                go(studio, cx, at);
            }
        }
        // Playback.
        "playback.play" => {
            let clock = match studio.focus {
                Monitor::Program => &mut studio.clock,
                Monitor::Source => &mut studio.source.clock,
            };
            if clock.playing {
                let sink = if studio.focus == Monitor::Program {
                    cx.sink()
                } else {
                    None
                };
                clock.pause(sink);
            } else {
                // From the start again at the end.
                let end = match studio.focus {
                    Monitor::Program => studio
                        .current(cx)
                        .map_or(Time::ZERO, sue_model::Sequence::duration),
                    Monitor::Source => studio
                        .source
                        .item
                        .and_then(|item| model::length_of(cx.arena(), item))
                        .unwrap_or(Time::ZERO),
                };
                let clock = match studio.focus {
                    Monitor::Program => &mut studio.clock,
                    Monitor::Source => &mut studio.source.clock,
                };
                if clock.position >= end {
                    clock.position = Time::ZERO;
                }
                let sink = if studio.focus == Monitor::Program {
                    cx.sink()
                } else {
                    None
                };
                if sink.is_none() && studio.focus == Monitor::Program {
                    cx.request(ShellRequest::Command("audio.start".to_owned()));
                }
                let sink = if studio.focus == Monitor::Program {
                    cx.sink()
                } else {
                    None
                };
                let clock = match studio.focus {
                    Monitor::Program => &mut studio.clock,
                    Monitor::Source => &mut studio.source.clock,
                };
                clock.play(1.0, now, sink);
            }
        }
        "playback.reverse" | "playback.forward" => {
            let forward = call.name == "playback.forward";
            match studio.focus {
                Monitor::Program => studio.clock.shuttle(forward, now, cx.sink()),
                Monitor::Source => studio.source.clock.shuttle(forward, now, None),
            }
        }
        "playback.stop" => match studio.focus {
            Monitor::Program => studio.clock.pause(cx.sink()),
            Monitor::Source => studio.source.clock.pause(None),
        },
        "playback.prev_frame" | "playback.next_frame" => {
            let at = match studio.focus {
                Monitor::Program => studio.clock.position,
                Monitor::Source => studio.source.clock.position,
            };
            let step = if call.name == "playback.next_frame" {
                one
            } else {
                -one
            };
            let rate = studio
                .current(cx)
                .map_or(sue_core::Rate::FPS_30, |sequence| sequence.rate);
            let snapped = Time::from_frames((at + step).frame(rate), rate);
            go(studio, cx, snapped);
        }
        "playback.prev_edit" | "playback.next_edit" => {
            let at = studio.clock.position;
            let points = cuts(studio, cx);
            let to = if call.name == "playback.next_edit" {
                points.into_iter().find(|point| *point > at)
            } else {
                points.into_iter().rev().find(|point| *point < at)
            };
            if let Some(to) = to {
                studio.focus = Monitor::Program;
                go(studio, cx, to);
            }
        }
        "playback.start" => go(studio, cx, Time::ZERO),
        "playback.end" => {
            let end = match studio.focus {
                Monitor::Program => studio
                    .current(cx)
                    .map_or(Time::ZERO, sue_model::Sequence::duration),
                Monitor::Source => studio
                    .source
                    .item
                    .and_then(|item| model::length_of(cx.arena(), item))
                    .unwrap_or(Time::ZERO),
            };
            go(studio, cx, end);
        }
        "playback.loop" => studio.clock.looping = !studio.clock.looping,
        "playback.source" => studio.focus = Monitor::Source,
        "playback.program" => studio.focus = Monitor::Program,
        // Tools.
        "tool.select" => studio.tool = Tool::Select,
        "tool.ripple" => studio.tool = Tool::Ripple,
        "tool.roll" => studio.tool = Tool::Roll,
        "tool.slip" => studio.tool = Tool::Slip,
        "tool.slide" => studio.tool = Tool::Slide,
        "tool.razor" => studio.tool = Tool::Razor,
        "tool.rate_stretch" => studio.tool = Tool::RateStretch,
        // View.
        "view.zoom_in" => {
            let from = studio.zoom_target.unwrap_or(studio.zoom);
            studio.zoom_target = Some((from * 1.5).min(crate::ui::timeline::MOST_ZOOM));
        }
        "view.zoom_out" => {
            let from = studio.zoom_target.unwrap_or(studio.zoom);
            studio.zoom_target = Some((from / 1.5).max(crate::ui::timeline::LEAST_ZOOM));
        }
        "view.zoom_fit" => {
            if let (Some(sequence), Some(rect)) = (studio.current(cx), studio.timeline_rect) {
                let seconds = sequence.duration().seconds().max(1.0) as f32;
                studio.zoom = ((rect.width() - crate::ui::timeline::HEADER_WIDTH) / seconds * 0.95)
                    .clamp(
                        crate::ui::timeline::LEAST_ZOOM,
                        crate::ui::timeline::MOST_ZOOM,
                    );
                studio.scroll = 0.0;
            }
        }
        "view.quality_full" | "view.quality_half" | "view.quality_quarter" => {
            studio.quality = match call.name {
                "view.quality_half" => Quality::Half,
                "view.quality_quarter" => Quality::Quarter,
                _ => Quality::Full,
            };
        }
        "view.proxies" => {
            studio.hub.proxies = !studio.hub.proxies;
            studio.hub.reset();
        }
        "view.page_media" => studio.page = Page::Media,
        "view.page_edit" => studio.page = Page::Edit,
        "view.page_effects" => studio.page = Page::Effects,
        "view.page_color" => studio.page = Page::Color,
        "view.page_audio" => studio.page = Page::Audio,
        "view.page_deliver" => studio.page = Page::Deliver,
        "view.reset_layout" => studio.dock = crate::ui::default_dock(),
        // Help.
        "help.shortcuts" => studio.dialog = Some(Dialog::Shortcuts),
        "help.updates" => cx.request(ShellRequest::Command("app.check_updates".to_owned())),
        "help.about" => studio.dialog = Some(Dialog::About),
        other => {
            let _ = crate::actions::run(studio, cx, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_label_and_keys_that_parse() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let registry = registry();
        assert!(registry.problems().is_empty(), "{:?}", registry.problems());
        for command in registry.iter() {
            let label = registry.label(command.name).unwrap();
            assert!(
                !label.is_empty() && label != command.label,
                "{} has no text",
                command.name
            );
        }
        // No two rows share a chord.
        let rice = sue_rice::RiceSchema::new();
        let conflicts = sue_wm::shortcuts::conflicts(&rice, KEYS_SECTION, shortcut_table());
        assert!(conflicts.is_empty(), "{conflicts:?}");
    }
}
