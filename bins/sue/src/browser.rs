//! Browser: an address bar and quick bookmarks. Pages open in the system
//! browser; Sue has no embedded web engine yet, and says so.

use sue_platform::{InputEvent, web_url};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, EditOutcome,
    FontFace, InputResponse, LineEdit, Rect, ShellRequest, SueApplet, Ui, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "browser";

/// The reference panel in the Right Dock.
#[derive(Debug, Default)]
pub struct Browser {
    address: LineEdit,
    editing: bool,
    recent: Vec<String>,
    error: Option<String>,
}

/// `https://docs.rs/serde` shows as `docs.rs/serde`.
fn short(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.trim_end_matches('/')
}

impl Browser {
    fn open(&mut self, text: &str, ctx: &mut AppletContext<'_>) {
        match web_url(text) {
            Ok(url) => {
                self.error = None;
                self.recent.retain(|seen| seen != &url);
                self.recent.insert(0, url.clone());
                self.recent.truncate(8);
                ctx.request(ShellRequest::OpenUrl(url));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

impl SueApplet for Browser {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Browser id is valid"),
            name: "Browser".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::RightDock,
            uses_timeline: false,
            summary: "Look things up: an address bar and quick bookmarks.".to_owned(),
            opens: Vec::new(),
            icon: "\u{1F310}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let InputEvent::Keyboard(key) = event else {
            return Ok(InputResponse::Ignored);
        };
        if !self.editing {
            return Ok(InputResponse::Ignored);
        }
        match self.address.handle(key) {
            EditOutcome::Submit => {
                let text = self.address.text.clone();
                self.open(&text, ctx);
                self.editing = false;
            }
            EditOutcome::Cancel => self.editing = false,
            EditOutcome::Ignored => return Ok(InputResponse::Ignored),
            EditOutcome::Changed | EditOutcome::Moved => {}
        }
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        if !ctx.focused {
            self.editing = false;
        }
        let go_width = ui.fonts.measure(FontFace::Ui, theme.ui_size, "Open") + pad * 3.0;
        let field = Rect::new(area.x, area.y, area.width - go_width - pad / 2.0, row);
        let response = ui.text_field(
            widget_id("browser", "address"),
            field,
            &self.address.text,
            self.editing.then_some(self.address.caret),
            "Search or type an address",
            FontFace::Ui,
        );
        if response.clicked {
            self.editing = true;
        }
        let go = Rect::new(field.right() + pad / 2.0, area.y, go_width, row);
        if ui.primary_button(
            widget_id("browser", "go"),
            go,
            "Open",
            !self.address.text.is_empty(),
        ) {
            let text = self.address.text.clone();
            self.open(&text, ctx);
        }
        let mut y = area.y + row + pad / 2.0;
        if let Some(error) = &self.error {
            y +=
                ui.wrapped(area.x, y, area.width, error, theme.small_size, theme.alert) + pad / 2.0;
        }
        y += ui.wrapped(
            area.x,
            y,
            area.width,
            "Pages open in your system browser.",
            theme.small_size,
            theme.muted,
        ) + pad;

        let bookmarks: Vec<String> = ctx
            .rice()
            .get_str("browser.bookmarks")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|bookmark| !bookmark.is_empty())
            .map(str::to_owned)
            .collect();
        let mut clicked = None;
        for (title, list) in [("BOOKMARKS", &bookmarks), ("RECENT", &self.recent)] {
            if list.is_empty() {
                continue;
            }
            ui.label_sized(
                Rect::new(area.x, y, area.width, row),
                title,
                theme.small_size,
                theme.muted,
            );
            y += row;
            for url in list {
                if y + row > area.bottom() {
                    break;
                }
                let rect = Rect::new(area.x, y, area.width, row);
                let response =
                    ui.interact(widget_id("browser/link", &format!("{title}{url}")), rect);
                if response.hovered {
                    ui.canvas
                        .draw_box(rect, BoxStyle::filled(theme.hover, theme.radius));
                    ui.set_cursor(sue_platform::CursorShape::Pointer);
                }
                ui.label(
                    Rect::new(rect.x + pad / 2.0, rect.y, rect.width - pad, row),
                    short(url),
                    theme.accent,
                );
                ui.tooltip(response, rect, url);
                if response.clicked {
                    clicked = Some(url.clone());
                }
                y += row;
            }
            y += pad;
        }
        if let Some(url) = clicked {
            self.address = LineEdit::new(url.clone());
            self.open(&url, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_read_without_their_scheme() {
        assert_eq!(short("https://docs.rs/"), "docs.rs");
        assert_eq!(short("http://example.com/a"), "example.com/a");
    }
}
