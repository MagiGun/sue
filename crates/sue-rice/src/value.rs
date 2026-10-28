//! Colors, keybinds and the dynamic value type used for edits.

use std::fmt;
use std::str::FromStr;

use sue_platform::{Key, KeyEvent, Modifiers};

use crate::RiceError;

/// An 8-bit RGBA color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 is opaque).
    pub a: u8,
}

impl Rgba {
    /// A color from its channels.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    #[must_use]
    pub fn parse_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |index: usize| u8::from_str_radix(&hex[index..=index], 16).ok();
        let pair = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
        match hex.len() {
            3 | 4 => {
                let short = |index: usize| digit(index).map(|value| value * 17);
                let alpha = if hex.len() == 4 { short(3)? } else { 255 };
                Some(Self::new(short(0)?, short(1)?, short(2)?, alpha))
            }
            6 | 8 => {
                let alpha = if hex.len() == 8 { pair(6)? } else { 255 };
                Some(Self::new(pair(0)?, pair(2)?, pair(4)?, alpha))
            }
            _ => None,
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.a == u8::MAX {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Rgba {
    type Err = RiceError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_hex(text).ok_or_else(|| RiceError::InvalidColor(text.to_owned()))
    }
}

/// A key chord such as `Ctrl+Shift+Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keybind {
    /// Modifiers that must be held, exactly.
    pub modifiers: Modifiers,
    /// The key that triggers the chord.
    pub key: Key,
}

impl Keybind {
    /// A chord from its parts.
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// `true` if `event` is a press of exactly this chord.
    #[must_use]
    pub fn matches(&self, event: &KeyEvent) -> bool {
        event.pressed
            && event.modifiers == self.modifiers
            && event.key.normalized() == self.key.normalized()
    }

    /// `true` if `text` parses as a chord with at least one modifier. Used to
    /// infer keybind parameters from untagged strings.
    #[must_use]
    pub fn looks_like_chord(text: &str) -> bool {
        text.parse::<Self>()
            .is_ok_and(|keybind| !keybind.modifiers.is_empty())
    }
}

impl FromStr for Keybind {
    type Err = RiceError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || RiceError::InvalidKeybind(text.to_owned());
        let trimmed = text.trim();
        // A trailing "++" means the chord's key is '+' itself.
        let (modifier_text, key) = if trimmed == "+" {
            ("", Key::Character('+'))
        } else if let Some(prefix) = trimmed.strip_suffix("++") {
            (prefix, Key::Character('+'))
        } else {
            let (prefix, key_name) = trimmed.rsplit_once('+').unwrap_or(("", trimmed));
            (prefix, Key::parse(key_name).ok_or_else(invalid)?)
        };
        let mut modifiers = Modifiers::NONE;
        for part in modifier_text
            .split('+')
            .filter(|part| !part.trim().is_empty())
        {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" | "option" | "opt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" | "cmd" | "command" | "win" | "meta" | "logo" => modifiers.super_key = true,
                _ => return Err(invalid()),
            }
        }
        Ok(Self::new(modifiers, key.normalized()))
    }
}

impl fmt::Display for Keybind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = [
            (self.modifiers.ctrl, "Ctrl"),
            (self.modifiers.alt, "Alt"),
            (self.modifiers.shift, "Shift"),
            (self.modifiers.super_key, "Super"),
        ];
        for (_, name) in names.iter().filter(|(held, _)| *held) {
            write!(f, "{name}+")?;
        }
        write!(f, "{}", self.key)
    }
}

/// A dynamically typed rice value, as produced by a Settings widget.
#[derive(Clone, Debug, PartialEq)]
pub enum RiceValue {
    /// A toggle state.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A color.
    Color(Rgba),
    /// A key chord.
    Keybind(Keybind),
    /// Text: free text, a choice, or a shader path.
    Text(String),
}

impl RiceValue {
    /// Name of the value's type, for error messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "a bool",
            Self::Int(_) => "an integer",
            Self::Float(_) => "a float",
            Self::Color(_) => "a color",
            Self::Keybind(_) => "a keybind",
            Self::Text(_) => "text",
        }
    }
}

impl fmt::Display for RiceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value:?}"),
            Self::Color(value) => write!(f, "{value}"),
            Self::Keybind(value) => write!(f, "{value}"),
            Self::Text(value) => write!(f, "\"{value}\""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_platform::NamedKey;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            Rgba::parse_hex("#c9a227"),
            Some(Rgba::new(0xc9, 0xa2, 0x27, 255))
        );
        assert_eq!(Rgba::parse_hex("#fff"), Some(Rgba::new(255, 255, 255, 255)));
        assert_eq!(Rgba::parse_hex("#0008"), Some(Rgba::new(0, 0, 0, 0x88)));
        assert_eq!(
            Rgba::parse_hex("#11223344").map(Rgba::to_hex).as_deref(),
            Some("#11223344")
        );
        assert_eq!(Rgba::parse_hex("c9a227"), None);
        assert_eq!(Rgba::parse_hex("#c9a22"), None);
        assert_eq!(Rgba::parse_hex("#ggg"), None);
        assert!("#12".parse::<Rgba>().is_err());
    }

    #[test]
    fn parses_and_prints_chords() {
        let redo: Keybind = "ctrl+shift+z".parse().unwrap();
        assert_eq!(redo.to_string(), "Ctrl+Shift+Z");
        let zoom: Keybind = "Cmd++".parse().unwrap();
        assert_eq!(zoom.key, Key::Character('+'));
        assert!(zoom.modifiers.super_key);
        assert_eq!(
            "F11".parse::<Keybind>().unwrap().key,
            Key::Named(NamedKey::F(11))
        );
        assert!("Hyper+X".parse::<Keybind>().is_err());
        assert!("Ctrl+".parse::<Keybind>().is_err());
        assert!(Keybind::looks_like_chord("Super+Space"));
        assert!(!Keybind::looks_like_chord("Space"));
        assert!(!Keybind::looks_like_chord("rock+roll"));
    }

    #[test]
    fn chords_match_key_events() {
        let undo: Keybind = "Ctrl+Z".parse().unwrap();
        let press = KeyEvent::press(Key::Character('z'), Modifiers::CTRL);
        assert!(undo.matches(&press));
        let release = KeyEvent {
            pressed: false,
            ..press.clone()
        };
        assert!(!undo.matches(&release));
        let shifted = KeyEvent {
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::CTRL
            },
            ..press
        };
        assert!(!undo.matches(&shifted));
    }
}
