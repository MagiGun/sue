//! The top bar: the Sue button and the menus of the page, or over the
//! Hub its search field and its Transfers and Settings buttons.

use egui::{Align, Align2, Id, Layout, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use sue_platform::time::Instant;

use super::{Shell, ShellIo, split};
use crate::icons::Icon;
use crate::ui::{Density, Radius, Space, TextRole};
use crate::{AppletId, HubButton, ItemState, LayoutSolution, ShellRequest, Theme};

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
            None => self.hub_bar(root, layout, io, theme),
        }
    }

    /// The bar over the Hub: a search field that opens the command
    /// palette, then Transfers and Settings at the right.
    fn hub_bar(
        &mut self,
        root: &mut Ui,
        layout: &LayoutSolution,
        io: &mut ShellIo<'_>,
        theme: &Theme,
    ) {
        let area = layout.menus.shrink2(vec2(theme.space(Space::S4), 0.0));
        let density = Density::of(root);
        let side = density
            .icon_button()
            .min(layout.bar.height() - theme.space(Space::S1) * 2.0);
        let gap = theme.space(Space::S2);
        // The two buttons, from the right.
        let settings = Rect::from_center_size(
            pos2(area.right() - side / 2.0, area.center().y),
            Vec2::splat(side),
        );
        let transfers = settings.translate(vec2(-(side + gap), 0.0));
        let mut clicked = None;
        for (button, rect, icon, name) in [
            (
                HubButton::Transfers,
                transfers,
                Icon::Transfers,
                tr!("files-transfers"),
            ),
            (
                HubButton::Settings,
                settings,
                Icon::Settings,
                tr!("settings"),
            ),
        ] {
            let response = crate::ui::icon_button(("sue-hub-bar", name.as_str()), icon, &name)
                .quiet()
                .show_at(root, rect, theme);
            self.hub.buttons.push((button.clone(), rect));
            if response.clicked() {
                clicked = Some(button);
            }
        }
        // The search field: as wide as there is room for, up to a calm
        // width, in the middle of what is left.
        let room = Rect::from_min_max(area.min, pos2(transfers.left() - gap * 2.0, area.max.y));
        let height = side.min(room.height());
        let width = room.width().min(density.row() * 16.0);
        if width >= height * 2.0 {
            let field = Rect::from_center_size(
                pos2(room.left() + width / 2.0, room.center().y),
                vec2(width, height),
            );
            let response = root
                .interact(field, Id::unique("sue-hub-search"), Sense::click())
                .on_hover_text(tr!("hub-search-hint"));
            let over = response.hovered() || response.has_focus();
            let t = crate::ui::motion::hover_t(root.ctx(), response.id.with("lit"), over);
            let painter = root.painter();
            let fill = theme.surface(2).lerp_to_gamma(theme.fill_hover(), t);
            painter.rect(
                field,
                theme.radius(Radius::Full),
                fill,
                theme.hairline(),
                egui::StrokeKind::Inside,
            );
            let pad = theme.space(Space::S5);
            let icon_side = density.icon();
            let ink = theme.text_muted_on(fill);
            crate::icons::paint(
                painter,
                Rect::from_center_size(
                    pos2(field.left() + pad + icon_side / 2.0, field.center().y),
                    Vec2::splat(icon_side),
                ),
                Icon::Search,
                ink,
            );
            let text = painter.with_clip_rect(field.shrink(theme.space(Space::S1)));
            text.text(
                pos2(
                    field.left() + pad + icon_side + theme.space(Space::S4),
                    field.center().y,
                ),
                Align2::LEFT_CENTER,
                tr!("hub-search"),
                theme.text_font(TextRole::Body),
                ink,
            );
            crate::ui::focus_ring(root, &response, theme.radius(Radius::Full), theme);
            self.hub.buttons.push((HubButton::Search, field));
            if response.clicked() {
                clicked = Some(HubButton::Search);
            }
        } else {
            let rect = Rect::from_center_size(
                pos2(room.left() + side / 2.0, room.center().y),
                Vec2::splat(side),
            );
            let name = tr!("hub-search");
            let response = crate::ui::icon_button("sue-hub-search-button", Icon::Search, &name)
                .quiet()
                .show_at(root, rect, theme);
            self.hub.buttons.push((HubButton::Search, rect));
            if response.clicked() {
                clicked = Some(HubButton::Search);
            }
        }
        match clicked {
            Some(HubButton::Search) => self.open_palette(),
            Some(HubButton::Transfers) => self.hub_transfers(),
            Some(HubButton::Settings) => io.requests.push(ShellRequest::OpenSettings(None)),
            _ => {}
        }
    }
}
