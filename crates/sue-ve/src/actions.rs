//! What the commands of stage 2 do: effects and presets, masks, eases,
//! text animators, time remapping, nesting, clip markers, bins, relinking,
//! LUTs, shapes, rendering ahead, the Color page's grade commands, the
//! Audio page's normalize and voice over, and the workspaces. The
//! registry (`crate::commands`) names them; their keys and menus are
//! there.

use sue_core::txn::Batch;
use sue_core::{Entity, EntityId, Time};
use sue_wm::{AppletContext, FilePick, ShellRequest};

use crate::edit::EditError;
use crate::fx::{self, mask};
use crate::grade::{self, Corrector};
use crate::keys::{self, Ease};
use crate::model::{self, SourceKind, key};
use crate::studio::{Dialog, Page, SueVe, me};

/// The loudness Normalize aims at, LUFS (the streaming services' level).
pub const LOUDNESS_TARGET: f64 = -16.0;

/// Runs stage 2 command `name`. `false` when it is not one of them.
#[allow(
    clippy::too_many_lines,
    reason = "one match over the commands, each a few lines, reads best in one place"
)]
pub fn run(studio: &mut SueVe, cx: &mut AppletContext<'_>, name: &str) -> bool {
    match name {
        // Effects.
        "fx.copy" => {
            if let Some((_, clip)) = studio.selected_clip(cx) {
                studio.fx_clipboard = Some(fx::effects(&clip));
            }
        }
        "fx.paste" => {
            let Some(copied) = studio.fx_clipboard.clone() else {
                return true;
            };
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-paste-effects"), |sequence, _| {
                let mut any = false;
                for id in &ids {
                    let video = sequence
                        .find(*id)
                        .is_some_and(|(track, _)| model::is_video(&sequence.tracks[track]));
                    if let (true, Some((_, clip))) = (video, sequence.find_mut(*id)) {
                        let mut list = fx::effects(clip);
                        for (next, effect) in (fx::next_id(clip)..).zip(copied.iter()) {
                            let mut effect = effect.clone();
                            effect.id = next;
                            list.push(effect);
                        }
                        fx::set_effects(clip, &list);
                        any = true;
                    }
                }
                if any { Ok(()) } else { Err(EditError::Nothing) }
            });
        }
        "fx.remove_all" => {
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-remove-effects"), |sequence, _| {
                let mut any = false;
                for id in &ids {
                    if let Some((_, clip)) = sequence.find_mut(*id) {
                        for effect in fx::effects(clip) {
                            any |= fx::remove(clip, effect.id);
                        }
                    }
                }
                if any { Ok(()) } else { Err(EditError::Nothing) }
            });
        }
        "fx.save_preset" => {
            if studio.selected_clip(cx).is_some() {
                studio.dialog = Some(Dialog::SavePreset {
                    name: tr!("preset-default-name"),
                });
            }
        }
        "fx.import_preset" => {
            let pick = FilePick::open(
                Some(me()),
                "preset-import",
                &tr!("cmd-import-effect-preset"),
            )
            .extensions(&[fx::preset::EXTENSION]);
            cx.request(ShellRequest::PickFile(pick));
        }
        "fx.export_preset" => {
            if studio.selected_clip(cx).is_some() {
                let pick = FilePick::save(
                    Some(me()),
                    "preset-export",
                    &tr!("cmd-export-effect-preset"),
                    &format!("{}.{}", tr!("preset-default-name"), fx::preset::EXTENSION),
                )
                .extensions(&[fx::preset::EXTENSION]);
                cx.request(ShellRequest::PickFile(pick));
            }
        }
        "fx.add_key" => {
            let (Some((_, clip)), Some(channel)) =
                (studio.selected_clip(cx), studio.graph.channel.clone())
            else {
                cx.notify(tr!("graph-pick-a-value"));
                return true;
            };
            let at = studio.clock.position;
            if at < clip.start || at >= clip.end() {
                cx.notify(tr!("graph-playhead-outside"));
                return true;
            }
            let time = clip.source_time(at);
            let on_key = clip
                .animation
                .channel(&channel)
                .is_some_and(|curve| curve.keys().iter().any(|key| key.time == time));
            let value = channel_value(&clip, &channel, at);
            let id = clip.id;
            studio.edit(cx, &tr!("cmd-toggle-keyframe"), |sequence, _| {
                if on_key {
                    crate::edit::remove_key(sequence, id, &channel, at)
                } else {
                    crate::edit::set_key(sequence, id, &channel, at, value)
                }
            });
        }
        "fx.ease" | "fx.ease_in" | "fx.ease_out" | "fx.linear" | "fx.hold" => {
            let ease = match name {
                "fx.ease_in" => Ease::In,
                "fx.ease_out" => Ease::Out,
                "fx.linear" => Ease::Linear,
                "fx.hold" => Ease::Hold,
                _ => Ease::Both,
            };
            ease_keys(studio, cx, ease);
        }
        "fx.mask_ellipse" | "fx.mask_rect" | "fx.mask_path" => {
            let Some((_, clip)) = studio.selected_clip(cx) else {
                return true;
            };
            let made = match name {
                "fx.mask_ellipse" => mask::Mask::ellipse(),
                "fx.mask_rect" => mask::Mask::rect(),
                _ => mask::Mask::of(mask::Shape::Path(
                    [[0.5, 0.2], [0.8, 0.5], [0.5, 0.8], [0.2, 0.5]]
                        .into_iter()
                        .map(mask::PathPoint::corner)
                        .collect(),
                )),
            };
            let count = mask::masks(&clip).len();
            if studio.change_clip(cx, &tr!("controls-add-mask"), clip.id, |clip| {
                let mut masks = mask::masks(clip);
                masks.push(made);
                mask::set_masks(clip, &masks);
                Ok(())
            }) {
                studio.mask_picked = Some(count);
            }
        }
        "fx.track_mask" => cx.notify(tr!("controls-mask-track-later")),
        "fx.text_typewriter" | "fx.text_drop" | "fx.text_spin" | "fx.text_grow" => {
            text_animator(studio, cx, name);
        }
        // Clips.
        "clip.time_remap" => {
            let Some((_, clip)) = studio.selected_clip(cx) else {
                return true;
            };
            let on = clip.animation.channel(fx::key::REMAP).is_some();
            studio.change_clip(cx, &tr!("controls-time-remapping"), clip.id, |clip| {
                if on {
                    clip.animation.remove_channel(fx::key::REMAP);
                } else {
                    keys::enable_remap(clip);
                }
                Ok(())
            });
        }
        "clip.nest" => nest(studio, cx),
        "marker.clip_marker" => {
            let Some((_, clip)) = studio.selected_clip(cx) else {
                return true;
            };
            let at = studio.clock.position;
            let count = keys::clip_markers(&clip).len();
            let label = tr!("marker-name", number = count as i64 + 1);
            studio.change_clip(cx, &tr!("cmd-add-clip-marker"), clip.id, |clip| {
                keys::add_clip_marker(clip, at, &label)
            });
        }
        // The pool.
        "file.new_bin" => {
            let Some(project) = studio.project else {
                return true;
            };
            let count = model::pool(cx.arena(), project)
                .iter()
                .filter(|(id, _)| {
                    cx.arena()
                        .get_by_id(*id)
                        .is_ok_and(|entity| entity.class() == sue_model::class::FOLDER)
                })
                .count();
            let bin = Entity::empty(
                tr!("bin-name", number = count as i64 + 1),
                sue_model::class::FOLDER,
            );
            let parent = studio
                .picked
                .filter(|id| {
                    cx.arena()
                        .get_by_id(*id)
                        .is_ok_and(|entity| entity.class() == sue_model::class::FOLDER)
                })
                .unwrap_or(project);
            let spawn = model::spawn_under(cx.arena(), parent, bin);
            studio.apply(cx, spawn);
        }
        "file.relink" => {
            let missing = missing_media(studio, cx);
            match missing.first() {
                None => cx.notify(tr!("relink-none-missing")),
                Some((id, name)) => {
                    studio.relinking = Some(*id);
                    let pick = FilePick::open(
                        Some(me()),
                        "relink",
                        &tr!("relink-title", name = name.clone()),
                    )
                    .extensions(&crate::media::import::extensions());
                    cx.request(ShellRequest::PickFile(pick));
                }
            }
        }
        "file.import_lut" => {
            let pick =
                FilePick::open(Some(me()), "lut", &tr!("cmd-import-lut")).extensions(&["cube"]);
            cx.request(ShellRequest::PickFile(pick));
        }
        "file.new_shape" => {
            studio.add_generated(cx, crate::layers::shape(&tr!("shape-name"), "rect"));
        }
        // Sequence.
        "sequence.render" => render_range(studio, cx),
        "sequence.wipe" => crate::commands::add_transition(studio, cx, "wipe"),
        "sequence.slide" => crate::commands::add_transition(studio, cx, "slide"),
        "sequence.push" => crate::commands::add_transition(studio, cx, "push"),
        "sequence.zoom" => crate::commands::add_transition(studio, cx, "zoom"),
        "sequence.blur_transition" => crate::commands::add_transition(studio, cx, "blur"),
        // Color.
        "color.copy_grade" => {
            if let Some((_, clip)) = studio.selected_clip(cx) {
                studio.color.clipboard = Some(grade::grade_of(&clip));
            }
        }
        "color.paste_grade" => {
            let Some(copied) = studio.color.clipboard.clone() else {
                return true;
            };
            let ids = studio.selected_with_links(cx);
            studio.edit(cx, &tr!("cmd-paste-grade"), |sequence, _| {
                let mut any = false;
                for id in &ids {
                    let video = sequence
                        .find(*id)
                        .is_some_and(|(track, _)| model::is_video(&sequence.tracks[track]));
                    if let (true, Some((_, clip))) = (video, sequence.find_mut(*id)) {
                        grade::set_grade(clip, &copied);
                        any = true;
                    }
                }
                if any { Ok(()) } else { Err(EditError::Nothing) }
            });
        }
        "color.add_corrector" => {
            let Some((_, clip)) = studio.selected_clip(cx) else {
                return true;
            };
            let count = grade::grade_of(&clip).len();
            if studio.change_clip(cx, &tr!("cmd-add-corrector"), clip.id, |clip| {
                let mut list = grade::grade_of(clip);
                list.push(Corrector::default());
                grade::set_grade(clip, &list);
                Ok(())
            }) {
                studio.color.corrector = count;
            }
        }
        "color.reset_grade" => {
            let Some((_, clip)) = studio.selected_clip(cx) else {
                return true;
            };
            studio.change_clip(cx, &tr!("cmd-reset-grade"), clip.id, |clip| {
                if grade::grade_of(clip).is_empty() {
                    return Err(EditError::Nothing);
                }
                grade::set_grade(clip, &[]);
                Ok(())
            });
            studio.color.corrector = 0;
        }
        "color.split" => studio.color.split = !studio.color.split,
        // Audio.
        "audio.normalize" => normalize(studio, cx),
        "audio.record" => record(studio, cx),
        // Workspaces.
        "view.workspace_editing"
        | "view.workspace_effects"
        | "view.workspace_color"
        | "view.workspace_audio" => {
            studio.dock = crate::ui::workspace(name);
            studio.page = Page::Edit;
        }
        _ => return false,
    }
    true
}

/// The value channel `channel` of `clip` has at sequence time `at`: its
/// keys, else the property, effect parameter or mask number it drives,
/// else its default.
#[must_use]
pub fn channel_value(clip: &sue_model::Clip, channel: &str, at: Time) -> f64 {
    if let Some(value) = clip
        .animation
        .channel(channel)
        .and_then(|curve| curve.sample(clip.source_time(at)))
    {
        return value;
    }
    if let Some((id, param)) = fx::parse_channel(channel) {
        return fx::effects(clip)
            .into_iter()
            .find(|effect| effect.id == id)
            .map_or(0.0, |effect| effect.number(clip, param, at));
    }
    if let Some(rest) = channel.strip_prefix("mask.") {
        let mut parts = rest.splitn(2, '.');
        if let (Some(Ok(index)), Some(param)) =
            (parts.next().map(str::parse::<usize>), parts.next())
            && let Some(found) = mask::masks(clip).get(index)
        {
            return match param {
                "feather" => found.feather,
                "expansion" => found.expansion,
                _ => found.opacity,
            };
        }
    }
    clip.value_at(channel, at, model::default_of(channel))
}

/// Eases the keys the graph editor picked, else every key at the playhead
/// on the selected clip.
fn ease_keys(studio: &mut SueVe, cx: &mut AppletContext<'_>, ease: Ease) {
    let Some((_, clip)) = studio.selected_clip(cx) else {
        return;
    };
    let picked = studio.graph.picked.clone();
    let channel = studio.graph.channel.clone();
    let at = clip.source_time(studio.clock.position);
    studio.change_clip(cx, &tr!("graph-ease"), clip.id, |clip| {
        let mut any = false;
        let names: Vec<String> = clip
            .animation
            .channels()
            .map(|(name, _)| name.to_owned())
            .collect();
        for name in names {
            let curve = clip.animation.channel_mut(&name);
            let times: Vec<Time> =
                if !picked.is_empty() && channel.as_deref() == Some(name.as_str()) {
                    picked.clone()
                } else if picked.is_empty() {
                    curve
                        .keys()
                        .iter()
                        .filter(|key| key.time == at)
                        .map(|key| key.time)
                        .collect()
                } else {
                    Vec::new()
                };
            for time in times {
                any |= keys::ease_key(curve, time, ease);
            }
        }
        if any { Ok(()) } else { Err(EditError::Nothing) }
    });
}

/// Puts a text animator on the selected title clip, its range keyed from
/// nothing to all over the clip's first second.
fn text_animator(studio: &mut SueVe, cx: &mut AppletContext<'_>, name: &str) {
    use crate::layers::{ANIMATOR, Animates, Animator, RANGE};
    let Some((_, clip)) = studio.selected_clip(cx) else {
        return;
    };
    let title = cx
        .arena()
        .get_by_id(clip.source)
        .is_ok_and(|entity| SourceKind::of(entity) == SourceKind::Title);
    if !title {
        cx.notify(tr!("text-animator-pick-a-title"));
        return;
    }
    let animator = match name {
        "fx.text_drop" => Animator {
            animates: Animates::Position,
            amount: -120.0,
            ..Animator::default()
        },
        "fx.text_spin" => Animator {
            animates: Animates::Rotation,
            amount: 180.0,
            ..Animator::default()
        },
        "fx.text_grow" => Animator {
            animates: Animates::Scale,
            amount: 0.0,
            ..Animator::default()
        },
        _ => Animator {
            softness: 0.0,
            ..Animator::default()
        },
    };
    let (start, length) = (clip.start, clip.duration.min(Time::SECOND));
    studio.change_clip(cx, &tr!("cmd-text-animator"), clip.id, |clip| {
        clip.properties.set(ANIMATOR, animator.to_value());
        clip.animation.remove_channel(RANGE);
        let times = [clip.source_time(start), clip.source_time(start + length)];
        let curve = clip.animation.channel_mut(RANGE);
        for (time, value) in times.into_iter().zip([0.0, 100.0]) {
            curve.insert(sue_model::Key {
                time,
                value,
                interpolation: sue_model::Interpolation::Linear,
            });
        }
        Ok(())
    });
}

/// Nests the selected clips (Premiere Pro's Nest): they move into a new
/// sequence, in their tracks and at their times from its start, and one
/// clip of that sequence takes their place. One undo step.
fn nest(studio: &mut SueVe, cx: &mut AppletContext<'_>) {
    let ids = studio.selected_with_links(cx);
    let (Some(project), Some(id), Some(sequence)) =
        (studio.project, studio.sequence, studio.current(cx).cloned())
    else {
        return;
    };
    if ids.is_empty() {
        return;
    }
    let chosen: Vec<(usize, sue_model::Clip)> = ids
        .iter()
        .filter_map(|id| {
            sequence
                .find(*id)
                .map(|(track, clip)| (track, clip.clone()))
        })
        .collect();
    let start = chosen
        .iter()
        .map(|(_, clip)| clip.start)
        .min()
        .unwrap_or(Time::ZERO);
    let end = chosen
        .iter()
        .map(|(_, clip)| clip.end())
        .max()
        .unwrap_or(start);
    let (width, height) = model::frame_size(&sequence);
    let mut inner = model::empty_sequence(width, height, sequence.rate, 0, 0);
    for track in &sequence.tracks {
        let mut copy = sue_model::Track::new(track.name.clone(), track.kind.clone());
        copy.properties = track.properties.clone();
        inner.tracks.push(copy);
    }
    for (track, clip) in &chosen {
        let mut moved = clip.clone();
        moved.start = clip.start - start;
        moved.id = inner.alloc_id();
        moved.link = 0;
        inner.tracks[*track].insert(moved);
    }
    let count = model::sequences(cx.arena(), project).len();
    let made = Entity::empty(
        tr!("nest-name", number = count as i64 + 1),
        sue_model::class::SEQUENCE,
    )
    .with_ext(inner);
    let nested = made.id();
    let top = chosen
        .iter()
        .filter(|(track, _)| model::is_video(&sequence.tracks[*track]))
        .map(|(track, _)| *track)
        .min()
        .or_else(|| chosen.first().map(|(track, _)| *track))
        .unwrap_or(0);
    let plan = model::plan(cx.arena(), id, &tr!("cmd-nest"), |sequence| {
        crate::edit::delete(sequence, &ids);
        let mut clip = sue_model::Clip::new(nested, start, end - start);
        clip.id = sequence.alloc_id();
        sequence.tracks[top].insert(clip.clone());
        Ok(clip.id)
    });
    match plan {
        Ok((Some(step), placed)) => {
            let batch = Batch::new(tr!("cmd-nest"))
                .then(model::spawn_under(cx.arena(), project, made))
                .then(step);
            if studio.apply(cx, batch) {
                studio.selection = vec![placed];
            }
        }
        Ok((None, _)) => {}
        Err(error) => cx.notify(error.text()),
    }
}

/// The movies and sounds of the pool whose file is not where it was.
#[must_use]
pub fn missing_media(studio: &SueVe, cx: &AppletContext<'_>) -> Vec<(EntityId, String)> {
    let Some(project) = studio.project else {
        return Vec::new();
    };
    model::pool(cx.arena(), project)
        .into_iter()
        .filter_map(|(id, _)| {
            let entity = cx.arena().get_by_id(id).ok()?;
            if !matches!(
                SourceKind::of(entity),
                SourceKind::Movie | SourceKind::Sound
            ) {
                return None;
            }
            if entity.ext::<sue_model::Encoded>().is_some() {
                return None;
            }
            let path = model::text(entity, key::PATH)?;
            (!std::path::Path::new(path).is_file()).then(|| (id, entity.name().to_owned()))
        })
        .collect()
}

/// Relinks `id` to the file at `path`, and every other missing file found
/// by its name in the same folder, as one undo step.
pub fn relink(
    studio: &mut SueVe,
    cx: &mut AppletContext<'_>,
    id: EntityId,
    path: &std::path::Path,
) {
    let folder = path.parent().map(std::path::Path::to_path_buf);
    let mut batch = Batch::new(tr!("cmd-relink"));
    batch.push(Box::new(sue_core::txn::SetProperty::new(
        id,
        key::PATH,
        path.to_string_lossy().into_owned(),
    )));
    let mut found = 1;
    if let Some(folder) = folder {
        for (other, _) in missing_media(studio, cx) {
            if other == id {
                continue;
            }
            let Some(name) = cx
                .arena()
                .get_by_id(other)
                .ok()
                .and_then(|entity| model::text(entity, key::PATH))
                .and_then(|old| {
                    std::path::Path::new(old)
                        .file_name()
                        .map(std::ffi::OsStr::to_os_string)
                })
            else {
                continue;
            };
            let candidate = folder.join(name);
            if candidate.is_file() {
                batch.push(Box::new(sue_core::txn::SetProperty::new(
                    other,
                    key::PATH,
                    candidate.to_string_lossy().into_owned(),
                )));
                found += 1;
            }
        }
    }
    if studio.apply(cx, batch) {
        studio.hub.reset();
        if let Some(project) = studio.project {
            for (item, _) in model::pool(cx.arena(), project) {
                studio.hub.prepare(cx.arena(), item);
            }
        }
        cx.notify(tr!("relinked", count = found as i64));
    }
}

/// Imports a `.cube` LUT into the pool; with a clip selected, its grade's
/// corrector uses it.
pub fn import_lut(studio: &mut SueVe, cx: &mut AppletContext<'_>, path: &std::path::Path) {
    let Some(project) = studio.project else {
        return;
    };
    let text = match sue_platform::fs::read(path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => {
            cx.fail(error.to_string());
            return;
        }
    };
    let table = match grade::parse_cube(&text) {
        Ok(table) => table,
        Err(why) => {
            cx.fail(tr!(
                "lut-could-not-read",
                name = crate::studio::file_name(path),
                error = why
            ));
            return;
        }
    };
    let name = path.file_stem().map_or_else(
        || tr!("lut-name"),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let entity = grade::lut_entity(&name, &table);
    let lut = entity.id();
    let mut batch =
        Batch::new(tr!("cmd-import-lut")).then(model::spawn_under(cx.arena(), project, entity));
    if let (Some((_, clip)), Some(id)) = (studio.selected_clip(cx), studio.sequence) {
        let corrector = studio.color.corrector;
        if let Ok((Some(step), ())) =
            model::plan(cx.arena(), id, &tr!("cmd-import-lut"), |sequence| {
                let (_, clip) = sequence
                    .find_mut(clip.id)
                    .ok_or(EditError::NoClip(clip.id))?;
                let mut list = grade::grade_of(clip);
                if list.is_empty() {
                    list.push(Corrector::default());
                }
                let at = corrector.min(list.len() - 1);
                list[at].lut = Some(lut);
                grade::set_grade(clip, &list);
                Ok(())
            })
        {
            batch.push(Box::new(step));
        }
    }
    studio.apply(cx, batch);
}

/// Draws every frame between the in and out marks (or the whole) that
/// needs the processor into the render cache.
fn render_range(studio: &mut SueVe, cx: &mut AppletContext<'_>) {
    let (Some(id), Some(sequence)) = (studio.sequence, studio.current(cx).cloned()) else {
        return;
    };
    let (mark_in, mark_out) = model::marks(&sequence);
    let start = mark_in.unwrap_or(Time::ZERO);
    let end = mark_out.unwrap_or(sequence.duration());
    let arena = cx.arena();
    let kind = |source: EntityId| {
        arena
            .get_by_id(source)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let size = studio.quality.of(model::frame_size(&sequence));
    let first = start.frame(sequence.rate);
    let last = end.frame(sequence.rate);
    let wants: Vec<crate::preview::Want> = (first..last)
        .map(|frame| Time::from_frames(frame, sequence.rate))
        .filter(|at| crate::compose::needs_processing(&sequence, *at, &kind))
        .map(|at| crate::preview::Want {
            time: at,
            key: crate::compose::frame_key(
                arena,
                &sequence,
                at,
                size,
                crate::compose::Options::default(),
            ),
            options: crate::compose::Options::default(),
        })
        .collect();
    if wants.is_empty() {
        cx.notify(tr!("render-nothing"));
        return;
    }
    let count = wants.len();
    studio.preview.sync(arena, id, size, studio.hub.hardware);
    studio.preview.render_ahead(wants);
    cx.notify(tr!("render-started", frames = count as i64));
}

/// Normalizes the selected sound clips to [`LOUDNESS_TARGET`]: each one's
/// gain set so its integrated loudness lands there, its peaks under
/// -1 dBFS.
fn normalize(studio: &mut SueVe, cx: &mut AppletContext<'_>) {
    let Some(sequence) = studio.current(cx).cloned() else {
        return;
    };
    let mut gains = Vec::new();
    for id in studio.selected_with_links(cx) {
        let Some((track, clip)) = sequence.find(id) else {
            continue;
        };
        if model::is_video(&sequence.tracks[track]) {
            continue;
        }
        let Some(sound) = studio.hub.sounds.get(clip.source) else {
            continue;
        };
        let rate = f64::from(crate::media::sound::MIX_RATE);
        let from = (clip.source_in.seconds() * rate).max(0.0) as usize * 2;
        let to = ((clip.source_out().seconds() * rate) as usize * 2).min(sound.samples.len());
        let part = sound.samples.get(from..to.max(from)).unwrap_or(&[]);
        let measured = crate::audio::loudness_of(part, crate::media::sound::MIX_RATE);
        let peak = part.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
        gains.push((
            id,
            crate::audio::normalize_gain(measured, LOUDNESS_TARGET, peak),
        ));
    }
    if gains.is_empty() {
        cx.notify(tr!("normalize-pick-sound"));
        return;
    }
    studio.edit(cx, &tr!("cmd-normalize"), |sequence, _| {
        for (id, gain) in &gains {
            let (_, clip) = sequence.find_mut(*id).ok_or(EditError::NoClip(*id))?;
            clip.animation.remove_channel(key::VOLUME);
            clip.properties
                .set(key::VOLUME, (*gain * 10.0).round() / 10.0);
        }
        Ok(())
    });
}

/// Starts or stops recording a voice over from the microphone, at the
/// playhead on the targeted audio track. Stopped, the recording is kept
/// as a WAV in Sue VE's folder, imported, and put on the track.
fn record(studio: &mut SueVe, cx: &mut AppletContext<'_>) {
    if let Some(mut recording) = studio.recording.take() {
        recording.input.take(&mut recording.samples);
        recording.input.stop();
        let channels = recording.input.channels.max(1);
        let rate = recording.input.rate;
        if recording.samples.is_empty() {
            return;
        }
        let pcm =
            match sue_core::PCMFloat1D::new(rate, channels, std::mem::take(&mut recording.samples))
            {
                Ok(pcm) => pcm,
                Err(error) => {
                    cx.fail(error.to_string());
                    return;
                }
            };
        let bytes = sue_io::encode_wav_as(&pcm, sue_io::SampleDepth::Int24);
        let dir = crate::media::import::cache_dir().join("voice-over");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("voice-over-{}.wav", EntityId::new()));
        if let Err(error) = sue_platform::fs::write(&path, bytes) {
            cx.fail(error.to_string());
            return;
        }
        let ids = studio.import(cx, std::slice::from_ref(&path));
        if let Some(sound) = ids.first() {
            studio.place(cx, *sound, recording.at, recording.track, None, false);
        }
        return;
    }
    let (_, audio) = studio.targets(cx);
    let Some(track) = audio else {
        cx.notify(tr!("record-no-track"));
        return;
    };
    match sue_platform::AudioInput::open(600.0) {
        Ok(input) => {
            studio.recording = Some(crate::studio::Recording {
                input,
                samples: Vec::new(),
                at: studio.clock.position,
                track,
            });
        }
        Err(error) => cx.fail(tr!("record-no-microphone", error = error.to_string())),
    }
}
