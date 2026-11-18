//! Sue Paint's New canvas page on phones and tablets: the categories as
//! tabs and their templates as cards that scroll sideways, from the
//! [`crate::templates::CanvasTemplates`] registry, with Advanced Settings
//! (the full New dialog) and Import. The empty canvas area's New opens
//! it on a touch screen; the desktop gets the New dialog.
//!
//! Sue 0.0.8 removed the rest of what was here, the start page's tabs of
//! projects and materials: they are the Hub's Files and Materials places
//! (owner note 11).

use egui::{Rect, Sense, Ui, UiBuilder, Vec2};
use sue_wm::icons::Icon;
use sue_wm::start::{self as page, Preview};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest, Theme};

use crate::studio::SuePaint;
use crate::templates::{Make, Work};

/// The movies an animation can start from.
const MOVIES: &[&str] = &["gif", "png", "apng", "webp", "avi"];

fn me() -> AppletId {
    AppletId::new(crate::ID).expect("a valid id")
}

/// The New canvas page's state.
#[derive(Debug, Default)]
pub(crate) struct NewCanvas {
    /// The page shows, on this category.
    pub(crate) category: Option<usize>,
    /// The device's screen in pixels, for the Screen size template.
    pub(crate) screen: [u32; 2],
}

impl NewCanvas {
    /// Opens or closes the page by name, for scripts and snapshots:
    /// `new-canvas`, `new-canvas/<category>`, `new-canvas/close`.
    pub(crate) fn navigate(&mut self, target: &str) -> bool {
        match target {
            "new-canvas" => self.category = Some(0),
            "new-canvas/close" => self.category = None,
            other => match other
                .strip_prefix("new-canvas/")
                .and_then(|n| n.parse().ok())
            {
                Some(category) => self.category = Some(category),
                None => return false,
            },
        }
        true
    }
}

/// What the page asks the studio for, after drawing.
enum Act {
    /// Make a canvas from the template with this id.
    Template(String),
    /// The full New dialog, for a category.
    Advanced(usize),
    /// A picture from the device becomes a new canvas.
    ImportImage,
}

impl SuePaint {
    /// The New canvas page over `rect`, while it is open.
    pub(crate) fn new_canvas_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(category) = self.new_canvas.category else {
            return;
        };
        let pixels = ui.ctx().content_rect().size() * ui.ctx().pixels_per_point();
        self.new_canvas.screen = [pixels.x.round() as u32, pixels.y.round() as u32];
        let mut acts = Vec::new();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt("sue-paint-new-canvas")
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect);
        child.multiply_opacity(self.cover.max(0.05));
        if !self.ready {
            child.disable();
        }
        child.painter().rect_filled(rect, 0.0, theme.void);
        // Nothing under the page takes a click.
        child.allocate_rect(rect, Sense::click_and_drag());
        let side = ((rect.width() - 24.0) / 3.3).clamp(96.0, 150.0);
        let mut area = child.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        area.set_clip_rect(rect);
        let width = rect.width() - 24.0;
        area.spacing_mut().item_spacing.y = 10.0;
        // The back arrow, the title, Import and Advanced Settings.
        area.horizontal(|ui| {
            let (back, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
            if page::icon_button(ui, back, Icon::ArrowLeft, &tr!("start-back"), false, theme)
                .clicked()
            {
                self.new_canvas.category = None;
            }
        });
        area.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("start-new-canvas"))
                    .size(theme.heading_size * 1.2)
                    .color(theme.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page::outline_button(ui, None, &tr!("start-advanced-settings"), theme).clicked()
                {
                    acts.push(Act::Advanced(category));
                }
                if page::outline_button(ui, None, &tr!("start-import"), theme).clicked() {
                    acts.push(Act::ImportImage);
                }
            });
        });
        let categories: Vec<(String, String)> = self.canvas_templates.categories().to_vec();
        let labels: Vec<String> = categories
            .iter()
            .map(|(_, name)| crate::localize(name))
            .collect();
        if let Some(tab) = page::tabs(&mut area, width, &labels, category, theme) {
            self.new_canvas.category = Some(tab);
        }
        let screen = self.new_canvas.screen;
        if let Some((id, _)) = categories.get(category) {
            let templates: Vec<crate::templates::CanvasTemplate> =
                self.canvas_templates.of(id).cloned().collect();
            egui::ScrollArea::horizontal()
                .id_salt(("sue-paint-templates", category))
                .show(&mut area, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for template in &templates {
                            let preview = match template.make {
                                Make::FromImage => Preview::Icon(Icon::Picture),
                                Make::FromMovie => Preview::Icon(Icon::Film),
                                Make::Custom => Preview::Icon(Icon::Plus),
                                Make::Size { .. } | Make::Screen => {
                                    let picture = template.picture(screen);
                                    Preview::Paper(picture.map_or(1.0, |picture| {
                                        picture.width as f32 / picture.height.max(1) as f32
                                    }))
                                }
                            };
                            let name = if template.name.is_empty() {
                                template.size_text(screen)
                            } else {
                                crate::localize(&template.name)
                            };
                            let detail = if template.name.is_empty() {
                                String::new()
                            } else {
                                template.size_text(screen)
                            };
                            if page::tile(ui, side, preview, &name, &detail, theme).clicked() {
                                acts.push(Act::Template(template.id.clone()));
                            }
                        }
                    });
                });
        }
        for act in acts {
            match act {
                Act::Template(id) => self.make_from_template(&id, cx),
                Act::Advanced(category) => {
                    self.new_canvas.category = None;
                    self.advanced_new(category, cx);
                }
                Act::ImportImage => {
                    self.new_canvas.category = None;
                    self.pick_image(cx, false);
                }
            }
        }
    }

    /// Makes a canvas from the template `id`, or asks for what it needs.
    pub(crate) fn make_from_template(&mut self, id: &str, cx: &mut AppletContext<'_>) {
        let Some(template) = self.canvas_templates.get(id).cloned() else {
            return;
        };
        match template.make {
            Make::FromImage => self.pick_image(cx, false),
            Make::FromMovie => cx.request(ShellRequest::PickFile(
                FilePick::open(Some(me()), "new-from-movie", &tr!("import-movie-2"))
                    .extensions(MOVIES),
            )),
            Make::Custom => {
                let category = self
                    .canvas_templates
                    .categories()
                    .iter()
                    .position(|(id, _)| *id == template.category)
                    .unwrap_or(0);
                self.advanced_new(category, cx);
            }
            Make::Size { .. } | Make::Screen => {
                let Some(spec) = template.picture(self.new_canvas.screen) else {
                    return;
                };
                let defaults = crate::dialogs::NewDialog::new(&spec);
                match template.work {
                    Work::Illustration => self.make_picture(cx, &spec),
                    Work::Comic => {
                        self.make_story(cx, &spec, defaults.pages as usize, defaults.right_bound)
                    }
                    Work::Animation => {
                        self.make_animation(cx, &spec, defaults.frames, defaults.fps);
                    }
                }
                self.new_canvas.category = None;
            }
        }
    }

    /// The full New dialog, set for the work of `category`.
    pub(crate) fn advanced_new(&mut self, category: usize, cx: &mut AppletContext<'_>) {
        let mut dialog = Self::new_dialog(cx);
        let work = self
            .canvas_templates
            .categories()
            .get(category)
            .and_then(|(id, _)| self.canvas_templates.of(id).next())
            .map_or(Work::Illustration, |template| template.work);
        dialog.project = match work {
            Work::Illustration => crate::dialogs::Project::Illustration,
            Work::Comic => crate::dialogs::Project::Comic,
            Work::Animation => crate::dialogs::Project::Animation,
        };
        self.dialog = Some(crate::dialogs::Dialog::New(dialog));
    }

    /// A movie picked on the New canvas page becomes a new animation.
    pub(crate) fn new_from_movie(&mut self, path: &std::path::Path, cx: &mut AppletContext<'_>) {
        let spec = crate::doc::NewPicture {
            name: path.file_stem().map_or_else(
                || tr!("animation"),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            width: 1920,
            height: 1080,
            ..crate::doc::NewPicture::default()
        };
        let defaults = crate::dialogs::NewDialog::new(&spec);
        self.make_animation(cx, &spec, defaults.frames, defaults.fps);
        self.import_file(cx, "import-movie", path);
        self.new_canvas.category = None;
    }
}
