//! Typed rice parameters and their inference from literals and metadata.

use crate::parser::{Bound, Literal, Metadata, Tag};
use crate::{Keybind, Rgba, RiceError, RiceValue, SHADER_EXTENSIONS};

/// One riceable setting, with its current value and the metadata that decides
/// its widget.
#[derive(Clone, Debug, PartialEq)]
pub enum RiceParameter {
    /// On/off toggle.
    Bool {
        /// Current state.
        value: bool,
    },
    /// Integer; a slider when both bounds are set.
    IntRange {
        /// Current value.
        value: i64,
        /// Inclusive minimum.
        min: Option<i64>,
        /// Inclusive maximum.
        max: Option<i64>,
    },
    /// Float; a slider when both bounds are set.
    FloatRange {
        /// Current value.
        value: f64,
        /// Inclusive minimum.
        min: Option<f64>,
        /// Inclusive maximum.
        max: Option<f64>,
    },
    /// Color picker.
    ColorHex {
        /// Current color.
        value: Rgba,
    },
    /// Dropdown of fixed choices.
    EnumChoice {
        /// Current choice.
        value: String,
        /// Allowed choices, in display order.
        options: Vec<String>,
    },
    /// Keybind recorder.
    Keybind {
        /// Current chord.
        value: Keybind,
    },
    /// Shader file picker. Empty means "no shader".
    ShaderPath {
        /// Current path.
        value: String,
    },
    /// Free text.
    Text {
        /// Current text.
        value: String,
    },
}

impl RiceParameter {
    /// Name of the parameter's type, for messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Bool { .. } => "a bool",
            Self::IntRange { .. } => "an integer",
            Self::FloatRange { .. } => "a float",
            Self::ColorHex { .. } => "a color",
            Self::EnumChoice { .. } => "a choice",
            Self::Keybind { .. } => "a keybind",
            Self::ShaderPath { .. } => "a shader path",
            Self::Text { .. } => "text",
        }
    }

    /// The current value.
    #[must_use]
    pub fn value(&self) -> RiceValue {
        match self {
            Self::Bool { value } => RiceValue::Bool(*value),
            Self::IntRange { value, .. } => RiceValue::Int(*value),
            Self::FloatRange { value, .. } => RiceValue::Float(*value),
            Self::ColorHex { value } => RiceValue::Color(*value),
            Self::Keybind { value } => RiceValue::Keybind(*value),
            Self::EnumChoice { value, .. } | Self::ShaderPath { value } | Self::Text { value } => {
                RiceValue::Text(value.clone())
            }
        }
    }

    /// Sets the value after type and range checks, returning the old value.
    /// Integers are accepted for floats; text is parsed for colors and keybinds.
    ///
    /// # Errors
    /// [`RiceError::TypeMismatch`], [`RiceError::OutOfRange`],
    /// [`RiceError::InvalidChoice`], [`RiceError::NonFinite`],
    /// [`RiceError::InvalidColor`] or [`RiceError::InvalidKeybind`].
    pub fn set(&mut self, path: &str, value: RiceValue) -> Result<RiceValue, RiceError> {
        let old = self.value();
        let mismatch = |found: &RiceValue, expected| RiceError::TypeMismatch {
            path: path.to_owned(),
            expected,
            found: found.type_name(),
        };
        let expected = self.type_name();
        match (&mut *self, value) {
            (Self::Bool { value }, RiceValue::Bool(new)) => *value = new,
            (Self::IntRange { value, min, max }, RiceValue::Int(new)) => {
                check_range(path, new, *min, *max, |v| v.to_string())?;
                *value = new;
            }
            (Self::FloatRange { value, min, max }, RiceValue::Float(new)) => {
                set_float(path, value, *min, *max, new)?;
            }
            (Self::FloatRange { value, min, max }, RiceValue::Int(new)) => {
                set_float(path, value, *min, *max, new as f64)?;
            }
            (Self::ColorHex { value }, RiceValue::Color(new)) => *value = new,
            (Self::ColorHex { value }, RiceValue::Text(text)) => *value = text.parse()?,
            (Self::Keybind { value }, RiceValue::Keybind(new)) => *value = new,
            (Self::Keybind { value }, RiceValue::Text(text)) => *value = text.parse()?,
            (Self::EnumChoice { value, options }, RiceValue::Text(text)) => {
                if !options.contains(&text) {
                    return Err(RiceError::InvalidChoice {
                        path: path.to_owned(),
                        value: text,
                    });
                }
                *value = text;
            }
            (Self::ShaderPath { value } | Self::Text { value }, RiceValue::Text(text)) => {
                *value = text
            }
            (_, other) => return Err(mismatch(&other, expected)),
        }
        Ok(old)
    }

    /// Like [`set`](Self::set), but clamps out-of-range numbers and falls back
    /// to the first choice for unknown choices. Returns a note when the value
    /// was adjusted.
    pub(crate) fn set_clamped(
        &mut self,
        path: &str,
        value: RiceValue,
    ) -> Result<Option<String>, RiceError> {
        match self.set(path, value.clone()) {
            Ok(_) => Ok(None),
            Err(RiceError::OutOfRange { .. }) => {
                let clamped = match (&*self, value) {
                    (Self::IntRange { min, max, .. }, RiceValue::Int(new)) => {
                        RiceValue::Int(clamp(new, *min, *max))
                    }
                    (Self::FloatRange { min, max, .. }, RiceValue::Float(new)) => {
                        RiceValue::Float(clamp(new, *min, *max))
                    }
                    (Self::FloatRange { min, max, .. }, RiceValue::Int(new)) => {
                        RiceValue::Float(clamp(new as f64, *min, *max))
                    }
                    (_, other) => other,
                };
                let note = format!("value is out of range; clamped to {clamped}");
                self.set(path, clamped)?;
                Ok(Some(note))
            }
            Err(RiceError::InvalidChoice {
                path: choice_path,
                value: rejected,
            }) => {
                if let Self::EnumChoice { value, options } = &mut *self
                    && let Some(fallback) = options.first().cloned()
                {
                    *value = fallback;
                    return Ok(Some(format!(
                        "\"{rejected}\" is not a valid choice; using \"{value}\""
                    )));
                }
                Err(RiceError::InvalidChoice {
                    path: choice_path,
                    value: rejected,
                })
            }
            Err(error) => Err(error),
        }
    }

    /// `true` if both parameters are the same variant.
    pub(crate) fn same_type(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The value as a TOML literal.
    pub(crate) fn literal(&self) -> String {
        match self {
            Self::Bool { value } => value.to_string(),
            Self::IntRange { value, .. } => value.to_string(),
            Self::FloatRange { value, .. } => format_float(*value),
            Self::ColorHex { value } => quote(&value.to_hex()),
            Self::Keybind { value } => quote(&value.to_string()),
            Self::EnumChoice { value, .. } | Self::ShaderPath { value } | Self::Text { value } => {
                quote(value)
            }
        }
    }

    /// Metadata that makes the literal re-parse to this exact parameter.
    pub(crate) fn metadata(&self) -> String {
        fn range<T>(min: Option<T>, max: Option<T>, fmt: impl Fn(T) -> String) -> String {
            if min.is_none() && max.is_none() {
                return String::new();
            }
            format!(
                "[{}..{}]",
                min.map(&fmt).unwrap_or_default(),
                max.map(&fmt).unwrap_or_default()
            )
        }
        match self {
            Self::Bool { .. } | Self::ColorHex { .. } => String::new(),
            Self::IntRange { min, max, .. } => range(*min, *max, |v| v.to_string()),
            Self::FloatRange { min, max, .. } => range(*min, *max, format_float),
            Self::EnumChoice { options, .. } => format!("{{{}}}", options.join("|")),
            Self::Keybind { .. } => "@keybind".to_owned(),
            Self::ShaderPath { .. } => "@shader".to_owned(),
            Self::Text { .. } => "@text".to_owned(),
        }
    }

    /// Infers the parameter for a literal and its metadata. Returns a note when
    /// the value had to be adjusted, and an error message for metadata that
    /// does not fit the literal.
    pub(crate) fn infer(
        literal: Literal,
        meta: &Metadata,
    ) -> Result<(Self, Option<String>), String> {
        let path = "value";
        match literal {
            Literal::Bool(value) => {
                if meta.range.is_some() || meta.choices.is_some() || meta.tag.is_some() {
                    return Err("booleans take no range, choices or tag".to_owned());
                }
                Ok((Self::Bool { value }, None))
            }
            Literal::Int(_) | Literal::Float(_) => {
                if meta.choices.is_some() || meta.tag.is_some() {
                    return Err("numbers take a [min..max] range, not choices or tags".to_owned());
                }
                let (min, max) = meta.range.unwrap_or((None, None));
                let floating = matches!(literal, Literal::Float(_))
                    || matches!(min, Some(Bound::Float(_)))
                    || matches!(max, Some(Bound::Float(_)));
                let (mut parameter, value) = if floating {
                    let (min, max) = (min.map(Bound::as_f64), max.map(Bound::as_f64));
                    check_order(min, max)?;
                    let value = match literal {
                        Literal::Float(float) => RiceValue::Float(float),
                        Literal::Int(int) => RiceValue::Float(int as f64),
                        _ => unreachable!("numeric literal"),
                    };
                    (
                        Self::FloatRange {
                            value: min.or(max).unwrap_or(0.0),
                            min,
                            max,
                        },
                        value,
                    )
                } else {
                    let int = |bound: Option<Bound>| match bound {
                        Some(Bound::Int(int)) => Some(int),
                        _ => None,
                    };
                    let (min, max) = (int(min), int(max));
                    check_order(min, max)?;
                    let Literal::Int(value) = literal else {
                        unreachable!("integer literal")
                    };
                    (
                        Self::IntRange {
                            value: min.or(max).unwrap_or(0),
                            min,
                            max,
                        },
                        RiceValue::Int(value),
                    )
                };
                let note = parameter
                    .set_clamped(path, value)
                    .map_err(|error| error.to_string())?;
                Ok((parameter, note))
            }
            Literal::Str(text) => {
                if meta.range.is_some() {
                    return Err("strings take choices or a tag, not a range".to_owned());
                }
                if let Some(options) = &meta.choices {
                    if meta.tag.is_some() {
                        return Err("choices cannot be combined with a tag".to_owned());
                    }
                    let mut parameter = Self::EnumChoice {
                        value: options[0].clone(),
                        options: options.clone(),
                    };
                    let note = parameter
                        .set_clamped(path, RiceValue::Text(text))
                        .map_err(|error| error.to_string())?;
                    return Ok((parameter, note));
                }
                let parameter = match meta.tag {
                    Some(Tag::Color) => Self::ColorHex {
                        value: text.parse().map_err(|error: RiceError| error.to_string())?,
                    },
                    Some(Tag::Keybind) => Self::Keybind {
                        value: text.parse().map_err(|error: RiceError| error.to_string())?,
                    },
                    Some(Tag::Shader) => Self::ShaderPath { value: text },
                    Some(Tag::Text) => Self::Text { value: text },
                    None => Self::infer_untagged(text),
                };
                Ok((parameter, None))
            }
        }
    }

    fn infer_untagged(text: String) -> Self {
        if let Some(value) = Rgba::parse_hex(&text) {
            return Self::ColorHex { value };
        }
        let lower = text.to_ascii_lowercase();
        if SHADER_EXTENSIONS
            .iter()
            .any(|extension| lower.ends_with(extension))
        {
            return Self::ShaderPath { value: text };
        }
        if let Ok(value) = text.parse::<Keybind>()
            && !value.modifiers.is_empty()
        {
            return Self::Keybind { value };
        }
        Self::Text { value: text }
    }
}

fn set_float(
    path: &str,
    slot: &mut f64,
    min: Option<f64>,
    max: Option<f64>,
    new: f64,
) -> Result<(), RiceError> {
    if !new.is_finite() {
        return Err(RiceError::NonFinite(path.to_owned()));
    }
    check_range(path, new, min, max, format_float)?;
    *slot = new;
    Ok(())
}

/// A riceable a subsystem or applet declares, with its default (LAW 3).
#[derive(Clone, Debug, PartialEq)]
pub struct RiceDeclaration {
    /// Dotted path, e.g. `layout.gap`.
    pub path: String,
    /// Default value and metadata.
    pub default: RiceParameter,
    /// Tooltip.
    pub description: String,
}

impl RiceDeclaration {
    /// A declaration.
    pub fn new(
        path: impl Into<String>,
        default: RiceParameter,
        description: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            default,
            description: description.into(),
        }
    }
}

fn check_order<T: PartialOrd>(min: Option<T>, max: Option<T>) -> Result<(), String> {
    match (min, max) {
        (Some(min), Some(max)) if min > max => Err("range minimum exceeds its maximum".to_owned()),
        _ => Ok(()),
    }
}

fn check_range<T: PartialOrd + Copy>(
    path: &str,
    value: T,
    min: Option<T>,
    max: Option<T>,
    fmt: impl Fn(T) -> String,
) -> Result<(), RiceError> {
    let below = min.is_some_and(|min| value < min);
    let above = max.is_some_and(|max| value > max);
    if below || above {
        return Err(RiceError::OutOfRange {
            path: path.to_owned(),
            value: fmt(value),
            range: format!(
                "[{}..{}]",
                min.map(&fmt).unwrap_or_default(),
                max.map(&fmt).unwrap_or_default()
            ),
        });
    }
    Ok(())
}

fn clamp<T: PartialOrd + Copy>(value: T, min: Option<T>, max: Option<T>) -> T {
    match (min, max) {
        (Some(min), _) if value < min => min,
        (_, Some(max)) if value > max => max,
        _ => value,
    }
}

/// Formats a finite float so that it re-parses as a TOML float.
pub(crate) fn format_float(value: f64) -> String {
    // Debug output is the shortest round-trip form and always has `.` or `e`.
    format!("{value:?}")
}

/// Quotes text as a TOML basic string.
pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(range: Option<(Option<Bound>, Option<Bound>)>, tag: Option<Tag>) -> Metadata {
        Metadata {
            range,
            tag,
            ..Metadata::default()
        }
    }

    #[test]
    fn integers_with_ranges_become_int_ranges() {
        let range = Some((Some(Bound::Int(0)), Some(Bound::Int(64))));
        let (parameter, note) = RiceParameter::infer(Literal::Int(8), &meta(range, None)).unwrap();
        assert_eq!(
            parameter,
            RiceParameter::IntRange {
                value: 8,
                min: Some(0),
                max: Some(64)
            }
        );
        assert!(note.is_none());
    }

    #[test]
    fn out_of_range_values_are_clamped_with_a_note() {
        let range = Some((Some(Bound::Int(0)), Some(Bound::Int(64))));
        let (parameter, note) =
            RiceParameter::infer(Literal::Int(100), &meta(range, None)).unwrap();
        assert_eq!(parameter.value(), RiceValue::Int(64));
        assert!(note.unwrap().contains("clamped to 64"));
    }

    #[test]
    fn float_bounds_promote_integers() {
        let range = Some((Some(Bound::Float(0.0)), Some(Bound::Float(1.0))));
        let (parameter, _) = RiceParameter::infer(Literal::Int(1), &meta(range, None)).unwrap();
        assert_eq!(parameter.value(), RiceValue::Float(1.0));
    }

    #[test]
    fn strings_are_inferred_from_their_shape() {
        let infer = |text: &str| {
            RiceParameter::infer(Literal::Str(text.into()), &Metadata::default())
                .unwrap()
                .0
        };
        assert!(matches!(infer("#7aa2f7"), RiceParameter::ColorHex { .. }));
        assert!(matches!(
            infer("shaders/aurora.WGSL"),
            RiceParameter::ShaderPath { .. }
        ));
        assert!(matches!(
            infer("Ctrl+Shift+Z"),
            RiceParameter::Keybind { .. }
        ));
        assert!(matches!(infer("Inter"), RiceParameter::Text { .. }));
    }

    #[test]
    fn mismatched_metadata_is_an_error() {
        assert!(RiceParameter::infer(Literal::Bool(true), &meta(None, Some(Tag::Color))).is_err());
        assert!(
            RiceParameter::infer(
                Literal::Str("x".into()),
                &meta(Some((Some(Bound::Int(0)), None)), None)
            )
            .is_err()
        );
        assert!(
            RiceParameter::infer(Literal::Str("blue".into()), &meta(None, Some(Tag::Color)))
                .is_err()
        );
        let reversed = Some((Some(Bound::Int(9)), Some(Bound::Int(1))));
        assert!(RiceParameter::infer(Literal::Int(5), &meta(reversed, None)).is_err());
    }

    #[test]
    fn set_checks_types_and_ranges() {
        let mut gap = RiceParameter::IntRange {
            value: 8,
            min: Some(0),
            max: Some(64),
        };
        assert_eq!(
            gap.set("layout.gap", RiceValue::Int(12)),
            Ok(RiceValue::Int(8))
        );
        assert!(matches!(
            gap.set("layout.gap", RiceValue::Int(65)),
            Err(RiceError::OutOfRange { .. })
        ));
        assert!(matches!(
            gap.set("layout.gap", RiceValue::Bool(true)),
            Err(RiceError::TypeMismatch { .. })
        ));
        let mut stiffness = RiceParameter::FloatRange {
            value: 1.0,
            min: None,
            max: None,
        };
        stiffness.set("k", RiceValue::Int(3)).unwrap();
        assert_eq!(stiffness.value(), RiceValue::Float(3.0));
        assert!(matches!(
            stiffness.set("k", RiceValue::Float(f64::NAN)),
            Err(RiceError::NonFinite(_))
        ));
        let mut accent = RiceParameter::ColorHex {
            value: Rgba::new(0, 0, 0, 255),
        };
        accent
            .set("accent", RiceValue::Text("#fff".into()))
            .unwrap();
        assert_eq!(accent.literal(), "\"#ffffff\"");
    }

    #[test]
    fn literals_and_metadata_regenerate() {
        let float = RiceParameter::FloatRange {
            value: 6.0,
            min: Some(0.0),
            max: Some(24.0),
        };
        assert_eq!(
            (float.literal(), float.metadata()),
            ("6.0".to_owned(), "[0.0..24.0]".to_owned())
        );
        let open = RiceParameter::IntRange {
            value: 3,
            min: Some(1),
            max: None,
        };
        assert_eq!(open.metadata(), "[1..]");
        assert_eq!(quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
