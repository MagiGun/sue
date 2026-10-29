//! A table of contents set in running text. It is an entity of class
//! [`CONTENTS`] shown where an `embed` mark names it, as a picture or a
//! table is, and it keeps only its settings: its entries are the headings
//! of the text it is in, made fresh wherever it is shown (on screen, in
//! print, in every format), so it never needs updating.

use sue_core::{Entity, Value};

/// The class of a table of contents in running text.
pub const CONTENTS: &str = "sue.text_contents";
/// Its title (`Text`; [`DEFAULT_TITLE`] without one).
pub const CONTENTS_TITLE: &str = "sue.contents.title";
/// The deepest heading level it lists, 1 to 6 (`Int`; 3 without).
pub const CONTENTS_LEVELS: &str = "sue.contents.levels";
/// The title a table of contents has unless it is given another.
pub const DEFAULT_TITLE: &str = "Contents";

/// A table of contents' settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contents {
    /// Its title (empty for none).
    pub title: String,
    /// The deepest heading level it lists.
    pub levels: u8,
}

impl Default for Contents {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.to_owned(),
            levels: 3,
        }
    }
}

impl Contents {
    /// The settings of `entity`, if it is a table of contents.
    #[must_use]
    pub fn read(entity: &Entity) -> Option<Self> {
        if entity.class() != CONTENTS {
            return None;
        }
        Some(Self {
            title: entity
                .property(CONTENTS_TITLE)
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_TITLE)
                .to_owned(),
            levels: entity
                .property(CONTENTS_LEVELS)
                .and_then(Value::as_i64)
                .map_or(3, |levels| levels.clamp(1, 6) as u8),
        })
    }

    /// A new table of contents entity with these settings.
    #[must_use]
    pub fn entity(&self) -> Entity {
        Entity::empty("Contents", CONTENTS)
            .with_property(CONTENTS_TITLE, Value::Text(self.title.clone()))
            .with_property(CONTENTS_LEVELS, Value::Int(i64::from(self.levels)))
    }

    /// `true` if a heading of `level` (1 the biggest) is listed.
    #[must_use]
    pub fn lists(&self, level: u8) -> bool {
        (1..=self.levels).contains(&level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contents_keep_their_title_and_depth() {
        let contents = Contents {
            title: "In This Book".to_owned(),
            levels: 2,
        };
        let entity = contents.entity();
        assert_eq!(Contents::read(&entity), Some(contents.clone()));
        assert!(contents.lists(2) && !contents.lists(3) && !contents.lists(0));
        assert_eq!(
            Contents::read(&Entity::empty("Picture", "sue.picture")),
            None
        );
        let bare = Entity::empty("Contents", CONTENTS);
        assert_eq!(Contents::read(&bare), Some(Contents::default()));
    }
}
