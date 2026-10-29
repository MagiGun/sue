//! Riced keybinds (`sue_rice::Keybinds`: any number of chords per action)
//! matched against egui key events.

use egui::{Event, InputState, Key};
use sue_platform::{Key as PortableKey, NamedKey};
use sue_rice::{Keybind, Keybinds};

/// The egui key for a portable key.
#[must_use]
pub fn egui_key(key: PortableKey) -> Option<Key> {
    match key {
        PortableKey::Character(character) => Key::from_name(&character.to_string()),
        PortableKey::Named(named) => Some(match named {
            NamedKey::Space => Key::Space,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::F(number) => return Key::from_name(&format!("F{number}")),
        }),
    }
}

/// `true` if `modifiers` are exactly the chord's (Ctrl is the command key).
fn same_modifiers(bind: &Keybind, modifiers: egui::Modifiers) -> bool {
    let wants = bind.modifiers;
    (wants.ctrl || wants.super_key) == modifiers.command
        && wants.alt == modifiers.alt
        && wants.shift == modifiers.shift
}

/// Consumes a press of exactly one of `binds`' chords from this frame's
/// input. Returns `true` if one was pressed.
pub fn consume(input: &mut InputState, binds: &Keybinds) -> bool {
    consume_except(input, binds, &[])
}

/// As [`consume`], leaving the chords in `reserved` alone (those an open
/// studio uses itself).
pub fn consume_except(input: &mut InputState, binds: &Keybinds, reserved: &[Keybind]) -> bool {
    let mut found = false;
    for chord in binds.chords() {
        if !reserved.contains(chord) {
            found |= consume_chord(input, chord);
        }
    }
    found
}

/// Consumes a press of exactly `bind` from this frame's input. Returns
/// `true` if it was pressed.
pub fn consume_chord(input: &mut InputState, bind: &Keybind) -> bool {
    let Some(wanted) = egui_key(bind.key) else {
        return false;
    };
    let mut found = false;
    input.events.retain(|event| {
        let hit = matches!(
            event,
            Event::Key { key, pressed: true, modifiers, .. }
                if *key == wanted && same_modifiers(bind, *modifiers)
        );
        found |= hit;
        !hit
    });
    found
}

/// `true` if one of `binds`' keys is held down now (for chords like Space
/// while dragging).
#[must_use]
pub fn held(input: &InputState, binds: &Keybinds) -> bool {
    binds
        .chords()
        .iter()
        .any(|bind| egui_key(bind.key).is_some_and(|key| input.key_down(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn chords_match_exactly_and_are_consumed_once() {
        let undo: Keybinds = "Ctrl+Z".parse().unwrap();
        let redo: Keybinds = "Ctrl+Shift+Z".parse().unwrap();
        let mut input = InputState::default();
        input.events.push(press(Key::Z, egui::Modifiers::COMMAND));
        assert!(!consume(&mut input, &redo));
        assert!(consume(&mut input, &undo));
        assert!(!consume(&mut input, &undo));
        input.events.push(press(
            Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        ));
        assert!(!consume(&mut input, &undo));
        assert!(consume(&mut input, &redo));
        let palette: Keybinds = "/".parse().unwrap();
        input.events.push(press(Key::Slash, egui::Modifiers::NONE));
        assert!(consume(&mut input, &palette));
        let settings: Keybinds = "Ctrl+,".parse().unwrap();
        input
            .events
            .push(press(Key::Comma, egui::Modifiers::COMMAND));
        assert!(consume(&mut input, &settings));
    }

    #[test]
    fn an_action_takes_every_one_of_its_chords() {
        let undo: Keybinds = "Ctrl+Z or Alt+Backspace".parse().unwrap();
        assert_eq!(undo.chords().len(), 2);
        assert_eq!(undo.to_string(), "Ctrl+Z or Alt+Backspace");
        let mut input = InputState::default();
        input
            .events
            .push(press(Key::Backspace, egui::Modifiers::ALT));
        assert!(consume(&mut input, &undo));
        input.events.push(press(Key::Z, egui::Modifiers::COMMAND));
        let reserved: Keybind = "Ctrl+Z".parse().unwrap();
        assert!(
            !consume_except(&mut input, &undo, &[reserved]),
            "a studio keeps its own"
        );
        assert!(consume(&mut input, &undo));
        let none: Keybinds = "".parse().unwrap();
        assert!(none.is_empty());
        assert!(!consume(&mut input, &none));
    }
}
