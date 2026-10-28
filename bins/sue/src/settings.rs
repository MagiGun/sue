//! Settings: the auto-generated rice GUI. Every `.suerice` parameter gets a
//! live control from `SettingsTree`; edits are undoable rice transactions,
//! and slider drags preview live and record one undo step on release.

use sue_platform::{InputEvent, Key, NamedKey, Point};
use sue_rice::{Control, Keybind, RiceParameter, RiceValue, SetRiceValue, SettingsTree};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, Color,
    FontFace, InputResponse, Rect, SliderEvent, SueApplet, Ui, Zone, widget_id,
};

/// The applet id.
pub const ID: &str = "settings";

/// The rice GUI in the Right Dock.
#[derive(Debug, Default)]
pub struct Settings {
    scroll: f32,
    editing: Option<(String, String)>,
    recording: Option<String>,
    drag: Option<(String, RiceValue)>,
    error: Option<String>,
}

fn slider_value(value: f64, integer: bool) -> RiceValue {
    if integer {
        RiceValue::Int(value.round() as i64)
    } else {
        RiceValue::Float((value * 100.0).round() / 100.0)
    }
}

fn value_text(value: f64, integer: bool) -> String {
    if integer {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

impl Settings {
    fn commit(&mut self, ctx: &mut AppletContext<'_>, path: &str, value: RiceValue) {
        match ctx.edit_rice(SetRiceValue::new(path, value)) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// The value typed into a field, typed like the parameter it edits.
    fn typed_value(ctx: &AppletContext<'_>, path: &str, text: &str) -> Result<RiceValue, String> {
        match ctx.rice().parameter(path) {
            Some(RiceParameter::IntRange { .. }) => text
                .trim()
                .parse()
                .map(RiceValue::Int)
                .map_err(|_| format!("`{text}` is not a whole number")),
            Some(RiceParameter::FloatRange { .. }) => text
                .trim()
                .parse()
                .map(RiceValue::Float)
                .map_err(|_| format!("`{text}` is not a number")),
            _ => Ok(RiceValue::Text(text.to_owned())),
        }
    }

    fn control_height(control: &Control, line: f32) -> f32 {
        match control {
            Control::Dropdown { options, .. } => line * 0.9 * options.len() as f32,
            _ => line * 0.9,
        }
    }
}

impl SueApplet for Settings {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Settings id is valid"),
            name: "Settings".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::RightDock,
            uses_timeline: false,
            summary: "Every .suerice parameter as a live control.".to_owned(),
            opens: Vec::new(),
            accepts_drops: false,
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
        if !key.pressed {
            return Ok(InputResponse::Ignored);
        }
        let done = InputResponse::Consumed { redraw: true };
        if let Some(path) = self.recording.clone() {
            self.recording = None;
            if key.key != Key::Named(NamedKey::Escape) {
                let chord = Keybind::new(key.modifiers, key.key.normalized());
                self.commit(ctx, &path, RiceValue::Keybind(chord));
            }
            return Ok(done);
        }
        let Some((path, mut text)) = self.editing.take() else {
            return Ok(InputResponse::Ignored);
        };
        match key.key {
            Key::Named(NamedKey::Escape) => return Ok(done),
            Key::Named(NamedKey::Enter) => {
                match Self::typed_value(ctx, &path, &text) {
                    Ok(value) => self.commit(ctx, &path, value),
                    Err(reason) => {
                        self.error = Some(reason);
                        self.editing = Some((path, text));
                    }
                }
                return Ok(done);
            }
            Key::Named(NamedKey::Backspace) => {
                text.pop();
            }
            _ => match &key.text {
                Some(typed) if !key.modifiers.is_command() => text.push_str(typed),
                _ => {
                    self.editing = Some((path, text));
                    return Ok(InputResponse::Ignored);
                }
            },
        }
        self.editing = Some((path, text));
        Ok(done)
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let tree = SettingsTree::build(ctx.rice());
        let line = ui.row_height();
        let pad = theme.padding();
        let mut content = 0.0;
        for section in &tree.sections {
            content += line;
            for widget in &section.widgets {
                content += line + Self::control_height(&widget.control, line) + pad / 2.0;
            }
        }
        if self.error.is_some() {
            content += line;
        }
        self.scroll =
            (self.scroll - ui.wheel_in(area)).clamp(0.0, (content - area.height).max(0.0));
        let mut y = area.y - self.scroll;
        if let Some(error) = self.error.clone() {
            ui.label(Rect::new(area.x, y, area.width, line), &error, theme.alert);
            y += line;
        }
        let half = area.width / 2.0;
        for section in &tree.sections {
            ui.label(
                Rect::new(area.x, y, area.width, line),
                &section.title.to_uppercase(),
                theme.accent,
            );
            y += line;
            for widget in &section.widgets {
                let height = Self::control_height(&widget.control, line);
                if y + line + height < area.y || y > area.bottom() {
                    y += line + height + pad / 2.0;
                    continue;
                }
                let binding = widget.binding.as_str();
                let id = widget_id("settings", binding);
                let label_rect = Rect::new(area.x, y, area.width, line);
                ui.label(label_rect, &widget.label, theme.text);
                let control = Rect::new(area.x, y + line * 0.95, area.width, height);
                let editing = self
                    .editing
                    .as_ref()
                    .filter(|(path, _)| path == binding)
                    .map(|(_, text)| text.clone());
                match &widget.control {
                    Control::Toggle { on } => {
                        let switch = Rect::new(control.x, control.y, line * 2.0, control.height);
                        if ui.toggle(id, switch, *on) {
                            self.commit(ctx, binding, RiceValue::Bool(!on));
                        }
                    }
                    Control::Slider {
                        value,
                        min,
                        max,
                        integer,
                        ..
                    } => {
                        let shown = value_text(*value, *integer);
                        let width = ui.fonts.measure(FontFace::Mono, theme.mono_size, &shown);
                        ui.text(
                            Point::new(
                                area.right() - width,
                                y + (line - ui.fonts.line_height(FontFace::Mono, theme.mono_size))
                                    / 2.0,
                            ),
                            &shown,
                            FontFace::Mono,
                            theme.mono_size,
                            theme.muted,
                        );
                        let track = Rect::new(
                            control.x + pad / 2.0,
                            control.y,
                            control.width - pad,
                            control.height,
                        );
                        match ui.slider(id, track, *value, *min, *max) {
                            SliderEvent::Preview(new) => {
                                if self.drag.as_ref().is_none_or(|(path, _)| path != binding)
                                    && let Some(start) = ctx.rice().get(binding)
                                {
                                    self.drag = Some((binding.to_owned(), start));
                                }
                                let _ = ctx.world.rice.set(binding, slider_value(new, *integer));
                            }
                            SliderEvent::Commit(new) => {
                                if let Some((path, start)) = self.drag.take()
                                    && path == binding
                                {
                                    let _ = ctx.world.rice.set(&path, start);
                                }
                                self.commit(ctx, binding, slider_value(new, *integer));
                            }
                            SliderEvent::Idle => {}
                        }
                    }
                    Control::NumberField { value, integer } => {
                        let text = editing
                            .clone()
                            .unwrap_or_else(|| value_text(*value, *integer));
                        if ui.text_field(
                            id,
                            control,
                            &text,
                            editing.is_some() && ctx.focused,
                            FontFace::Mono,
                        ) {
                            self.editing = Some((binding.to_owned(), value_text(*value, *integer)));
                        }
                    }
                    Control::ColorPicker { rgba } => {
                        let swatch =
                            Rect::new(control.x, control.y, control.height * 1.6, control.height);
                        let field = Rect::new(
                            swatch.right() + pad / 2.0,
                            control.y,
                            (half).min(control.width - swatch.width - pad),
                            control.height,
                        );
                        let start_edit =
                            ui.swatch(id, swatch, Color::from_rgba(*rgba), editing.is_some());
                        let text = editing.clone().unwrap_or_else(|| rgba.to_hex());
                        let clicked = ui.text_field(
                            id.wrapping_add(1),
                            field,
                            &text,
                            editing.is_some() && ctx.focused,
                            FontFace::Mono,
                        );
                        if start_edit || clicked {
                            self.editing = Some((binding.to_owned(), rgba.to_hex()));
                        }
                    }
                    Control::Dropdown { options, selected } => {
                        if let Some(picked) = ui.choices(id, control, options, *selected)
                            && picked != *selected
                        {
                            self.commit(ctx, binding, RiceValue::Text(options[picked].clone()));
                        }
                    }
                    Control::KeybindRecorder { chord } => {
                        let recording = self.recording.as_deref() == Some(binding);
                        let text = if recording {
                            "Press a chord…".to_owned()
                        } else {
                            chord.clone()
                        };
                        let button = Rect::new(
                            control.x,
                            control.y,
                            half.min(control.width),
                            control.height,
                        );
                        if ui.button(id, button, &text) {
                            self.recording = Some(binding.to_owned());
                        }
                    }
                    Control::ShaderPicker { path, .. } => {
                        let text = editing.clone().unwrap_or_else(|| path.clone());
                        if ui.text_field(
                            id,
                            control,
                            &text,
                            editing.is_some() && ctx.focused,
                            FontFace::Mono,
                        ) {
                            self.editing = Some((binding.to_owned(), path.clone()));
                        }
                    }
                    Control::TextField { text: current } => {
                        let text = editing.clone().unwrap_or_else(|| current.clone());
                        if ui.text_field(
                            id,
                            control,
                            &text,
                            editing.is_some() && ctx.focused,
                            FontFace::Ui,
                        ) {
                            self.editing = Some((binding.to_owned(), current.clone()));
                        }
                    }
                }
                if let Some(tooltip) = &widget.tooltip
                    && ui.state.hot == Some(id)
                {
                    let tip = Rect::new(area.x, control.bottom(), area.width, line);
                    ui.canvas
                        .draw_box(tip, BoxStyle::filled(theme.raised, theme.radius / 2.0));
                    ui.label(
                        Rect::new(tip.x + pad / 2.0, tip.y, tip.width - pad, tip.height),
                        tooltip,
                        theme.muted,
                    );
                }
                y += line + height + pad / 2.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_values_snap_to_the_parameter_type() {
        assert_eq!(slider_value(7.6, true), RiceValue::Int(8));
        assert_eq!(slider_value(0.856, false), RiceValue::Float(0.86));
        assert_eq!(value_text(170.0, false), "170.00");
        assert_eq!(value_text(8.2, true), "8");
    }
}
