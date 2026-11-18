//! The pages besides Edit, Effects, Color and Audio: the start (no project
//! yet), Media and Deliver.

use egui::{Rect, Ui, UiBuilder, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::Space;
use sue_wm::{AppletContext, FilePick, ShellRequest, Theme};

use super::{SQUARE, monitor};
use crate::export::Target;
use crate::studio::{SueVe, file_name, me};

/// With no project open: how to start.
pub fn start_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    let mut chosen = None;
    sue_wm::ui::empty_state("sve-start", &tr!("start-title"))
        .icon(Icon::AppVideo)
        .detail(&tr!("start-detail"))
        .show_actions(&mut child, theme, |ui| {
            ui.horizontal(|ui| {
                if sue_wm::ui::button("sve-start-new", &tr!("cmd-new-project"))
                    .primary()
                    .show(ui, theme)
                    .clicked()
                {
                    chosen = Some("file.new");
                }
                if sue_wm::ui::button("sve-start-import", &tr!("cmd-import"))
                    .icon(Icon::Import)
                    .show(ui, theme)
                    .clicked()
                {
                    chosen = Some("file.import");
                }
            });
        });
    if let Some(command) = chosen {
        studio.run(command, cx);
    }
}

/// The Media page: the pool large, and the source monitor beside it.
pub fn media_page(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let split = body.left() + body.width() * 0.55;
    let pool = Rect::from_min_max(body.min, pos2(split, body.bottom()));
    let viewer = Rect::from_min_max(pos2(split, body.top()), body.max);
    ui.painter().rect_filled(pool, SQUARE, theme.panel);
    ui.painter().vline(split, body.y_range(), theme.hairline());
    let mut child = ui.new_child(UiBuilder::new().max_rect(pool).id_salt("sve-media-pool"));
    child.set_clip_rect(pool);
    super::pool::pool_ui(studio, &mut child, cx, theme);
    // What is picked in the pool shows in the viewer, with its facts
    // under it.
    if let Some(picked) = studio.picked
        && studio.source.item != Some(picked)
        && crate::model::sequence(cx.arena(), picked).is_none()
    {
        studio.show_source(cx, picked);
    }
    let facts_height = viewer.height() * FACTS_SHARE;
    let monitor = Rect::from_min_max(
        viewer.min,
        pos2(viewer.right(), viewer.bottom() - facts_height),
    );
    let facts = Rect::from_min_max(pos2(viewer.left(), monitor.bottom()), viewer.max);
    monitor::source_monitor(studio, ui, monitor, cx, theme);
    facts_ui(studio, ui, facts, cx, theme);
}

/// The share of the Media page's viewer column the clip's facts take.
const FACTS_SHARE: f32 = 0.28;

/// The facts of the clip in the source monitor: its kind, its file, its
/// length, and what decoding found.
fn facts_ui(studio: &SueVe, ui: &mut Ui, rect: Rect, cx: &AppletContext<'_>, theme: &Theme) {
    ui.painter().rect_filled(rect, SQUARE, theme.panel);
    ui.painter()
        .hline(rect.x_range(), rect.top(), theme.hairline());
    let Some(item) = studio.source.item else {
        return;
    };
    let Ok(entity) = cx.arena().get_by_id(item) else {
        return;
    };
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink(theme.space(Space::S6)))
            .id_salt("sve-media-facts"),
    );
    sue_wm::ui::heading(&mut child, &tr!("media-facts"), theme);
    let kind = crate::model::SourceKind::of(entity);
    let length = crate::model::length_of(cx.arena(), item).map(|length| {
        super::timecode(
            length,
            crate::model::rate_of(entity).unwrap_or(sue_core::Rate::FPS_30),
        )
    });
    let rows = [
        (tr!("field-name"), entity.name().to_owned()),
        (tr!("media-kind"), super::pool::kind_name(&kind)),
        (tr!("media-length"), length.unwrap_or_default()),
        (
            tr!("media-path"),
            crate::model::text(entity, crate::model::key::PATH)
                .unwrap_or_default()
                .to_owned(),
        ),
        (tr!("field-size"), crate::media::import::summary(entity)),
    ];
    for (label, value) in rows {
        child.horizontal(|ui| {
            ui.label(egui::RichText::new(label).color(theme.text_muted()));
            ui.label(value);
        });
    }
}

/// The Deliver page: what to write and where, the program monitor, and
/// the exports under way.
pub fn deliver_page(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let form_width = theme.space(Space::S9) * 9.0;
    let form = Rect::from_min_size(
        body.min,
        vec2(form_width.min(body.width() / 2.0), body.height()),
    );
    let viewer = Rect::from_min_max(pos2(form.right(), body.top()), body.max);
    ui.painter().rect_filled(form, SQUARE, theme.panel);
    ui.painter()
        .vline(form.right(), body.y_range(), theme.hairline());
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(form.shrink(theme.space(Space::S6)))
            .id_salt("sve-deliver"),
    );
    let ui_form = &mut child;
    sue_wm::ui::heading(ui_form, &tr!("deliver-title"), theme);
    let names: Vec<String> = Target::ALL.iter().map(|target| target.name()).collect();
    let mut chosen = Target::ALL
        .iter()
        .position(|target| *target == studio.deliver.target)
        .unwrap_or(0);
    let mut export = false;
    sue_wm::ui::form_grid(ui_form, "sve-deliver-form", theme, |form| {
        form.row(&tr!("deliver-format"), |ui| {
            sue_wm::ui::dropdown("sve-deliver-target", &mut chosen, &names).show(ui, theme);
        });
        form.row(&tr!("field-width"), |ui| {
            sue_wm::ui::number_field("sve-deliver-width", &mut studio.deliver.width)
                .range(16.0..=4096.0)
                .decimals(0)
                .suffix(" px")
                .show(ui, theme);
        });
        form.row(&tr!("field-height"), |ui| {
            sue_wm::ui::number_field("sve-deliver-height", &mut studio.deliver.height)
                .range(16.0..=4096.0)
                .decimals(0)
                .suffix(" px")
                .show(ui, theme);
        });
        form.row(&tr!("deliver-bitrate"), |ui| {
            sue_wm::ui::number_field("sve-deliver-bitrate", &mut studio.deliver.megabits)
                .range(0.0..=200.0)
                .decimals(1)
                .suffix(" Mbit/s")
                .show(ui, theme);
        });
        form.row(&tr!("deliver-range"), |ui| {
            if sue_wm::ui::switch(
                "sve-deliver-whole",
                &tr!("deliver-whole"),
                studio.deliver.whole,
            )
            .show(ui, theme)
            .clicked()
            {
                studio.deliver.whole = !studio.deliver.whole;
            }
        });
        form.row(&tr!("deliver-sound"), |ui| {
            if sue_wm::ui::switch(
                "sve-deliver-sound",
                &tr!("deliver-with-sound"),
                studio.deliver.sound,
            )
            .show(ui, theme)
            .clicked()
            {
                studio.deliver.sound = !studio.deliver.sound;
            }
        });
    });
    ui_form.add_space(theme.space(Space::S6));
    ui_form.label(tr!("deliver-bitrate-note"));
    ui_form.add_space(theme.space(Space::S6));
    if sue_wm::ui::button("sve-deliver-export", &tr!("deliver-export"))
        .primary()
        .icon(Icon::Export)
        .enabled(studio.sequence.is_some())
        .show(ui_form, theme)
        .clicked()
    {
        export = true;
    }
    studio.deliver.target = Target::ALL.get(chosen).copied().unwrap_or(Target::Mp4);
    // The exports of this run.
    ui_form.add_space(theme.space(Space::S6));
    for running in &studio.exports {
        let progress = match cx
            .transfers
            .get(running.transfer)
            .map(|transfer| transfer.state.clone())
        {
            Some(sue_wm::TransferState::Running(done)) => done,
            _ => 1.0,
        };
        ui_form.label(file_name(&running.path));
        sue_wm::ui::progress_bar(progress).show(ui_form, theme);
    }
    if let Some((path, report)) = &studio.last_report {
        ui_form.label(tr!(
            "deliver-done",
            name = file_name(path),
            frames = report.frames as i64,
            seconds = format!("{:.1}", report.seconds)
        ));
    }
    if export {
        let name = studio
            .sequence
            .and_then(|id| cx.arena().get_by_id(id).ok())
            .map_or_else(
                || tr!("untitled-project"),
                |entity| entity.name().to_owned(),
            );
        let extension = studio.deliver.target.extension();
        let pick = FilePick::save(
            Some(me()),
            "export",
            &tr!("deliver-export"),
            &format!("{name}.{extension}"),
        )
        .extensions(&[extension]);
        if cx.view.interactive {
            cx.request(ShellRequest::PickFile(pick));
        } else {
            // Without a window (tests, the snapshot tour) it is written to
            // Sue VE's own folder.
            let path = crate::media::import::cache_dir()
                .join("exports")
                .join(format!("{name}.{extension}"));
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            studio.export_to(cx, &path);
        }
    }
    monitor::program_monitor(studio, ui, viewer, cx, theme);
}
