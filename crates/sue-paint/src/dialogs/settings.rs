//! What the modifier keys, the pen and the mouse do, for Modifier Key
//! Settings and the Keyboard Shortcuts dialog.

/// What each modifier can do: its rice key, label and choices.
pub(crate) const MODIFIERS: [(&str, &str, &[&str]); 3] = [
    (
        "sue_paint.alt_tool",
        "Alt while drawing",
        &["eyedropper", "eraser", "none"],
    ),
    ("sue_paint.space_tool", "Space", &["hand", "none"]),
    (
        "sue_paint.ctrl_shift",
        "Ctrl+Shift click",
        &["pick_layer", "none"],
    ),
];

/// What the pen and mouse do, for the Keyboard Shortcuts dialog under
/// the keys Settings has: Clip Studio Paint's gestures.
pub(crate) const GESTURES: [(&str, &str); 11] = [
    ("Scroll", "Space+drag, middle drag"),
    ("Rotate the view", "Shift+Space+drag, Shift+wheel"),
    (
        "Zoom in / out by clicking",
        "Ctrl+Space+click / Alt+Space+click",
    ),
    ("Zoom", "Ctrl+wheel"),
    ("Pick a color", "Alt+click, right-click"),
    ("Pick the layer under the pointer", "Ctrl+Shift+click"),
    ("Change the brush size", "Ctrl+Alt+drag"),
    ("Straight line", "Shift+drag"),
    (
        "Selection: add / subtract / intersect",
        "Shift / Alt / Shift+Alt",
    ),
    ("Zoom tool: the other way", "Alt+click"),
    ("Command palette", "Command in the status bar"),
];
