//! The top bar: the Sue button and the menus of the page.

use egui::{Align, Align2, Id, Layout, Rect, Sense, Ui, UiBuilder, Vec2, vec2};
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::{AppletId, ItemState, LayoutSolution, Theme};

impl Shell {
    /// The top bar: the Sue button and its menu, then the page's menus (or
    /// the word Hub).
    #[allow(clippy::too_many_arguments, reason = "the bar and what is in it")]
    pub(super) fn bar_ui(
        &mut self,
        root: &mut Ui,
        layout: &LayoutSolution,
        page: Option<&AppletId>,
        moving: bool,
        io: &mut ShellIo<'_>,
        theme: &Theme,
        now: Instant,
        dt: f32,
    ) {
        let painter = root.painter().clone();
        painter.rect_filled(layout.bar, 0.0, theme.panel);
        painter.hline(layout.bar.x_range(), layout.bar.bottom(), theme.hairline());

        // The Sue button: Sue's silhouette ("Sue" without a GPU).
        let button = layout.sue_button;
        let response = root
            .interact(button, Id::unique("sue-button"), Sense::click())
            .on_hover_text(tr!("sue-hub-every-studio-tool-files"));
        let open = self.sue_menu.is_open();
        let face = button.shrink2(vec2(4.0, 3.0));
        if open || response.hovered() {
            painter.rect_filled(
                face,
                theme.control_radius,
                if open { theme.active } else { theme.hover },
            );
        }
        let mark = Rect::from_center_size(face.center(), Vec2::splat(face.height() - 4.0));
        if !crate::brand::mark(&painter, &mut *io.textures, mark, theme.text) {
            painter.text(
                face.center(),
                Align2::CENTER_CENTER,
                "Sue",
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
        if response.clicked() && !open {
            self.build_sue_menu(io);
            if let Some(palette) = self.palette.as_mut() {
                palette.close();
            }
        }
        let picked = {
            let runs = &self.sue_runs;
            let state = |command: &str| {
                command
                    .strip_prefix("sue:")
                    .and_then(|index| index.parse::<usize>().ok())
                    .and_then(|index| runs.get(index))
                    .map_or(ItemState::off(), |(_, checked)| {
                        if *checked {
                            ItemState::check(true)
                        } else {
                            ItemState::on()
                        }
                    })
            };
            self.sue_menu.dropdown(
                root,
                button,
                response.clicked(),
                &self.sue_items,
                &state,
                theme,
            )
        };
        if let Some(command) = picked {
            self.run_sue_item(&command, io, now);
        }
        painter.vline(
            button.right(),
            layout.bar.shrink2(vec2(0.0, 6.0)).y_range(),
            theme.hairline(),
        );

        // The page's own menus, or where people are.
        match page {
            Some(id) => {
                let view = self.view(now, dt);
                let mut bar = root.new_child(
                    UiBuilder::new()
                        .max_rect(layout.menus)
                        .id_salt(("sue-bar", id.as_str()))
                        .style(root.ctx().global_style())
                        .layout(Layout::left_to_right(Align::Center)),
                );
                bar.set_clip_rect(layout.menus);
                if moving {
                    bar.set_opacity(self.reveal());
                    bar.disable();
                }
                let _language = sue_lang::scope(&crate::applet_language(&io.world.rice, id));
                let (applets, mut cx) = split(io, theme, view);
                if let Some(applet) = applets.get_mut(id) {
                    applet.bar_ui(&mut bar, &mut cx);
                }
            }
            None => {
                painter.text(
                    layout.menus.left_center() + vec2(theme.padding * 1.5, 0.0),
                    Align2::LEFT_CENTER,
                    tr!("hub"),
                    theme.font(theme.ui_size),
                    theme.muted,
                );
            }
        }
    }
}
