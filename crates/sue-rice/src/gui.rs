//! Settings GUI descriptors generated from a schema.

use crate::schema::humanize;
use crate::{Rgba, RiceParameter, RiceSchema, SHADER_EXTENSIONS};

/// The native control a parameter is shown with.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// On/off switch.
    Toggle {
        /// Current state.
        on: bool,
    },
    /// Bounded slider.
    Slider {
        /// Current value.
        value: f64,
        /// Minimum.
        min: f64,
        /// Maximum.
        max: f64,
        /// Step between positions.
        step: f64,
        /// `true` if only whole numbers are allowed.
        integer: bool,
    },
    /// Unbounded number entry.
    NumberField {
        /// Current value.
        value: f64,
        /// `true` if only whole numbers are allowed.
        integer: bool,
    },
    /// Color picker.
    ColorPicker {
        /// Current color.
        rgba: Rgba,
    },
    /// Drop-down list.
    Dropdown {
        /// Choices, in display order.
        options: Vec<String>,
        /// Index of the current choice.
        selected: usize,
    },
    /// "Press a key" chord recorder.
    KeybindRecorder {
        /// Current chord, e.g. `Ctrl+Shift+Z`.
        chord: String,
    },
    /// Shader file picker.
    ShaderPicker {
        /// Current path (empty for none).
        path: String,
        /// Accepted file extensions.
        extensions: &'static [&'static str],
    },
    /// Free text entry.
    TextField {
        /// Current text.
        text: String,
    },
}

impl Control {
    /// The control for `parameter`.
    #[must_use]
    pub fn for_parameter(parameter: &RiceParameter) -> Self {
        match parameter {
            RiceParameter::Bool { value } => Self::Toggle { on: *value },
            RiceParameter::IntRange {
                value,
                min: Some(min),
                max: Some(max),
            } => Self::Slider {
                value: *value as f64,
                min: *min as f64,
                max: *max as f64,
                step: 1.0,
                integer: true,
            },
            RiceParameter::IntRange { value, .. } => Self::NumberField {
                value: *value as f64,
                integer: true,
            },
            RiceParameter::FloatRange {
                value,
                min: Some(min),
                max: Some(max),
            } => Self::Slider {
                value: *value,
                min: *min,
                max: *max,
                step: ((max - min) / 100.0).max(f64::EPSILON),
                integer: false,
            },
            RiceParameter::FloatRange { value, .. } => Self::NumberField {
                value: *value,
                integer: false,
            },
            RiceParameter::ColorHex { value } => Self::ColorPicker { rgba: *value },
            RiceParameter::EnumChoice { value, options } => Self::Dropdown {
                options: options.clone(),
                selected: options
                    .iter()
                    .position(|option| option == value)
                    .unwrap_or(0),
            },
            RiceParameter::Keybind { value } => Self::KeybindRecorder {
                chord: value.to_string(),
            },
            RiceParameter::ShaderPath { value } => Self::ShaderPicker {
                path: value.clone(),
                extensions: SHADER_EXTENSIONS,
            },
            RiceParameter::Text { value } => Self::TextField {
                text: value.clone(),
            },
        }
    }
}

/// One control in the Settings GUI, bound to a rice path.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsWidget {
    /// The rice path the widget reads and writes, e.g. `layout.gap`.
    pub binding: String,
    /// Label shown next to the control.
    pub label: String,
    /// Tooltip.
    pub tooltip: Option<String>,
    /// The control.
    pub control: Control,
}

/// A group of widgets: one rice table.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsSection {
    /// The table name (`""` for top-level keys).
    pub id: String,
    /// Display title.
    pub title: String,
    /// Widgets in file order.
    pub widgets: Vec<SettingsWidget>,
}

/// The whole Settings GUI, generated from a [`RiceSchema`].
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsTree {
    /// Schema revision the tree was built from.
    pub revision: u64,
    /// Sections in table order.
    pub sections: Vec<SettingsSection>,
}

impl SettingsTree {
    /// Builds the tree: one section per table, one widget per parameter.
    #[must_use]
    pub fn build(schema: &RiceSchema) -> Self {
        let sections = schema
            .sections()
            .iter()
            .map(|id| SettingsSection {
                id: id.clone(),
                title: if id.is_empty() {
                    "General".to_owned()
                } else {
                    humanize(id)
                },
                widgets: schema
                    .entries()
                    .iter()
                    .filter(|entry| entry.section() == id)
                    .map(|entry| SettingsWidget {
                        binding: entry.path().to_owned(),
                        label: entry.label(),
                        tooltip: entry.description().map(str::to_owned),
                        control: Control::for_parameter(entry.parameter()),
                    })
                    .collect(),
            })
            .filter(|section| !section.widgets.is_empty())
            .collect();
        Self {
            revision: schema.revision(),
            sections,
        }
    }

    /// The widget bound to `binding`.
    #[must_use]
    pub fn widget(&self, binding: &str) -> Option<&SettingsWidget> {
        self.sections
            .iter()
            .flat_map(|section| &section.widgets)
            .find(|widget| widget.binding == binding)
    }

    /// Total number of widgets.
    #[must_use]
    pub fn widget_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.widgets.len())
            .sum()
    }

    /// `true` if `schema` changed since the tree was built.
    #[must_use]
    pub fn is_stale(&self, schema: &RiceSchema) -> bool {
        self.revision != schema.revision()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_RICE, RiceValue};

    #[test]
    fn every_default_parameter_gets_a_widget() {
        let schema = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let tree = SettingsTree::build(&schema);
        assert_eq!(tree.widget_count(), schema.len());
        assert_eq!(tree.sections[0].title, "General");

        let gap = tree.widget("layout.gap").unwrap();
        assert_eq!(gap.label, "Gap");
        assert_eq!(gap.tooltip.as_deref(), Some("Space between panels (px)"));
        assert_eq!(
            gap.control,
            Control::Slider {
                value: 0.0,
                min: 0.0,
                max: 32.0,
                step: 1.0,
                integer: true
            }
        );
        assert!(matches!(
            tree.widget("colors.accent").unwrap().control,
            Control::ColorPicker { .. }
        ));
        assert!(matches!(
            tree.widget("motion.instant_transitions").unwrap().control,
            Control::Toggle { on: false }
        ));
        assert!(matches!(
            &tree.widget("profile.hardware").unwrap().control,
            Control::Dropdown { selected: 0, options } if options.len() == 5
        ));
        assert_eq!(
            tree.widget("keys.undo").unwrap().control,
            Control::KeybindRecorder {
                chord: "Ctrl+Z".into()
            }
        );
        assert!(matches!(
            tree.widget("launchpad.boot_into").unwrap().control,
            Control::TextField { .. }
        ));
        let shaders =
            RiceSchema::parse("[shaders]\nbackground = \"\" # @shader Background pass\n").unwrap();
        assert!(matches!(
            SettingsTree::build(&shaders)
                .widget("shaders.background")
                .unwrap()
                .control,
            Control::ShaderPicker { .. }
        ));
    }

    #[test]
    fn unbounded_numbers_use_number_fields() {
        let schema = RiceSchema::parse("[x]\ncount = 3\nratio = 0.5 # [0.0..]\n").unwrap();
        let tree = SettingsTree::build(&schema);
        assert_eq!(
            tree.widget("x.count").unwrap().control,
            Control::NumberField {
                value: 3.0,
                integer: true
            }
        );
        assert!(matches!(
            tree.widget("x.ratio").unwrap().control,
            Control::NumberField { integer: false, .. }
        ));
        assert_eq!(tree.sections[0].title, "X");
    }

    #[test]
    fn trees_notice_schema_edits() {
        let mut schema = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let tree = SettingsTree::build(&schema);
        assert!(!tree.is_stale(&schema));
        schema.set("layout.gap", RiceValue::Int(10)).unwrap();
        assert!(tree.is_stale(&schema));
        let rebuilt = SettingsTree::build(&schema);
        assert!(
            matches!(rebuilt.widget("layout.gap").unwrap().control, Control::Slider { value, .. } if value == 10.0)
        );
    }
}
