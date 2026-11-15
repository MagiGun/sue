//! The center of the studio: the text, a figure's table and composition
//! mode.

use egui::{Align2, CornerRadius, Rect, Ui, UiBuilder, Vec2, vec2};
use std::collections::HashMap;
use sue_core::{Entity, EntityId, Value};
use sue_model::manuscript;
use sue_model::{mark_kind as kind, picture};
use sue_type::{EditorView, Ink, ViewMode};
use sue_wm::icons::Icon;
use sue_wm::ui::{Space, TextRole, button, empty_state};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::project;
use crate::studio::{Figure, SueWriter, View};

use super::PHONE_GAP;
use super::widgets::{desk, texture_key};

impl SueWriter {
    pub(super) fn center_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        if self.project.is_none() && self.open.is_none() {
            self.start_ui(ui, rect, cx, theme);
            return;
        }
        // A picture kept in Research shows as itself.
        if let Some(open) = self.open
            && cx.arena().get_by_id(open).is_ok_and(|entity| {
                !manuscript::is_document(entity)
                    && !manuscript::is_folder(entity)
                    && !manuscript::is_manuscript(entity)
                    && entity.string_buffer().is_none()
            })
            && self.view == View::Document
        {
            self.research_item_ui(ui, rect, open, cx, theme);
            return;
        }
        match self.view {
            View::Document => self.text_ui(ui, rect, cx, theme, false),
            View::Corkboard => self.corkboard_ui(ui, rect, cx, theme),
            View::Outliner => self.outliner_ui(ui, rect, cx, theme),
        }
    }

    /// Textures for the pictures the shown documents embed.
    fn figure_table(
        &mut self,
        docs: &[EntityId],
        cx: &mut AppletContext<'_>,
    ) -> HashMap<EntityId, (Option<egui::TextureId>, Vec2)> {
        let mut wanted = Vec::new();
        for doc in docs {
            let marks = sue_type::edit::marks_of(cx.arena(), *doc);
            for mark in marks.marks() {
                if mark.kind == kind::EMBED
                    && let Value::Entity(id) = mark.value
                {
                    wanted.push(id);
                }
            }
        }
        let mut table = HashMap::new();
        for id in wanted {
            let seen = if cx.arena().get_by_id(id).is_ok_and(picture::is_picture) {
                sue_picture::composite::signature(cx.arena(), id)
                    .iter()
                    .map(|(_, revision)| *revision)
                    .sum::<u64>()
            } else {
                cx.arena().get_by_id(id).map_or(0, Entity::revision)
            };
            let fresh = self
                .figures
                .get(&id)
                .is_some_and(|figure| figure.seen == seen);
            if !fresh
                && let Some((pixels, [width, height])) =
                    crate::compile::picture_pixels(cx.arena(), id, 1200)
            {
                self.figures.insert(
                    id,
                    Figure {
                        texture: None,
                        size: vec2(width as f32, height as f32) * 0.75,
                        seen,
                        pixels,
                        pixel_size: [width, height],
                    },
                );
            }
            if let Some(figure) = self.figures.get_mut(&id) {
                // Every frame: an unused texture is let go.
                figure.texture = cx.textures.texture(
                    texture_key(id),
                    figure.seen,
                    figure.pixel_size,
                    4,
                    &figure.pixels,
                    false,
                );
                table.insert(id, (figure.texture, figure.size));
            }
        }
        table
    }

    fn text_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        composing: bool,
    ) {
        let docs = self.shown_docs(cx);
        if docs.is_empty() {
            let room = rect.shrink(theme.space(Space::S9));
            let mut add = false;
            ui.scope_builder(UiBuilder::new().max_rect(room), |ui| {
                empty_state("writer-empty", &tr!("nothing-write-here-yet"))
                    .icon(Icon::PageEmpty)
                    .show_actions(ui, theme, |ui| {
                        add = button("writer-empty-new", &tr!("new-document"))
                            .primary()
                            .show(ui, theme)
                            .clicked();
                    });
            });
            if add {
                self.add_item(cx, false);
            }
            return;
        }
        let sheet = self.sheet(cx);
        let page = self.page(cx);
        let titles: Vec<String> = docs
            .iter()
            .map(|id| {
                cx.arena()
                    .get_by_id(*id)
                    .map_or_else(|_| String::new(), |entity| entity.name().to_owned())
            })
            .collect();
        let figures = self.figure_table(&docs, cx);
        let lookup = |id: EntityId| figures.get(&id).copied();
        let dark = cx.rice().get_bool("sue_writer.dark_paper").unwrap_or(false) || composing;
        // Composition mode writes on the theme's own page, the paper a
        // step above the backdrop; dark paper takes the pill color.
        let ink = if dark {
            Ink {
                paper: if composing {
                    theme.surface(1)
                } else {
                    theme.pill
                },
                text: theme.text,
                soft: theme.text_muted(),
                link: theme.link(),
                shade: theme.fill_hover(),
                colors: false,
            }
        } else {
            Ink::paper()
        };
        // The page fits the space, until people zoom themselves. A phone
        // shows its whole page width too; asked for Draft View it reflows
        // the text to its screen at a size that reads (as Word's mobile
        // view does).
        let pages = self.pages_shown() && !composing;
        let reflow = self.phone && !pages;
        let phone_zoom = if self.fit { 1.0 } else { self.zoom };
        let (mode, across) = if pages {
            (ViewMode::Page, page.size[0])
        } else if reflow {
            let width =
                ((rect.width() - 2.0 * PHONE_GAP) / (phone_zoom * sue_type::PX_PER_PT)).max(120.0);
            (ViewMode::Draft { width }, width)
        } else if composing {
            (ViewMode::Draft { width: 560.0 }, 600.0)
        } else {
            (
                ViewMode::Draft {
                    width: page.text_width(),
                },
                page.text_width() + 60.0,
            )
        };
        // Never smaller than can be read, past which the page scrolls
        // across; a phone's or a tablet's page shrinks further to be seen
        // whole, upright or on its side.
        let least = if cx.view.form_factor == sue_platform::FormFactor::Desktop {
            0.6
        } else {
            0.2
        };
        let fit = ((rect.width() - 56.0) / (across * sue_type::PX_PER_PT)).clamp(least, 3.0);
        let zoom = if reflow {
            phone_zoom
        } else if self.fit {
            fit.min(if composing { 1.3 } else { 1.0 })
        } else {
            self.zoom
        };
        self.shown_zoom = zoom;
        let smart_quotes = cx
            .rice()
            .get_bool("sue_writer.smart_quotes")
            .unwrap_or(true);
        let own_words = self.own_words(cx);
        let also = |word: &str| own_words.contains(word);
        let spelling = self.spell_check(cx) && !composing;
        let view = EditorView {
            spelling: spelling.then_some(&also as &dyn Fn(&str) -> bool),
            mode,
            page: &page,
            zoom,
            sheet: &sheet,
            ink,
            desk: if composing || dark {
                theme.void
            } else {
                desk(theme)
            },
            figures: &lookup,
            typewriter: self.typewriter || composing,
            smart_quotes,
            read_only: false,
            titles: &titles,
            gap: reflow.then_some(PHONE_GAP),
        };
        let mut area = ui.new_child(UiBuilder::new().max_rect(rect));
        let tracking = self
            .tracking(cx)
            .then(|| sue_model::changes::Change::new(&self.author_name(cx), &project::today()));
        self.editor.set_tracking(tracking);
        let output = self.editor.show(&mut area, cx, &docs, &view);
        if let Some(factor) = output.zoom {
            self.fit = false;
            self.zoom = (zoom * factor).clamp(0.2, 4.0);
        }
        if let Some(word) = output.learn.clone() {
            self.learn(cx, &word);
        }
        if let Some(link) = output.link {
            cx.request(ShellRequest::Command(format!("open-url:{link}")));
        }
        self.page_info = (output.page, output.pages);
        if let Some(current) = self.editor.current()
            && self.selected != Some(current)
            && docs.len() > 1
            && output.focused
        {
            self.selected = Some(current);
        }
    }

    pub(super) fn composition_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.void);
        self.text_ui(ui, rect, cx, theme, true);
        let (_, session) = self.counts(cx);
        let words = self
            .editor
            .current()
            .map_or(0, |doc| manuscript::words(cx.arena(), doc));
        let hint = tr!(
            "words-today-esc-to-leave",
            words = words,
            session = format!("{:+}", session)
        );
        ui.painter().text(
            rect.center_bottom() - Vec2::Y * theme.space(Space::S6),
            Align2::CENTER_BOTTOM,
            hint,
            theme.text_font(TextRole::Caption),
            theme.text_muted(),
        );
    }

    // ----- Dialogs ----------------------------------------------------------------------
}

/// Commands after which the text gets the keyboard back: they change how
/// it looks, or add to it, without asking anything.
pub(super) fn keeps_typing(command: &str) -> bool {
    [
        "format.", "align.", "list.", "indent.", "spacing.", "style.",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix))
        || matches!(
            command,
            "edit.undo" | "edit.redo" | "insert.page_break" | "insert.scene_break" | "insert.date"
        )
}
