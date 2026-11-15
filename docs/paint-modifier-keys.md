# Sue Paint's modifier keys file

File, Modifier Key Settings in Sue Paint says what each key held while
pressing on the canvas does, what the mouse wheel does with each key,
and what the right and middle buttons do. It has rows for every tool
(General) and rows for one tool, which win over General's.

Sue Paint keeps only the rows a person changed, in the file
`modifier_keys.txt` in Sue Paint's data folder (`data/paint` under
Sue's home folder). The file is plain text, so it can be shared: copy
it into another Sue's data folder and Sue Paint reads it the next time
it starts. A line it cannot read is left out, and Sue Paint says how
many lines that was.

## Lines

Each line is one row:

```text
<scope> <input> <keys> = <action>
```

A line that starts with `#` is a comment; empty lines are skipped.

- `<scope>`: `general`, or a tool by its stable name (`pen`, `pencil`,
  `brush`, `airbrush`, `decoration`, `eraser`, `blend`, `fill`,
  `gradient`, `figure`, `text`, `balloon`, `frame_border`, `ruler`,
  `correct_line`, `operation`, `move_layer`, `selection`,
  `auto_select`, `eyedropper`, `zoom`, `hand`).
- `<input>`: `press` (a press and drag on the canvas with the pen or the
  left button), `wheel`, `right` (a right-click) or `middle` (a
  middle-click).
- `<keys>`: the keys held, joined by `+` in this order: `ctrl`, `shift`,
  `alt`, `space`; or `none`. A press needs at least one key (a plain
  press is the tool's own), and `right` and `middle` take `none` only.
  On a Mac `ctrl` is the Command key and `alt` the Option key.
- `<action>`:
  - `common`: a tool's row does what General's does (only in a tool's
    rows).
  - `own`: the tool's own use of the key (Shift draws a straight line,
    Alt takes away from a selection, Alt zooms the other way). Press
    rows only.
  - `borrow <tool> [<mode>]`: the tool while the key is held (Change tool
    temporarily), with its sub tool of that mode, or the one picked on
    it when no mode is given. Modes: Zoom 0 in, 1 out; Hand 0 scroll, 1
    rotate; Operation 0 Object, 1 Select layer, 2 Light table, 3 Edit
    timeline; Eyedropper 0 the color shown, 1 the layer's color. Not for
    the wheel.
  - `brush_size`: a sideways drag (or the wheel) sizes the brush.
  - `zoom`, `rotate`, `scroll`: what the wheel does to the view (wheel
    rows only).
  - `nothing`: nothing happens (not for press rows).

## Example

```text
# Ctrl alone turns the view, for every tool.
general press ctrl = borrow hand 1
# The Fill tool keeps Alt for itself.
fill press alt = own
# The wheel scrolls; Ctrl and the wheel zoom.
general wheel none = scroll
general wheel ctrl = zoom
# A right-click does nothing.
general right none = nothing
```

## Sue's own rows

Without a file Sue Paint does what Clip Studio Paint does:

| Keys on the canvas | General |
| --- | --- |
| Ctrl | Object (the Operation tool), which moves a raster layer or picks vector lines |
| Alt | Eyedropper, for the tools that lay color (Pen, Pencil, Brush, Airbrush, Decoration, Fill, Gradient); the other tools keep Alt for themselves |
| Ctrl+Alt | Change brush size |
| Ctrl+Shift | Select layer: picks the layer painted under the pointer |
| Space | Hand |
| Shift+Space | Rotate the view |
| Ctrl+Space, Alt+Space | Zoom in, zoom out |

The wheel zooms, and turns the view with Shift. A right-click picks the
color shown, and the middle button scrolls.

Three of these rows were settings of their own before Sue 0.0.8:
`sue_paint.alt_tool`, `sue_paint.space_tool` and `sue_paint.ctrl_shift`.
They still decide General's rows for Alt, Space and Ctrl+Shift, and the
dialog keeps those three rows there (so Settings shows them) rather than
in the file. A row for them in the file wins over the setting.

## A tool key held down

Holding a tool's key (E for the Eraser, say) picks that tool only while
the key is down: let go after using it, or after holding it half a
second, and the tool before comes back. A tap picks the tool for good.
