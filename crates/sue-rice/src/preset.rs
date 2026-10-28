//! Theme presets: named sets of rice values, shipped as `.suerice` files.

use crate::{RiceError, RiceSchema, RiceValue, SetRiceValues};

/// A named set of rice values, applied as one undoable step.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemePreset {
    /// Short id, e.g. `graphite`.
    pub id: String,
    /// Display name, e.g. `Graphite`.
    pub name: String,
    /// The values the preset sets.
    pub values: Vec<(String, RiceValue)>,
}

impl ThemePreset {
    /// Reads a preset from `.suerice` text.
    ///
    /// # Errors
    /// The parse error of `source`.
    pub fn parse(id: &str, name: &str, source: &str) -> Result<Self, RiceError> {
        let schema = RiceSchema::parse(source)?;
        Ok(Self {
            id: id.to_owned(),
            name: name.to_owned(),
            values: schema
                .entries()
                .iter()
                .map(|entry| (entry.path().to_owned(), entry.parameter().value()))
                .collect(),
        })
    }

    /// `true` if every value of the preset is what `rice` holds now.
    #[must_use]
    pub fn is_active(&self, rice: &RiceSchema) -> bool {
        self.values
            .iter()
            .all(|(path, value)| rice.get(path).as_ref() == Some(value))
    }

    /// The transaction that applies the preset to `rice`. Values `rice`
    /// does not know are skipped.
    #[must_use]
    pub fn transaction(&self, rice: &RiceSchema) -> SetRiceValues {
        SetRiceValues::new(
            format!("Apply the {} theme", self.name),
            self.values
                .iter()
                .filter(|(path, value)| rice.get(path).is_some_and(|current| &current != value))
                .cloned(),
        )
    }
}

/// The presets Sue ships with, default first.
#[must_use]
pub fn builtin_presets() -> Vec<ThemePreset> {
    [
        (
            "graphite",
            "Graphite",
            include_str!("../rice/presets/graphite.suerice"),
        ),
        (
            "midnight",
            "Midnight",
            include_str!("../rice/presets/midnight.suerice"),
        ),
        (
            "daylight",
            "Daylight",
            include_str!("../rice/presets/daylight.suerice"),
        ),
    ]
    .into_iter()
    .map(|(id, name, source)| {
        ThemePreset::parse(id, name, source)
            .unwrap_or_else(|error| panic!("built-in preset {id}: {error}"))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_RICE;
    use sue_core::UndoStack;

    #[test]
    fn the_default_rice_wears_the_first_preset() {
        let presets = builtin_presets();
        let rice = RiceSchema::parse(DEFAULT_RICE).unwrap();
        assert!(presets[0].is_active(&rice));
        assert!(!presets[1].is_active(&rice));
        assert!(presets.iter().all(|preset| preset.values.len() == 12));
    }

    #[test]
    fn presets_apply_and_undo_as_one_step() {
        let presets = builtin_presets();
        let mut rice = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let mut history = UndoStack::new();
        let txn = presets[2].transaction(&rice);
        assert!(!txn.is_empty());
        history.apply(&mut rice, txn).unwrap();
        assert!(presets[2].is_active(&rice));
        assert_eq!(history.history(), vec!["Apply the Daylight theme"]);
        history.undo(&mut rice).unwrap();
        assert!(presets[0].is_active(&rice));
        assert!(presets[0].transaction(&rice).is_empty());
    }
}
