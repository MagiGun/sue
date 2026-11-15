# Sue Paint's layout and workspace files

Sue Paint keeps where its palettes are in plain text files in its data
folder (Settings, Sue Paint shows the folder; on Windows it is
`%APPDATA%\Sue\data\paint` unless `SUE_HOME` says otherwise). A person
can copy them to another computer, share them, edit them by hand or
delete them; Sue Paint reads them at start and writes them again as the
palettes move. None of them is ever part of a picture.

The words in these files are a file format, not interface text: they
are always written in English, whatever language Sue speaks. Sue Paint
0.0.75 and older wrote four of them (`panels`, `menus`, `hide`, `add`)
in the language in use; such a file is read once and written again in
English.

## Layout styles

A layout style is which design Sue Paint's workspace follows: which
palettes there are and where they sit until people move them. It is
the setting `sue_paint.layout_style` (Window, Layout Style, or
Settings). The styles and their ids:

| Id | Name | |
| --- | --- | --- |
| `classic` | Clip Studio | The default: Clip Studio Paint's arrangement |
| `sue_paint` | Sue Paint (Coming Later) | Listed, not built yet; never in use |

Ids never change: they are written in settings files, in these files
and in the command names `window.layout_style.<id>`. Each style keeps
its own files. The default style uses the names below; another style
puts its id before the extension (`layout.sue_paint.txt`,
`workspaces.sue_paint.txt`, `default_workspace.sue_paint.txt`), so
switching styles and back loses nothing.

## The files

| File | What it holds |
| --- | --- |
| `layout.txt` | The palettes as they were last left (when Settings, Remember the palette layout is on) |
| `workspaces.txt` | The registered workspaces (Window, Workspace, Register Workspace) |
| `default_workspace.txt` | The workspace Reset to Default goes back to (Use Current Workspace as Default) |
| `command_bar.txt` | The command bar's buttons (File, Command Bar Settings): [`paint-command-bar-files.md`](paint-command-bar-files.md) |
| `quick_access.txt` | Quick Access's sets of buttons, in the same lines |

`workspaces.txt` and `default_workspace.txt` hold sections: a line
`[name]`, then that workspace's lines. `layout.txt` holds the lines of
one workspace, with no section line. Blank lines are skipped, and a
line Sue Paint does not know is skipped too, so a file written by a
newer Sue opens in an older one.

## A workspace's lines

Palettes are named by their keys: `command_bar`, `tool`, `sub_tool`,
`tool_property`, `brush_grid` (Brush Size), `brush_size` (Tool
Sliders), `color` (Color Wheel), `color_slider`, `color_set`,
`intermediate_color`, `approximate_color`, `color_history`,
`navigator`, `sub_view`, `layer`, `layer_property`, `history`,
`animation_cels`, `timeline`, `auto_action`, `item_bank`, `info`,
`material`.

| Line | Meaning |
| --- | --- |
| `column <left\|right> <width> <group> <group> ...` | A column of palettes, from the window's edge inward; each group is `<weight>:<key>+<key>@<tab>` (its share of the column's height, its tabs, and the tab showing when it is not the first) |
| `top <height> <group> ...`, `bottom <height> <group> ...` | The row across the top and the row under the canvas |
| `float <x> <y> <width> <height> <key>` | A palette floating over the canvas |
| `window <x> <y> <width> <height> <key>` | A palette in a window of its own, placed from Sue's window |
| `icon <key>` | A palette collapsed to an icon in the strip at the right |
| `icon-join <key>` | That icon shares a group with the one before it |
| `fold <left\|right\|top\|bottom>` | A side or row folded into icons |
| `hide <left\|right\|top\|bottom>` | A side or row closed, leaving a thin bar |
| `close <key>` | A palette closed for good; only the Window menu brings it back |
| `panels <ten 0 or 1>` | Whether the Command Bar, Tool, Sub Tool, Tool Property, Tool Sliders, Color Wheel, Color Set, Navigator, Layer and Timeline palettes show |
| `menus <0 or 1>` | Whether the menu bar shows |
| `style <id>` | The layout style the workspace was made in |
| `strip tab` | Icons of the strip open their palettes as tabs (left out: as pop-ups) |
| `bar` | The workspace holds the command bar's buttons: the `bar` lines after it replace them |
| `bar <line>` | A line of `command_bar.txt` after its first ([`paint-command-bar-files.md`](paint-command-bar-files.md)), such as `bar command view.fit`. Sue Paint 0.0.75 wrote `bar hide <command>` and `bar add <command>`, still read: Sue Paint's buttons but those hidden, then those added |
| `quick` | The workspace holds Quick Access's sets: the `quick` lines after it replace them |
| `quick <line>` | A line of `quick_access.txt` after its first, such as `quick [Set 1]` |

A palette the lines lack goes where the layout style has it. A layout
kept by Sue Paint 0.0.75 or older, where Tool Property, Brush Size,
Color Slider, Intermediate Color, Approximate Color and Color History
were tabs inside other palettes, gets them back as tabs of those
palettes. A workspace kept before it held the command bar (no `bar`
line, no `quick` line) leaves the command bar and Quick Access as they
are.

## The command bar file

`command_bar.txt` and `quick_access.txt` are described in
[`paint-command-bar-files.md`](paint-command-bar-files.md). Sue Paint
0.0.75 wrote `command_bar.txt` as one line per item, `hide <command>` for
each built-in button hidden and `add <command>` for each command added
(in the language in use, and run together); such a file is read once as
Sue Paint's buttons but those hidden, then those added after a line,
and written again in the form of now.

## Example

```text
[Inking]
column left 46 1:tool
column left 322 0.24:sub_tool 0.3:tool_property 0.16:brush_grid 0.3:color+color_slider+color_set
column right 300 0.6:navigator 0.4:layer+history
top 38 1:command_bar
bottom 160 1:timeline
fold bottom
icon material
panels 1111011111
menus 1
style classic
bar
bar reorder ctrl-drag
bar command file.new
bar separator
bar command view.fit
```
