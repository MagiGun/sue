//! Inspector: the properties of whatever the active tab shows (characters
//! and lines of a text, pixels and depth of a picture, the values under the
//! hex cursor), as the applet itself reports them.

use sue_platform::InputEvent;
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, FontFace, InputResponse,
    Rect, SueApplet, Ui, Zone,
};

/// The applet id.
pub const ID: &str = "inspector";

/// The context panel in the Right Dock.
#[derive(Debug, Default)]
pub struct Inspector;

impl SueApplet for Inspector {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Inspector id is valid"),
            name: "Inspector".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::RightDock,
            uses_timeline: false,
            summary: "Properties of whatever the active tab shows.".to_owned(),
            opens: Vec::new(),
            icon: "\u{1F50D}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        _event: &InputEvent,
        _ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        Ok(InputResponse::Ignored)
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        if ctx.inspection.is_empty() {
            ui.wrapped(
                area.x,
                area.y,
                area.width,
                "Open a file or a tool to see its properties here.",
                theme.ui_size,
                theme.muted,
            );
            return;
        }
        let names_width = ctx
            .inspection
            .iter()
            .flat_map(|group| &group.properties)
            .map(|property| {
                ui.fonts
                    .measure(FontFace::Ui, theme.small_size, &property.name)
            })
            .fold(0.0_f32, f32::max)
            .min(area.width * 0.45);
        let mut y = area.y;
        for group in &ctx.inspection {
            if y > area.bottom() {
                break;
            }
            ui.label_sized(
                Rect::new(area.x, y, area.width, row),
                &group.title.to_uppercase(),
                theme.small_size,
                theme.muted,
            );
            y += row;
            for property in &group.properties {
                let value_x = area.x + names_width + pad;
                let value_width = (area.right() - value_x).max(1.0);
                let height = ui
                    .wrapped_height(&property.value, value_width, theme.ui_size)
                    .max(ui.line_height());
                ui.label_sized(
                    Rect::new(area.x, y, names_width, ui.line_height()),
                    &property.name,
                    theme.small_size,
                    theme.muted,
                );
                ui.wrapped(
                    value_x,
                    y,
                    value_width,
                    &property.value,
                    theme.ui_size,
                    theme.text,
                );
                y += height + pad / 2.0;
            }
            ui.hline(area.x, y, area.width);
            y += pad;
        }
    }
}
