# Icon packs

Every icon in Sue can be drawn by someone else. A person can replace a
single icon, or install an **icon pack** that replaces many; whatever a
pack leaves out is still drawn by Sue. Packs are files you make with any
vector drawing program (Inkscape, Figma, Illustrator, a text editor) and
share as a folder or a `.zip`. Nothing from Sue is needed to make one.

This page is for pack makers. It describes the format (version 1), what
Sue reads from an SVG file, and the name of every icon.

## The quickest start

Start from Sue's own set: it can be written out as a pack, one SVG file
per icon with a manifest to fill in (`sue_wm::icons::pack::export` or
`export_archive`; the icons page of Settings is where people will find
it). Change the drawings you
want to change, delete the files you do not want to change, put your
name in the manifest, and install the folder.

## What a pack is

A folder, or a `.zip` archive of one, that holds:

```text
my-pack/
  icon-pack.txt        the manifest
  folder.svg           one SVG file per icon, named after the icon
  heart.svg
  heart.solid.svg      the filled twin of an on and off pair
  LICENSE.txt          optional: your licence, kept with the pack
  README.md            optional: a note from you, kept with the pack
```

- Every icon file sits at the top of the folder. An archive may hold
  the files at its top or inside one folder; anything deeper is left
  out.
- A file is named after the icon's stable name (the tables at the end
  of this page) and ends in `.svg`. Names are lower case.
- A few icons are the "on" half of an on and off pair and have a filled
  twin: `star`, `heart`, `eye`, `lock`, `pin` and `keyframe`. The twin is
  `<name>.solid.svg`. If a pack replaces such an icon but not its twin,
  Sue draws its own twin for the state that is on, and says so in the
  pack's notes, so replace both.
  A twin is optional: the pack's card in Settings says which twins
  it leaves out, and does not count them among the files that could
  not be read.
- Other files (`thumbs.db`, pictures, folders, links) are left out and
  named in the notes. Only the manifest, the icons and a `LICENSE`,
  `LICENCE`, `COPYING` or `README` file (bare, `.txt` or `.md`) are
  copied when a pack is installed.
- Limits: one SVG file may be up to 256 KiB, a pack may hold up to 2048
  files and 32 MiB.

## The manifest: `icon-pack.txt`

Plain text, one `key = value` per line; a line that starts with `#` is a
comment.

```text
# A Sue icon pack
format = 1
name = Rounded Lines
author = Your Name
licence = CC-BY-4.0
version = 1.2.0
description = Soft, round icons for every part of Sue.
```

| Key | Meaning |
| --- | --- |
| `format` | The version of this format the pack is written in: `1`. A pack for a newer format is still read as far as Sue can, and says so. |
| `name` | The pack's name as people read it. Its id (the folder it is installed in) is made from it: lower case letters, digits and hyphens (`rounded-lines`). A name with letters outside `a-z` gets a short number at the end of its id (`n-code-ame-ca453922` for `Ünïcode Ñame`, `pack-5ddf9a30` for `アイコン`), the same number each time, so two such packs never replace each other. |
| `author` | Who made it. |
| `licence` | Its licence, best as an SPDX name (`CC-BY-4.0`, `CC0-1.0`, `MIT`). `license` is read too. |
| `version` | Its own version. Installing a pack whose name matches an installed one replaces it: that is how a pack is updated. |
| `description` | One sentence about it (optional). |

A pack without a manifest still installs, named after its folder or
archive, and says that it has none.

## What Sue reads from an SVG file

Sue draws icons as one ink on a 24 by 24 unit canvas, tinted with the
color of the theme exactly as its own icons are, and fitted to the
pixel grid at every size. It reads this subset of SVG:

- **Path data** (`<path d="...">`): `M L H V C S Q T A Z`, each in its
  absolute (upper case) and relative (lower case) form, numbers written
  any way SVG allows (`1-2`, `.5.5`, `1e-3`).
- **Shapes**: `line`, `polyline`, `polygon`, `rect` (with `rx` and
  `ry`), `circle`, `ellipse`.
- **Groups**: `g`, `a`, `switch` and a nested `svg` are read as groups;
  what a group says about paint is inherited.
- **Paint**: `fill`, `stroke`, `stroke-width`, `fill-rule` (`nonzero`
  and `evenodd`), `display="none"` and `visibility="hidden"`, written as
  attributes or in a `style` attribute (`style="fill:none;stroke:#000"`).
  SVG's own defaults hold: a shape with no paint is filled.
- **Placement**: `transform` (`matrix`, `translate`, `scale`, `rotate`,
  `skewX`, `skewY`), and the root's `viewBox`, scaled evenly into the 24
  unit canvas and centered. Without a `viewBox`, the root's `width` and
  `height` (in plain numbers or `px`) stand for it; with neither, the
  canvas is 24 by 24.
- **Masks**: `mask="url(#id)"` on a shape or a group, the `mask` drawn in
  black and white: light keeps, dark cuts away. This is how a line
  crosses a shape with a gap on each side (Sue's own icons do it).

What is not read, and why:

- **Colors.** An icon is one ink, so that it follows the theme and the
  state of its button. Every color draws ink; white and black only mean
  something inside a mask. A gradient or pattern draws plain ink.
- **Opacity**, **caps and joins** (always round, like Sue's pen),
  **dashes**, **markers**, **filters** and **clip paths**. Each is
  named in the notes where a file asks for it (`opacity="0.4"`,
  `stroke-linecap="square"`); `round` caps and joins and an opacity of
  1 are what Sue draws anyway and are not noted.
- **Text**, **pictures** (`image`), **`use`** and **style sheets**
  (`<style>` and `class`). Convert text to paths and write paint as
  attributes.

Nothing in a file stops the rest of it from being read. Each thing Sue
could not read is named in the pack's notes with the file it is in: an
element that is not drawn, an attribute that cannot be read, path data
that breaks off (what comes before the break is drawn, as browsers do),
a file that is not text, has no `svg` element or draws nothing. A file
that draws nothing falls back to Sue's own icon.

An icon is small, and Sue keeps it so: elements nested more than 64
deep and path segments past the 4096th (a shape counts each of its
segments, plus one per subpath) are left out and noted, and a drawing
that would need more than 16384 straight pieces at one size is drawn
with coarser curves. Sue's own icons use at most a few hundred
segments. A file is at most 256 KiB.

### Pens and the grid

- Draw on a 24 unit canvas (or any `viewBox`, which is scaled to it).
  Sue's own icons keep 2 units clear on every side.
- Sue's own pen is 1.5 units wide. A stroke of 1.5 in a pack is drawn as
  wide as Sue's own pen at every size (a little heavier at 16 points, so
  small icons do not thin out); a wider or thinner stroke is drawn in
  proportion.
- Straight lines along an axis are sharpest where their outer edge lies
  on a whole unit: a 1.5 wide line at `x = 4.75` (left of the middle) or
  `x = 19.25` (right of it). Sue's pixel fitting moves lines a little
  either way, so this is advice, not a rule.

## Replacing one icon

A single icon can be replaced without a pack. The SVG goes into the
overrides folder under the icon's file name (`folder.svg`,
`heart.solid.svg`), which the icons page of Settings
will do for you. The order is always:

1. a single override,
2. the chosen pack,
3. Sue's own icon.

Removing the override gives the icon back to the pack, or to Sue.

## Where Sue keeps them

In Sue's data folder, under `icons` (with `SUE_HOME` set, that is
`SUE_HOME\data\icons`):

```text
icons/
  chosen.txt      the id of the pack in use; none means Sue's own
  packs/<id>/     each installed pack
  overrides/      single icons
```

Installing, choosing, removing and replacing take effect at once: the
next frame draws the new icons, with no restart.

## The legal rule

**A pack must not copy another program's icons.** Do not trace, copy,
convert or closely imitate the icons of Clip Studio Paint, or of any
other program, icon set or web site, unless their licence allows it and
you follow it. Draw your own, or use a set whose licence lets you (and
say which in your `licence` line and `LICENSE` file).

Sue ships no pack that copies another program's icons. Sue's own icons
are drawn from a written description of what each one means, never
traced from or matched against another program's artwork (the rule at
the top of `crates/sue-wm/src/icons/mod.rs`).

## Icon names are kept

An icon's name is saved data: packs and overrides are named after it. A
name, once in a release, does not change or disappear. A test holds
every name of 0.0.8 (`crates/sue-wm/tests/golden/icon-names.txt`); a
name that must go is retired in that test with what stands in its
place, and listed here.

Retired names: none yet.

## Every icon

The name of each icon and the picture Sue draws for it, grouped as Sue's
own files are. A twin marked "yes" also has a `<name>.solid.svg`.

### The twelve reference icons

| Name | Twin | What it shows |
| --- | --- | --- |
| `settings` |  | A gear: a toothed wheel with a round hole in its middle. Settings. |
| `search` |  | A magnifier: a round lens with a short handle down to the right. Search. |
| `folder` |  | A closed folder seen from the front: a box with a tab on its top left. A folder of files. |
| `delete` |  | A bin: a can with a lid, a handle on the lid and two ribs. Delete. |
| `info` |  | A ring with a small letter i in it: a stem under a solid dot. Information about something. |
| `tool-brush` |  | A round paint brush lying from the top right down to the bottom left: a handle, and a solid pointed tuft of bristles. The brush tool. |
| `picture` |  | A framed picture of two hills under a solid round sun. An image. |
| `document` |  | A sheet of paper with its top right corner folded down, solid, and two lines of writing. A document. |
| `star` | yes | A star with five points. A favorite; solid when it is one. |
| `heart` | yes | A heart. Something liked; solid when it is. |
| `eye` | yes | An open eye: two lids and a round iris. Shown; solid for the state that is on. |
| `lock` | yes | A padlock: a body with a keyhole and a shackle over it. Locked; solid for the state that is on. |

### The window, moving about and shared controls

| Name | Twin | What it shows |
| --- | --- | --- |
| `window-minimize` |  | A level line across the middle, a little shorter than the minus. Puts a window away, down to the bar. |
| `window-maximize` |  | One box that fills the picture. Makes a window fill the screen. |
| `window-restore` |  | A box in front of the corner of a second box. Brings a window that fills the screen back to its own size. |
| `window-close` |  | A large cross from corner to corner. Closes a window. |
| `close` |  | A cross, clearly smaller than the one of the window. Closes or removes the thing it sits on: a tab, a tag, a search. |
| `check` |  | A tick: a short stroke down and a long one up to the right. Chosen, done, switched on. |
| `chevron-left` |  | An angle that points left. Back, the one before, fold in. |
| `chevron-right` |  | An angle that points right. On, the next one, a list that opens to the side. |
| `chevron-up` |  | An angle that points up. Fold away, one up. |
| `chevron-down` |  | An angle that points down. Unfold, one down. |
| `chevrons-left` |  | Two angles, one behind the other, that point left. Fold a whole side of the window away to the left, or go to the very first. |
| `chevrons-right` |  | Two angles, one behind the other, that point right. Fold a whole side of the window away to the right, or go to the very last. |
| `caret-down` |  | A small solid triangle that points down. A button that opens a list under it; an open branch of a tree. |
| `caret-right` |  | A small solid triangle that points right. A closed branch of a tree. |
| `arrow-left` |  | An arrow to the left: a shaft and an open head. Go back. |
| `arrow-right` |  | An arrow to the right: a shaft and an open head. Go on. |
| `arrow-up` |  | An arrow that points up: a shaft and an open head. Move up. |
| `arrow-down` |  | An arrow that points down: a shaft and an open head. Move down. |
| `menu` |  | Three level lines of one length, one under the other and spread over the height of the picture. Opens a menu. |
| `more` |  | Three solid dots in a row, as wide as the picture. More that did not fit. |
| `more-vertical` |  | Three solid dots, one under the other, as tall as the picture. More, on a narrow or upright bar. |
| `grip` |  | Six solid dots, two beside each other three times. A place to take hold of and drag. |
| `plus` |  | A plus: an upright and a level line that cross. Add. |
| `minus` |  | A minus: one level line across the middle, a little heavier than a stroke so that it holds its own at 16 points. Take away. |
| `filter` |  | A funnel: wide at the top, with a narrow spout. Show only what passes. |
| `sort-up` |  | An arrow that points up beside three lines that grow longer downward. Sorted from first to last. |
| `sort-down` |  | An arrow that points down beside three lines that grow shorter downward. Sorted from last to first. |
| `view-grid` |  | Four boxes, two by two. Shown as large cards. |
| `view-grid-small` |  | Nine small solid squares, three by three. Shown as small cards. |
| `view-list` |  | Three rows, each a solid dot and a line. Shown as a list. |
| `external` |  | A box with its top right corner open and an arrow leaving through it. Opens outside Sue. |
| `details` |  | A ring with a large angle in it that points right. Everything about one thing, on a page of its own. |
| `pin` | yes | A pushpin standing upright: a cap, a neck that widens into a plate, and a needle. Kept in place; solid when it is. |
| `help` |  | A ring with a question mark in it. Help. |
| `warning` |  | A triangle with round corners that stands on its base, and in it an exclamation mark. Take care: something may go wrong. |
| `error` |  | A sign with eight sides and in it an exclamation mark. Something went wrong and stopped. |
| `question` |  | A large question mark by itself. Sue asks and waits for an answer. |
| `radio-on` |  | A ring with a large solid dot in it. The one that is chosen, of several. |
| `radio-off` |  | An empty ring. One that is not chosen, of several. |
| `reload` |  | A ring, open at the right, that runs clockwise and ends in the corner of an arrow head. Load again. |
| `home` |  | A house: a roof, walls and a door. The start page. |

### Files and data

| Name | Twin | What it shows |
| --- | --- | --- |
| `file-new` |  | A sheet of paper with a folded corner and a plus on it. A new file. |
| `file-open` |  | An open folder with a sheet of paper standing up out of it behind its front. Open a file. |
| `folder-open` |  | A folder whose front leans open. A folder that shows what is in it. |
| `folder-new` |  | A closed folder with a plus on it. A new folder. |
| `save` |  | A storage disk: a square case with one corner cut, a shutter at the top and a label below. Save. |
| `save-as` |  | The storage disk with three dots in place of its label. Save under another name; Sue asks which. |
| `import` |  | An arrow that goes from the left into a box open on that side. Bring something in from a file. |
| `export` |  | An arrow that leaves to the right from a box open on that side. Write something out to a file. |
| `export-pdf` |  | A sheet of paper with a folded corner and an arrow that leaves it to the right. Make the finished pages: a PDF. |
| `print` |  | A printer: a box, a sheet going in at the top and a sheet coming out at the front. Print. |
| `download` |  | An arrow that points down into a tray. Fetch something and keep it. |
| `transfers` |  | Two arrows side by side, one up and one down. What is being sent and fetched. |
| `copy` |  | A sheet in front of the corner of a second sheet. Copy. |
| `cut` |  | A pair of scissors, open, points up. Cut. |
| `paste` |  | A clipboard: a board with a clip at its top and two lines of writing. Paste. |
| `duplicate` |  | A sheet in front of the corner of a second sheet, with a plus on it. Make another one of this. |
| `rename` |  | A text field with a little writing in it and a text cursor after the writing. Give another name. |
| `edit` |  | A pencil, point down to the left, over the line it is writing. Edit. |
| `tag` |  | A label with a hole for its string: square at one end, pointed at the other. A tag. |
| `move-to` |  | A closed folder with an arrow in it that points right. Move into a folder. |
| `recent` |  | A clock: a ring and two hands. Used a short while ago. |
| `history` |  | A clock whose ring runs backward and ends in the corner of an arrow head. What was done, step by step. |
| `link` |  | Two links of a chain, joined. A link to something elsewhere. |
| `wrench` |  | A wrench lying from the bottom left up to its open jaw at the top right. Tools, repair, setting up. |
| `all` |  | An asterisk of six arms: the sign that stands for anything. Everything, with nothing left out. |

### Editing and the view

| Name | Twin | What it shows |
| --- | --- | --- |
| `undo` |  | An arrow that comes back to the left out of a turn. Undo. |
| `redo` |  | An arrow that goes on to the right out of a turn. Redo. |
| `zoom-in` |  | A magnifier with a plus in its lens. Zoom in. |
| `zoom-out` |  | A magnifier with a minus in its lens. Zoom out. |
| `zoom-fit` |  | The four corners of a frame around a small box. Show the whole picture in the window. |
| `zoom-actual` |  | The numbers one to one, with a colon between them. One pixel of the picture on one of the screen. |
| `flip-horizontal` |  | An upright line with a triangle on its left that points away from it, and the same triangle mirrored, solid, on its right. Flip left to right. |
| `flip-vertical` |  | A level line with a triangle above it that points away from it, and the same triangle mirrored, solid, below. Flip top to bottom. |
| `rotate-left` |  | A box, and a quarter of a circle with an arrow head that turns over it to the left. Turn a quarter counterclockwise. |
| `rotate-right` |  | A box, and a quarter of a circle with an arrow head that turns over it to the right. Turn a quarter clockwise. |
| `reset` |  | An arrow that runs backward round a solid dot. Back to how it was at the start. |
| `swap` |  | Two level arrows, the upper to the right and the lower to the left. The two change places. |
| `replace` |  | An empty square and a solid one side by side at the bottom, and an arrow that arcs over from the first into the place of the second. Put another in its place. |
| `reverse` |  | An upright arrow with a head at both ends beside three lines of one length. Turn the order round. |
| `confirm` |  | A ring with a tick in it. Yes: apply it. |
| `cancel` |  | A ring with a cross in it. No: leave it. |
| `eye-off` |  | The open eye, struck through from the top left to the bottom right. Hidden. |
| `unlock` |  | The padlock with its shackle open: one end free, lifted off the body. Not locked. |
| `fullscreen` |  | Two arrows that point apart, to the top right and to the bottom left corner. Fill the whole screen. |
| `keyboard` |  | A keyboard: a wide box with a row of keys and a long key under them. Keyboard shortcuts. |
| `finger` | yes | A hand seen from its back pointing up with its index finger, the other fingers curled and the thumb out to the left. Fingers on a touch screen: whether they draw. Its solid twin is the hand filled. |
| `moon` |  | A crescent moon. The dark look. |
| `sun` |  | A sun: a ring with eight short rays. The light look. |
| `language` |  | A globe: a ring, the line round its middle and a narrow oval from pole to pole. The language. |
| `update` |  | A ring with an arrow in it that points up. A newer version. |

### The Hub

| Name | Twin | What it shows |
| --- | --- | --- |
| `app-paint` |  | An artist's palette: a round board with a bite out of its lower right where the hand holds it, and four solid dabs of paint in an arc along its upper edge. Sue Paint, the studio. |
| `app-writer` |  | A quill: a long feather leaning from the lower left up to the top right, its shaft running on down past it to the nib. Sue Writer, the studio. |
| `app-generic` |  | An easel with nothing on it: a blank canvas standing on three legs, ready for any kind of work. A studio or tool that has no picture of its own. |
| `template` |  | A sheet laid out before anything is on it: a small box for a picture at the top and two lines for writing under it. A template, a ready-made start for something new. |
| `recover` |  | A lifebuoy: a ring with a hole, bound across in four places. Work Sue kept safe when it was not saved, and bringing it back. |
| `folder-reveal` |  | A closed folder with an arrow leaving it at its top right corner. Showing a folder in the system's own file manager. |
| `layout-style` |  | A wide window cut into a bar along its top, a column on each side and a large space in the middle. How a studio's palettes are laid out around the canvas: its layout style. |
| `film` |  | A strip of film: a frame between two rails cut into cells, with a solid play mark in the frame. A film or a video. |
| `audio` |  | A sound wave: five upright strokes side by side, tallest in the middle. Sound, a kind of file. |
| `cube` |  | A cube seen from above a corner: three faces meeting in the middle. A 3D model or scene. |
| `book` |  | A book lying open: two pages curving down to the spine. A guide to read. |
| `mail` |  | A closed envelope: a box with its flap folded down to a point in the middle. Mail, writing to someone. |
| `feedback` |  | A round speech bubble with its tail at the lower left and three solid dots in a row in it. Telling the makers of Sue what you think. |
| `materials` |  | Three shapes together: a triangle on top, a circle and a square under it. Materials, the parts kept ready to put into a work. |
| `files` |  | A filing cabinet: a tall box of two drawers, each with a pull, on two short feet. Files: everything made in Sue, kept to look through. |

### Tools

| Name | Twin | What it shows |
| --- | --- | --- |
| `tool-zoom` |  | A magnifying glass held in the hand: a round lens with a glint of light along its upper left, and a solid stout handle down to the right. The zoom tool. |
| `tool-hand` |  | An open hand seen from its back: four fingers up, the thumb out to the left, a round palm. The hand tool, which moves the view. |
| `tool-rotate-view` |  | A sheet tilted to one side, inside one curved arrow that turns nearly all the way round it, its solid head at the lower left. The tool that rotates the view of the canvas. |
| `tool-operation` |  | A box with an arrow pointer, solid, resting on its lower right corner. The tool that picks an object and works on it. |
| `tool-move-layer` |  | A cross of four arrows, pointing up, down, left and right. The tool that moves a layer. |
| `tool-select` |  | A box drawn with a dashed line: the marching line round a selected area. The selection tool. |
| `tool-select-lasso` |  | A lasso: a flat loop of rope, and the loose end of the rope hanging from its lower left and curling away to the right. The tool that selects an area drawn freehand. |
| `tool-auto-select` |  | A magic wand: a slim rod rising to the right, a band round it near its tip, and a solid sparkle of four points just past the tip. The tool that selects an area of one color with a click. |
| `tool-eyedropper` |  | A dropper lying from the top right down to the bottom left: a solid rubber bulb, a collar, and a glass tube that narrows to a point. The tool that picks a color from the canvas. |
| `tool-pen` |  | The steel nib of a dip pen in its holder, pointing down to the bottom left: a broad pointed nib with a slit and a solid round hole. The pen tool. |
| `tool-pencil` |  | A pencil pointing down to the bottom left: a straight shaft, a sharpened cone, and a band near its end where the rubber sits. The pencil tool. |
| `tool-airbrush` |  | A spray can: a tall body, a narrow square cap on it and a nozzle sticking out of the cap to the left, with a fan of solid drops flying from the nozzle and spreading as they go. The airbrush tool. |
| `tool-decoration` |  | Three sparkles of four points: a large one drawn as a line, and two small solid ones beside it. The tool that stamps a trail of ornaments. |
| `tool-eraser` |  | A block of rubber lying on its lower left corner, with a line across it where its sleeve ends, and the ground it rubs on. The eraser tool. |
| `tool-blend` |  | Two rings side by side that overlap, solid where they overlap: two colors running into each other. The blend tool. |
| `tool-liquify` |  | A spiral that winds inward: the picture stirred round like liquid. The liquify tool. |
| `tool-fill` |  | A bucket of paint tipped on its corner, with its handle, the paint in it solid up to a level line, and a solid drop falling from its lip. The fill tool. |
| `tool-gradient` |  | A solid upright band, and after it three columns of solid dots, each column of smaller dots than the last, the middle one set between the rows of the other two: a tone that fades from full to empty, as print shows it. The gradient tool. |
| `tool-figure` |  | A circle in front of a square, the square showing behind it at the top left. The tool that draws lines and shapes. |
| `tool-frame` |  | A page of a comic cut into three panels: a slanted gutter across it and an upright one below. The tool that draws panel frames. |
| `tool-balloon` |  | A speech balloon: an oval with a pointed tail down to the lower left. The tool that draws the balloons of a comic. |
| `tool-ruler` |  | A flat ruler lying from the top left down to the bottom right, with three marks along one edge. The ruler tool. |
| `tool-text` |  | A capital letter T with serifs: a bar whose ends turn down, a stem and a foot. The text tool. |
| `tool-correct-line` |  | A curved line between two small square points, with a solid round point on its middle: a drawn line and the points it is reshaped by. The tool that corrects lines. |

### The canvas

| Name | Twin | What it shows |
| --- | --- | --- |
| `select-all` |  | A dashed box hugging a box drawn whole that nearly fills it: the selection round everything there is. Select all. |
| `deselect` |  | A dashed box struck through by a line from the bottom left to the top right. Deselect: the selection is let go. |
| `select-invert` |  | A box cut from corner to corner: the upper left half a dashed line, the lower right half solid. Invert the selection: what was outside is now inside. |
| `select-expand` |  | A dashed box, and inside it one slanted line with an arrow head at each end, pushing two corners apart. Expand the selection. |
| `select-shrink` |  | A dashed box, and inside it two arrows that come from two corners and point at each other. Shrink the selection. |
| `clear-inside` |  | A dashed box with a cross inside it. Clear: what is inside the selection is taken away. |
| `clear-outside` |  | A small dashed box at the bottom left and a cross outside it, at the top right. Clear outside: what is not in the selection is taken away. |
| `fill-selection` |  | A dashed box with a solid drop of paint inside it. Fill: the selection is filled with the drawing color. |
| `crop` |  | Two right angles that cross at two corners and leave a square between them: the marks a picture is cut along. Crop. |
| `transform` |  | A box with a solid square handle on each of its four corners: the frame a picture is scaled and turned by. Transform. |
| `snap-ruler` |  | An upright horseshoe magnet with solid poles, over the edge of a ruler with three marks, the middle one longer. Snap to the ruler. |
| `snap-grid` |  | An upright horseshoe magnet with solid poles, over a row of the square cells of a grid. Snap to the grid. |
| `color-swap` |  | Two color chips, a solid one at the top left and an empty one at the bottom right, and two bent arrows that lead from each to the other. Swap the drawing color and the second color. |

### Layers and the light table

| Name | Twin | What it shows |
| --- | --- | --- |
| `layers` |  | Three sheets lying one over the other: the layers of a picture, and the palette that lists them. |
| `layer-new` |  | Two sheets lying one over the other, with a plus at the lower right: a new raster layer. |
| `layer-new-vector` |  | A curve between two solid square anchor points, with a plus at the lower right: a new vector layer. |
| `layer-new-folder` |  | A folder with a plus at its lower right corner: a new folder that layers are put into. |
| `layer-duplicate` |  | Two equal sheets, one lying behind and above the other: a copy of the layer. |
| `layer-merge-down` |  | An arrow pressing down into a sheet: the layer is merged into the one below it. |
| `layer-transfer-down` |  | Two sheets one over the other, and beside them an arrow pointing down: what is drawn moves to the layer below. |
| `layer-clip` |  | An arrow that bends down onto a sheet: the layer is clipped to the layer below it. |
| `layer-reference` |  | A marker pin standing on a sheet: the layer that other tools look at. |
| `layer-draft` |  | A sheet drawn with a broken line: a draft layer, which is neither printed nor exported. |
| `layer-lock-alpha` |  | A checkered square, the sign of see-through pixels, with a padlock at its lower right: the transparent pixels are locked. |
| `layer-mask` |  | A frame with a solid round shape in it: a layer mask, which hides what lies outside the shape. |
| `layer-mask-new` |  | A layer mask with a plus at its lower right corner: a new layer mask. |
| `layer-mask-apply` |  | A layer mask with a check at its lower right corner: the mask is applied to the layer for good. |
| `layer-color` |  | Two sheets one over the other, with a solid drop of paint at the lower right: the layer is shown in its layer color. |
| `layer-color-mode` |  | A ring whose left half is solid: the layers on the light table are each shown in one color. |
| `layer-property` |  | Two sheets one over the other, with a solid four pointed spark at the lower right: the effects a layer can have. |
| `layer-opacity-target` |  | A sheet over the edge of a second one drawn with a broken line: whether the opacity changes all the layers or each on its own. |
| `layer-paper` |  | A sheet lying over a solid one: the paper, a layer of one color under the whole picture. |
| `border-effect` |  | A solid ball and a solid block, with one line running round both at a distance: a border drawn round what is on the layer. |
| `watercolor-edge` |  | A drop with a second line inside its lower left rim, where the paint gathers: the watercolor edge. |
| `extract-line` |  | A solid hill with its own outline lifted off above it: the lines are taken out of a picture. |
| `tone` |  | Rows of dots that grow from small at the upper left to large at the lower right: the layer is shown as a halftone. |
| `light-table` |  | A lit panel with rays of light rising from it: the light table, which shows other drawings through the layer. |
| `light-table-add` |  | A light table with a plus at its upper right: the chosen layer is put on the light table. |
| `light-table-remove` |  | A light table with a minus at its upper right: the chosen layer is taken off the light table. |
| `light-table-file` |  | A light table with a page at its upper right: a picture file is put on the light table. |
| `light-table-tool` |  | A light table with a solid pointer at its upper right: the tool that moves what lies on the light table. |
| `cel-lock` |  | A cel with its peg holes, and a padlock at its lower right: the cel drawn on stays the same, whichever cel is shown. |

### Palettes, color and the brush

| Name | Twin | What it shows |
| --- | --- | --- |
| `palette-command-bar` |  | A long bar with three solid keys in it: the command bar. |
| `palette-tools` |  | A cup with three pens standing in it: the palette of tools. |
| `palette-sub-tools` |  | A solid square with a branch running down from it to two boxes: the sub tools of the tool. |
| `palette-tool-property` |  | Three upright sliders with round knobs at different heights: the settings of the tool. |
| `palette-brush-size` |  | Three solid dots in a row, each much larger than the one before, filling the width: the brush sizes. |
| `palette-brush-grid` |  | Four rings of four sizes laid out in a square: the brush sizes as a grid. |
| `palette-color-wheel` |  | A ring with a square in it and a solid knob on the ring: the wheel of hues, and the square that sets how light and how strong the color is. |
| `palette-color-sliders` |  | Three level bars, each with a solid knob at its own place: the color sliders. |
| `palette-color-set` |  | Three color cards fanned out from one rivet: a set of colors to pick from. |
| `palette-intermediate` |  | A strip cut into three cells, the middle one solid: a color that lies between the colors on either side of it. |
| `palette-approximate` |  | A square cut into three by three cells, the middle one solid: colors close to the current one. |
| `palette-color-history` |  | An arrow pointing back to the left over a row of three color chips, the first one solid: the colors used before. |
| `palette-navigator` |  | A frame with a smaller frame inside it, up and to the right: the part of the picture the view shows. |
| `palette-sub-view` |  | A picture with a push pin through its top: a picture kept beside the canvas to look at. |
| `palette-materials` |  | A ball, a block and a cone standing on a shelf: the materials kept for use in pictures. |
| `palette-history` |  | Three steps in a column, each a ring with its line of text, the rings joined by a line and the last one solid: what was done, in order. |
| `palette-auto-action` |  | A solid play mark beside three lines: a list of steps that run by themselves. |
| `palette-item-bank` |  | A box with its lid on and a handle slot on its front: items put away to use again. |
| `palette-info` |  | A cross hair beside two lines of numbers: where the pointer is and what is under it. |
| `palette-timeline` |  | Two tracks of different lengths crossed by a playhead with a solid head: the timeline. |
| `palette-cels` |  | Two cels lying one behind the other, peg holes along the top: the cels of an animation. |
| `pressure-curve` |  | A curve rising in a corner between two axes, with a solid point at its end: how the pressure of the pen sets the width of the stroke. |

### Animation and the timeline

| Name | Twin | What it shows |
| --- | --- | --- |
| `play` |  | A triangle that points to the right. Play. |
| `stop` |  | A small square, well inside the square of maximize. Stop playing. |
| `pause` |  | Two upright bars with round ends, side by side. Hold the playing where it is, to go on from there. |
| `keyframe-add` |  | A diamond outline, the mark of a key on the timeline, with a plus at its lower right. Add a keyframe. |
| `keyframe` | yes | The diamond of a key alone, solid when its value is keyed at this frame. Key one value. |
| `keyframes` |  | A solid diamond on a level track that runs out of both its sides. Keyframes are on for this layer. |
| `frame-first` |  | Two solid triangles that point to the left, the one at the front touching the back of the other, at an upright bar they run into. Go to the first frame. |
| `frame-last` |  | Two solid triangles that point to the right, the one at the front touching the back of the other, at an upright bar they run into. Go to the last frame. |
| `frame-prev` |  | A triangle that points to the left and leaves an upright bar behind it. One frame back. |
| `frame-next` |  | A triangle that points to the right and leaves an upright bar behind it. One frame on. |
| `loop` |  | Two arrows that follow each other round a track: one along the top to the right, one along the bottom to the left. Play over and over. |
| `cel` |  | A sheet of animation paper lying on its long side, with the three peg holes along its top: a slot, a round hole, a slot. One cel. |
| `cel-new` |  | The sheet of a cel with a plus where its lower right corner would be. A new cel. |
| `cel-specify` |  | The sheet of a cel with a solid diamond, the mark of a key on the timeline, where its lower right corner would be. This frame shows this cel. |
| `onion-skin` |  | A solid ball, and behind it on the left the edges of the same ball where it was the two frames before. Onion skin, the frames before and after shown through. |
| `timeline-new` |  | A long track crossed by a playhead with a solid head, a shorter track under it, and a plus in the lower right corner. A new timeline. |
| `animation-folder` |  | A folder with a solid triangle that points to the right in it. An animation folder, the folder that holds the cels of one track. |

### Documents and materials

| Name | Twin | What it shows |
| --- | --- | --- |
| `doc-illustration` |  | An easel: a canvas with one brush stroke on it, standing on two legs. An illustration, one picture. |
| `doc-webtoon` |  | A tall narrow strip cut into three panels, and beside it an arrow that points down. A webtoon, one long picture to scroll. |
| `doc-comic` |  | A page cut into three panels: one across the top and two below it with a slanted gutter between them. A comic, a story of pages. |
| `doc-print-book` |  | A closed book seen from the front: a cover with a line of title, over the block of its pages, which curls round at the spine. Pages sized for a printed book. |
| `doc-comic-settings` |  | A page with two panels across its top and a solid gear where its lower right corner would be. A comic with every setting shown. |
| `doc-animation` |  | A sheet lying on its long side with a small solid triangle that points to the right, over a scrub bar with a solid knob on it. An animation, a picture that plays. |
| `material-image` |  | An instant photo: a square card with a picture of one peak under a solid sun, and a blank strip below it. A picture kept as a material. |
| `material-pattern` |  | A square tile laid with bricks: three rows, the joints of each row halfway between those of the next. A pattern that repeats. |
| `material-tone` |  | A square swatch of screentone: three rows of three solid dots, small at the upper left and growing toward the lower right, so that the tone fades. A tone. |
| `material-brush-tip` |  | The head of a brush seen from the side, pointing down: the metal band with a crimp across it and under it a solid tuft that swells and ends in a point. A brush tip. |
| `material-pose` |  | A drawing figure in a pose: a round head, a body, one arm up and one down, the legs in a stride. A pose for a 3D figure. |
| `draw` |  | A pencil lying from the top right down to the left, and the wavy line its point is drawing. Start drawing. |
| `projects` |  | A pile: a box with two lines over it, each shorter than the one under it, as cards lie stacked. The projects, every work kept. |
| `picture-list` |  | Two rows, each a small square picture with two lines of writing beside it. Every picture, as a list of pictures. |

### Writing and text

| Name | Twin | What it shows |
| --- | --- | --- |
| `bold` |  | A capital letter B with its upright drawn solid and wide: the letters made heavier. Bold. |
| `italic` |  | A capital letter I leaning to the right, with a bar at its head and at its foot. Italic, the letters slanted. |
| `underline` |  | A capital letter U standing over a level line. Underline. |
| `strikethrough` |  | A capital letter S with a level line through its middle. Strikethrough, the letters struck out. |
| `align-left` |  | Four lines of writing, the second and the fourth shorter, all starting at the left edge. Lines aligned to the left. |
| `align-center` |  | Four lines of writing, the second and the fourth shorter, all centered on the middle. Lines centered. |
| `align-right` |  | Four lines of writing, the second and the fourth shorter, all ending at the right edge. Lines aligned to the right. |
| `align-justify` |  | Four lines of writing, every one running from edge to edge: a justified paragraph. Lines justified. |
| `indent-more` |  | Four lines of writing, the middle two starting further in, and a solid head at the left pointing right, into the space. Indent more. |
| `indent-less` |  | Four lines of writing, the middle two starting further in, and a solid head at the left pointing left, out of the space. Indent less. |
| `list-bullets` |  | Two items of a list, each a solid round dot before a line of writing and a shorter line under it. A bulleted list. |
| `list-numbers` |  | Two items of a list, numbered 1 and 2, each number before a line of writing and a shorter line under it. A numbered list. |
| `comment` |  | A note: a wide box with rounded corners and a short tail down from its lower left, two lines of writing in it. A comment on the text. |
| `table` |  | A table of three columns and three rows, its heading row solid. A table. |
| `page` |  | A sheet of paper standing upright, nothing folded, with three lines of writing on it, the last one shorter. A page of text in a manuscript. |
| `page-empty` |  | A sheet of paper standing upright, nothing on it but the short upright mark where typing would begin. A page not yet written. |
| `binder` |  | A ring binder standing upright: a cover with three rings across its left edge and a solid label on its front. The binder, where the parts of a manuscript are kept in order. |
| `text-larger` |  | A capital letter A, and beside it a solid head pointing up. The letters made larger. |
| `text-smaller` |  | A small capital letter A, and beside it a solid head pointing down. The letters made smaller. |
| `text-color` |  | A capital letter A over a solid bar: the color the letters are drawn in. |
| `highlight` |  | A marker pen with a slanted tip, lying from the top right down to the bottom left, over the solid stripe it has drawn. Highlight, color laid behind the letters. |
| `spelling` |  | A capital letter A over a wavy line: a word marked as spelled wrong. Spelling. |
| `style` |  | A paragraph mark: a solid round bowl at the left of two tall stems. The style of a paragraph. |
| `footnote` |  | A sheet of paper with lines of writing at its top and, near its foot, a short rule over a solid dot before a line: the note at the foot of a page. A footnote. |
| `text-vertical` |  | Two columns of writing running down, the right one longer, and at the left an arrow with a solid head pointing down. Text written from top to bottom. |
| `text-center` |  | Three short lines of writing in the middle, and a solid head on each side pointing in at them: words centered on their spot. |
| `corkboard` |  | Four index cards in two rows, each held by a solid round pin in its middle. The corkboard, where the parts of a manuscript are laid out as cards. |
| `outliner` |  | An outline: a line of writing after a solid dot, and two lines further in hanging from it by a bent line. The outliner, the parts of a manuscript as a tree. |

### Video editing

| Name | Twin | What it shows |
| --- | --- | --- |
| `app-video` |  | A clapperboard: a box with round lower corners, and over its top a slanted board with two stripes across it. Sue VE, the studio. |
| `tool-razor` |  | A small box like a blade, a short line across it, and a line running down from it: cut a clip in two where it is clicked. |
| `tool-ripple` |  | An opening bracket at the left, and beside it an arrow pointing right: trim an edge and move what follows with it. |
| `tool-roll` |  | A closing and an opening bracket back to back, with an arrow head on each side pointing out: move the cut between two clips. |
| `tool-slip` |  | A wide box with an arrow inside pointing both ways: other frames of the clip, in the same place. |
| `tool-slide` |  | A box between two open boxes, and over it an arrow pointing both ways: move a clip between its neighbours. |
| `magnet` |  | A horseshoe magnet, its two poles marked by a line across each: edges snap to each other. |
| `marker` |  | A tag hanging point down, as on a ruler: a marker. |
| `mark-in` |  | An opening bracket, a short line before it: where the part to use begins, the in mark. |
| `mark-out` |  | A closing bracket, a short line after it: where the part to use ends, the out mark. |
| `insert` |  | An arrow pointing down into the gap between two boxes: put a clip in, moving what follows. |
| `overwrite` |  | An arrow pointing down onto a long box: put a clip over what is there. |
| `effects` |  | A four pointed star: effects. |
