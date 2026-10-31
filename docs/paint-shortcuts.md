# Sue Paint's keyboard shortcuts next to Clip Studio Paint's

Clip Studio Paint's default keyboard shortcuts next to Sue Paint's, so
Sue Paint can match them. Clip Studio's defaults come from its official
manual for Ver. 5.0 (help.clip-studio.com/en-us, the Shortcut list
chapter: Tool Shortcuts, Menu Shortcuts, Optional Shortcuts and
Shortcuts usable during operation, plus the Shortcut Settings and
Modifier Key Settings pages). Sue Paint's come from the keymaps in
`crates/sue-rice/rice/default.suerice` (the `sue_paint_keys` section,
with the shell's `keys` where shared) and the menu defaults in
`crates/sue-paint/src/menus.rs`, read on 2026-10-30.

- **Clip Studio** and **Sue Paint** name the keys; Windows keys, as the
  manual lists them. When the keymap and the menu default differ inside
  Sue, both are shown.
- **Same?** is `yes` when both sides match (the same chord, in any
  order), `differs` when both exist but disagree, and `missing in Sue`
  when Clip Studio has a default and Sue has none. Rows with no default
  on either side say so.
- Modifier rows marked "to verify" describe behavior Sue may have but
  that neither the keymap nor `HUMANS.md` documents; check the running
  app before building on them.

## File

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| New | Ctrl+N | Ctrl+N | yes |
| Open | Ctrl+O | Ctrl+O | yes |
| Close | Ctrl+W | Ctrl+W | yes |
| Save | Ctrl+S | Ctrl+S | yes |
| Save as | Shift+Alt+S, Ctrl+Shift+S, Ctrl+Alt+S | Shift+Alt+S, Ctrl+Shift+S, Ctrl+Alt+S | yes |
| Print | Ctrl+P | Ctrl+P | yes |
| Preferences | Ctrl+K | Ctrl+K in the keymap; the menu default shows Ctrl+, | differs |
| Shortcut Settings | Ctrl+Shift+Alt+K | Ctrl+Alt+Shift+K, and F1 or Alt+I | yes, Sue adds F1 and Alt+I |
| Modifier Key Settings | Ctrl+Shift+Alt+Y | no key | missing in Sue |
| Quit | Ctrl+Q | Ctrl+Q | yes |

## Edit

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Undo | Ctrl+Z | Ctrl+Z | yes |
| Redo | Ctrl+Y or Ctrl+Shift+Z | Ctrl+Y or Ctrl+Shift+Z | yes |
| Cut | Ctrl+X or F2 | Ctrl+X or F2 | yes |
| Copy | Ctrl+C or F3 | Ctrl+C or F3 | yes |
| Paste | Ctrl+V or F4 | Ctrl+V or F4 | yes |
| Paste to shown position | Ctrl+Shift+V | the command exists, no key | missing in Sue |
| Delete | Delete, Backspace, Ctrl+Delete, Ctrl+Backspace | Delete, Backspace, Ctrl+Delete, Ctrl+Backspace | yes |
| Delete Outside Selection | Shift+Delete, Shift+Backspace | Shift+Delete in the menu; the keymap gives Shift+Delete to Fill | differs |
| Fill | Alt+Delete, Alt+Backspace | Alt+Delete or Alt+Backspace, and Shift+Delete or Shift+Backspace | differs, Sue adds Shift+Delete |
| Hue/Saturation/Luminosity | Ctrl+U | Ctrl+U | yes |
| Reverse Gradient | Ctrl+I | Ctrl+I | yes |
| Scale/Rotate | Ctrl+T | Ctrl+T | yes |
| Free Transform | Ctrl+Shift+T | Ctrl+Shift+T | yes |

## Layer

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| New Raster Layer | Ctrl+Shift+N | Ctrl+Shift+N | yes |
| Create folder and insert layer | Ctrl+G | Ctrl+G | yes |
| Ungroup Layer Folder | Ctrl+Shift+G | Ctrl+Shift+G | yes |
| Clip to Layer Below | Ctrl+Alt+G | Ctrl+Alt+G | yes |
| Merge with Layer Below | Ctrl+E | Ctrl+E | yes |
| Merge Selected Layers | Shift+Alt+E | Alt+Shift+E | yes, the same chord |
| Merge Visible Layers | Ctrl+Shift+E | Ctrl+Shift+E | yes |
| Select Layer Above | Alt+] | Alt+] | yes |
| Select Layer Below | Alt+[ | Alt+[ | yes |
| Move Up | Ctrl+] | Ctrl+] | yes |
| Move Down | Ctrl+[ | Ctrl+[ | yes |

## Select

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Select All | Ctrl+A | Ctrl+A | yes |
| Deselect | Ctrl+D | Ctrl+D | yes |
| Reselect | Ctrl+Shift+D | Ctrl+Shift+D | yes |
| Invert Selected Area | Ctrl+Shift+I or Shift+F7 | Ctrl+Shift+I or Shift+F7 | yes |

## View

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Rotate Left | - | - | yes |
| Rotate right | ^ | = in the keymap; the menu default shows ^ | differs |
| Zoom in | Ctrl+Num+ or Ctrl+; | Ctrl++, Ctrl+= or Ctrl+; | yes, Sue adds Ctrl+= |
| Zoom out | Ctrl+Num- or Ctrl+- | Ctrl+- | yes |
| 100% | Ctrl+Alt+0 | Ctrl+Alt+0 | yes |
| Fit to Screen | Ctrl+0 | Ctrl+0 | yes |
| Reset display | Ctrl+@ | no key | missing in Sue |
| Reset Rotation | no default in the manual | Ctrl+Shift+2 | Sue adds it |
| Ruler | Ctrl+R | Ctrl+R | yes |
| Snap to Ruler | Ctrl+1 | Ctrl+1 | yes |
| Snap to Special Ruler | Ctrl+2 | Ctrl+2 | yes |
| Snap to Grid | Ctrl+3 | Ctrl+3 | yes |
| Toggle special ruler snapping | Ctrl+4 | no key | missing in Sue |

## Filter

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Hue/Saturation/Luminosity | Ctrl+U | Ctrl+U | yes, also under Edit above |
| Reverse Gradient | Ctrl+I | Ctrl+I | yes, also under Edit above |

Every other command in Clip Studio's Edit and Filter menus has no
default key, and neither does its opposite number in Sue.

## Animation

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Animation menu commands (New Timeline, Play, Move Frame and the rest) | no default keys in the manual | no keys | yes, neither side has any |

## Window

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Next canvas | Ctrl+Tab | Ctrl+Tab | yes |
| Previous canvas | Ctrl+Shift+Tab | Ctrl+Shift+Tab | yes |
| Show or Hide All Palettes | Tab | Tab | yes, in Sue Paint the shell's Tab for the command palette steps aside (the Command button stands in) |
| Hide Menu Bar | Shift+Tab | Shift+Tab | yes |

## Help

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Training and Tutorials | F1 | F1, and Alt+I, opens Sue's Keyboard Shortcuts list | differs |

## Tools

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Pen, Pencil | P | P | yes |
| Brush, Airbrush, Decoration | B | B | yes |
| Eraser | E | E | yes |
| Blend, Distort | J | J | yes |
| Fill, Gradient | G | G | yes |
| Figure, Frame border, Ruler | U | U | yes |
| Text, Balloon | T | T | yes |
| Correct line | Y | Y | yes |
| Selection area | M | M | yes |
| Auto select | W | W | yes |
| Eyedropper | I | I | yes |
| Operation (Object) | O | O | yes |
| Operation (Select layer) | D | no key | missing in Sue |
| Operation (Light table, Edit timeline) | L | no key | missing in Sue |
| Move layer | K | K | yes |
| Hand | H | H | yes |
| Rotate | R | R | yes |
| Zoom | / | / | yes |
| Previous sub tool | , | , | yes |
| Next sub tool | . | . | yes |
| Hold a tool's key to borrow the tool while held | yes, built in | no such behavior in the keymap | missing in Sue |
| Studio Mode keys T1 to T7 | none | run commands you choose (T1 Undo, T2 Redo, T3 Select All, T4 Deselect, T5 New Raster Layer, T6 Fit to Screen, T7 Flip Horizontal by default) | Sue adds it |

## Options

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Reduce opacity | Ctrl+[ | no key | missing in Sue |
| Increase opacity | Ctrl+] | no key | missing in Sue |
| Reduce brush density | Ctrl+Shift+O | no key | missing in Sue |
| Increase brush density | Ctrl+Shift+P | no key | missing in Sue |
| Reduce particle density | Ctrl+Shift+K | no key | missing in Sue |
| Increase particle density | Ctrl+Shift+L | no key | missing in Sue |
| Toggle refer multiple | 0 | no key | missing in Sue |
| Smaller brush preset | [ | [ | yes |
| Larger brush preset | ] | ] | yes |
| Switch using layer color | Ctrl+B | no key | missing in Sue |
| Swap main and sub color | X | X | yes |
| Switch drawing and transparent color | C | C | yes |

## Modifier keys, while drawing and on the canvas

| Command | Clip Studio | Sue Paint | Same? |
| --- | --- | --- | --- |
| Hand | Space and drag | Space and drag | yes |
| Rotate the view | Shift+Space and drag, or Shift and wheel | Shift+Space and drag, or Shift and wheel | yes |
| Zoom in | Ctrl+Space and click | Ctrl+Space and click | yes |
| Zoom out | Alt+Space and click | Alt+Space and click | yes |
| Select the layer under the pointer | Ctrl+Shift and click | Ctrl+Shift and click | yes |
| Eyedropper on the canvas | right-click | right-click | yes |
| Eyedropper while drawing | Alt and click | Alt and click | yes |
| Change brush size | Ctrl+Alt and drag | Ctrl+Alt and drag | yes |
| Draw a straight line | Shift and drag | Shift and drag | yes |
| Object (move a 3D or vector object while drawing) | Ctrl and drag | not documented | missing in Sue, to verify |
| Add to a selection | Shift and drag | not documented | missing in Sue, to verify |
| Delete from a selection | Alt and drag | not documented | missing in Sue, to verify |
| Select from a selection | Shift+Alt and drag | not documented | missing in Sue, to verify |
| Move layer, keep the original | Alt and drag | not documented | missing in Sue, to verify |
| Move layer, fix the direction | Shift and drag | not documented | missing in Sue, to verify |
| Fill, toggle referring to several layers | Shift and click | not documented | missing in Sue, to verify |
| Control point: delete, add, switch corner | Alt and click, Shift and click, Shift+Alt and click | not documented | missing in Sue, to verify |
| Modifier Key Settings (per key, per tool) | a dialog over Ctrl, Alt, Shift, Space, the wheel, right-click and touch | no dialog | missing in Sue |

## The differences, most used first

1. Reduce and increase opacity, Ctrl+[ and Ctrl+], are missing in Sue.
   The most used pair of all for painting.
2. Shift+Delete does two different things: Delete Outside Selection in
   Clip Studio (and in Sue's own menu), Fill in Sue's keymap. Sue's
   Alt+Delete matches Clip Studio's Fill. One chord should be settled.
3. Switch using layer color, Ctrl+B, is missing.
4. Toggle refer multiple, 0, is missing.
5. Operation's Select layer (D) and Light table or Edit timeline (L)
   keys are missing.
6. Brush density (Ctrl+Shift+O and P) and particle density
   (Ctrl+Shift+K and L) are missing.
7. Paste to shown position, Ctrl+Shift+V, is missing.
8. Reset display (Ctrl+@) and Toggle special ruler snapping (Ctrl+4)
   are missing.
9. Modifier Key Settings (Ctrl+Shift+Alt+Y) is missing, and Sue has no
   modifier key settings dialog to rebind any of the modifier rows.
10. Rotate right: Clip Studio uses ^, Sue's keymap = (Sue's own menu
    default shows ^, so the keymap and the menu disagree).
11. Preferences: both keymaps use Ctrl+K; Sue's menu default shows
    Ctrl+,.
12. F1 opens Sue's shortcut list, not a training page.
13. Sue adds: Alt+I for the shortcut list, Ctrl+Shift+2 for Reset
    Rotation, and the Studio Mode keys T1 to T7.
14. Holding a tool's key to borrow the tool temporarily works in Clip
    Studio and has no equivalent in Sue.
