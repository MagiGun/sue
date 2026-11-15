# Color sets: the file Sue keeps and the files people share

Sue Paint's Color Set palette shows one set of swatches at a time. Sue
ships four sets (Default, Grays, Skin Tones, Pastels) and people make
their own. Sets are not part of a picture: they belong to the person,
like brushes and layouts, so they live in Sue Paint's data folder and
travel between people as files. This page says what those files hold.
The code is `crates/sue-paint/src/colorsets.rs`.

## What a set is

A set has a name and a row of cells. A cell holds a color, with a name
when someone gave it one, or is empty. A new set starts as 32 empty
cells (four rows of eight). Deleting a color empties its cell and leaves
the rest where they were, so a set arranged by hand keeps its
arrangement. Two sets never share a name: Sue numbers a name that is
taken (`Grays 2`).

What can be done to sets:

- In the palette: click a swatch to draw with it, click an empty cell
  to put the drawing color there, double click a swatch for Color
  Settings, drag a swatch to move it, and right click a cell for Add
  Color, Replace Color, Delete Color, Change Color Name and Edit Color.
  The buttons under the swatches add the drawing color, replace the
  picked swatch with it and delete the picked swatch. The wrench menu
  also has small, medium and large swatches.
- From the palette's wrench (also the commands `color.new_set` and
  `color.edit_sets`, which can be given keys): New Color Set asks for a
  name; Rename Color Set; Edit Color Sets lists every set, which can be
  dragged by its grip or moved with Alt and Up or Down, renamed,
  duplicated, deleted (never the last one), and Add Standard Set brings
  back a copy of one of Sue's four sets.
- Import and Export, the two buttons beside the wrench.

## The file Sue keeps

`<Sue's data folder>/paint/color_sets.txt`, plain UTF-8 text, a set a
line under a versioned header:

```text
# sue paint color sets 2
Default<TAB>#000000/Black #ffffff - #f03c5a ...
My inks<TAB>#14141c #781e28/Wine%20red - - -
```

- The first line is the header; the number is the version.
- Each further line is a set: its name, a tab, then its cells separated
  by single spaces.
- A cell is `#rrggbb`, or `#rrggbb/name`, or `-` for an empty cell.
- In a color's name, `%`, spaces, tabs and line breaks are written as
  `%25`, `%20`, `%09`, `%0A` and `%0D`. In a set's name only `%`, tabs and
  line breaks are written so; its spaces stay as they are.
- The order of the lines is the order of the sets in the palette.

Version 1 (Sue 0.0.75) had the header `# sue paint color sets 1`, no
names and no empty cells; Sue still reads it and writes version 2 the
next time a set changes.

## Sharing a set: GIMP palettes

Export writes the set showing as a GIMP palette (`.gpl`), which GIMP,
Krita, Inkscape, Aseprite and most painting programs read:

```text
GIMP Palette
Name: My inks
Columns: 8
#
 20  20  28	#14141c
120  30  40	Wine red
# Sue: empty cell
```

A named color carries its name after the three numbers; an unnamed one
its hex code. An empty cell is the comment line `# Sue: empty cell`,
which other programs skip and Sue reads back as the empty cell, so a set
shared between two people who use Sue arrives as it was arranged.

## What Import reads

- GIMP palettes (`.gpl`): the `Name:` line names the new set, and what
  follows a color's three numbers names the color (`Untitled` or the
  color's own hex code names nothing).
- Lists of hex colors (`.txt`, `.hex`): one color a line, `#rrggbb`,
  `rrggbb` or `#rgb`; other lines are skipped.
- Clip Studio Paint color sets (`.cls`): the set's name and colors,
  empty cells and color names included. Sue reads the file's layout
  with its own code; nothing of Clip Studio's is copied. A file Sue
  cannot read is reported and changes nothing.

An imported set is added after the others and shown; its name is
numbered when another set has it.
