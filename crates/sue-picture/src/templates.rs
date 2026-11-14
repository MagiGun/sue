//! Canvas templates: the cards of the New canvas page on phones and
//! tablets, grouped in categories (Illustration, Webtoon, Comic,
//! Animation). Neither the categories nor the templates are a fixed list:
//! each registers in [`CanvasTemplates`], and adding one is a line in
//! [`CanvasTemplates::builtin`] or a call to [`CanvasTemplates::register`].

use crate::doc::NewPicture;

/// What a new canvas is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    /// One picture.
    Illustration,
    /// A story of pages.
    Comic,
    /// A picture with a timeline.
    Animation,
}

/// The unit a template's size is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Pixels.
    Px,
    /// Millimeters, turned into pixels at the template's resolution.
    Mm,
}

/// How a template makes its canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Make {
    /// A fixed size.
    Size {
        /// Width in `unit`.
        width: f64,
        /// Height in `unit`.
        height: f64,
        /// The unit.
        unit: Unit,
        /// Print resolution in pixels per inch.
        dpi: f64,
    },
    /// The size of this device's screen, in pixels.
    Screen,
    /// A picture the person picks becomes the canvas.
    FromImage,
    /// A movie the person picks becomes an animation.
    FromMovie,
    /// The full New dialog, to choose every setting.
    Custom,
}

/// One template card.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasTemplate {
    /// Its id, under the registering feature's namespace.
    pub id: String,
    /// The category it shows in.
    pub category: String,
    /// What people read, kept in English (`localize` shows it); empty
    /// for a template named by its size.
    pub name: String,
    /// How it makes its canvas.
    pub make: Make,
    /// What the canvas is.
    pub work: Work,
}

impl CanvasTemplate {
    /// The picture it makes at `screen` pixels (the device's screen), if
    /// it has a size of its own.
    #[must_use]
    pub fn picture(&self, screen: [u32; 2]) -> Option<NewPicture> {
        let (width, height, dpi) = match self.make {
            Make::Size {
                width,
                height,
                unit,
                dpi,
            } => {
                let scale = match unit {
                    Unit::Px => 1.0,
                    Unit::Mm => dpi / 25.4,
                };
                (
                    (width * scale).round() as u32,
                    (height * scale).round() as u32,
                    dpi,
                )
            }
            Make::Screen => (screen[0], screen[1], 72.0),
            Make::FromImage | Make::FromMovie | Make::Custom => return None,
        };
        let name = if self.name.is_empty() {
            match self.work {
                Work::Illustration => tr!("illustration"),
                Work::Comic => tr!("comic"),
                Work::Animation => tr!("animation"),
            }
        } else {
            crate::localize(&self.name)
        };
        Some(NewPicture {
            name,
            width: width.max(1),
            height: height.max(1),
            dpi,
            ..NewPicture::default()
        })
    }

    /// The line under its name: its size, as it was given.
    #[must_use]
    pub fn size_text(&self, screen: [u32; 2]) -> String {
        match self.make {
            Make::Size {
                width,
                height,
                unit: Unit::Px,
                ..
            } => tr!("template-px", width = width, height = height),
            Make::Size {
                width,
                height,
                unit: Unit::Mm,
                ..
            } => tr!("template-mm", width = width, height = height),
            Make::Screen => tr!("template-px", width = screen[0], height = screen[1]),
            Make::FromImage | Make::FromMovie | Make::Custom => String::new(),
        }
    }
}

/// The categories and templates, in the order they registered.
#[derive(Clone, Debug, Default)]
pub struct CanvasTemplates {
    categories: Vec<(String, String)>,
    templates: Vec<CanvasTemplate>,
}

/// Sue Paint's categories.
pub const ILLUSTRATION: &str = "sue_paint.illustration";
/// Long pictures read by scrolling.
pub const WEBTOON: &str = "sue_paint.webtoon";
/// Comics of pages.
pub const COMIC: &str = "sue_paint.comic";
/// Animations.
pub const ANIMATION: &str = "sue_paint.animation";

impl CanvasTemplates {
    /// Sue Paint's own categories and templates.
    #[must_use]
    pub fn builtin() -> Self {
        let mut all = Self::default();
        for (id, name) in [
            (ILLUSTRATION, "Illustration"),
            (WEBTOON, "Webtoon"),
            (COMIC, "Comic"),
            (ANIMATION, "Animation"),
        ] {
            all.register_category(id, name)
                .expect("distinct built-in categories");
        }
        let px = |width, height| Make::Size {
            width,
            height,
            unit: Unit::Px,
            dpi: 72.0,
        };
        let mm = |width, height| Make::Size {
            width,
            height,
            unit: Unit::Mm,
            dpi: 350.0,
        };
        let own: [(&str, &str, &str, Make, Work); 12] = [
            (
                "from_image",
                ILLUSTRATION,
                "Create from image",
                Make::FromImage,
                Work::Illustration,
            ),
            (
                "custom",
                ILLUSTRATION,
                "Custom size",
                Make::Custom,
                Work::Illustration,
            ),
            (
                "screen",
                ILLUSTRATION,
                "Screen size",
                Make::Screen,
                Work::Illustration,
            ),
            (
                "webtoon_690",
                WEBTOON,
                "",
                px(690.0, 20_000.0),
                Work::Illustration,
            ),
            (
                "webtoon_1380",
                WEBTOON,
                "",
                px(1380.0, 20_000.0),
                Work::Illustration,
            ),
            (
                "webtoon_800",
                WEBTOON,
                "",
                px(800.0, 10_000.0),
                Work::Illustration,
            ),
            ("comic_a4", COMIC, "A4 Color", mm(210.0, 297.0), Work::Comic),
            ("comic_a5", COMIC, "A5 Color", mm(148.0, 210.0), Work::Comic),
            ("comic_a6", COMIC, "A6 Color", mm(105.0, 148.0), Work::Comic),
            (
                "from_movie",
                ANIMATION,
                "Create from movie",
                Make::FromMovie,
                Work::Animation,
            ),
            (
                "square",
                ANIMATION,
                "1:1",
                px(1080.0, 1080.0),
                Work::Animation,
            ),
            (
                "vertical",
                ANIMATION,
                "9:16",
                px(1080.0, 1920.0),
                Work::Animation,
            ),
        ];
        for (id, category, name, make, work) in own {
            all.register(CanvasTemplate {
                id: format!("sue_paint.{id}"),
                category: category.to_owned(),
                name: name.to_owned(),
                make,
                work,
            })
            .expect("distinct built-in templates");
        }
        all
    }

    /// Adds a category, `name` in English.
    ///
    /// # Errors
    /// If its id is taken.
    pub fn register_category(&mut self, id: &str, name: &str) -> Result<(), String> {
        if self.categories.iter().any(|(known, _)| known == id) {
            return Err(format!("canvas category {id} is already registered"));
        }
        self.categories.push((id.to_owned(), name.to_owned()));
        Ok(())
    }

    /// Adds a template.
    ///
    /// # Errors
    /// If its id is taken or its category is not registered.
    pub fn register(&mut self, template: CanvasTemplate) -> Result<(), String> {
        if self.templates.iter().any(|known| known.id == template.id) {
            return Err(format!(
                "canvas template {} is already registered",
                template.id
            ));
        }
        if !self
            .categories
            .iter()
            .any(|(id, _)| *id == template.category)
        {
            return Err(format!("no canvas category {}", template.category));
        }
        self.templates.push(template);
        Ok(())
    }

    /// The categories: their ids and English names.
    #[must_use]
    pub fn categories(&self) -> &[(String, String)] {
        &self.categories
    }

    /// The templates of `category`, in order.
    pub fn of<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a CanvasTemplate> {
        self.templates
            .iter()
            .filter(move |template| template.category == category)
    }

    /// The template with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CanvasTemplate> {
        self.templates.iter().find(|template| template.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_register_by_category_and_make_their_sizes() {
        let mut all = CanvasTemplates::builtin();
        assert_eq!(all.categories().len(), 4);
        assert_eq!(all.of(ILLUSTRATION).count(), 3);
        assert_eq!(all.of(WEBTOON).count(), 3);
        // A4 at 350 dpi.
        let a4 = all
            .get("sue_paint.comic_a4")
            .unwrap()
            .picture([0, 0])
            .unwrap();
        assert_eq!((a4.width, a4.height), (2894, 4093));
        assert_eq!(all.get("sue_paint.comic_a4").unwrap().work, Work::Comic);
        let tall = all
            .get("sue_paint.webtoon_690")
            .unwrap()
            .picture([0, 0])
            .unwrap();
        assert_eq!((tall.width, tall.height), (690, 20_000));
        let screen = all.get("sue_paint.screen").unwrap();
        assert_eq!(
            screen.picture([1080, 2280]).map(|p| (p.width, p.height)),
            Some((1080, 2280))
        );
        assert!(
            all.get("sue_paint.custom")
                .unwrap()
                .picture([1, 1])
                .is_none()
        );
        // One more registers in its category; a taken id or an unknown
        // category does not.
        let more = CanvasTemplate {
            id: "test.square".to_owned(),
            category: ILLUSTRATION.to_owned(),
            name: String::new(),
            make: Make::Size {
                width: 2048.0,
                height: 2048.0,
                unit: Unit::Px,
                dpi: 72.0,
            },
            work: Work::Illustration,
        };
        all.register(more.clone()).unwrap();
        assert!(all.register(more.clone()).is_err());
        assert!(
            all.register(CanvasTemplate {
                id: "test.other".to_owned(),
                category: "test.nowhere".to_owned(),
                ..more
            })
            .is_err()
        );
        assert_eq!(all.of(ILLUSTRATION).count(), 4);
        assert!(all.register_category(COMIC, "Comic").is_err());
    }
}
