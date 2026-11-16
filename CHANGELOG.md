# Changelog

What changed in each release of Sue, newest first. Each version's section
opens its release notes on GitHub, so write for the people who use Sue.

While you work, add your change under **Unreleased**, in one of these
groups: Added, Changed, Fixed, Removed. Making a release turns Unreleased
into the new version's section (see `CONTRIBUTING.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Sue Paint, Gradient tool: twelve sub tools, among them Ellipse, Soft
  Glow, Repeating Bands, Dusk, Sea, Forest, Spectrum and Contour line
  fill (click an area and the gradient runs from its outline inward).
  Tool Property has a bar of stops you edit by clicking and dragging,
  a row that takes a gradient from a gradient set or keeps yours in My
  Gradients (the same set files as Gradient Map), Linear, Circle or
  Ellipse, the edge process (Do not repeat, Repeat, Reverse, Do not
  draw), Step of angle (Shift keeps 45 degrees), Where to create,
  opacity and blending mode.
- Sue Paint: a gradient can be made as a gradient layer that stays
  editable. With the Gradient tool on it, drag to lay its line again or
  drag one of its ends; Tool Property (Gradient or Object tool) changes
  its stops, shape and edge, each change one undo step. Layer, New
  Layer, Gradient makes such a layer.
- Sue Paint, Close and fill: Target color fills only what the editing
  layer has in that color, so painted areas can be filled again.
- Sue Paint, Fill and Auto Select: Refer multiple's Exclude row is five
  toggles; the fifth leaves out the editing layer itself.
- Sue Paint, Selection area: Tool Property shows Aspect (Free, Square,
  a ratio), Anti-aliasing and Adjust angle after fixed: after the drag
  the rectangle or ellipse turns with the pointer until you click.
- Sue Paint: the Animation Cels palette lists the cel you draw and each
  layer on its light table as a row with its own eye, opacity (with the
  number) and colors, so one reference can be fainter or keep its own
  colors. Drag a layer from the Layer palette onto the palette to trace
  from it. Window, Animation Cels docks the palette beside the Layer
  palette, both in sight.
- Sue Paint: the Timeline palette's menu has Clip Studio Paint's whole
  list and order, in submenus, with Go to Previous and Next Timeline and
  Audio Scrubbing (stepping or dragging through the frames plays each
  frame's sound).
- Sue Paint: on the timeline, a folder holding animation folders is a
  row of its own with them indented under it and a fold arrow; a folder
  with keyframes shows its keys in a band over its cels, and each key
  names its frame and easing on hover. Click a frame and the arrow keys,
  Home and End move through the frames, with the focus ring on the
  frame shown.
- Sue Paint: Insert Frame shows the result on the timeline while you
  set it (Preview on the timeline), and buttons that do not fit a narrow
  Timeline bar are all under More, Specify Cels included.
- Sue Paint: movie, image sequence, animated GIF, PNG and WebP, cels and
  timelapse exports run in the background. The window keeps drawing and
  you can keep painting; a notice shows how far the export is, with a
  Cancel button that leaves no file behind, and when it is done offers
  Show in Folder. The export is of the picture as it was when it began.
- Sue Paint: Export Timelapse plays a preview of the video before you
  choose a file, takes an aspect ratio (the canvas's, 16:9, 9:16, 1:1,
  4:5 or 4:3, the drawing fitted on white) and writes MP4 where Windows
  has an encoder.
- Quitting Sue while an export still runs asks first: Keep Exporting, or
  Stop and Quit, which writes nothing of it.
- Sue Paint: drag the Fill tool across a picture to fill every area the
  drag crosses that is like the one it started in, as one undo step; the
  lines it crosses on the way stay as they are.
- Sue Paint: Shrink selection, the Selection area tool's seventh sub
  tool: draw round a drawing and the selection shrinks to the drawing,
  with the holes it closes in. Its path shows while you draw it.
- Sue Paint: rectangle and ellipse selections keep the sub tool's
  Aspect while you drag, and selection shapes soften their edge by its
  Anti-aliasing.
- Sue Paint, Figure tool: Continuous Curve, Bezier Curve and Polygon in
  Direct draw. Curve now works as in Clip Studio Paint: drag from start
  to end, let go, move the pointer to bend the line, click to set it
  (the old click-by-click curve is Continuous Curve). Bezier Curve: a
  click lays a point, a drag pulls out its handles; click the first
  point to close, double-click the last to end. Shift keeps straight
  lines to every 45 degrees and rectangles, ellipses and polygons
  square or round. With the rulers shown, a straight line follows the
  ruler it starts on and a rectangle, ellipse or polygon lies in the
  perspective ruler's plane. A figure on a vector layer stays a vector
  line. Saturated and speed lines land on a layer named after their
  sub tool.
- Sue Paint, File, Modifier Key Settings is rebuilt: for every tool
  (General) or one tool, say what each key held on the canvas does
  (the tool's own use, change tool temporarily to any tool or sub tool,
  change brush size), what the mouse wheel does with each key (zoom,
  rotate, scroll, brush size) and what the right and middle buttons do.
  Ctrl alone can now be set (to rotate the view, say). Changed rows are
  kept in a file you can share (docs/paint-modifier-keys.md).
- Sue Paint: Ctrl held borrows the Object tool, which moves a raster
  layer and picks vector lines; Alt picks a color with Fill and
  Gradient too; Ctrl and Alt size the Blend tool's brush.
- Sue Paint: hold a tool's key (E for the Eraser) to use that tool only
  while the key is down; let go and the tool before is back.
- Sue Paint: the Rotate tool snaps to 45 degrees with Shift, or to its
  Snap angle; the Zoom tool zooms as you drag across the canvas.
- Sue Paint: Move layer moves only what is selected when there is a
  selection, and every layer of a folder when a folder is current.
- Sue Paint: the Operation tool's Select layer picks every layer painted
  in a box you drag out, and the Operation tool has Light table and Edit
  timeline sub tools (a click steps a frame, a drag scrubs the frames).
- Sue Paint: the Eyedropper's Obtain layer color on an empty spot picks
  the transparent color.
- Sue Paint: the Selection launcher has Clip Studio Paint's buttons in
  its order (with Cut and Paste, Copy and Paste and New Tone, also new in
  the Edit menu) and ends with Selection Launcher Settings: add any menu
  command by searching for it, move buttons, take them off, add
  separators or restore the default. The set is kept in a file you can
  share (`docs/paint-launcher-format.md`). Every launcher button shows its
  name and key in its tooltip and is grayed while it cannot run.
- Sue Paint: transforms have their settings on the bar over the canvas:
  the mode (Scale/Rotate, Scale, Rotate, Free Transform, Skew, Distort,
  Perspective), the center of rotation (any corner, edge, the middle, or
  dragged anywhere), Keep aspect ratio, Keep original image, and the
  scale, angle and position as numbers. Ctrl on a corner distorts it in
  any mode. Mesh Transformation takes the number of points across and
  down. A folder, or several layers picked together, transform as one,
  in one undo step.
- Sue Paint: Puppet warp bends each separate part of a drawing by its own
  pins. Shift and click picks several pins to move together, the ring
  round picked pins turns them and the part under them, Delete takes the
  picked ones away. Its bar has Keep original image, Show mesh, Click to
  add pins, Pick several pins, Correct pin rotation, Expansion and the
  interpolation.
- Sue Paint: the 2D camera's frame has handles on its corners and edges
  and a turning handle on a stalk; Alt zooms about the middle, Shift keeps
  a move to one axis and turns in steps of 15 degrees. Its name sits under
  its corner, guides run to the canvas corners, and its path through its
  keyframes and the frames at the keys before and after show faintly.
- Sue Paint: the Light table tool shows a box round the light table's
  drawing with handles to scale it, a stalk to turn it, and a launcher to
  flip it, put it back or put the tool away.
- Sue Paint: the Light table tool turns around a centre of rotation you
  can move: Alt and drag puts it anywhere, and a turn keeps it in place.
- Sue Paint: Cut and Paste and Copy and Paste leave what you copied before
  on the clipboard. Selection Launcher Settings lists the selection
  commands first, under "On the launcher" and "More commands".
- Sue Paint: the path of the lasso and of the polyline is now
  drawn in two tones, so it reads on white paper and on dark drawing.
- Sue Paint: drag a canvas tab down out of the tab strip (or use Window,
  Canvas, Canvas in Its Own Window, the tab's right-click menu or the
  button at the end of the status strip) to open that canvas in a window
  of its own, which you can move to another screen. A click in it makes
  it the canvas you work on; closing it puts the canvas back in the tabs.
  Tabs can also be dragged along the strip to change their order.
- Sue Paint: a picture file dropped on the canvas becomes a new layer of
  that canvas; dropped on the tab strip it opens as a canvas of its own.

- Sue Paint, vector layers: with the Object tool, draw a loop round
  lines to pick several (Shift adds), then drag the box round them by
  its eight handles to stretch them (Mode: Scale, Shift keeps the
  corner in proportion) or bend them corner by corner (Mode: Free
  transform), with Adjust line thickness when scaling. Tool Property
  recolors the picked lines from the color wheel or a typed hex code,
  and changes their brush size and brush shape; each change is one
  undo step.
- Sue Paint, vector layers: the Figure tool's straight lines,
  rectangles, ellipses, polylines and curves become vector lines drawn
  with the sub tool's brush. Vector magnet (pen, pencil, brush and
  Figure tools) joins the ends of a line you draw to the ends of lines
  near it. A line's open end dragged with Correct Line onto its other
  end closes the shape.
- Sue Paint: the Vector eraser erases on every picked vector layer at
  once when several are picked, and Fill has Fill up to vector path,
  which fills in under vector lines to their middle so no gap shows.
- Sue Paint: Layer, Update Lines to New Brush Engine moves a vector
  layer's lines to the new brush engine in one undoable step. Lines
  with a flick or post correction stay as they are, since the new
  engine cannot draw those yet, and the notice says how many.
- Sue Paint: the Page Manager is part of the canvas area, no longer a
  dialog over the page. It shows the story's spreads, read from the
  bound side, or its pages in webtoon columns; double-click a page to
  open it and the pages stay in a column beside it. Drag a page to move
  it, and right-click one for the Page Manager's menu: open, step, add,
  duplicate, delete, combine, split, settings, Page Reordering, View,
  Webtoon Reading Direction, Batch Process, Edit Text and 3D Preview for
  Binding.
- Sue Paint: 3D Preview for Binding (Page Manager, or File, Export
  Multiple Pages) shows the story as a bound book with the bleed cut
  off; zoom, and turn the pages with the arrows.
- Sue Paint: Layer, Ruler/Frame, Divide Frame Border Equally and Combine
  Frames work on the panels the Object tool picks (Shift picks more). A
  Rectangle frame drawn on a story's page snaps to its default border.
- Sue Paint: balloons are lined in the color you draw with and filled
  with the other color, and a balloon layer keeps the colors it was
  made with.
- Sue Paint's Auto Actions come in sets you can share: pick a set at
  the top of the Auto Action palette, turn each action and each of its
  steps on or off with its box, unfold an action to see its steps, and
  import or export a set as a `.sueactions` file. Nine actions of Sue
  Paint's own come ready, Record turns into Stop while it records,
  commands are recorded wherever you run them (menus, keys, the command
  bar and palette buttons), and each action can be given a key in
  Keyboard Shortcuts.
- Sue Paint's Item Bank keeps pictures as well as sub tools: keep the
  current layer or add a picture, then drag it onto the canvas or
  double-click it.
- The Material palette has a folder tree (a folder picker when the
  palette is narrow), a search over names and tags,
  tags you type, small or large thumbnails or a list, favorites, several
  materials picked at once, and materials dragged onto the canvas.
- Sub View can flip its picture, and its menu turns on Switch to
  Eyedropper Automatically, Show Command Bar and Show Rotate and Flip.
- The Information palette shows the memory the picture's pixels take and
  the length of a drag on the canvas, in pixels, millimeters,
  centimeters or inches, as its menu chooses.

- Sue Paint's command bar holds more than menu commands: tools, sub
  tools, drawing colors, palettes that pop up under their button, Auto
  Actions, lines between groups, and groups of buttons with a list. Right-click
  a button for Icon Settings, Delete, Add Drawing Color, Insert Drawing
  Color into Group, Add Separator, Change Order (Drag or Ctrl + Drag) and
  Command Bar Settings; drag a button to move it. Command Bar Settings is
  new: categories, a search, the bar's buttons as a list to move, group,
  give icons or delete, Restore Default Layout, and Export and Import of
  the bar as a file to share. The timelapse button is on the bar.
- Quick Access (Window, Quick Access, or its icon in the strip): sets of
  buttons with their names, Set 1 as Clip Studio Paint ships it, with New
  Set, Delete Set and Quick Access Settings, the same right-click menu
  and drags as the command bar, and buttons dragged between the two.
  Workspaces keep Quick Access with the command bar; a set taller than
  the palette scrolls.
- Sue Paint: every sub tool can be given a key of its own in Settings,
  Keyboard Shortcuts, Sub Tools; the key picks the sub tool and its tool.
- Sue Paint: a Sub Tool Detail palette (Tool Property's wrench, Window)
  shows every setting of the picked sub tool by category, with a sample
  stroke, a help box, and an eye on each setting that says whether Tool
  Property shows it.
- Sue Paint: Tool Property has a lock that keeps a sub tool's settings
  from changing, a reset button, and Show Categories in its menu.
- Sue Paint: the Tool palette's menu has Tool Settings, Delete Tool, Add
  Tool from Default, Reset Tool Palette, Import and Export Tool Layout and
  Show Color Icon. Drag a tool to reorder the palette; drag a sub tool
  onto it to make a tool of its own, or onto a tool to move it there.
- Sue Paint: the Sub Tool palette has delete, duplicate and reset
  buttons under its list, an Add Sub Tool row at its end, a right-click
  menu (rename, new group, make into a tool, Sub Tool Detail), and named
  dialogs for Duplicate Sub Tool and New Sub Tool Group. Sub tools are
  dragged to reorder them or onto a group's tab.
- Sue Paint: Fill and Auto Select show Apply to connected pixels only,
  and Refer multiple can leave out draft, text, paper and locked layers.
- Sue Paint, Layer palette: every button is a command of the Layer menu
  with its name and key in the tooltip, and a grayed one says why it
  cannot run now. A box in each row picks layers to act on together
  (Ctrl+click and Shift+click pick too), and lock, show, hide, delete and
  palette color then take every picked layer. Change Palette Color tints
  rows; a draft layer has a dashed bar; locked layers show a lock at the
  row's end and layers with a border, tone or layer color show its badge.
  The ruler button sets where rulers show. Search Layer finds layers by
  name and kind, and Split Layer List shows the list in two panes. Drag
  a layer onto New Raster Layer to copy it or onto New Layer Folder to put
  it in a new folder; Ctrl+click a thumbnail to select what it holds; the
  link between a layer and its mask can be undone; an animation folder's
  row names the cel it shows, and a click on a cel moves the playhead
  there. A right click on a layer opens a short list of its own.
- Sue Paint, Layer Property: Anti-aliasing on border effect, the tone's
  density source, posterization, dot shape, Reflect layer opacity and dot
  position, a sub color for the layer color, and for Monochrome the
  choice of black, white or both, with Apply Expression Color of Preview.
- Sue Paint: New Layer, Tone opens Simple Tone Settings (frequency with
  Clip Studio Paint's presets, density, dot shape, angle) and makes a
  layer of tone in the selection.
- Sue Paint's timeline can be edited as Clip Studio Paint's: pick cels
  with a click, a drag or Shift and a click, drag them to move them,
  cut, copy, paste and delete frames, split and merge clips, set the
  playback range by dragging its ends, add timeline and track labels,
  and key a layer's position with Smooth, Linear or Hold. New Timeline
  and Change Settings open their dialog, a canvas can hold several
  timelines, and Insert Frame asks how many frames and where.
- Sue Paint: File, Export Animation, Movie makes an MP4 (H.264 with the
  timeline's sound) through Windows' own encoder, with size, frame rate
  and quality; where Windows has no encoder it makes an AVI and says
  what brings MP4. Image sequences, animated GIF, PNG and WebP and the
  cels ask for their settings first (file names, size, loop count,
  frames), and the cels of two animation folders go in a folder each.
- Sue Paint: Audio Clip Settings on a sound of the timeline (volume,
  cuts and fades, one step that undoes), and File, Export Animation,
  Audio writes the timeline's sound as WAV, FLAC or AIFF.
- Sue Paint: stopping Record Timelapse asks whether to keep what was
  recorded or delete it.
- Sue Paint: on the timeline, drag a picked cel's first frame to make
  the cel before it hold longer or shorter; a double click on a frame
  shows there the layer picked before. A movie exported at another
  frame rate than the timeline's keeps its length and its sound in
  step. Onion Skin Settings no longer hides its OK button.

- Sue Paint, New: each kind of work has its own settings. An animation
  has five presets with blank space (1920 x 1080 at 192 dpi gives a
  2304 x 1296 canvas), its output frame with a preview, title-safe area,
  overflow frame, timeline name, scene, shot, division line and the
  playback time in frames or in seconds and frames. A comic has its
  bleed and border, monochrome with black and white inks and a default
  tone frequency, and Show all comic settings shows its story
  information. Create animated illustration gives a picture a timeline.
  A registered preset brings all of it back.
- Sue Paint, Convert to Lines and Tones asks first: line and tone
  settings over a live preview of the result.
- Sue Paint, View, Grid Settings: the grid's gap, divisions and origin,
  saved with the picture; Snap to Grid follows its finest lines.
- Sue Paint, Convert Layer has an expression color, a blending mode and
  Keep original layer; Create Perspective Ruler can make a new layer to
  draw along it on (on at first); Expand Selected Area can keep square corners;
  Text takes the main, the sub or its own color.
- Sue Paint on a touch screen: Draw with Fingers (on the top bar in
  Simple Mode, the command bar in Studio Mode and the More sheet) makes a
  finger only move the view while the pen draws. Studio Mode's edge key
  bar swipes in from either edge, see-through at first and pinned beside
  the canvas on a second swipe, and its command bar records a timelapse.
  Settings, Sue Paint, Phones and Tablets sets what the two-finger swipe,
  pinch and rotation do.
- Sue Paint, File: every flat picture export (Export (Single Layer) and
  Save Duplicate) shows the export settings first: format, output size,
  resampling, color or grays, background, quality and effort. The next
  export starts where the last one left them.
- Sue Paint, File, Save Duplicate and Export (Single Layer) also write
  GIF, QOI, ICO and PPM, and Save As offers Photoshop Big Document and Photoshop Document with their
  layers.
- Sue Paint, File, Batch Convert Pictures: turn a folder of pictures into
  another type in the background, with a status per file, an output
  folder, a name pattern, and turn, mirror, brightness, contrast and
  saturation for every picture.
- Sue Paint, File, Export Animation, Audio writes the timeline's sound as
  WAV, FLAC or AIFF, with bits, sample rate, channels and normalizing.
- Sue Paint, File, Show in Folder shows the canvas's file in Explorer.
- Sue Paint, 3D: Tool Property for a 3D layer is laid out in sections
  (3D object, Camera, Light source, Outline, Body shape, Pose). Drag
  buttons move, turn and slide the object along the ground and turn,
  move and zoom the camera; set the object's angle, position and scale
  with presets, Put on the ground and resets; turn on Manga
  Perspective. Drawing figures gain male and female body types, a body
  shape chart (thin to fat, less to more muscle) for the whole body and
  each part, a head to body ratio that can follow the height,
  Initialize, and Register body shape as material. Each drag is one
  undo step.
- The Hub's Home on a first run: Start Something New offers each
  studio's kinds of work, one click from a new file, and Good to Know
  shows the command search with its key, opening files by dropping
  them, and Settings, so the page no longer has a blank lower half.
- Notices stay few: three at most (one on a phone, above the tab bar),
  with older ones folded behind them under a "2 more" button that
  unfolds them. They never cover a dialog: they wait under it with
  their time stopped, and the Hub leaves room to scroll past them.
- Sue Writer's binder: press F2 to rename the chosen item (the whole
  name is chosen, so typing replaces it) and Delete to move it to the
  Trash; the right-click menu shows each command's key. Items left out
  of the export say so on their row, and a row's tooltip shows its
  synopsis.
- Sue Writer's corkboard, outliner and inspector: Enter opens the card
  or row that has the keyboard; an empty folder says what to do and
  offers New Document; the synopsis and notes fields grow as you write.
- Sue Writer's binder: the up and down arrows walk its rows and show
  each one, and F2 renames a document also after a click has put you
  in its text. The inspector names its Label and Status lists and shows
  the label's color.
- Settings, Appearance, Icons: choose an icon pack, see every icon it
  draws on a sheet, install one from a folder or a .zip, remove one,
  replace single icons with your own SVG files, and export Sue's own
  icons as a template to start a pack from. Changes show at once, and the
  chosen pack is back when Sue starts.
- Settings, Keyboard Shortcuts lists every command of Sue Paint and Sue
  Writer grouped as their menus are, plus Sue Paint's tools, and any of
  them can get a key, not only those that ship with one. Search finds a
  command by its name or its key. A key that two commands hold is marked
  on both rows, with a button to take it from the other one.
- Sue Paint: Layout Styles. Window, Layout Style (and Settings) chooses
  which design Sue Paint's workspace follows. "Clip Studio", the default,
  lays the palettes out as Clip Studio Paint does: Sub Tool, Tool
  Property and Brush Size as three palettes over the six color palettes,
  the Timeline folded under the canvas, Material in the strip. "Sue
  Paint" is listed for later. Each style keeps its own layout and
  workspaces.
- Sue Paint: each palette's menu, at the top left of its header and on
  floating palettes too, holds the palette's own commands and ends with
  Hide (name) Palette. Floating palettes roll up on a double-click; strip
  icons can open their palettes as tabs or as pop-ups; the row under the
  canvas folds into icons or closes with its arrows.
- Sue Paint: workspaces keep the command bar's buttons too; Window,
  Workspace checks the one in use, offers Reload, and ships the
  Illustration and Animation workspaces. The layout and workspace files
  are described for people who share them
  (`docs/paint-workspace-files.md`).

- Sue Paint: every tool that shares its letter with others (Pen and
  Pencil on P, Brush, Airbrush and Decoration on B, Fill and Gradient on
  G, Figure, Frame Border and Ruler on U, Text and Balloon on T) has a
  command of its own, such as Pencil Tool, that the command palette
  finds and Settings can give a key of its own.
- Sue Paint: Manage Fonts shows Sue's font list, the one every font menu
  of Sue offers, with a search, each family drawn in its own font, and its
  faces and a sample line; Pose Scanner (Scan Pose from Image) runs from
  the command palette and makes a 3D drawing figure first when none is
  picked. Both join the File menu with the menu work of this release.
- Sue Paint: Edit, Copy Vectors as SVG puts the lines of the vector
  layer (only the ones the Operation tool picked, when it picked some) on
  the clipboard as SVG code, for any program that draws vectors.
- Sue Paint: Ctrl+B (Switch Using Layer Color) shows the current
  layer in its layer color (blue unless Layer Property says otherwise) and
  back in its own colors on the next press, for tinting a sketch.
- Sue Paint: Select, Selection from Layer, Select Overlapping Area keeps
  only the part of the selection where the current layer has something
  drawn.
- Sue Paint's Select menu has Select Overlapping Vectors and Select
  Vectors Within Area: on a vector layer they pick the lines the selected
  area touches, or holds whole, for the Object tool, which then moves or
  deletes them.
- Sue Paint's View menu has Safety Margin (a line inside a page's trim
  that the art you must keep stays within), Show Vector Paths (the paths
  of vector lines on the selected layers or on every vector layer, with
  their end points or every control point), Show Playback FPS (the frame
  rate the canvas really shows while the timeline plays) and, under
  Snap, Toggle Special Ruler Snapping (Ctrl+4: the picture's special,
  perspective and symmetrical rulers stop snapping, as one step undo
  takes back).
- Sue Paint: Webtoon Reading Direction, Right to Left or Left to Right,
  from the command palette (a key can be given in Settings). The choice
  is saved with the comic and undoes like any change; until it is made
  it follows the binding. The Page Manager lays a webtoon's columns out
  by it once its webtoon view arrives.
- Sue Paint can draw with its new brush engine: write
  `brush_engine = "new"` under `[sue_paint]` in your settings file and
  the Pen and Pencil sub tools with a round tip draw through it. The
  stroke is laid on its own threads while the pen moves, shown as it
  grows, ends as thin as the pen ended it, and goes into the layer as
  one undo step; every packet of the pen keeps its own pressure. Until
  the rest of the tools move over, strokes it cannot draw yet (with a
  selection, on a locked, masked or vector layer, along rulers, with
  Shift) use the brush engine as before, and it draws no paper grain.
- Sue Paint has the Glow Dodge and Add (Glow) blend modes: soft paint
  in them glows brighter than in Color Dodge and Add, as a rim light
  wants. The blend mode list follows Clip Studio Paint's order, and
  Luminosity reads Brightness.
- Sue Paint's Liquify has seven modes: Push, Expand, Pinch, Twirl
  clockwise and counterclockwise, Push left and Push right, picked from
  a Mode list in Tool Property.
- Auto Select has a third sub tool, Refer Reference Layers.
- Sue Writer opens CSV and TSV files: each becomes a document holding
  the table, after one question, whether the first row is a heading row.
  Table, Convert, Table from File puts such a table at the caret, and
  Export Table As writes the table the caret is in as CSV or TSV. Cells
  keep their text as the file wrote it (007 stays 007).
- Sue Writer's Export dialog tidies plain text: line endings (LF or CRLF,
  the system's by default), spaces at line ends dropped, and tabs as a
  number of spaces. Markdown takes the line endings only, since two
  spaces end a line there and tabs carry its lists.
- Sue Writer's menus follow the skeleton every studio shares: File has
  Open, Save, Save As (Ctrl+Shift+S), Settings and Quit Sue; Edit has
  Cut, Copy, Paste and Select All; Help has Check for Updates and About
  Sue Writer.
- `SUE_HOME`: set it to a folder and Sue keeps its settings, data and
  caches there instead of the system's usual places, for a portable copy
  or a test run that leaves your own settings alone.
- `sue --snapshot <dir>` takes `--scale <n>` (pixels per point, as on a
  display set to 150 % or 200 %), `--preset <id>` (a theme preset) and
  `--profile <dir>`, and `sue --snapshot-compare <before> <after>` counts,
  scene by scene, the pixels that differ between two snapshot folders, so a
  change to how Sue looks can be checked against the scenes it was meant to
  touch.
- `sue --gallery` opens Sue with a Gallery under Tools in the Hub: every
  color, size, radius, shadow and control of Sue's design system on one
  page, live, in the theme you use and in each density, for whoever
  styles or reviews Sue's interface. A plain run does not list it.
- Sue Paint saves pictures as ICO (Windows icons, at most 256 by 256
  pixels) and PPM too: both are in the "save as type" list of every
  picture export. A PPM holds no transparency, so the picture is laid
  over white, as for a JPEG.
- `sue-cli clip dump <file>` prints what a Clip Studio file holds (its
  chunks, what each stored bitmap is made of, and every table with its
  `CREATE` text, row count and rows), and `sue-cli clip diff <before>
  <after>` prints what differs between two such files. For people
  working on the converter; neither changes a file.
- Every Sue Paint command can be given a key, not only the ones that
  come with one: each filter, layer, selection, view, window and
  animation command, and the light table and cel commands of their
  palettes. A key for one without a default goes in Settings, Keymaps,
  Sue Paint, as the command's name and its key
  (`sue_paint_keys.custom`, for example `layer.merge_visible=Ctrl+M`).
  Keyboard Shortcuts (Help) lists every command that has a key, in the
  order of the menus.
- Every Sue Writer command can be given a key, not only the ones that
  come with one: each paragraph style, each export format, line spacing,
  the table edits, the review and project commands. Keyboard Shortcuts
  (Help) lists them all, and a key for one without a default goes in
  Settings, Keymaps, Sue Writer (`sue_writer_keys.custom`).
- The Sue menu and the menus along the top answer the keyboard: Up and
  Down walk the items, Home and End jump to the first and the last,
  Right and Left open and leave a nested menu (on the menu bar they go
  to the menu beside), Enter or Space runs the item, Escape closes, and
  typing the start of an item's name goes to it. A menu taller than the
  window scrolls (with the wheel, by resting the pointer on the arrow at
  its end, or by itself to follow the keys) instead of being cut off.
- Every color wheel can show a triangle instead of the square: the
  small button at the wheel's bottom right switches between a square of
  saturation and value (HSV) and a triangle between the hue, white and
  black (HLS), and the numbers under the wheel follow it.
- Sue Paint: Edit, Tonal Correction, Grayscale turns the current layer,
  and every other layer picked with it, into grays of the same
  brightness in one step, transparency kept; with a selection only the
  selected area.
- Sue Paint's command palette offers the help pages on the web that the
  start page's Help button linked to: Basic Tips, Using Sue Paint
  (Online), Frequently Asked Questions, Contact, and Feedback and
  Requests. Each opens in your browser, and each can be given a key in
  Settings.
- Sue Paint renames an animation folder's cels 1, 2, 3 in the order
  the timeline shows them (Rename in Timeline Order) or in the order of
  the layers (Rename in Layer Order), as one undo step; both are in the
  command palette until the Animation menu lists them.
- Sue Paint: a layer folder inside an animation folder is a cel of its
  own (a folder cel) and shows and hides as a whole on the timeline,
  onion skin included. A new cel made after a folder cel copies its
  folders and layers, empty, and a cel made between two others is named
  after the one before it (1a, 1b) and sits right above it.
- Sue Paint opens Photoshop documents with 16 bits a channel (rounded
  to 8 bits) and documents whose channels are zip compressed; before,
  both were refused.
- Sue Paint: Color Settings opens on a double click of the main or sub
  color or of a swatch (and from the command palette): a wheel, the new
  color beside the current one, the hex code, and HSV, RGB, CMYK and Lab
  numbers. The main and sub color circles under the Color Wheel and in
  the tool strip open it too. The Color Slider palette switches between
  HSV, RGB and CMYK, and each slider's track shows the colors it gives.
- Sue Paint's color sets: a new set asks for its name and starts as
  empty cells to fill; colors take names (Change Color Name); deleting a
  color leaves its cell empty; Edit Color Sets renames, duplicates,
  deletes and reorders sets (drag a set by its grip, or Alt and Up or
  Down) and adds back any of Sue's standard sets. Clip Studio Paint
  color sets (`.cls`) import, and an exported GIMP palette keeps names
  and empty cells, so a shared set arrives as it was arranged. Deleting
  a set that holds colors asks first.
- Sue Paint: Edit, Pick Screen Color and Hide Windows and Pick Screen
  Color (Windows). The next click anywhere on any screen, also outside
  Sue, takes the color there as the drawing color; a label by the
  pointer shows it as R, G and B, and a right click or Esc cancels. The
  second one hides Sue's windows while it waits.
- Sue Paint: Edit, Colorize. Paint a stroke of each color on a layer and
  Colorize fills the areas the line art closes from them, on a new layer
  under the line art, here on your computer.
- Sue Paint: Gradient Map has stops: as many as 16, each with its place,
  color (its own, or the main or sub color), opacity, and a mixing rate
  curve; click the bar to add one and drag it to move it. Gradients come
  in sets you can add to, duplicate and delete from, kept as files you
  can share (docs/gradient-sets.md).
- Sue Paint: Level Correction works on RGB, or on red, green or blue
  alone, as a filter and as a correction layer.
- Sue Paint: Filter, Distort, Curved Surface and Filter, Effect, Artistic
  (lines and flat colors with Clip Studio Paint's nine settings).
- Sue Paint: Radial Blur and Zoom Blur turn round a center you put with a
  click on a small picture of the layer; Perlin Noise has Scale,
  Amplitude, Attenuation, Repetition and Offset X and Y; Smart Smoothing
  has Weak, Medium and Strong and Remove Noise.
- Sue Paint, File, New, Animation: the overflow frame takes an Offset X
  and an Offset Y, moving the output frame from its reference point.
- Sue Paint: an animation's output frame is drawn on the canvas, named
  under its lower left corner, with its title-safe area when that is on.
- Sue Paint, Timeline palette menu: Show Palette Color, Show Palette
  Color on Clips. On (as it starts), each track's clips carry its
  layer's palette color: a band along a layer's clip, the line under an
  animation folder's cels.
- Sue Paint, Timeline: a right click on the frames offers Thumbnail
  Size, as Clip Studio Paint's does.

### Changed

- Sue Paint wears the theme you pick in Settings, Appearance, as the Hub
  and Sue Writer do: in Paper its menus, command bar, palettes, Tool
  Property, dialogs, canvas surround, timeline and status are light, in
  Void and Graphite dark. It used to keep its own Gray in every theme.
  Gray is still there for Sue Paint alone (Settings, Sue Paint, Theme).
  The opacity and blend line of a chosen layer row, the size under the
  chosen Brush Size and the key of the chosen sub tool are easier to
  read, and in light themes a list that runs on fades out cleanly instead of
  showing a gray line along its edge.
- Sue Paint: every brush now paints with the new brush engine: the
  airbrushes, the watercolor and oil brushes that mix with the picture,
  the textured pencils, the sprays and every Decoration brush, not only
  the pens. What you see while drawing is what stays, symmetry's copies,
  the selection and locked transparent pixels included, and Undo takes a
  stroke back exactly. The figure tools and the focus and speed lines
  paint with it too.
- Sue Paint: a new line on a vector layer is kept as a line of the brush
  engine, so it paints again to the very pixels it was drawn with when
  the layer is redrawn.
- Sue Paint: ribbon brushes (Film strip, Rope, Checkered band, Stitched
  ribbon, Lace ribbon) bend one band along the whole stroke again.
- Sue Paint: Flick into a point carries a quick stroke on along the way
  the line was going and narrows it smoothly to a point, for a pen and a
  mouse alike; a straight line or a line along a ruler does not flick.
- Sue Paint: post correction smooths a shaky line more at its higher
  levels, and still keeps its corners and its ends where they were.
- Sue Paint: a pen's tilt turns and mirrors with the view.
- Sue Paint: the sample strokes in the Sub Tool palette and under Tool
  Property are painted by the brush engine with each sub tool's whole
  brush, so a spray, a Decoration brush or a textured pencil shows there
  as it paints on the canvas.
- Sue Paint paints with its new brush engine. Every brush whose tip is
  round (the pens, the pencils, the hard eraser and more) draws through
  it, on a layer, on a layer's mask, inside a selection, with Lock
  Transparent Pixels, with the symmetrical ruler, along rulers and the
  grid, as Shift's straight line, with flicks, post correction and Smart
  Shape. A stroke ends as thin as the pen ended it, never with a blob,
  and what shows while drawing is what stays. The `brush_engine` setting
  is gone. Brushes that dab (build-up, mixing, picture tips) and the
  lines of vector layers still draw as before.
- Layer masks hold gray pixels now: a new mask is gray, and a mask made
  by an older Sue turns gray when its picture opens, showing the same and
  leaving the picture unchanged (nothing to undo, nothing to save).
- The Blend tool works on a layer's mask while Edit Mask is on, so a
  mask's edge can be softened with Blur.

- Sue Paint: in an animation, Fill, Close and fill, Auto Select and
  Shrink selection look only at the cels of the frame shown, and a
  folder set as the reference layer (an animation folder, say) is what
  they refer to as the reference.
- Sue Paint: while a layer's mask is edited, Fill, Close and fill,
  lasso fill, Gradient and Edit, Fill write to the mask: any color shows
  the layer, the transparent color hides it.
- Sue Paint: Fill, Gradient and Edit, Fill with the transparent color
  clear what they cover instead of painting the drawing color.
- Sue Paint: Window, Canvas, New Window adds the second view as a tab and
  leaves the canvases tabbed or tiled as they were, as Clip Studio Paint
  does; it no longer tiles them. The canvas tabs, the status strip and
  the scroll bars use Sue's own tabs, fields and buttons, with tooltips
  that name each button and its keys; a scroll bar also moves with the
  arrow keys once it has the keyboard. A picture open in two tabs no
  longer shows its second tab as changed.
- Sue Paint's dialogs are as tall as what they hold, on the shared
  controls with focus rings and tooltips; Drop Shadow, Focus Lines and
  Speed Lines no longer hide their OK and Cancel.
- Sue Paint: a floating palette has a slim header in place of a window
  title bar: its name and menu, and buttons to roll it up to the
  header, dock it again and hide it. Dragging the header moves it, and
  over the dock docks it. Tabs can be dragged along a header to reorder
  them, and the palette menus, palette tabs and the buttons that hide or
  fold a side use Sue's shared menus, icons and focus rings.

- Sue Paint, Edit, Change Image Resolution resamples with the same
  bicubic resampler as every export, so a picture made larger keeps
  crisper edges and one made smaller stays clean at its transparent edges.
- Sue Writer's toolbar, status bar and find bar use Sue's own icons
  and controls: Bold, Italic, Underline, the alignments, the lists and
  the indents are icon buttons that light up while they are on, the
  paragraph style and the size are dropdowns, the font box opens a
  search over your fonts, each shown in its own face, and Highlight
  opens a small list of colors. The text color is a letter A over a
  bar of the color, and opens a color wheel. Every button names itself
  and its keys in its tooltip, keys you change in Settings included,
  and Tab reaches each one with a focus ring; the find bar's boxes name
  themselves too. After a font or a size is picked, the keys go back to
  the text. The edges of the binder and the inspector can be moved
  with the arrow keys too, and a double click puts them back.
  Composition mode writes on your theme's page.
- Sue Writer's binder, outliner, corkboard, inspector and comments are
  drawn with Sue's shared controls and icons, so they follow the theme
  (the corkboard's cards were paper colored in every theme) and an icon
  pack changes their icons too. Every button names its key in its
  tooltip, and the keyboard shows where it is.
- Sue Writer's dialogs (New Project, Export, Project Settings, Link,
  footnotes and endnotes, Insert Table, Header and Footer, Table of
  Contents, comments, Writing Statistics, Keyboard Shortcuts, About)
  are drawn like every dialog of Sue: the buttons always stand in the
  same order with the one that accepts first, Enter accepts, Escape
  cancels, and every field, list and check box is Sue's own, with a
  focus ring when it is reached with Tab. Project Settings sorts its
  rows under Page, Fonts and Targets, and picks the orientation from a
  pair of segments.
- Sue Writer's start page shows the presets for long work (Novel,
  Screenplay and the others) as pages like the templates, in place of
  a list folded into a button; every page names itself and says what
  it is for when the pointer rests on it, and can be reached with Tab
  and opened with Enter.
- Sue Writer on a phone or a tablet: the buttons of the top bar draw
  Sue's icons, so an icon pack changes them too, and pressing and
  holding one shows its name and its keys.
- The rich text editor (Sue Writer and every studio that writes) marks
  its pages in the theme's colors: the selection and its handles, the
  wave under a misspelled word, tracked insertions and deletions, the
  words a comment or a note is on, and the shadow under a page. Each is
  made to read on the paper shown, light or dark. In Sue's monochrome
  themes a tracked insertion is underlined in the text color and a
  deletion struck through in the softer one. The spelling menu is Sue's
  own menu, and the Cut, Copy, Paste and Select All bar over a finger's
  selection has touch-sized buttons whose tips name their keys.
- Settings is regrouped: General (with updates), Language, Appearance,
  Display and Performance, Sound, Files, a page for each studio, Keyboard
  Shortcuts, Extensions and Advanced. Rows most people never need wait
  under closed titles, Sue Paint's custom colors show only while its theme
  is Custom, and settings say what they do in plain words ("Performance",
  "Button and field corners", "Largest file to open") instead of the
  settings file's names. The MCP page that only said "not yet available"
  is gone.
- Sue Paint's File, Preferences opens Settings on the Sue Paint page
  instead of a copy of Clip Studio Paint's dialog with 125 options that did
  nothing. Typing a Clip Studio option's name in the Settings filter finds
  Sue's setting. Studio Mode's keys T1 to T7 are picked from a list of
  commands instead of typed as command names.
- Sue Paint's menus follow Clip Studio Paint's order and read in Title
  Case. File puts the exports before Import, then the settings, Manage
  Fonts and Pen Pressure Settings; Import has Pose Scanner. Animation
  starts with New Animation Layer, lists Play/Stop and each frame move
  once, and Edit Track has Rename in Timeline Order, Rename in Layer
  Order and Select Previous and Next Cel. Window names each palette as
  the palette's own title does (Sub Tool, Tool Property, Layer Property,
  Auto Action, Information), in groups, and Window, Canvas lists the
  open canvases with a check on the current one. Help adds the online
  guide, Tips, the FAQ, Contact and Feedback. Layer Settings has Switch
  Using Layer Color, Story the Webtoon Reading Direction, and View's
  first submenu is Rotate/Flip.
- Sue Paint: Edit, Paste to Shown Position answers to Ctrl+Shift+V, as
  in Clip Studio Paint (it had no key).
- The Hub is a workspace of places: a rail (a tab bar on a phone) with
  Home and Files. Home has New and Open, a rounded card per studio with a
  round pin, Jump Back In with pictures of your newest work, and Your
  Work buttons (Recent, Projects, Unsaved Work, Favorites, Folder) that
  open Files already sorted. Files shows every studio's work as cards with
  Recover and Discard on kept work, and its menu rebuilds thumbnails and
  forgets files that are gone. Over the Hub the top bar has a search
  field for commands and buttons for Transfers and Settings; the arrow
  keys walk the Hub.
- Sue Paint opens on its canvas. With no canvas open it keeps its menus
  and palettes and shows New and Open where the canvas goes; on a phone
  or tablet New opens the New canvas page. Its materials are the Hub's
  **Materials** place, whose menu finds them again, draws their pictures
  again and shows or moves their folder. File, Back to the Hub (and the
  back arrow on a phone) goes to the Hub, where your work is in Files.
  The Help menu now also links to the basic tips, this guide online,
  frequently asked questions, contact and feedback.
- The start pages draw Sue's icons and have rounded corners.
- The Sue menu's Layout is now called **Interface**. Inside a studio the
  Sue menu leaves out File and Project (the studio's own File menu opens
  and saves), and Language and "Open ... at Launch" left it for Settings.
  Command Palette shows its key in the key column, and no key where the
  studio you are in keeps it.
- What Sue tells you (a file saved, something that failed) shows as
  notices that stack at the bottom of the page, with a button that closes
  each; a saved file's has **Show in Folder**.
- The question about unsaved work when quitting, and Settings on a
  computer or a tablet, use Sue's own dialog: the same title, close
  button, buttons, `Enter` and `Escape` as every other dialog.
- Sue Paint saves a picture in a Sue project (`.sue`) by default;
  Clip Studio Paint's `.clip` is opened and exported. A canvas opened
  from a `.clip` asks on its first Save whether to export back to that
  file or keep working in a Sue project beside it, and File, Export Back
  to the Same .clip writes it there in one step. A canvas opened from a
  PNG, JPEG or other picture saves back in its format, and asks first
  when its layers would be merged. The window title names the file Save
  writes. A `.clip` opens with masks, paper, gray layers, folder blend
  modes and pixels off the canvas kept, and a layer you did not change
  goes back into the file exactly as it was, with everything Sue does
  not show yet; the status bar says what did not convert exactly.
- In Sue Paint a new layer made (or pasted) with a folder picked goes in
  that folder, on top of what it holds, and one made with a layer in a
  folder picked stays in that folder; Duplicate Layer on a folder copies
  everything in it (an animation folder with its track), and a copied
  layer keeps a copy of its mask.
- Merge Down and Merge Selected Layers bake masks and layer effects into
  the merged pixels, and a folder picked among the layers merges with
  all it holds.
- Liquify is a tool of its own with one sub tool, after Blend; J picks
  Blend, and J again Liquify. The Fill tool's sub tools follow Clip
  Studio Paint Ver. 4's order, with Lasso fill among them instead of
  under Figure.
- Sue Paint's sub tools come in Clip Studio Paint Ver. 4's groups and
  order: the Pen tool's pens before its markers, the Brush tool's
  watercolor, thick paint and India ink brushes, the Sketch tool's
  pencils before its pastels.
- Two built-in pens have names of Sue's own: Real G-Pen is now Grainy
  G-pen and Milli pen is now Fineliner.
- Sue Writer's Compile is called Export (File, Export...), and "Include
  in Compile" reads "Include in Export". Project Settings moved to the
  Project menu, and Save as Template and the templates folder to File,
  Templates.
- A Sue project (`.sue`) with pictures is far smaller and the same on
  disk however it is saved: pixels are stored as 256 pixel tiles, an
  empty or single color tile costs a byte, a tile found twice (a duplicated
  layer, a held animation cel) is stored once, and the rest is
  compressed without loss. An empty A4 layer at 350 dpi took 47 MB and
  now takes a few bytes. Sue 0.0.75 and older cannot open such a file
  and say that it was saved by a newer Sue; projects without pictures
  are written as before.
- A picture you open with Open File (`Ctrl+O`), drop on the window or
  hand to Sue from the system now opens in Sue Paint as a new canvas,
  not in the Image Viewer. That includes GIF, TGA, QOI, ICO and the
  PPM family (PGM, PBM, PNM, PAM), which Sue Paint did not open before.
  Opening changes nothing in the file, and a canvas you have not
  touched is not asked about when you quit.
- A brush file (`.sut`, `.abr`, `.gbr`, `.gih`) dropped on Sue becomes
  sub tools in Sue Paint, and a sound file dropped while a canvas is in
  front goes on its timeline.
- Sue Paint's 3D controls (camera, light, shadow, shapes and body
  shape) are translated into German, French, Spanish, Italian,
  Brazilian Portuguese, Japanese, Simplified Chinese and Arabic.
- A pen on a tablet whose driver has Windows Ink turned off (WinTab):
  every point of a stroke now takes the pressure the pen had at that
  point. All the points drawn within one frame used to share the frame's
  last pressure, which stepped a line's width on fast strokes. The pen's
  tilt reaches Sue Paint's tilt settings on this route as well.
- For contributors: the tests that drive real frames through Sue moved
  out of `bins/sue/src/runtime.rs` into `bins/sue/src/runtime/tests/`,
  one file for each area, with shared helpers that find what to click
  by where it was drawn.
- Your settings file moves to format 7 the first time this version
  starts (the old one is kept as `default.suerice.v6.bak`). Everything
  you changed in Settings stays as you set it.
- A rounder, easier to read look: buttons and fields have corners of 6
  (they had 4) and windows and menus of 14 (they had 12), secondary text
  is lighter in Void, Graphite and Gray and darker in Paper so it still
  reads on a row under the pointer, and hairlines in Void and Graphite
  are a gray of their own instead of the hover color. A button or field
  that holds keyboard focus shows it with a thicker outline. Two new
  settings, Card radius (Layout) and Title size (Typography), join the
  ones you can tune. A radius or color you set yourself stays as it is.
- Duplicating a layer or a page no longer copies its pixels: the copy
  and the original share them until one of the two is painted on, so
  duplicating is instant and costs no memory, however large the picture.
- Sue Paint's entries in the command palette show the key each one
  has, Keyboard Shortcuts too, and New is named as in the File menu
  ("Sue Paint: New...").
- Window title bars, the color wheel's popup, the command palette and
  the font list follow the theme's spacing, corners and type sizes. The
  title bar's buttons light up smoothly under the pointer and tell a
  screen reader their names.
- Sue Paint: New Camera Folder makes an empty camera folder beside the
  picked layer, so a cel you are drawing stays in its animation folder,
  and a new animation folder is named "Folder 1", "Folder 2" and so on,
  as in Clip Studio Paint.
- Saving a project no longer stops your work: it runs in the background
  with its progress in Data Transfers, and writes only the parts of the
  picture that changed since the last save. Every saved project carries
  a small preview, which the Hub's cards show without opening the file.
- Unsaved work is kept the same way: after the first time, Sue adds only
  what changed to a journal beside the kept copy, so keeping it after a
  stroke takes a moment instead of writing the whole picture again. A
  crash in the middle loses at most the last change kept.
- Sue Paint: every filter dialog uses Sue's own sliders, switches and
  buttons, each with a focus ring and a tooltip, and is as tall as its
  settings. Filter, Blur, Smooth is called Smoothing, as in Clip Studio
  Paint, and Mesh Transformation names its step so in History.

- Settings and the update window are drawn with Sue's own controls:
  switches, dropdowns, sliders beside a field that types the number,
  search, key chips and buttons that all show where the keyboard is and
  name themselves (and their keys) when the pointer rests on them. On a
  phone or a tablet everything is finger sized, and a slider that has
  little room beside its name goes under it. Release Notes open in Sue's
  dialog and close with Escape.
- Settings, Appearance, Icons shows each icon set as a card with its
  own icons as a preview; choose one with a click or the arrow keys.
- Sue Paint, Fill tool: Refer Other Layers starts with draft layers left
  out, as Clip Studio Paint's does, so color lines on a draft layer stop
  no fill.

### Fixed

- Sue Paint: a fast mouse stroke no longer ends in a small kink at its
  tip, and a flick no longer bends off the way the stroke was going.
- Sue Paint: masks of older files become gray masks as the file opens,
  once, instead of being looked at every frame; the picture stays as
  unchanged as it was saved.
- Sue Paint: Edit, Delete while a layer's mask is edited hides on the
  mask and leaves the layer's pixels as they were.
- A dialog keeps its place while what it holds grows or shrinks: a
  row added to a list no longer moves the buttons under the pointer.
  Dialogs that drew their own OK and Cancel (Gradient Map, Color
  Settings, Colorize) are as tall as what they hold again, and Edit
  Color Sets no longer covers the window.
- Fills, gradients and Edit, Fill reach a layer mask while it is
  edited.
- On a phone, Sue Writer's find bar shows Replace beside the field.
- A layer with keyframes on no longer takes brush or Blend strokes it
  would lose: Sue Paint says why and paints nothing.
- Exporting a PSD reads a layer mask wherever it sits, also a gray one.

- Sue Paint: Auto Select with a selection already there is no longer
  cut down to that selection.
- Sue Paint, Ruler tool: with Shift held, the line shown while dragging
  a ruler now keeps to 45 degrees, as the ruler it lays does.
- Sue Paint's Navigator frames the part of the picture the canvas shows
  in the theme's own colors, and its buttons run the View commands, so
  they say their keys and follow keys you change.

- Sue Paint: a brush file let go on Sue Paint now lands at the end of
  the sub tool group shown, and is picked, instead of the tool's last
  group.
- Sue Paint: a layer folder with no blending mode set shows Through, as
  it draws, and its blend list offers Through; Layer Comps no longer turn
  such folders to Normal when shown again (comps kept by older Sue are
  read the same way).
- Sue Paint: Merge with Layer Below, Merge Selected Layers, Rasterize and
  Create Layer Mask work with a folder current instead of doing nothing,
  and Merge Selected Layers keeps a layer clipped to a layer that was not
  merged clipped to it.
- Sue Paint: deleting picked layer folders asks first, with "Do not show
  again".
- Sue Paint: picked layers drag together in the Layer palette, and
  Create Folder and Insert Layer puts every picked layer into one folder.
  The Layer palette has a Switch Using Layer Color button, and the marks
  of a row (lock, draft, effects, keyframes, layer colors) say what they
  are when the pointer rests on them. Layer, Layer Property brings the
  Layer Property palette forward.
- Sue Paint, Layer palette: a search that finds no layer says so; a
  right click on one of several picked layers keeps them all picked for
  its list; rows and mask thumbnails show a focus ring under Tab.
- Sue Paint: each palette shows its name once, on its tab in the
  palette's header, instead of a tab and a second title under it. In a
  narrow column the tabs no longer cut their names in the middle ("Color
  Wh"): when the names do not all fit, tabs show their icons and name
  themselves on hover, with the key that shows the palette.

- Sue Writer: in a new comment, a footnote or an endnote, Tab now
  leaves the text for the dialog's buttons (the comment field took the
  keys back at once). In Project Settings, the heading font's search
  keeps what you type (it emptied itself), and both font boxes look and
  work like Sue's other boxes; Escape closes their list first.
- Sue Writer: Undo after Insert Table takes the whole table away at
  once (it took three steps, and the first left the table behind).
- With line spacing above single, the highlight on words a comment or
  a note is on reached far below them, toward the next line. It now
  sits on the letters.
- Inserting a table, a table of contents or a picture into rich text,
  and Text to Table, is one step to undo. Undo used to take the table's
  place in the text away first and leave an empty table behind for a
  second undo.
- A settings file from before 0.0.8 that wore the Graphite, Paper or
  Gray theme kept that theme's old, harder to read secondary text after
  the update. It now takes the theme as it ships; colors you picked
  yourself stay. Sue Paint's Custom colors also start from today's Gray,
  with its readable secondary text.
- Recording a shortcut in Settings over Sue Paint no longer also runs the
  key in Sue Paint (recording R picked the Rotate tool).
- Sue Paint: every palette is a palette of its own. The Window menu
  opens Tool Property, Brush Size and each color palette as itself, no
  palette shows in two places, and picking a sub tool no longer switches
  the Sub Tool palette to Tool Property.
- Sue Paint: closing a palette hides it for good, also the X of a
  floating palette (which docked it again) and the palettes of the strip
  (which only folded back); Hide All Palettes hides every palette and
  shows them back exactly as they were, reading Show All Palettes
  meanwhile.
- Sue Paint: in a language other than English, the palette layout,
  workspaces and the menu bar's state are restored again, and the
  command bar keeps every button added (two or more ran into one line).
  Files written that way are read once and kept in English.

- Sue Paint's History names steps as the menus do: New Animation Folder
  and Convert to Drawing Color, and a duplicated layer or page is called
  "<name> Copy" with a capital, as in Clip Studio Paint. The help on the
  color palettes says that X switches between the main and sub color.
- Sue Paint's keys match its menus and Clip Studio Paint: Shift+Delete
  deletes outside the selection (it used to fill, while the menu showed
  it on Delete Outside Selection), Ctrl+Shift+Alt+K opens Shortcut
  Settings and Ctrl+Shift+Alt+Y Modifier Key Settings; the read-only key
  list keeps F1 and Alt+I.
- Sue Paint's History names the steps of Edit, Rotate/Flip Canvas as the
  menu does: Flip Horizontal, Flip Vertical and Rotate 180 Degrees, where
  it used to say "Flip Canvas" for both flips and "Rotate Canvas".
- Sue Paint: Flip Pose, Reset Pose, the preset poses and the Pose and
  Hand Scanners, given a key or run by an Auto Action, are off where there
  is no 3D drawing figure (or hand) to pose, instead of only saying so,
  and a pose number the figure does not have no longer resets its pose.
- In Sue Paint, X switches between the main and the sub color, as in
  Clip Studio Paint: the other color becomes the one you draw with and
  both keep their values. It used to swap the two colors. The arrow
  button beside the colors still swaps them.
- Sue Paint's Drawing Color to Transparent and Radial gradients start
  at the color you draw with. They always used the main color, even
  while the sub color was picked.
- The second line of a file's card no longer runs under its heart and
  details button.
- A Windows Ink pen whose packets reached Sue several at a time (while
  a frame was drawn) gave them all the newest packet's pressure and the
  same time; each now keeps its own, for the strokes of the new brush
  engine.
- Change Canvas Size, Crop and Change Image Resolution in Sue Paint left
  layer masks at their old size and place.
- A tone effect in Sue Paint drew a faint dot in every white cell, and
  dark grays never reached solid black.
- Sue Paint's Fill Area scaling spreads as many pixels under the lines
  as it says; without Close gap it reached one pixel further.
- Choosing which layers Fill and Auto Select refer to no longer turns
  Close and fill or Paint unpainted area into a plain fill.
- Add from downloads takes a Clip Studio Paint sub tool without a picture
  tip (a round pen, an airbrush) instead of saying the file has no brush,
  and each sub tool of a `.sut` gets only the tips its own settings name,
  not every tip in the file.
- Instant transitions and Animation speed (Settings, Motion) now reach
  the switches in Settings and on the start pages and everything else
  the interface toolkit animates by itself: they used to keep their own
  fixed time whatever the settings said.
- Sue Paint's pen strokes no longer end in a blob wider than the line
  with tablets whose pressure comes from the driver (Windows Ink turned
  off, X11, a browser): the stroke's last points keep the pressure the
  pen had as it lifted instead of taking full pressure, and letting go
  no longer paints one more point. A big brush lifted with a hook no
  longer leaves a scrap of the hook behind.
- Sue Paint's palette icons in the strips beside the canvas answer clicks
  again: the grip that drags an icon out of the strip was sitting over the
  icon and taking its clicks, so a collapsed palette could not be opened
  from its icon at all.
- Sue Paint's color wheel is smooth at every size: the hue ring's two
  circular edges are feathered, since egui antialiases no mesh, and the
  saturation and value square's mesh follows the wheel's size. It used to
  draw a stair-stepped circle over a gradient with a visible grid in it.
- A picture drawn smaller than it is stays smooth: Sue Paint's canvas now
  samples the mip chain of the picture's texture, so what a screen pixel
  covers is averaged instead of sampled. Thin lines used to come out ragged,
  of a weight that wandered along them, and places where lines met went
  solid. Magnified pixels are untouched: the nearest filter is still what
  draws them.
- A Smart Shape line or shape can still be adjusted while the pen is down:
  after a stroke corrects itself, dragging before you lift still moves it, the
  way Clip Studio Paint does. A line's far end follows the pen, a shape turns
  and grows around its middle, and what the drag leaves is what lifting the
  pen keeps, still one stroke to undo.
- Undo, Redo, Open, Save and Quit show in a studio's menus the keys they
  really have: change one in Settings (Keymaps) and the menu follows, as
  it already did for the studio's own commands.
- Sue Paint's Edit menu no longer shows Shift+Del beside Delete Outside
  Selection: that key fills. The item shows the key you give it in
  Settings.
- Swap Main and Sub Color, Drawing or Transparent Color, Smaller Brush
  and Bigger Brush do their work when Auto Actions or Studio Mode's keys
  run them, as their keys always did.
- Sue Writer's entries in the command palette name the studio once
  ("Sue Writer: New Project..."), not twice.
- Ctrl+F12 inserts a table in Sue Writer, as its Table menu always said.
- The response curve graph (pen pressure and the like) moves the point
  you pressed on even when the hand moves quickly, instead of adding a
  second point beside it.
- The opacity bar's handle shows on light themes too.
- Sue Paint's Help, Sue Paint Guide and Help, Check for Updates open
  before a canvas is, instead of staying grayed.
- Undoing Assign Animation Cel, Add Keyframe or Delete Keyframe in Sue
  Paint names the step after the menu item, in your language, instead
  of "Specify Cel" or "Delete Key".
- Reverse Layers Vertically on Sue Paint's light table turns the
  table's layers round; it used to leave them as they were.
- Sue Paint's Story menu grays out what cannot work on the page shown:
  First and Previous Page on the first page, Next and Last Page on the
  last, Delete Page on a story's only page, Combine Pages on a page that
  faces none, Split Pages on a page that is no spread, and Change Page
  Settings and Export All Pages on a picture that is no story. Before,
  they looked ready and did nothing or only said why.
- Add Page, Insert Page and Duplicate Page on a picture that is no story
  yet are one undo step: one Ctrl+Z leaves the picture as it was,
  instead of a story of one page.
- Sue Paint: Edit, Rotate/Flip Canvas, Flip Horizontal, Flip Vertical
  and Rotate 180 Degrees turn layer masks and vector lines with the
  picture; before, a mask stayed where it was and no longer covered its
  layer.
- Sue Paint: OK and Cancel no longer fall off the bottom of the Drop
  Shadow, Focus Lines and Speed Lines dialogs.

- An icon pack that leaves out the optional filled twins (of star,
  heart, eye and lock) no longer reads "4 things could not be read".
  Its card says plainly that Sue's own twins show for the on state;
  only files that really could not be read are counted, and listed on
  a click. Remove on a pack's card asks again if anything else was
  pressed after the first click.
- A setting whose value is no longer one of its choices (a sound
  device that was unplugged) still opens its list in Settings, so
  another can be picked.
- Sue Paint's Quick Access, popped out of the palette strip, is as tall
  as its set when the window has the room, so Set 1's last button
  (Quick Access Settings) is no longer hidden below its edge; in a
  shorter window the set scrolls and its scroll bar stays in sight.
- Sue Paint's Sub Tool list scrolls to the sub tool chosen (by a key, a
  command or another tool), and its scroll bar shows when the list
  holds more than fits.
- The path of a selection being drawn (Lasso, Lasso fill, Shrink
  selection and polyline selections) is a dark line on a light rim, so
  it reads on white paper, black ink and every color between, whatever
  the theme.
- Sue Paint: renaming a layer or an Auto Action ends with Enter, Tab or a
  click elsewhere and keeps the new name (the name typed was lost).
- Sue Paint: a new animation lists every layer on the timeline, the
  paper too, so it can be locked or hidden there.
- Sue Paint: a cel put in a folder of its own (Create Folder and Insert
  Layer, or dragged onto New Layer Folder) stays the cel: the folder
  takes its name and its frames on the timeline.
- Sue Paint: Merge Selected Layers on a folder cel keeps it the cel on
  the timeline, on the frames it showed.
- Sue Paint, Timeline: the playback range's start and end handles on the
  ruler can be dragged again (a press there moved the playhead).

### Removed

- Sue Paint's own start page (Draw, Projects, Manage Materials, Data
  Transfers, Help and Settings, and its phone and tablet tabs), and File,
  Start Page. Everything on it lives on: your work in the Hub's Files,
  the materials in its Materials place, Transfers in the Hub's top bar,
  Help in Sue Paint's Help menu, and its settings in Settings, Sue Paint.
- **Browse Assets** is gone from Sue Paint's start page, on computers,
  phones and tablets, with everything that read another company's
  material site. Sue Paint no longer goes online for materials at all,
  and the pages and pictures it had kept from that site are deleted from
  Sue's cache the next time Sue Paint starts.
- The **Downloads** button at the top of Sue Paint's tool palette, which
  only opened Browse Assets. The tools move up by one place.
- Downloading from a link in **Data Transfers**. Brushes and pictures
  that are already on your device still come in as before: **Add from
  downloads** in the Tool Group palette and `+` in Manage Materials.
- A page of Sue Paint's Preferences for a hand controller that Sue
  never worked with; its one setting did nothing.
- A damaged `.clip` whose sizes are too large to add up is refused with a
  plain message instead of being read from the wrong place.
- A `.sue` file that a later version of Sue saved opens even when a layer
  holds pixels in a format this version does not know: that layer is not
  drawn, the rest is, and saving the file again keeps the layer and
  everything else the later version wrote that this one cannot read.
- The **Image Viewer** and the **Converter**, with their boxes on the
  Hub, the Sue menu's Tools and their pages in Settings. A picture opens
  in Sue Paint as a canvas, where turning, flipping, cropping, resizing,
  color adjustments and saving as any picture type already are, and the
  Sue menu's Send To puts it into Sue Writer. Converting many pictures at
  once, export settings and audio export move into Sue Paint, and tables
  and text tidying into Sue Writer. Gone for good: the Converter's own
  Markdown to HTML (Sue Writer reads Markdown and makes web pages itself),
  JSON tables and JSON re-indenting, turning one sound file into another,
  tidying source code files, Base64, hex dumps, byte map pictures, plain
  copies, sound speed and reverse, Add from the Project, the Image
  Viewer's info row and its shortcuts, and stepping through a project's
  pictures with the arrow keys.

## [0.0.75] - 2026-11-09

### Added

- Sue Paint's palettes can be arranged your way, as in Clip Studio
  Paint: drag one by its title bar, tab or icon to dock it at the left,
  the right or under the canvas, stack it as a tab with another palette
  (and drag tabs to reorder them), give it a column of its own, fold it
  into the strip or float it. Sue Paint opens with the palettes where you
  left them. (#3)
- **More ways to arrange Sue Paint's palettes**, as in Clip Studio
  Paint: dock a palette at the top beside the command bar, give it a new
  column on the left or right, pop it out into a window of its own, or
  tear a tab (Color Slider, Tool Settings, History and the others) off
  into a palette of its own. The command bar and the tool palette move
  like the other palettes, and widening the tool palette's column lays
  its tools out in several columns. Window, Workspace has **Use Current
  Workspace as Default**, and **Reset to Default** goes back to it.
  Ideas and code from mahesh-labs' fork. (#3)
- **Timeline navigation** in Sue Paint, as in Clip Studio Paint: over
  the Timeline's frames the mouse wheel zooms around the pointer, Shift
  and the wheel pan sideways, Alt and the wheel scroll the tracks, and
  Space and a drag (or the middle button) pan; scrollbars run along the
  Timeline's bottom and right edges. The eye beside an animation folder
  shows and hides it, in step with the Layer palette. From mahesh-labs'
  fork.
- **Tilt** in brush dynamics: beside pen pressure and speed, how far the
  pen leans can change a brush's size and opacity, with a minimum and a
  curve, as in Clip Studio Paint. A mouse counts as an upright pen.
- **The pen's eraser end** picks the Eraser, and turning the pen back
  picks the tool you were using, between strokes (under Linux, where the
  tablet reports it).
- **Turn with pen tilt** for brushes with a shape: each shape turns the
  way the pen leans, so a flat or chisel tip follows your hand, as Clip
  Studio Paint's direction of the pen does. Vector lines keep it. (#2)

### Fixed

- Sue Paint's **animation folders** are folders in the Layer palette
  again, as in Clip Studio Paint: every animation cel sits inside its
  folder, indented, and folds away with it. New Animation Cel adds the
  cel to the folder you picked, dragging a layer onto the folder makes it
  a cel, and animations from earlier versions get their folders when
  they open (#1).
- **Pen tablets on Linux** (XP-Pen, Huion, Wacom and others): under
  Wayland compositors such as niri the pen did nothing in Sue. Sue now
  reads the pen through Wayland's own tablet protocol, so it moves and
  draws with its pressure, tilt and eraser end, and on X11 it reads them
  too. Where a Wayland compositor has no tablet protocol and a pen tablet
  is connected, Sue opens through XWayland instead. Set
  `SUE_DISPLAY_SERVER=wayland` or `x11` to choose yourself. Under
  Wayland the pen shows the same cursor as the mouse.
- **Pen tablets under Wayland** answer at once: a pen that only hovers
  or draws no longer waits for the mouse or keyboard to wake Sue, every
  point of a stroke has the pressure the pen had there rather than the
  last one of its frame, and reading the tablet can no longer stall
  beside the graphics driver. Under X11 a pen's tilt reads in degrees as
  under Wayland, so tilt settings feel the same on both. Found and tested
  on a Huion Kamvas 22 Plus under KDE Plasma 6 by mahesh-labs. (#2)
- A palette dragged with a pen (or a finger) now lands where the pen
  lifts; before, the drop was lost and the palette stayed where it was.
  (#3)

Thanks to mahesh-labs for their fork and to Zush for the Wayland pen
work and testing in #4.

## [0.0.74] - 2026-11-02

### Added

- **Pose scanner** and **Hand scanner**: pick a 3D drawing figure or a
  3D hand, choose a photo in Tool Settings, and it takes the pose of the
  person or hand in it. The models (MoveNet and MediaPipe, both openly
  licensed) download once and then run offline on your computer.
- **Ruby** (furigana) for text: select words while typing and set their
  reading in Tool Settings; it sits over them, or beside them in
  vertical text.
- The Figure tool's **Polyline** and **Curve**, and the Fill tool's
  **Paint unpainted area**, as in Clip Studio Paint.
- **Browse Assets** on phones and tablets, with Data Transfers as its own
  tab beside it.

### Fixed

- The Linux packages (AppImage, .deb, .rpm and tarball) carry Sue's
  AppStream description, so software centers and AppImage tools show it.

## [0.0.73] - 2026-11-01

### Added

- New Clip Studio Paint sub tools: **Close and fill** (Fill: draw round
  some lines and every area they close off fills), **Lasso fill**
  (Figure: fills whatever you draw round) and **Copy stamp** (Blend: Alt
  and click where to copy from, then paint).
- The Download button at the top of the tool bar opens Browse Assets
  instead of doing nothing.

## [0.0.72] - 2026-11-01

### Added

- Sue Paint downloads by itself: paste a link to a brush file, a PNG or a
  zip of them in Data Transfers (or on a material's page in Browse
  Assets) and it downloads as a transfer you can watch and cancel. Its
  brushes join the current tool and its pictures the materials, without
  a browser or Clip Studio. **Open on CLIP STUDIO ASSETS** still opens
  the site, only when you press it.
- Layer Property has the Watercolor edge (edge width and darkness) and
  Extract line (edge threshold and line width) effects, as Clip Studio
  Paint's. Like the other effects they never change the drawing.
- Everything in the Animation Cels palette works: a general light table
  shown on every cel, Select and register file, the Light table tool
  (drag to move a traced layer, Shift to turn it), Reset position of
  layers on light table and the opacity of either light table.
- Register preset and Delete preset in File > New keep your own canvas
  sizes for next time.

### Changed

- A layer button that cannot be used right now says why when you hover
  it: Enable Mask and Apply Mask to Layer need a layer mask, Merge with
  Layer Below needs a raster layer below.

### Fixed

- Lifting the pen after a long stroke no longer stalls: only the
  tapered ends are drawn again, so the finish takes a few milliseconds
  however long the stroke is (74 ms before on a long stroke).

## [0.0.71] - 2026-11-01

### Fixed

- The Decoration tool draws: in 0.0.7 a stroke with it only said
  "Decoration arrives in a later Sue Paint". B now cycles to it too,
  after the Brush and Airbrush, as in Clip Studio Paint.
- A new test runs every Sue Paint menu command and draws with every tool,
  so a tool or command that is not wired up cannot ship again.

## [0.0.7] - 2026-11-01

### Added

- Sue Paint's Balloon tool has Clip Studio Paint's balloons: rounded
  rectangle, cloud (thought) and flash (shout) balloons, the Balloon pen
  for a balloon in any outline, and the thought balloon tail's bubbles.
  The Text tool centers words in any of them.
- Curve and polyline balloons and the Polyline frame, clicked out corner
  by corner as in Clip Studio Paint, and the Object tool drags a panel's
  corners or the whole panel.
- Sue Paint's Figure tool draws Saturated Lines around the clear middle
  you drag, and Speed Lines along the way you drag, each on an effect line
  layer that stays editable: the Object tool moves their ends, and Tool
  Property changes how many lines, their width, their scatter and color.
- Folios: Story, Change Story Information numbers the pages, at their
  outer corners or centered, on the canvas and in every export, and the
  first page can start alone or facing page 2 (also in the Page Manager).
- File, Import, Batch import brings every picture in a folder in, one a
  page, in name order.
- The Text tool's Text on Curve: draw a curve, then type along it.
- The Text tool sets letter spacing, line spacing and justification.
- Layer, New Layer, 3D Hand and 3D Head, posed like the drawing figure
  by dragging their joints or from their presets, as in Clip Studio Paint
  5.0 and 5.1.
- Register pose as material keeps any pose of a figure, hand or head in
  the Material palette's Poses, to use again.
- 3D layers can carry their light with the camera and have fog rising
  from the foot, as in Clip Studio Paint 5.0 and 5.1.
- Animation, Timeline, Exposure Sheet: the timing as a table to read and
  change, saved and read back as CSV (File, Export Animation and File,
  Import).
- File, Export Animation, Animated WebP.
- File, Export Animation, Animated sticker (APNG).
- Smart Shape, as in Clip Studio Paint 5.0: hold the pen still at the end
  of a stroke and it becomes a clean line, arc, ellipse or polygon.
- Add from downloads in Sue Paint's Tool Group brings in brushes you
  downloaded: Clip Studio Paint sub tools (.sut), Photoshop brushes
  (.abr), GIMP brushes (.gbr, .gih) and pictures. Their size, opacity,
  spacing, spray, ribbon and turn come along, colored tips keep their
  colors, and tips of several pictures take them in turn.
- Ribbon decorations: Rope, Rickrack, Film strip, Checkered band,
  Stitched ribbon and Lace ribbon bend along the whole stroke, and any
  brush with a shape can be laid as a ribbon.
- Sue Paint's Decoration tool, as in Clip Studio Paint: Stars, Hearts,
  Sparkles, Snow, Flowers and Bubbles scatter about the stroke; Leaves,
  Chain, Lace, Stitches, Dotted line and Dashed line run along it and
  turn with it. Any brush with a shape gets Turn along the line, Angle,
  Scatter, Size jitter and Turn jitter in Tool Settings.
- Filter, Effect has Glow, Inflate and Drop Shadow, as in Clip Studio
  Paint 5.1.
- Tonal corrections change every picked layer at once, as in Clip Studio
  Paint 5.0, and Layer, Change Selected Layer has Select All Layers and
  Deselect Layers.
- Layer comps, as in Clip Studio Paint EX: named versions of which
  layers show, how strongly and how they blend, shown again in one step
  and exported each as its own picture.
- File, Special Export, Export vectors saves vector layers as SVG.
- The Object tool's Tool Property says what it does on each kind of
  layer instead of that it arrives later.
- Release Notes in the update window opens the whole of what is new,
  inside Sue, on phones and computers.
- On Windows and Linux, Sue draws its own window frame, as Clip Studio
  Paint does: a dark title bar with the title in the middle and minimize,
  maximize and close at the right, on the main window and on every dialog
  and floating palette. Drag it to move the window, double-click it to
  maximize, and drag any edge to resize.
- Sue Paint's window title says what is open, as Clip Studio Paint's
  does: `Illustration* (1600 x 1200px 72dpi 42.4%) - Sue Paint`, with a
  `*` while it has changes not saved and the print profile while View,
  Color Profile, Preview is on.

### Changed

- Sue Paint's Text tool types right on the canvas, as Clip Studio Paint
  does: click, type, Enter for a new line, Esc to keep the words. A click
  on the words puts the caret there, and Tool Property sets the font, size,
  direction and centering, also for the words being typed. Letters that are
  shortcuts no longer switch tools while you type.
- Sue Paint's pens, pencils, brushes, airbrushes and erasers start as
  Clip Studio Paint's do: the same sizes, opacity, hardness, density,
  anti-aliasing, color mixing and stabilization, and its Starting and
  ending by speed, so a quick line tapers and a slow one keeps its size
  from the first point. The Brush tool has Clip Studio's brushes (Round
  and Flat watercolor brush, Gouache, Oil paint, Thick oil paint, Wet
  wash, Brush pen, Dry ink, Watery ink, Pointillism), the Airbrush its
  Spray and Droplet, and the Eraser its Kneaded eraser.
- Quitting asks about each picture with changes not saved, as Clip
  Studio Paint does: "Illustration has been changed. Do you want to save
  it?" with Save, Do Not Save and Cancel. "Quit Sue?" is no longer asked
  unless you turn it on (Settings, Confirm on exit).
- Under Sue Paint's color wheel the main and sub colors are the same
  overlapping circles as in the tool strip, with the transparent pill.
- On phones the Hub lists studios and tools as rows across the screen,
  easy to reach with a thumb, and menus have taller rows without keyboard
  shortcuts.
- On phones the update window is a sheet across the bottom of the screen.
- Sue Paint's side palettes fold and close as in Clip Studio Paint, on
  both sides: the single arrow at the top folds the side's palette columns
  into icons (the tools stay) and opens them again, and the double arrow
  closes the whole side to a thin bar whose arrows bring it back. Both
  are kept with the workspace.
- Sue Paint's color wheel is Sue's own, the one every color control
  shares, and its transparent color is a rounded checkered pill as in
  Clip Studio Paint. Clear looks the same checkerboard everywhere.
- Sue Paint's Tool Sliders look like Clip Studio Paint's: the size and
  opacity in round badges, the size slider lit from the bottom up to its
  knob, and the opacity slider solid black at the top fading to clear.

### Fixed

- Pictures in Sue Paint show in their own colors instead of too dark and
  too contrasty.
- WebP, GIF, BMP, TIFF and ICO pictures open, in Sue Paint and from
  anywhere in Sue, instead of showing their raw bytes.
- Moving a layer with the Move tool redraws it where it goes, leaving no
  copies behind.
- A layer scaled or moved past the canvas edge with Scale/Rotate keeps the
  part outside, so moving it back shows the whole picture.
- Pinch vector line bends a line drawn quickly, whose points lie far
  apart, instead of leaving it as it was.
- A dot no longer appears at the start or end of pen strokes: a pen
  bouncing on the tablet as it lands or lifts touched it for a few
  thousandths of a second, which Sue Paint took for a tap. A real tap
  still leaves its dot.
- Sue on Android can check for updates and download them: it could not
  reach the internet at all. The update installs from inside Sue, asking
  once to be allowed to install apps, and says plainly when it could not.
- Android updates install over the version you have instead of failing
  with "conflicting package": every release is now signed with the same
  key. Versions up to 0.0.6 were signed differently, so going from one of
  them takes uninstalling Sue once.
- The small launcher buttons in Sue Paint (under a selection, a
  transform, text and a layer's drawing) work: a click on one used to
  start a new selection or stroke under it and close the launcher.

## [0.0.6] - 2026-10-30

### Added

- 3D layers in Sue Paint take Clip Studio Paint's camera presets (Front,
  Back, Left, Right, Top, Bottom), the light's intensity, ambient light
  and cast shadows, a line color and opacity, and two more shapes, prism
  and pyramid. Drawing figures take a body shape: Height and Head to body
  ratio, from a child or chibi to a tall adult.
- Layer, Convert to Vector Lines and Tones: a 3D layer (a shape, a model
  or a posed drawing figure) becomes its tone and a vector layer of
  editable strokes along its outline and folds.
- 3D drawing figures in Sue Paint (Layer, New Layer, 3D Drawing Figure),
  as in Clip Studio Paint: a body of seventeen joints to pose by dragging
  a body part with the Object tool (Shift twists it), or by its Bend,
  Twist and Swing in Tool Settings, with pose presets (Stand, T-pose,
  Walk, Run, Sit, Arms up, Think), Flip pose and Reset pose. Figures draw
  shaded, as line art or both, and Convert to Lines and Tones turns them
  into line art and tone to draw over.
- Translations into French, German, Spanish, Japanese, Arabic, Italian,
  Brazilian Portuguese and Simplified Chinese, covering Sue Paint and
  Settings with Clip Studio Paint's own words for its menus and tools,
  each waiting for a native speaker's review. Pick one in Settings,
  Language.
- `docs/paint-parity.md`: every Clip Studio Paint feature with where Sue
  Paint stands on it, for people and agents choosing what to build next.

### Changed

- Sue Paint's tool palette is Clip Studio Paint's: Downloads, Pen,
  Sketch, Brush, Eraser, Airbrush, Decoration, Blend, Liquify, Selection
  area, Auto select, Fill, Gradient, Operation, Figure, Text, Comic, Ruler,
  Correct line, Move and Eyedropper, then the main and sub colors as
  circles with their swap and the transparent color. The Pen tool has
  Clip Studio's eleven pens (G-pen to Flat marker) and Sketch its six
  (Pencil, Mechanical pencil, Charcoal, Crayon, Pastel, Chalk).
- The Tool Group palette (once Sub Tool) shows each brush's own stroke and
  has Add from downloads.
- A tap with a pen leaves a dot as large as the brush, and pens no longer
  start thin with a mouse.
- `cargo run` builds the interface, the project reader and the format
  codecs optimized, so a debug build no longer stalls on clicks and
  projects.
- Edit, Advanced Fill, as in Clip Studio Paint: fill the selection (or
  the canvas) with the main, sub, transparent or a chosen color at an
  opacity and blending mode, only where the target color is (all,
  transparent, black, white and transparent, or what they surround), in
  closed areas with gaps closed, grown or shrunk under the lines, looking
  at the current layer or every shown one. Its settings are kept.
- Edit, Smart Smoothing, as in Clip Studio Paint: jagged, stair-stepped
  line art becomes smooth, with lines kept as thick and as dark, previewed
  on the canvas as you set its strength.
- Filter, Quick Filters and Quick Effects, as in Clip Studio Paint: a
  gallery of one-click looks, each shown on your own layer, with a
  strength. Quick Filters recolor (Vivid, Soft, Warm, Cool, Sepia,
  Monochrome, Vintage, Fade, High contrast, Night); Quick Effects restyle
  (Glow, Halftone, Outline, Pencil, Pixel art, Film grain, Vignette,
  Poster, Watercolor, Emboss). Apply to all works on a merged copy of
  everything shown, taken away again on Cancel.
- Sue Paint has Clip Studio Paint's launchers, each with its switch in
  the View menu: the Transform launcher under a transform's box (reset,
  flip, turn a quarter, cancel, OK), the Text launcher under a text
  layer's words (edit, smaller, larger, vertical, centered) and the Object
  launcher under the layer's drawing with the Operation tool (scale and
  rotate, flip, layer properties, delete).
- More of Sue Paint's menus follow Clip Studio Paint: File, Create New
  from Clipboard makes a canvas the size of what you copied with it
  pasted in; Edit, Change canvas height; Convert to lines and tones in
  the Layer menu; View in Clip Studio's order with Selection Border,
  Selection Launcher and a Snap submenu; Window, Brush Size; and Clip
  Studio's names for Add Page (Advanced), Page Reordering, Split Pages,
  Change project settings, Edit track, Correction, Layer Properties and
  Auto Action.
- Sue Paint's menus use Clip Studio Paint's names (Delete, Delete Outside
  Selection, Select All, Save as, Tool Group, Tool Settings, Tool Sliders,
  Information) and gain Edit, Align/Distribute (line the layer's drawing
  up with the canvas's edges or middle), View, Scroll Bar, and New Raster
  Layer at the top of the Layer menu. Each palette column has Clip
  Studio's bar with arrows that fold it into the strip.
- Sue Paint opens with Clip Studio Paint's layout: a strip of folded
  palettes at the far right (Material, Layer Property, Animation Cels,
  History, Sub View, Auto Actions, Item Bank and Info, each a palette of
  its own), the Tool Sliders column with brush size and opacity side by
  side, a two-row Navigator with a red view frame, scrollbars on the
  canvas, the zoom and turn wedges under it, and red, green and blue
  values under the color wheel. New filters: Lens Blur and Remove JPEG
  Noise.
- On a computer, Sue Paint's dialogs and floating palettes are windows of
  their own: move them anywhere, outside Sue's window or onto another
  screen, and resize them. Closing a floating palette's window docks it
  again.
- Sue Paint's Timeline looks and works like Clip Studio Paint's: tall
  tracks with the picture each folder shows, every key's cel as a picture,
  alternating second bands, a red current frame, zoom buttons and the
  frame counter. The New dialog is laid out as Clip Studio's, with OK and
  Cancel beside the Project row, and has all six projects: Illustration,
  Webtoon, Comic, Printing of fanzine, Show all comic settings and
  Animation.
- Sue Paint's Navigator, Animation Cels and Layer Property look and work
  like Clip Studio Paint's: wedge sliders for zoom, rotation and opacity,
  the light table's buttons (reverse its layers, Color mode, lock the cel
  being drawn, previous and next cel, deregister), and the layer effects
  as a row of buttons. New filters: Twirl, ZigZag and Crystallize.
- Sue Paint's palettes look and work like Clip Studio Paint's: tabs show
  an icon and their name, the Sub Tool list shows each brush's own
  stroke, the tool palette has a Brush Size grid of sizes, History lists
  every step to go back or forward to with a click, and the layer
  buttons are Clip Studio's (with Transfer To Lower Layer).
- Icons in Sue Paint are bigger and two-toned.
- Each document keeps its own undo history, as in Clip Studio Paint:
  undo in one Sue Paint canvas (or one story's pages) takes back only its
  own steps, every studio and tool keeps a history per file, and Sue's own
  changes (Settings, the Hub's pin, the language) have theirs. Edit, Clear
  work history, History in Sue Paint forgets the canvas's steps.
- Sue Paint's menus have more of Clip Studio Paint's items: more formats
  under Save Duplicate and Export, Special Export, Outline Selection,
  Scale, Rotate and Skew, Ungroup Layer Folder, Delete empty or hidden
  layers, Hide all visible draft layers, Change Layer Name, Transfer To
  Lower Layer, Merge visible layers in place (Ctrl+Shift+E), view
  rotations of 90, 180 and 270 degrees, Reset Display, Paper, and Move
  frame.
- Sue Paint's start page has **Browse Assets**: CLIP STUDIO ASSETS'
  top materials laid out as Clip Studio's start page does, See more for a
  whole ranking, and each material's own page. What it read is kept, so
  it shows the last copy without a connection.
- File, Preferences in Sue Paint opens its own Preferences, page by page
  as in Clip Studio Paint, instead of Sue's Settings. Options Sue Paint
  does not do yet are shown greyed out.

### Fixed

- Sue Paint's stabilization never lets the line fall far behind the pen,
  however fast you draw, and when you lift the pen the line finishes along
  the way your pen went instead of jumping there in a straight line.
- Sue Paint's brushes keep up better with lower opacity and hardness and
  with color mixing and paper grain, which made big brushes several times
  slower, most of all in the browser. Strokes look exactly as before.

## [0.0.5] - 2026-10-29

### Added

- Sue Writer starts like Word: a blank document and templates shown as
  pictures of their first page (letter, cover letter, resume, report,
  essay, newsletter, memo, meeting notes). Novels, short stories,
  screenplays, non-fiction and poetry are in a Presets list beside them.
- Your own templates: File, Save as Template keeps the open document as
  one, and Word, LibreOffice and Sue Writer templates in the templates
  folder (File, Templates Folder) are offered for new documents.
- Sue Writer opens PDF files (their text, paragraphs and headings) and
  Word and LibreOffice templates (`.dotx`, `.ott`).
- Pages on their side: Sue Writer's Project Settings, Orientation.
- Selecting text with a finger in Sue Writer works as on phones: two
  handles to drag, and a bar with Cut, Copy, Paste and Select All. On
  Android, Copy and Paste use the system clipboard, so text moves to and
  from other apps.
- More of Sue Writer on phones and tablets: the formatting row slides
  sideways to more tools (strikethrough, Clear, Link, Picture, Table,
  Footnote, Comment, Page Break), and the top bar holds Page View,
  Picture, Comments, Save as PDF and, on tablets, Find and Table.
- Sue keeps itself up to date. When it starts it looks for a newer
  release, at most once a day, and a small window in the corner shows
  what is new; **Check for Updates** is in the Sue menu and in Settings,
  and **Check automatically** in Settings turns the daily look off.
  Updating follows how you installed Sue: on Windows it downloads the new
  setup program and restarts into it, macOS opens the new disk image, an
  AppImage replaces itself, Android hands the new APK to its installer,
  and Linux packages are downloaded with the command that installs them.
  Downloads run in the background and are checked against the release's
  checksums.
- Settings can go back to how Sue comes: a Reset beside any changed
  setting, Reset This Page, and Reset All Settings.
- The Image Viewer has Undo and Redo buttons.

### Changed

- In the browser, a phone with Request Desktop Site turned on gets the
  desktop layout. Tablets keep the tablet layout.
- The status bar along the bottom is off at first; Settings turns it on.
  Its messages show as a small notice at the bottom meanwhile.
- The Hub's pin is a real pushpin, and pointing at it lights up the pin,
  not its whole box.
- Slider handles are thin rectangles, a little shorter than the circles
  they were: `---[]---`.
- Huge brushes and erasers in Sue Paint keep up: painting with a 2000
  pixel brush on a large picture with many layers takes about a
  millisecond a frame, where it used to take a quarter of a second, in
  the browser too. Zoomed out, the canvas first shows a big stroke at the
  detail the screen can show and fills in the rest a moment later.

### Fixed

- Sue Writer on a phone shows the whole page across the screen, upright
  or turned, instead of a strip of text; tablets fit the whole page too.
- Dragging a finger on Sue Writer's page scrolls it, exactly as far as
  the finger moves, instead of selecting text; pinching zooms. Tap to
  place the caret, double tap for a word, and press and hold to select.
- The on-screen keyboard no longer closes and reopens when you tap the
  text or the toolbar in Sue Writer.
- The paragraph style box on a phone's toolbar holds its name and arrow
  again (some boxes near the shape of a slider handle were drawn too
  narrow).
- Clip Studio Paint files (`.clip`) open in Sue Paint with their layers
  however you open them: the Sue menu's Open File, the start page's
  Open, dropping one on the window, or the Hub. On phones and tablets
  the browser's file picker no longer greys them out.
- Nothing paints or reacts under an open menu, popup, dialog or sheet,
  most of all on phones and tablets, where taps went through Sue Paint's
  sheets to the canvas.
- Pinching to zoom in Sue Paint no longer undoes your work or closes the
  picture: the touch the pinch began with is taken back, nothing else.
- Pen strokes on phones and tablets no longer end in a big blob: the
  stroke's last point keeps the pen's last pressure.
- Pen pressure works with drawing tablets whose driver has Windows Ink
  turned off (WinTab, common with Huion and XP-Pen), and with pens in the
  browser version. Sue Paint starts with a slightly soft pressure curve,
  says where to fit it to your hand the first time it sees your pen, and
  the start page's Settings has Pen Pressure.
- Sue Paint no longer crashes on a short stroke with post correction
  ("min > max", seen in the browser).

## [0.0.4] - 2026-10-29

### Added

- Sue Paint opens and saves Clip Studio Paint `.clip` files. Save keeps a
  picture as a `.clip` that Clip Studio Paint opens with its layers,
  folders, blend modes, opacity and clipping, and that Sue opens again
  with everything. Settings can make Save write a Sue project instead.
- Settings fits every screen: full screen on a phone, with the categories
  as a list that opens one page at a time and controls big enough for a
  thumb; a large sheet with the categories at the side on a tablet, upright
  or on its side; and a window that never spills out of a small one on a
  computer.
- The Sue menu's **Layout** switches all of Sue, on a computer or in the
  browser, to the Tablet or Phone layout (the same buttons, bars and sheets
  as on a tablet or phone) and back, at once. **Phone Shaped** keeps the
  phone layout to a phone-wide column. Mouse and keyboard keep working,
  and Sue remembers your choice.
- Sue Paint opens on a start page on the desktop: **Draw** opens your last
  picture or project (or a new canvas), **Projects** shows your pictures
  and Sue Paint projects with thumbnails, sorted into Favorite, Work and
  labels of your own, and **Manage Materials** browses tones, patterns,
  pictures and brush tips. Data Transfers lists what Sue opened, saved and
  copied; Help and Settings sit beside it. Everything stays on your
  device. File, Start Page brings it back.
- Sue Paint's start page on phones and tablets: big buttons to create a
  file, start from a picture or open a file, your projects, and a round
  Draw button in a bar along the bottom. The New canvas page offers
  templates for illustrations, webtoons, comics and animations, and
  Continue drawing. Canvases can now be up to 20000 pixels a side.
- Simple Mode for Sue Paint on phones and tablets: just the canvas, brush
  size and opacity sliders, and a bar to swap brush and eraser, pick pens
  and brushes, the color and the layers; "..." holds the rest.
- Studio Mode for Sue Paint on phones and tablets: a tool column with size
  and opacity sliders, a row of palettes, a command bar, fullscreen, a
  slide-out bar of modifier keys (Esc, Ctrl, Shift, Alt, Space and seven
  keys of your own) and a Menu with every menu. It replaces the earlier
  phone and tablet layouts.
- Try Sue in your browser at https://magigun.github.io/sue/try/, with
  nothing to install: the Hub, Sue Paint, Sue Writer and the tools, opening
  files from your computer (or dropped on the page), saving them as
  downloads, and settings and unsaved work kept in the browser.
- Footnotes in Sue Writer's PDFs print at the foot of their page, and
  endnotes (Insert, Endnote) gather at the end; Word and OpenDocument keep
  both.
- `Shift+Enter` in Sue Writer starts a new line inside a paragraph.
- Sue Writer opens OpenDocument (`.odt`) files from LibreOffice, with their
  styles, lists, links, footnotes, tables and table of contents.
- All Capitals, Grow Font and Shrink Font in Sue Writer's Format, Text,
  and Insert, Formatting Mark for non-breaking spaces and hyphens.
- Word files opened in Sue Writer bring their tables as tables.
- Spelling as you type in Sue Writer: misspelled words are underlined;
  right-click for suggestions, Ignore All or Add to Dictionary, and `F7`
  goes to the next one. English ships with Sue.
- Track changes in Sue Writer (Review, Track Changes): typed text is
  underlined, deleted text struck through, until each change is accepted
  or rejected. Word files keep the changes with their authors.
- Sue speaks your language. It starts in your system's language when Sue
  has it; the Sue menu's Language changes it for every studio and tool at
  once, and Settings, Language lets any studio or tool keep a language of
  its own. Spanish and Japanese ship as drafts waiting for a native
  speaker's review; anything not yet translated shows in English.
- Comments in Sue Writer (Review, New Comment, `Ctrl+Alt+C`): shown beside
  the text with their replies, to reply to, resolve or delete. They go into
  Word and OpenDocument files as comments.
- A table of contents in Sue Writer (Insert, Table of Contents): the
  headings with their pages, kept up as you write; click an entry to go
  there. It prints in PDFs and goes into Word, OpenDocument, web pages,
  e-books, RTF, Markdown and plain text.
- Find and Replace in Sue Writer takes regular expressions, and the
  replacement can put back what a group caught (`$1`).
- Change Case in Sue Writer (Format, Text): Sentence case, lowercase,
  UPPERCASE, Capitalize Every Word and tOGGLE cASE.
- Headers and footers in Sue Writer (Insert, Header and Footer): left,
  middle and right parts with the page number, page count, title, author
  and date, and a different first page. They print in PDFs and go into
  Word, OpenDocument and RTF as live page numbers.
- Tables in Sue Writer: Table, Insert Table (`Ctrl+F12`), then type in the
  cells; `Tab` moves on and adds a row at the end. Insert and delete rows
  and columns, a heading row that repeats on every page, and text to table
  and back. Tables print in PDFs and go into Word, OpenDocument, EPUB, web
  pages, RTF, Markdown and plain text.
- The Hub is Sue's home page: a box for every studio (Sue Paint, Sue
  Writer) and every tool (the Image Viewer, the Converter). The pin at a
  box's bottom right makes Sue open straight into it; one pin at a time,
  and Settings, General, Startup shows it too.
- The Sue menu, under the Sue button at the window's top left: the Hub,
  every studio and tool, New, Open File, projects, Send To, the pin, the
  command palette and Settings.
- Opening and saving work on phones and tablets again, through the
  system's own picker: Android's Files picker (Google Drive too) and the
  Files browser on iPhone and iPad. On Android, saving a file you opened
  writes back to the original, and a folder you choose is read and
  written (the Converter's Add a Folder and its output folder).
- The Hub lists your work under its boxes: **Recent** files and projects,
  **Saved Projects**, and **Unsaved Work**. Click one to open it in its
  studio. The Sue menu's File, Open Recent shows the same recent files.
- Sue keeps work you have not saved by itself (a minute after a change, and
  when you quit), so nothing is lost to a crash or a quick Quit. The Hub's
  Unsaved Work brings it back (Recover) or throws it away (Discard, which
  asks once more). Opening another project keeps the unsaved work it
  replaces there too. Settings, General sets how often, or turns it off.
- Typing in Sue Writer feels smooth: the caret glides to its new place
  instead of jumping, as you type, delete, move with the keys or click, and
  as lines and pages wrap. It blinks only while it rests. Settings, Display
  & Graphics, Motion has Smooth Caret to turn it off and Caret Glide Time
  for its speed.

### Changed

- Font menus in Sue Writer and Sue Paint show each font's name in that
  font, so you can see what you pick.
- The two fonts Sue carries go by their real names, Ubuntu Light and
  Hack, instead of "Sue Sans" and "Sue Mono". Documents that use the
  old names open as before.
- Sue Paint's File, Open Recent now shows the pictures from Sue's one list
  of recent files, the same one the Hub shows.
- Sue Writer on a phone: the text flows to the screen's width at a size you
  can read, the formatting is a row of big buttons right above the keyboard,
  and the line you are writing stays in sight when the keyboard comes up.
  Tapping a formatting button keeps the keyboard up.
- Sue Writer has fewer options in the way: one short toolbar (what Word and
  LibreOffice Writer keep at hand), the Format menu grouped as LibreOffice
  groups it, Draft and Page beside the zoom, and the inspector hidden until
  View, Inspector. Tablets get larger toolbar buttons.
- The Sue button sits once, in the window's top left corner, and the
  open studio's or tool's menus follow it.
- The Image Viewer and the Converter are tools with pages of their own,
  like the studios, each with its own menus.
- Every file Sue asks for goes through your system's own Open, Save and
  folder windows.
- Opening a text, Markdown or HTML file brings it into Sue Writer; a
  picture opens in the Image Viewer.
- Your settings file moves to format 5 (the old one is kept as
  `default.suerice.v4.bak`); a studio you chose to start in stays pinned.

### Removed

- The infinite canvas and everything on it (cards, the selection and top
  pills, the right-click marking menu, the timeline).
- The side panels: the File Manager, Notes, the Player, the Inspector and
  History. Projects saved with the canvas still open, and keep its layout
  in the file untouched.

### Fixed

- Sue Writer no longer crashes on a page narrower than about 543 points,
  such as a phone's.
- Studios and tools wear their own colors again: with a light Sue theme
  (Paper), Sue Paint in Gray showed white fields and buttons with dark text
  on its gray panels.
- On iPhone and iPad the on-screen keyboard no longer covers the bottom of
  Sue: Sue fits itself above the keyboard, as on Android, so the line you
  are writing in Sue Writer stays in sight.
- On Android, a file you chose in the system's picker is named as you know
  it, such as "Picture.png in Download", in the status bar, the Image
  Viewer, the Converter and the Hub, instead of a path inside Sue's
  cache. There the Image Viewer and the Converter no longer offer to
  show files in Explorer, which a phone does not have.
- On Android, the Converter no longer loses what it makes from a file you
  picked on its own: "Next to each file" asks which folder the results go
  to first, and the status bar names them there, such as "Picture.jxl in
  Download". On iPhone and iPad it asks too.

## [0.0.3] - 2026-10-29

### Added

- A setup program for Windows (`-setup.exe`) that carries Sue's icon,
  shows the license and installs Sue. The `.msi` stays for managed PCs.
- Install with [Scoop](https://scoop.sh), which skips Windows' download
  warning: `scoop install
  https://github.com/MagiGun/sue/releases/latest/download/sue.json`, then
  `scoop update sue` for new versions.
- Releases can be sent to winget (`winget install MagiGun.Sue`) as soon as
  the repository holds a token for it.
- Windows builds sign every program and installer when a code signing
  certificate is set up.
- RISC-V Linux packages (`.deb`, `.rpm`, tarball).
- An Android launcher icon that fits any launcher's shape, and a themed
  icon on Android 13 and newer.

### Changed

- On Windows, Sue's window appears once its first frame is drawn, instead
  of empty.
- The installers show Sue's icon on their pages.
- Linux desktops match Sue's window to its icon: the packages, the
  AppImage, the Flatpak and the window all use the id
  `io.github.MagiGun.Sue`.
- Release notes start with how to install Sue on each system.

### Fixed

- The Windows taskbar showed a blank or generic icon instead of Sue's.
- The Start menu shortcut from the `.msi` had no icon.
- The installer's license page broke the text at every line.
- The RISC-V build, which could not install its libraries.

## [0.0.2] - 2026-10-29

### Added

- A package for every system, each named after the release's version:
  - Windows: an `.msi` installer and a portable zip, for x64, x86 and ARM.
  - macOS: a universal `.dmg`, and the command line tool zipped.
  - Linux: AppImage (with `.zsync` for updaters), Flatpak, `.deb`, `.rpm`,
    an Arch package and a tarball, for Intel and ARM.
  - FreeBSD, Android (one APK for phones and tablets) and iPhone and iPad
    (to sign with your own Apple account).
- `sue-cli.exe` on Windows, the same program for use in a terminal.

### Changed

- On Windows, `Sue.exe` opens without a console window.
- Sue reports the release's version.
- The README links the website and the Discord.

## [0.0.1] - 2026-10-29

The first release.

### Added

- **Sue Paint**, a painting, comics and animation studio laid out the way
  Clip Studio Paint users expect: a smooth, pressure sensitive brush
  engine; layers, folders, masks, correction layers and layer effects;
  vector layers, rulers and perspective; selections; text and balloons;
  manga frames, tones and effect lines; pages and stories; animation
  timelines with onion skin, audio, camera work and movie export; 3D
  layers; filters and adjustments; and Clip Studio Paint's menus,
  palettes and shortcuts.
- **Sue Writer**, a writing studio that publishes to PDF, Word, EPUB and
  more.
- The canvas, the Hub, the File Manager, the Converter, a music player and
  notes, with themes in gray tones.
- Layouts for computers, tablets and phones, with touch gestures and a
  switch between them.
- `.sue` projects: every studio works on the same project, and every
  change can be undone.
- Opens PNG, JPEG, JPEG XL, Photoshop and many more formats.
- Packages for Windows, macOS, Linux, Android and iOS, under the Sayn
  License.

[Unreleased]: https://github.com/MagiGun/sue/compare/v0.0.74...HEAD
[0.0.74]: https://github.com/MagiGun/sue/compare/v0.0.73...v0.0.74
[0.0.73]: https://github.com/MagiGun/sue/compare/v0.0.72...v0.0.73
[0.0.72]: https://github.com/MagiGun/sue/compare/v0.0.71...v0.0.72
[0.0.71]: https://github.com/MagiGun/sue/compare/v0.0.7...v0.0.71
[0.0.7]: https://github.com/MagiGun/sue/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/MagiGun/sue/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/MagiGun/sue/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/MagiGun/sue/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/MagiGun/sue/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/MagiGun/sue/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/MagiGun/sue/releases/tag/v0.0.1
