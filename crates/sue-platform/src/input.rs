//! Unified input: one event type for mouse, keyboard, touch and stylus.

use std::fmt;

/// A position in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
}

impl Point {
    /// A point at `(x, y)`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Wheel button.
    Middle,
    /// "Back" side button.
    Back,
    /// "Forward" side button.
    Forward,
}

/// A mouse event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MouseEvent {
    /// The pointer moved.
    Moved {
        /// New position.
        position: Point,
    },
    /// A button changed state.
    Button {
        /// Which button.
        button: MouseButton,
        /// `true` on press, `false` on release.
        pressed: bool,
        /// Pointer position.
        position: Point,
    },
    /// The wheel or trackpad scrolled.
    Wheel {
        /// Horizontal scroll in logical pixels.
        delta_x: f32,
        /// Vertical scroll in logical pixels.
        delta_y: f32,
    },
    /// The pointer left the window.
    Left,
}

/// Keyboard modifier state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Control.
    pub ctrl: bool,
    /// Alt / Option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Super / Windows / Command.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifiers held.
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };

    /// Control only.
    pub const CTRL: Self = Self {
        ctrl: true,
        ..Self::NONE
    };

    /// `true` if no modifier is held.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.super_key)
    }

    /// `true` if Control or Super is held, i.e. the key is a command, not text.
    #[must_use]
    pub const fn is_command(self) -> bool {
        self.ctrl || self.super_key
    }
}

/// A non-character key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Space bar.
    Space,
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Function key `F1` to `F24`.
    F(u8),
}

impl fmt::Display for NamedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Space => "Space",
            Self::Enter => "Enter",
            Self::Escape => "Escape",
            Self::Tab => "Tab",
            Self::Backspace => "Backspace",
            Self::Delete => "Delete",
            Self::ArrowUp => "Up",
            Self::ArrowDown => "Down",
            Self::ArrowLeft => "Left",
            Self::ArrowRight => "Right",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::F(number) => return write!(f, "F{number}"),
        };
        f.write_str(name)
    }
}

/// A key identity, independent of the text it produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A character key, e.g. `A` or `/`.
    Character(char),
    /// A named key.
    Named(NamedKey),
}

impl Key {
    /// Parses a key name such as `"Space"`, `"esc"`, `"F11"`, `"z"` or `"Plus"`.
    /// Matching is case-insensitive.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        let mut chars = name.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            return Some(Self::Character(ch.to_ascii_uppercase()));
        }
        let lower = name.to_ascii_lowercase();
        let named = match lower.as_str() {
            "space" => NamedKey::Space,
            "enter" | "return" => NamedKey::Enter,
            "esc" | "escape" => NamedKey::Escape,
            "tab" => NamedKey::Tab,
            "backspace" => NamedKey::Backspace,
            "del" | "delete" => NamedKey::Delete,
            "up" | "arrowup" => NamedKey::ArrowUp,
            "down" | "arrowdown" => NamedKey::ArrowDown,
            "left" | "arrowleft" => NamedKey::ArrowLeft,
            "right" | "arrowright" => NamedKey::ArrowRight,
            "home" => NamedKey::Home,
            "end" => NamedKey::End,
            "pageup" | "pgup" => NamedKey::PageUp,
            "pagedown" | "pgdn" => NamedKey::PageDown,
            "plus" => return Some(Self::Character('+')),
            other => {
                let number: u8 = other.strip_prefix('f')?.parse().ok()?;
                if !(1..=24).contains(&number) {
                    return None;
                }
                NamedKey::F(number)
            }
        };
        Some(Self::Named(named))
    }

    /// The canonical form: ASCII letters uppercased.
    #[must_use]
    pub fn normalized(self) -> Self {
        match self {
            Self::Character(ch) => Self::Character(ch.to_ascii_uppercase()),
            named @ Self::Named(_) => named,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.normalized() {
            Self::Character(ch) => write!(f, "{ch}"),
            Self::Named(named) => write!(f, "{named}"),
        }
    }
}

/// A key press or release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// Which key.
    pub key: Key,
    /// `true` on press (and auto-repeat), `false` on release.
    pub pressed: bool,
    /// Modifiers held at the time.
    pub modifiers: Modifiers,
    /// Text produced by the key after layout and IME processing, if any.
    pub text: Option<String>,
}

impl KeyEvent {
    /// A key press that produces no text, e.g. a shortcut.
    #[must_use]
    pub fn press(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            pressed: true,
            modifiers,
            text: None,
        }
    }

    /// A plain key press that types `ch`.
    #[must_use]
    pub fn typed(ch: char) -> Self {
        Self {
            key: Key::Character(ch),
            pressed: true,
            modifiers: Modifiers::NONE,
            text: Some(ch.to_string()),
        }
    }
}

/// A screen edge, for edge swipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenEdge {
    /// Left edge.
    Left,
    /// Right edge.
    Right,
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
}

/// Direction of a swipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwipeDirection {
    /// Towards the left.
    Left,
    /// Towards the right.
    Right,
    /// Upwards.
    Up,
    /// Downwards.
    Down,
}

/// A recognised touch gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchGesture {
    /// One or more fingers tapped.
    Tap {
        /// Centroid of the tap.
        position: Point,
        /// Number of fingers.
        fingers: u8,
    },
    /// A finger was held in place.
    LongPress {
        /// Where the finger rests.
        position: Point,
    },
    /// A fling across the screen.
    Swipe {
        /// Direction of travel.
        direction: SwipeDirection,
        /// Speed in logical pixels per second.
        velocity: f32,
    },
    /// A two-finger pinch.
    Pinch {
        /// Midpoint between the fingers.
        center: Point,
        /// Scale factor relative to the gesture start.
        scale: f32,
    },
    /// A swipe that started on a screen edge.
    EdgeSwipe {
        /// Edge the swipe started from.
        edge: ScreenEdge,
        /// How far it travelled, as a fraction `0.0..=1.0` of the panel it
        /// pulls in.
        progress: f32,
    },
}

/// One stylus sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StylusSample {
    /// Tip position.
    pub position: Point,
    /// Raw pressure, nominally `0.0..=1.0`.
    pub pressure: f32,
    /// Tilt along X in degrees.
    pub tilt_x: f32,
    /// Tilt along Y in degrees.
    pub tilt_y: f32,
    /// `true` if the eraser end is in use.
    pub eraser: bool,
}

impl StylusSample {
    /// Pressure after the rice-configured response curve `p' = p^gamma`.
    #[must_use]
    pub fn curved_pressure(&self, gamma: f32) -> f32 {
        self.pressure.clamp(0.0, 1.0).powf(gamma.max(f32::EPSILON))
    }
}

/// Every input Sue reacts to, from any device.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// Mouse or trackpad.
    Mouse(MouseEvent),
    /// Keyboard.
    Keyboard(KeyEvent),
    /// Touch screen gesture.
    TouchGesture(TouchGesture),
    /// Pressure-sensitive stylus.
    StylusPressure(StylusSample),
    /// The window changed.
    Window(WindowEvent),
    /// A file is being dragged onto, or was dropped on, the window.
    FileDrop(FileDrop),
}

/// Changes to the window itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowEvent {
    /// The drawable area changed size or pixel density.
    Resized {
        /// Width in logical pixels.
        width: f32,
        /// Height in logical pixels.
        height: f32,
        /// Physical pixels per logical pixel.
        scale_factor: f32,
    },
    /// The window gained (`true`) or lost keyboard focus.
    Focused(bool),
}

/// A file dragged from the operating system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDrop {
    /// A file hovers over the window.
    Hovered(std::path::PathBuf),
    /// The drag left the window without dropping.
    Cancelled,
    /// A file was dropped on the window.
    Dropped(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_names() {
        assert_eq!(Key::parse("space"), Some(Key::Named(NamedKey::Space)));
        assert_eq!(Key::parse("Esc"), Some(Key::Named(NamedKey::Escape)));
        assert_eq!(Key::parse("F11"), Some(Key::Named(NamedKey::F(11))));
        assert_eq!(Key::parse("f25"), None);
        assert_eq!(Key::parse("z"), Some(Key::Character('Z')));
        assert_eq!(Key::parse("Plus"), Some(Key::Character('+')));
        assert_eq!(Key::parse("Hyper"), None);
        assert_eq!(Key::parse(""), None);
    }

    #[test]
    fn key_display_round_trips() {
        for name in ["Space", "Up", "PageDown", "F3", "B", "/"] {
            let key = Key::parse(name).unwrap();
            assert_eq!(key.to_string(), name);
            assert_eq!(Key::parse(&key.to_string()), Some(key));
        }
        assert_eq!(Key::Character('q').normalized(), Key::Character('Q'));
    }

    #[test]
    fn typed_events_carry_text() {
        let event = KeyEvent::typed('s');
        assert_eq!(event.text.as_deref(), Some("s"));
        assert!(event.modifiers.is_empty());
        assert!(Modifiers::CTRL.is_command());
    }

    #[test]
    fn pressure_curve_is_clamped() {
        let sample = StylusSample {
            position: Point::default(),
            pressure: 0.25,
            tilt_x: 0.0,
            tilt_y: 0.0,
            eraser: false,
        };
        assert!((sample.curved_pressure(0.5) - 0.5).abs() < 1e-6);
        let hard = StylusSample {
            pressure: 3.0,
            ..sample
        };
        assert!((hard.curved_pressure(2.0) - 1.0).abs() < f32::EPSILON);
    }
}
