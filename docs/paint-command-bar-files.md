# Sue Paint's command bar and Quick Access files

The buttons of Sue Paint's command bar and of its Quick Access palette
are kept in two plain text files in Sue Paint's data folder (Settings,
Sue Paint shows the folder; on Windows it is `%APPDATA%\Sue\data\paint`
unless `SUE_HOME` says otherwise):

| File | What it holds |
| --- | --- |
| `command_bar.txt` | The command bar's buttons |
| `quick_access.txt` | Quick Access's sets and their buttons |

A person can copy them to another computer, share them, edit them by
hand or delete them (Sue Paint then starts from its own buttons). The
settings dialogs of both (File, Command Bar Settings; Quick Access
Settings in its palette menu) have **Export...** and **Import...**
buttons that write and read the same files anywhere, so a bar can be
passed on without looking for the data folder. A registered workspace
keeps both too (the `bar` and `quick` lines of
[`paint-workspace-files.md`](paint-workspace-files.md)).

The words in these files are a file format, not interface text: they
are always written in English, whatever language Sue speaks. Blank lines
are skipped, and so is a line Sue Paint does not know (a word of a newer
Sue, a command an older Sue does not have, a palette that cannot pop
up), so a file written by a newer Sue opens in an older one with what
that one can show.

## `command_bar.txt`

```text
sue-command-bar 1
reorder ctrl-drag
group
command file.save
command file.save_as
end
command edit.undo
icon star
separator
tool pen
sub-tool pen G-Pen
color #1e90ff
popup color
auto-action Flip Canvas and Fit
```

The first line says what the file is and its version. Then:

| Line | Meaning |
| --- | --- |
| `reorder drag` or `reorder ctrl-drag` | How buttons are moved: by a plain drag, or only by a drag with Ctrl held (Change Order). Ctrl + Drag when the line is missing |
| `command <name>` | A button that runs a command: a menu command or an option, by its stable command name (the names Shortcut Settings and Auto Actions keep). `app.palette` opens the command palette |
| `tool <id>` | A button that picks a tool: `zoom`, `hand`, `operation`, `move_layer`, `selection`, `auto_select`, `eyedropper`, `pen`, `pencil`, `brush`, `airbrush`, `decoration`, `eraser`, `blend`, `fill`, `gradient`, `figure`, `frame_border`, `ruler`, `text`, `balloon`, `correct_line` |
| `sub-tool <tool id> <name>` | A button that picks the sub tool of that tool with that name (the rest of the line, spaces and all). It is grayed while no sub tool has the name |
| `color #rrggbb` | A drawing color: the button makes it the color brushes paint with |
| `popup <palette>` | A button that pops a palette up under itself: `color`, `color_slider`, `color_set`, `intermediate_color`, `approximate_color`, `color_history`, `sub_tool`, `tool_property`, `brush_grid`, `layer`, `layer_property`, `navigator`, `history` |
| `auto-action <name>` | A button that plays the Auto Action of that name |
| `separator` | A line between two groups of buttons |
| `group` ... `end` | One button holding the buttons between: it shows and runs the first, and the mark at its corner opens a list of all of them (Insert into Group). A group holds no lines and no groups; one left with a single button becomes that button |
| `icon <name>` | Gives the button on the line before it another icon, by its stable name in Sue's icon set (the tables at the end of [`icon-packs.md`](icon-packs.md)). An icon pack that draws that name replaces it too. An unknown name shows the button's own icon |

## `quick_access.txt`

```text
sue-quick-access 1
reorder ctrl-drag
shown 0
[Set 1]
command edit.undo
command edit.redo
separator
command edit.clear
[Inking]
tool pen
color #000000
```

The first line says what the file is and its version; `reorder` is as
above, and `shown` is the set showing, counted from 0. Each set starts
with its name in square brackets, then its buttons in the lines of
`command_bar.txt`. On Quick Access a `separator` is a line across the
palette, and the buttons after it start a new row.

## Making one by hand

Any text editor will do. Start from an exported file or from the
examples above, keep the first line as it is, and put one button on
each line. Command names are listed in Settings, Shortcuts (each row's
name), and icon names at the end of [`icon-packs.md`](icon-packs.md). Import the file in the
settings dialog, or put it in the data folder while Sue is closed.
