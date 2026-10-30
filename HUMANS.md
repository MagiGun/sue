# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue already opens its own window on the Hub, where you pick a studio or a
> tool. You can paint in Sue Paint, write and publish in Sue Writer, look at
> and touch up pictures in the Image Viewer and turn files into other kinds
> in the Converter. Everything you change can be undone, and `Ctrl+S` saves
> everything you made as one project file. The big studios (music, video, 3D)
> are added in the releases that follow. Sue reads your `.suerice` file when
> it starts (or when you press Reload in Settings), so close Sue or reload
> after you edit the file by hand.

---

## What Sue is

Sue is **one small app that can become many apps**.

Some days you'll use it for everyday jobs, with its **tools**:

- an **image viewer** to look at pictures large and fix them up quickly,
- a **file converter**: drop a file in, get it out in another format.

When you need more, one click turns Sue into a full **studio**:

| Studio | What you do there |
| --- | --- |
| Sue Paint | Draw, paint, ink comics and animate |
| Sue Writer | Write and lay out letters, reports, novels and scripts |
| Music studio (DAW) | Record, arrange and mix songs (planned) |
| Video editor | Cut, color and caption videos (planned) |
| CAD / 3D modeler | Design parts and build 3D scenes (planned) |
| Code editor (IDE) | Write and run programs (planned) |
| Game engine | Build and play games (planned) |

Every studio works on **the same files at the same time**. A picture you
paint in Sue Paint is the very picture you put in a chapter in Sue Writer.
Nothing is exported, imported or converted between them.

---

## How the screen works

Sue's window has three parts: a **bar along the top**, the **page** under it,
and a thin **status bar** along the bottom. Out of the box Sue is black,
white and grays only, so your own pictures and words are the only color you
see (you can color it yourself in Settings).

```text
┌──────┬────────────────────────────────────────────────────────────┐
│ Sue  │ File  Edit  Layer  ...   (the menus of what is open)       │
├──────┴────────────────────────────────────────────────────────────┤
│  Studios                                                          │
│  ┌──────────────┐ ┌──────────────┐                                │
│  │ Sue Paint    │ │ Sue Writer   │                                │
│  │          [⚲] │ │          [⚲] │                                │
│  └──────────────┘ └──────────────┘                                │
│  Tools                                                            │
│  ┌──────────────┐ ┌──────────────┐                                │
│  │ Image Viewer │ │ Converter    │                                │
│  │          [⚲] │ │          [⚲] │                                │
│  └──────────────┘ └──────────────┘                                │
│  Recent            Saved Projects     Unsaved Work                │
│  ┌──────────────┐  ┌──────────────┐   ┌──────────────────────┐    │
│  │ album  Proj. │  │ album        │   │ Novel   [Recover]    │    │
│  └──────────────┘  └──────────────┘   └──────────────────────┘    │
├───────────────────────────────────────────────────────────────────┤
│ Hub                                    0 entities  Undo 0  Idle   │
└───────────────────────────────────────────────────────────────────┘
```

### The Hub

Sue opens on the **Hub**, a page of boxes: first the **Studios** (Sue Paint,
Sue Writer), then the **Tools** (the Image Viewer, the Converter). Each box
says what it is for. Click one to go there: it comes in over the Hub, and a
studio slides its panels in from the edges. New studios and tools appear in
the Hub by themselves as they are added.

At the bottom right of every box is a **pin**. Pin the studio or tool you
use most and Sue opens straight into it from then on, with no stop at the
Hub. Only one can be pinned: pinning another moves the pin, and clicking the
pinned one again takes it away. `Ctrl+Z` undoes a pin like any other change,
and you can also choose it under Settings, General, Startup.

Under the boxes are three lists, side by side when the window is wide and
one under the other when it is not:

- **Recent**: the files and projects you opened or saved lately, newest
  first, each with when, the folder it is in, and what it is (a project, a
  PNG). Settings, General, Startup says how many it shows.
- **Saved Projects**: every Sue project (`.sue`) you saved or opened, by
  name, with the studio it was saved from.
- **Unsaved Work**: work Sue kept for you by itself. Sue keeps whatever you
  have not saved a minute after you change it, and again when you quit, so
  a crash or a quick Quit loses nothing. Each piece says when it was kept
  and whether it was never saved or is newer than its project file.
  **Recover** (or a click on it) brings it back in the studio it was made
  in; save it to keep it. **Discard** asks once more (Really Discard) and
  then throws it away.

Click a recent file or a saved project to open it where it belongs: a
project opens in the studio it was saved from. Opening a project while you
have unsaved work does not lose that work: it goes to Unsaved Work. Files
that are gone drop off the lists by themselves.

### The Sue button and its menu

At the very top left of the window, always in the same place, is the **Sue**
button (Sue's silhouette). Right after it come the menus of whatever is open:
Sue Paint's File, Edit, Story and the rest, or Sue Writer's, or a tool's. On
the Hub the bar just says Hub.

The Sue button opens the **Sue menu**, for Sue as a whole:

- **Hub**: back to the Hub;
- **Studios** and **Tools**: go straight to any of them;
- **New**: a new picture (Illustration, Print A4, Full HD, Square Post,
  Sprite or a custom size) or a new writing project (a blank document, a
  novel, a short story, a screenplay and more), opened right where it
  belongs;
- **File**: Open File (`Ctrl+O`) and Open Recent, the Hub's recent files;
- **Project**: open, save and save as;
- **Send To**: the thing you are working on, into another studio or tool,
  with no exporting and importing. A picture in Sue Paint goes into the
  Converter or into the text in Sue Writer; a picture in the Image Viewer
  opens in Sue Paint. Studios that come later appear here by themselves;
- **Language**: the language Sue speaks;
- **Layout**: Automatic (what your device is), Desktop, Tablet or Phone,
  for the whole of Sue at once. On a computer, Tablet and Phone show the
  same buttons, bars and sheets as on a tablet or phone, in your window;
  **Phone Shaped** keeps the phone layout to a phone-wide column in the
  middle. Phones and tablets switch between their two layouts. It works in
  the browser too, and Sue remembers it;
- **Open ... at Launch**: pin what is open, as the Hub's pin does;
- the command palette, Settings and Quit Sue.

### The Image Viewer

The **Image Viewer** fills its page with one picture: zoom with the wheel,
fit it or see it at 100%, drag to pan, and step to the project's other
pictures with the arrow keys. Its menus (File, Image, View, Send) and the
toolbar under them do the quick fixes (turn, mirror, crop, resize,
brightness and contrast, saturation and hue, grayscale, invert), each one
`Ctrl+Z` step; save the picture as any picture type; re-encode it as a
lossless JPEG XL; show it in its folder; and send it to the Converter, Sue
Paint or Sue Writer. File, Open Picture opens one with your system's own
Open window.

### The Converter

The **Converter** turns any file into another kind. Add files with **Add
Files** or **Add a Folder** (your system's own Open window), with the Sue
menu's Send To from any studio or tool, or with **Add from the Project** (the
pictures and sounds you have open). For each kind of file in the list a
drop-down says what it becomes:

- pictures: PNG, JPEG, JPEG XL, WebP, TIFF, BMP, GIF, TGA, QOI, ICO or PPM,
  with a new size (scale, width, height, fit inside, exact) and how to
  resample, turning, mirroring, brightness, contrast, saturation, grays
  only, filling transparency with a color you pick on the wheel, and the
  JPEG quality or JPEG XL effort;
- sounds: FLAC, WAV or AIFF, at a new sample rate, mono or stereo, 16 or
  24-bit (or 32-bit float WAV), louder or quieter (or normalized), with the
  start or end cut, faded in or out, sped up or slowed down, or backwards;
- text: tidied (line endings, trailing spaces, tabs), Markdown as an HTML
  page, and tables between CSV, TSV and JSON (JSON can also be re-indented);
- anything at all: Base64 text, a hex dump, a byte map picture, or a plain
  copy.

Choose where the results go (next to each file, or a folder you pick), how
they are named (`{name}-small`, say), and whether to keep both or replace a
file of the same name; the file being converted is never written over. Click
Convert (or `Ctrl+Enter`): big files convert in the background, the list says
where each one went (click it to see it in Explorer, or Finder on a Mac), and
the status bar names the folder. On a phone or tablet, Sue works on a copy of
a file you pick one by one, and nothing can be put next to that copy where
you would see it, so "Next to each file" asks which folder the results go
to before it converts; they land there, named as the status bar says.

### Opening and saving files

Whenever Sue needs a file, or a place to save one, it shows your system's own
Open, Save or folder window, the one every other app uses. Open File (in the
Sue menu, or `Ctrl+O`) and dropping a file anywhere on the window both put
the file where it belongs: a picture opens in the Image Viewer, a Word,
Markdown or text file in Sue Writer, and a `.sue` project replaces what is
open. A file nothing in Sue opens yet is left alone, and the status bar says
so.

Whenever Sue saves or exports a file (a project, a picture from Sue Paint, a
re-encoded JPEG XL, an exported rice), the status bar says the file's name and
the folder it went to. Click that message to see the file, selected, in your
system's file manager.

Every file you open and every project or picture you save goes first in
Sue's one list of recent files: the Hub's Recent, the Sue menu's File, Open
Recent, and Sue Paint's File, Open Recent (its pictures) all show the same
list.

**On a phone or tablet** the system's own picker slides over Sue: on Android
the Files picker, which also reaches Google Drive and other apps' storage,
and on iPhone and iPad the Files browser. Sue works on a copy of a file you
open. On Android, saving that file writes your changes back to the
original, and saving somewhere new asks where first, then every save
(`Ctrl+S` too) goes there. A folder you choose on Android works both
ways: Sue reads the files in it (the Converter's Add a Folder) and each
file Sue makes in it lands there (the Converter's results; Android lets
you choose any folder but the top of Download). iPhone and iPad save
first and then ask where in Files the file goes, each time you save; a
folder you choose there is written to directly. Wherever Sue names such a
file (the status bar's "Saved", the Image Viewer's info row and its Save
over the original question, the Converter's output folder, the Hub's
recent work), it names what
you chose, such as "Picture.png in Download", with its folder where the
system says it. A phone has no file manager for Sue to show files in, so
those messages are not clickable there, and Show in Folder and the
Converter's "show the result" choice are left out.

### The command palette

Press **Tab** (or `/`) for the **command palette**: start typing and it finds
any action by a few letters, such as "open viewer" for the Image Viewer or
"new square" for a new square picture. (In Sue Paint, Tab hides the palettes
instead, as in other painting apps; the palette is in the Sue menu.)

Everything moves on springs: nothing jumps, it glides into place and then
stops. When nothing moves, Sue draws nothing at all.

### Text, everywhere

Every place you type (documents, search boxes, names) works the same: drag
to select, double-click a word, `Ctrl+A` to select all, `Ctrl+C`, `Ctrl+X` and
`Ctrl+V` to copy, cut and paste, and `Ctrl+Z` and `Ctrl+Y` to undo and redo.
Undo is one history for everything: typing, a brush stroke, the Hub's pin or
a Settings slider all go back in the order you did them.

### Settings

The Sue menu's **Settings** (or `Ctrl+,`) opens Settings as a floating
window. A list of categories runs down the left (General, Language,
Appearance & Rice, Display & Graphics, Audio & DSP, Files & Codecs, Studios, Packages & Updates,
Keymaps, MCP), and the right side shows only that category. Type in "Filter
settings..." to search all of them.

Settings fits the screen it is on. On a phone it fills the screen: held
upright, the categories are a list, a tap opens one, and **All settings** at
the top goes back, with each setting's control under its name; on its side,
the categories run down the left. Everything is big enough for a thumb. On a tablet it is a large sheet with the categories at
the left, as wide as the tablet upright and as tall as it on its side. On a
computer it is a window that grows with yours up to a comfortable size and
shrinks into a small one.

Under General, **Open At Launch** is the Hub's pin: the Hub, or the studio or
tool Sue opens straight into. **Recent Files on the Hub** says how many the
Hub's Recent list shows (0 hides it), and **Autosave Every** how soon after a
change Sue keeps unsaved work for the Hub's Unsaved Work (0 turns it off). Under Display & Graphics, **Performance
Preference** sets how smooth and how frugal Sue is (Auto picks for your
machine). **GPU preference** chooses which graphics card draws the window:
Auto uses the one your screen is plugged into, Integrated the power-saving
one and Discrete the fast one. Sue shows which card it is drawing on and
every card it found; the choice takes effect when Sue starts, so press
**Restart Sue** after changing it. **MCP** (connecting Sue to AI tools) is
not yet available.

### Languages

Sue speaks your language. The first time it starts it uses your system's
language when Sue has it, and English otherwise. To change it, open the Sue
menu and choose **Language**: pick a language by its own name, or
**System** to follow your system again. Every studio and tool changes at
once; nothing needs a restart.

A studio or tool can also keep a language of its own: say Sue in English
and Sue Paint in Japanese. Settings, **Language** lists Sue and every studio
and tool; each one is **Follow Sue** unless you choose a language for it.
The same choice sits in each studio's own settings (Settings, Studios, and
Sue Paint's File, Preferences takes you there).

Sue ships in English, with drafts in Spanish and Japanese so far, written
without a native speaker and waiting for one to review them. Anything a
language does not have yet shows in English. Names you type, such as layer
and tool names, stay as you wrote them, and the window's title is always
Sue. CONTRIBUTING.md explains how to add a language or improve one.

---

## Sue Paint: painting and illustration

Click **Sue Paint** in the Hub (or find it with `Tab`) and the page becomes a
painting studio. If you have used a painting app, it will feel familiar.

**The start page.** On a computer, Sue Paint opens on its start page
while no canvas is open (and File, **Start Page** brings it back). Down
the left side:

- **Draw** (the round button) opens the picture or project you worked on
  last, or, when there is none, the New dialog to make a canvas.
- **Projects** shows your pictures, Sue Paint projects and unsaved work as
  cards with pictures, newest first, or by name, as large cards or a
  list. **All**, **Favorite** and **Work** sort them, and the buttons over
  that list add, delete and rename labels of your own. Search by name,
  choose a type (Picture, Project, Unsaved work), tick several cards (or
  **Select All**) to **Move to** a label or **Remove from list** (the
  files stay where they are). The heart makes a card a favorite and `>`
  shows its details. Click a card to open it. These are the same lists
  the Hub shows.
- **Manage Materials** shows every material: Sue Paint's own tones and
  patterns, your pictures and your brush tips, under All materials, by
  kind, Favorite and Recently used, as large or small cards or a list.
  With a canvas open, a click puts a material on it; `+` copies a PNG
  picture in as a material.

At the top right: **Data Transfers** (two arrows) lists the files Sue
opened, saved, exported and copied in this run, with **Clear Completed
Transfers**; **Help** links to tips, this guide, questions, contact and
feedback on the Sue site and GitHub; **Settings** turns dark mode on or off,
shows or changes where your materials are kept, has a Maintenance Menu
(draw thumbnails again, look for materials again, forget projects that are
gone) and shows Sue Paint's version. Nothing on the start page goes
online.

**On a phone or tablet** the start page is laid out for fingers. The top
bar shows Sue Paint with Help and Settings; Projects starts with three big
buttons, **Create new file**, **New from device storage** (a picture
becomes a new canvas) and **Open file**, then your projects, sorted by
label. Along the bottom: Projects, the round **Draw** button and Manage
Materials. Help, Settings and the rest come up over the whole screen.

Draw opens your last project, or the **New canvas** page when there is
none: Illustration, Webtoon, Comic and Animation tabs, each with a row of
templates to swipe through (Create from image, Custom size, your screen's
size; 690 x 20000, 1380 x 20000 and 800 x 10000 px webtoons; A4, A5 and A6
color comics; an animation from a movie, 1:1 or 9:16). Tap one and draw.
**Advanced Settings** opens every setting. Under the templates,
**Continue drawing** lists your recent work, **Import** brings in a
picture, and **See list of projects** goes back to Projects.

Once a canvas is open:

- **Menus along the top,** right after the Sue button: File, Edit, Story,
  Animation, Layer, Select, View, Filter, Window and Help. Everything Sue Paint can do is in them. Items shown
  in gray are planned but not built yet (correcting vector lines,
  materials and a few more), so you can see what is coming.
- **Tools on the left:** pen, pencil, brush, airbrush, eraser, fill bucket,
  gradient, shapes, selections, eyedropper, hand and zoom. Each tool has
  ready-made **sub tools** (a G-pen, a mapping pen, a watercolor brush, a soft
  airbrush) whose size, opacity, hardness, smoothing and pen pressure you can
  tune. Your changes stick to that sub tool. The small graph button beside
  **Brush Size** and **Opacity** opens their dynamics, as in Clip Studio
  Paint: pen pressure, the pen's speed and chance can each change them, down
  to a minimum you choose, along a curve you shape by dragging points (press
  an empty spot to add one, drag one out or double-click it to take it
  away). Brush Size also offers **Specify by size on screen** and **At least
  1 pixel**.
- **Pen Pressure Settings** (File menu): draw on the pad the way you
  normally draw, choose **Check adjusted settings**, and Sue fits the
  pressure to your hand; **Stronger** and **Lighter** fine-tune it, or shape
  the graph yourself. **Done** keeps it for every tool.
- **Clean lines without lag:** pens smooth out the shake of your hand, and
  smooth less the faster you move, so quick strokes never trail behind
  ("Adjust by speed"). When you lift the pen, Sue tidies the line: it removes
  the little hook at the end, turns a few fast mouse points into a smooth
  curve, evens out wobbles while keeping sharp corners sharp ("Correction"),
  thins the end into a point ("Taper"), and lets a quick flick run out into a
  fine tail. Watercolor and gouache brushes pick up the color underneath and
  drag it along ("Color mixing"). All of it is in Tool Property.
- **Colors:** a color wheel, sliders, color sets and the colors you recently
  painted with. `X` swaps your main and sub color, `C` switches to the
  transparent color (brushes then erase); hold `Alt` and click, or
  right-click, to pick a color from the picture. The **Color Set** palette
  has several sets (Default, Grays, Skin Tones, Pastels and your own) in a
  drop-down; the wrench makes, copies and deletes sets and changes the
  swatch size; the arrows import a set (a GIMP palette or a list of color
  codes) and export one; and the buttons under the swatches add the drawing
  color, replace the swatch you picked with it, or delete that swatch. Your
  sets are kept for next time.
- **The canvas in the middle:** tabs for every open picture. The mouse wheel
  zooms, Space and drag (or the middle button) scrolls, and the Hand tool's
  Rotate turns the view. Tilting the view never changes your picture.
- **On the right:** the navigator, then your **layers**: show or hide them,
  change their opacity and blend mode, clip one to the layer below, lock it,
  or lock its transparent pixels so you only paint over what is already there.
  Double-click a name to rename it. The navigator's **Sub View** tab holds
  reference pictures: import them, step through them or pick one from the
  list, zoom and turn the view of one, and click it to pick its colors.
- **Arrange the palettes your way:** drag the edge of a column to make it
  wider, or the line between two palettes to share the height differently.
  Each palette's menu (the three lines at its top right) docks it on the
  other side, floats it in its own window you can move and resize, folds it
  into an icon in a strip at the far right (click the icon to see it), or
  closes it. In a narrow window, the widest palettes fold into the strip by
  themselves and come back when there is room. Drag the top of the timeline
  to make it taller. Window, then Reset Palette Layout, puts everything
  back. `Tab` hides or shows every palette and `Shift+Tab` the menus.
- **Shortcuts:** Sue Paint uses Clip Studio Paint's default keys: the tool
  letters (`P`, `B`, `E`, `G`, `M`, `W`, `I`, `K`, `H`, `R`, `/` and the
  rest), `,` and `.` for the previous and next sub tool, `[` and `]` for the
  brush size, Shift and drag for a straight line, Ctrl, Alt and drag to size
  the brush, Shift and Space to turn the view, Ctrl and Space or Alt and Space
  to click-zoom, Ctrl, Shift and click to pick the layer under the pointer,
  and the usual menu keys (`Ctrl+T`, `Ctrl+U`, `Ctrl+G`, `Ctrl+E`,
  `Shift+Del` to fill and so on). Keys for things Sue Paint cannot do yet say
  so. `F1` or `Alt+I` lists them all. In Sue Paint the command palette is the
  Command button in the status bar, since `Tab` belongs to the palettes.
- **Tabs** show a dot when a picture changed since it was opened or saved.
- **Starting and ending, color jitter:** under Tool Property, the start and
  end of a line can be in pixels, a share of the line, or a fade by a
  length, thinning the line, fading it or both, longer when you draw fast.
  Color jitter changes the color of every dab (brushes that stamp) or of
  each whole stroke.
- **Transform** (Ctrl+T, Ctrl+Shift+T): the selection, or the whole layer,
  in a box. Drag a corner to scale (Shift frees the shape), an edge to
  stretch, inside to move and outside to turn, or type the size and angle;
  Free Transform moves each corner on its own. Enter keeps it.
- **Vector layers** (Layer, New Layer, Vector Layer, or the button beside
  New Raster Layer): lines drawn on one are kept as lines. The eraser's
  **Vector** setting decides what it takes: only where it touches, the
  piece of a line up to where it crosses other lines (the Vector eraser
  does this, so cleaning up crossing lines is one tap), or whole lines.
- **Object** (O): click a vector line to pick it (Shift picks more), drag
  to move it, Delete to remove it. **Select layer** picks the layer under
  the pointer.
- **Correct Line** (Y), on vector layers: drag a line's control points to
  reshape it, or add, delete, corner, widen, fade them and split the line
  there; pinch a stretch of line into shape; brush over lines to smooth
  them, join their ends, or make them thicker or thinner; trace over part of
  a line to redraw it, or to give it your pen pressure. **Remove dust**
  clears the little specks left on a raster layer.
- **Folders** (Ctrl+G puts the current layer in a new folder): hide or fade
  a folder and everything in it follows; its arrow opens and closes it.
  Moving a layer up or down past a folder's edge takes it out or puts it in.
- **Masks** (Layer, Layer Mask): hide part of a layer without erasing it.
  Click the mask beside the layer to paint on it: the pen shows, the eraser
  hides.
- **Canvas** (Edit): change the canvas size around a reference point,
  change the image resolution, or crop to the selection.
- **Symmetry:** the Ruler tool's Symmetrical Ruler; everything you draw
  appears on the other side too, or turned around its middle as many times
  as you set. Ctrl+2 turns it off and on.
- **Rulers** (the Ruler tool, and Layer, Ruler/Frame): a **Linear Ruler**
  keeps lines that start near it straight along it, and **Curve**,
  **Figure** (rectangle or ellipse) and **Ruler Pen** rulers keep them on a
  curve you click out, a shape you drag or a line you draw (Ctrl+1 turns
  them off and on); a **Special Ruler** makes every line parallel, run toward a middle,
  or go round it in circles; a **Perspective Ruler** makes lines run toward
  its vanishing points. Create Perspective Ruler lays one with 1, 2 or 3
  points; or drag along two edges of your drawing that meet in the distance
  and Sue puts a vanishing point where they cross. Drag a point or the eye
  level to move it. Your line picks the direction it starts in, and stays
  exactly on it. Delete Ruler takes them away, Show Ruler hides them, and
  they are saved with the picture.
- **Text** (T): click and type, in any font on your computer, across or in
  vertical columns; click inside a balloon and the words sit in its middle.
  Click the words again to change them.
- **Tone Curve, Levels, Color Balance, Gradient Map** (Edit, Tonal
  Correction): shape the curve by dragging its points, with a live preview.
- **Correction layers** (Layer, New Correction Layer): the same corrections
  as a layer that changes everything under it while leaving those layers
  alone. Hide it or lower its opacity to weaken it, and double-click it to
  change its settings.
- **Fill that stays inside:** the fill bucket's **Close gap** keeps the paint
  inside lines with small breaks, and **Area scaling** spreads it a little
  under the lines so no white edge is left.
- **Blend** (J): Blur softens, Finger Tip smudges the colors along your
  stroke, and Liquify pushes the drawing along, swells it, pinches it or
  twirls it under the brush.
- **Puppet warp** (Edit, Transform): click to put pins on a layer, drag one
  and the drawing bends around the others while keeping its shape; Alt and
  click takes a pin away. OK (or Enter) keeps it, Cancel (or Esc) does not.
- **Comics and manga:** File, New, Comic makes a story of pages, bound on the
  right for manga or on the left. The Story menu adds, inserts, duplicates
  and deletes pages and steps through them, and the **Page Manager** shows
  them side by side as spreads: click one to choose it, double-click to open
  it, move it earlier or later. Export All Pages saves every page.
- **Panels, balloons and effects:** Layer, New Layer, Frame Border Folder
  lays out panels with gutters; what you draw on layers under it shows only
  inside the panels. The Frame Border tool draws more panels, or divides one
  when you drag a line across it. The Balloon tool draws speech balloons and
  their tails as one outlined shape. Filter, Render has focus lines, speed
  lines and screentone, and Layer, New Layer, Tone makes a layer of tone.
- **Animation:** File, New, Animation (or New Animation Cel on any picture)
  starts a timeline. The Timeline palette makes folders and cels, plays and
  stops (with loop), steps frame by frame and sets the frame rate and length.
  Click a frame to go there; double-click to show the current layer from
  there. Onion skin shows the drawing before in blue and after in green;
  Onion Skin Settings choose how many, their colors and how faint they
  get. The **light table** (Animation, Light Table, or the Animation Cels
  palette) shows other layers under the cel you are drawing, to trace
  from: pick the layer, then Register Selected Layer.
  File, Export Animation saves an animated GIF, a movie or every frame as a
  picture.
- **3D layers:** Layer, New Layer, 3D Layer puts a cube, sphere, cylinder,
  cone or plane on the canvas, and File, Import, 3D Data brings in an OBJ
  model. With the Object tool (O), drag to turn the camera, Shift and drag
  to move it, Ctrl and drag to come nearer; Tool Property sets the camera,
  light and whether it draws shaded, as line art or both. Layer, Convert to
  Lines and Tones makes clean line art and tone from it to draw over.
- **Webtoons:** File, New has a tall Webtoon canvas, and File, Export
  Webtoon cuts it into numbered pictures at the height and width you
  choose, ready to upload.
- **Sound for animations:** File, Import, Audio puts a sound on the
  timeline from the frame you are on. It plays along when you play the
  animation; drag its bar to move it, and right-click it to mute or delete
  it.
- **Timeline:** Animation, Timeline, Edit Timeline changes the frame rate
  and length without moving your keys; Export Animation, Animation Cels
  saves every cel as its own picture.
- **Line art from photos and scans:** Filter, Line Extraction turns a
  picture into clean black lines; Convert to Lines and Tones also makes its
  shading into screentone, each on a layer of its own.
- **PDF and EPUB:** File, Export as PDF or Export as EPUB makes a book of
  every page of your comic (manga read right to left), or of one picture.
- **More color palettes:** **Intermediate Color** blends four colors you set
  at its corners, and **Approximate Color** shows the shades near your
  color, as in Clip Studio Paint.
- **Reference layers** (Layer, Layer Settings, Set as Reference Layer): the
  fill and auto select can look only at them, so you fill on one layer
  between the lines of another. **Rasterize** turns a vector layer into an
  ordinary one.
- **Selections:** the Selection tool also has **Polyline**, **Selection
  pen** and **Erase selection** (paint what to select or leave out). The
  Select menu grows, shrinks and softens a selection, selects by color,
  keeps a selection as a layer and takes it back, and has **Quick Mask**
  (paint the selection in red, then turn it back). A bar of handy buttons
  (the Selection Launcher) sits under every selection, as in Clip Studio
  Paint.
- **Layer Property effects:** give a layer a **Border effect** (an outline
  of any thickness and color round what is on it), a **Tone** (halftone
  dots at the frequency and angle you choose), a **Layer color**, or show it
  in gray or monochrome (**Expression color**). The drawing itself is not
  changed, so effects can be turned off again.
- **Right-click** a layer for the layer menu, a sub tool to duplicate,
  delete or reset it, a canvas tab to save or close it, a material to paste
  or delete it, or a swatch to add, replace or delete a color. On the canvas
  a right-click picks a color, as in Clip Studio Paint.
- **Materials** (the Material tab beside Color Set, or Window, Material):
  screentones and patterns ready to use, your own pictures and your brush
  tips. Double-click one to put it on a new layer (tones and patterns in
  your drawing color). Edit, Register Material, Image keeps the current
  layer there for every project. Settings (or the start page's Settings)
  can keep them in a folder of your choice.
- **Camera folder** (Animation, New Animation Layer, Camera Folder): the
  layers in it are filmed by a camera that can move, zoom and turn over the
  timeline. With the Object tool (O) on it, drag its red frame (inside to
  move, a corner to zoom, outside to turn) or type its position, scale and
  rotation, and add a keyframe; between keyframes it glides. Playback and
  exports show the animation through it.
- **Timelapse** (File, Timelapse, or Record timelapse in New): while it
  records, Sue keeps a picture of the canvas after every change. Export
  Timelapse makes a video (or a GIF) of how you drew it, fitted to the
  length you choose or with every frame.

`Ctrl+N` makes a new canvas: pick a preset or a paper size, a unit, a
resolution and a paper color. `Ctrl+O` opens a PNG, JPEG or JPEG XL picture,
and File, then Export Single Layer, saves your picture as one again. **Save
As** (`Ctrl+Shift+S`) asks for a type too: a Sue project keeps every layer,
and PNG, JPEG XL, JPEG, WebP, TIFF, BMP, TGA, GIF or QOI saves the picture
flattened. Both
use your system's own Open and Save windows, showing only the pictures Sue
Paint can read. `Alt+I`
lists every shortcut as you have set it, and **Change Shortcuts** takes you
to Settings to set them.

Your pictures are part of your project: `Ctrl+S` saves them with everything
else, and every stroke can be undone with `Ctrl+Z`. File, **Back to the Hub**
(or the Sue menu's Hub) leaves Sue Paint; your pictures wait for you there
the next time you open it.

Going in and out is smooth: opening Sue Paint, its page comes in over the Hub
and the palettes slide in from the edges around the picture. Leaving, they
slide back out and the Hub comes back. How fast this happens follows the
Motion settings (Settings, Display & Graphics): a faster animation speed
shortens it, and "Instant transitions" makes it immediate.

---

## Sue Writer: writing and publishing

**Sue Writer** is where you write, from a letter to a trilogy. Open it from
the Hub or the Sue menu's Studios, or with the Sue menu's New, Writing, which
starts a project from a template: a blank document, a novel, a short story, a
screenplay, a non-fiction book, an essay or paper, a poetry collection, a
letter or a report.

It looks like a word processor: pages that fit the window and one short
toolbar, with what Word and LibreOffice Writer keep at hand: Undo and Redo,
paragraph styles, every font installed on your computer, sizes, bold, italic,
underline, colors, highlights, alignment, lists and indents. Everything else
is one menu away, grouped the way LibreOffice groups it: Format, Text for
strikethrough, raised and lowered text and small capitals, then Align Text,
Line Spacing and Lists. Draft and Page sit at the bottom right beside the
zoom. The usual keys work: `Ctrl+B`,
`Ctrl+I`, `Ctrl+U`, `Ctrl+E` to center, `Ctrl+J` to justify, `Ctrl+Alt+1` for
a heading, `Ctrl+Enter` for a new page, `Ctrl+K` for a link, `Ctrl+F` to find
and `Ctrl+H` to replace. Enter after a heading goes on in body text, Enter on
an empty list item ends the list, and Tab moves a list item in. In a
screenplay, Enter and Tab move between scene headings, action, characters,
parentheticals and dialogue, each at its proper indent, typed in capitals
where it should be. Quotes turn curly as you type (Settings, Studios).

Typing feels smooth: the caret glides to its new place instead of jumping,
whether you type, delete, move with the arrow keys, Home and End, click
somewhere else, or the text wraps to a new line or page. It blinks only while
it rests, and after ten seconds without a move it stays lit, so an idle
window draws nothing. **Smooth Caret** (Settings, Display & Graphics, Motion)
turns the glide off, and **Caret Glide Time** sets how fast it is; older
machines skip it.

For long work it is also a writing studio:

- **The binder** on the left holds your chapters and scenes, research and a
  trash. Click a folder to read all its scenes one after another; drag to
  reorder; right-click for the rest.
- **The corkboard** shows each item as an index card with its synopsis,
  label color and status; **the outliner** lists them in a table.
- **The inspector** (View, Inspector) keeps each scene's synopsis, notes,
  label, status, word target and keywords, and **snapshots**: take one
  before a big rewrite and put it back whenever you like.
- **Targets**: a word target for the book and for each scene, and a daily
  goal; the status bar counts as you go, and Writing Statistics shows the
  words you wrote each day.
- **Composition mode** (`F11`) leaves nothing but your words on a dark page,
  the line you are on kept in the middle. `Esc` comes back.
- **Find and replace** across one scene or the whole project, matching
  case or whole words, or with **regular expressions**: `\d+` finds numbers,
  `^Chapter` finds lines that start with it, and in the replacement `$1`
  puts back what the first group in brackets caught (`(\w+), (\w+)` with
  `$2 $1` turns "Smith, John" into "John Smith").
- **Change Case** (Format, Text, Change Case): Sentence case, lowercase,
  UPPERCASE, Capitalize Every Word and tOGGLE cASE, for the selection or the
  word you are in, keeping its formatting.

**Tables** come from Table, Insert Table (`Ctrl+F12`): choose the columns
and rows, and whether the first row is a heading. Click in a cell and type;
`Tab` goes to the next cell and, in the last one, adds a row; `Shift+Tab`
goes back, and the arrow keys move between cells and out of the table. The
Table menu inserts rows and columns, deletes them or the whole table, turns
the heading row on and off, and converts text with tabs or commas into a
table and back. A table that runs onto the next page breaks between rows and
repeats its heading row at the top. Tables go into every format you compile
to, PDF and Word included.

**Footnotes** (Insert, Footnote, `Ctrl+Alt+F`) print at the foot of the
page that calls them, under a short rule; **endnotes** (Insert, Endnote,
`Ctrl+Alt+D`) are numbered i, ii, iii and gathered under Notes at the end.
Word and OpenDocument files keep both kinds.

`Shift+Enter` starts a new line without starting a new paragraph (for
addresses and verse); it keeps its place in every format Sue writes.

Format, Text also has **All Capitals** (shown and printed in capitals,
kept as typed) and **Grow Font** and **Shrink Font** (`Ctrl+]` and `Ctrl+[`,
along the usual sizes). Insert, Formatting Mark puts in a non-breaking space
(`Ctrl+Shift+Space`), a non-breaking hyphen, a soft hyphen, a narrow
non-breaking space or a zero width space.

**Spelling** is checked as you type: a misspelled word gets a red wavy
underline (not the word you are still typing). Right-click it for what you
may have meant, **Ignore All** for this sitting, or **Add to Dictionary**,
which keeps the word in the project's own dictionary. Review, Next
Misspelling (`F7`) goes to the next one, and Review, Check Spelling While
Typing (or Settings, Studios, Sue Writer) turns the underlines off. English
ships with Sue; words in other alphabets are left alone for now.

**Comments** are in the Review menu. Select some words and choose New
Comment (`Ctrl+Alt+C`); type it and press `Ctrl+Enter`. The words are shaded,
and Review, Comments shows every comment beside the text: the words it is
about (click them to go there), who wrote it and when, its replies, a box to
reply in, Resolve and Delete. Previous and Next Comment step through them.
Comments carry the project's author (Project Settings). They go into Word
and OpenDocument files as real comments with their replies, and into web
pages as notes on the words.

**Track changes** (Review, Track Changes, Record Changes, `Ctrl+Shift+C`)
keeps every edit for someone to look over: what you type is underlined in
blue, and what you delete stays, struck through in red, instead of
disappearing (deleting something you typed while recording takes it out for
good). Previous and Next Change step through them; Accept Change keeps one
and Reject Change undoes it, or Accept All and Reject All do every one at
once. Word files keep the changes with who made them and when, and web pages
show them; PDFs and the other formats print the text with every change
accepted.

A **table of contents** comes from Insert, Table of Contents: give it a
title and how many heading levels it lists. It shows every heading with its
page and a dotted leader, set in by level, and it keeps up by itself as you
write, so there is nothing to update. Click an entry to go to its heading. It
prints in PDFs with the right page numbers, and goes into Word (as Word's
own table of contents, which Word fills in when it opens), OpenDocument, web
pages, e-books, RTF, Markdown and plain text.

**Headers and footers** come from Insert, Header and Footer: each has a
left, a middle and a right part, and the buttons below put in the page
number, the page count, the title, the author or the date, which every page
fills in (`Page {page} of {pages}`). The footer holds the page number until
you change it. **Different first page** leaves a title page without them.
They show on the pages as you write, print in PDFs, and go into Word,
OpenDocument and RTF files as live page numbers.

When you are done, **Compile** (`Ctrl+Shift+E`) puts the book together, with
chapter titles from your folders, scene breaks and a title page, and saves it
as a PDF (set in your fonts, with bookmarks and page numbers), a Word
document, an OpenDocument file, an EPUB e-book, a web page, RTF, Markdown or
plain text. `Ctrl+P` goes straight to PDF.

**Sue Writer on a phone** gives the screen to your words. The text flows to
the width of the screen at a size you can read (View, Page View shows the
pages instead). The top bar has the Sue button, a menu button with every
menu, Undo, Redo, the binder and Find. The formatting is a row of big
buttons at the bottom (style, bold, italic, underline, alignment, lists,
indents, font, size and colors; swipe it sideways for the rest), so it sits
right above the keyboard while you type, and tapping one keeps the keyboard
up. When the keyboard comes up, the line you are writing moves into sight
above it, on Android phones and on iPhones alike. The binder covers the
page when you open it and closes when you pick a scene. **On a tablet** the
toolbar's buttons are larger, for fingers.

Sue Writer works with the rest of Sue. Pictures from Sue Paint go straight
into your text with the Sue menu's Send To. Open a Word, OpenDocument
(LibreOffice), Markdown, HTML or plain text file anywhere in Sue and it comes
into Sue Writer, formatting and all; tables come in as tables, heading rows
and column widths kept, and an OpenDocument table of contents comes in as
one.
Everything you do can be undone, and it all saves inside your project.

---

## Projects: one file for everything

Press `Ctrl+S` to save your work as a **project**, a single `.sue` file. It
holds everything you made in every studio and tool: your documents, pictures,
sounds and whatever else, and how they are linked. The first time, Sue asks
where to save (your system's own Save window); after that `Ctrl+S` just
saves. The Sue menu's Project, Save Project As picks a new file. Open a
project like any other file (Open File, `Ctrl+O`, or drop it on the window)
and everything comes back exactly as you left it.

A few things make projects safe to keep for years:

- **Nothing is lost.** If a project holds something this copy of Sue cannot
  show yet (made by a newer Sue, or by a feature you have not installed), Sue
  keeps it untouched and saves it back as it was. A project saved by an older
  Sue, with its infinite canvas, still opens: everything in it comes back,
  and the canvas layout is kept in the file as it was.
- **Links survive.** Everything has a permanent name inside the project, so a
  clip on the timeline, a picture on a page or a link in a note still points at
  the right thing after you save, reopen, copy or paste.
- **Damage is caught.** Each part of the file carries a checksum, so a damaged
  file says so instead of opening wrong.

---

## Where Sue is heading

Sue is meant to grow into the one app for nearly every kind of creative and
technical work: writing, spreadsheets, slides, books, PDFs and notes; design,
photos and video; music, sound and singing voices; 3D, animation and a game
engine called **Noelle**; and engineering drawings for buildings, machines,
heating, plumbing and electrics. Many of these are only ideas today, and some
may never ship. What is already in place is the shared foundation they will
all stand on, so that whatever arrives works with everything else: one kind of
project file, one undo history, one way to open and save every format, one set
of presets, and units, colors and timing that every part of Sue agrees on. The
engineers' map of how each idea fits is in `HUMANZ.md` §8.

---

## One app on every device

Sue runs on **Windows, Linux, macOS, Android and iOS**, and you can **try it in
your browser**. It is the same app with the same files and the same tools on
each.

Everywhere it is laid out the same way: the Sue button at the top left with
the menus after it, the Hub or a studio or tool filling the page, and the
status bar at the bottom.

- **On a computer**, you work with the mouse and keyboard.
- **On a tablet**, the top bar is taller, so its buttons are easy to tap, and
  a pen gets pressure-sensitive strokes.
- **On a phone**, the top bar is taller still, for your thumb, and the Hub
  shows its boxes two by two with big pins.

On phones and tablets the on-screen keyboard never covers Sue: when it comes
up, Sue fits itself into the space above it, and takes the whole screen back
when it goes away. On an iPad, a floating or split keyboard leaves Sue as it
is.

**Sue Paint on a tablet** keeps the menus, the tool strip and the Layer
palette, and folds the other palettes into icons at the right edge: tap one to
pop it over the canvas. **On a phone** the canvas fills the screen. The top
bar has the Sue button, then Sue Paint's menus, Undo and Redo, the layers,
the color, the tool settings and the timeline; the bottom bar has the tools
you reach for most, **More** for every tool, and your color. Palettes come up
from the bottom as sheets, one at a time; tap above a sheet to put it away.

Everywhere, **two fingers** move, zoom and turn the canvas, a quick
**two-finger tap undoes** and a **three-finger tap redoes**, as in Clip Studio
Paint.

**Switching layouts.** The Sue menu's **Layout** (or Settings, Display,
**Interface**) chooses Automatic (the device decides), Desktop, Tablet or
Phone. Sue Paint's Window, Interface menu does the same, and on a phone the
row at the top of the menus does too. So a phone can use the full desktop
layout, and a computer can try the touch ones. With a mouse, the wheel
zooms, `Shift` and the wheel turn the canvas, and the middle button or
`Space` and a drag move it, just as two fingers do; `Ctrl+Z` and `Ctrl+Y`
stand in for the two- and three-finger taps.

**Getting Sue.** Each release has a package for every system: for Windows a
setup program (`-setup.exe`, the easiest), the same installer as an `.msi`
(for managed PCs) or a portable zip, for 64-bit, 32-bit and ARM PCs; a
`.dmg` for macOS; FreeBSD; for Linux (Intel, ARM and RISC-V) an AppImage
(runs almost anywhere), a
Flatpak, a `.deb` (Debian, Ubuntu, Mint), an `.rpm` (Fedora, openSUSE), a
`.pkg.tar.zst` (Arch, CachyOS, Manjaro: `sudo pacman -U` it) or a tarball;
one APK for every Android phone and tablet, and a package for iPhone and iPad
to sign with your own Apple account. Sue is not code-signed yet, so Windows
may warn before running a download ("Windows protected your PC"): More info,
then Run anyway; or install it with Scoop (`scoop install
https://github.com/MagiGun/sue/releases/latest/download/sue.json`), which
does not warn. Opening and saving on Android and iOS go through the
system's own picker, which needs no permission.

**Trying Sue in your browser.** Open https://magigun.github.io/sue/try/ in a
recent Chrome, Edge, Firefox or Safari: Sue downloads once (about 5 MB) and
opens with nothing to install. It is the same Sue, with the Hub, Sue Paint,
Sue Writer, the Image Viewer and the Converter.

- **Opening** a file brings up the browser's own file chooser, or drop files
  from your computer onto the page.
- **Saving** asks where to save in Chrome and Edge, and later saves go to that
  same file. Other browsers save a download each time.
- **Settings and unsaved work** stay in the browser, for this site only, until
  you clear its site data; the Hub offers to recover unsaved work after a
  reload. Files you opened are not kept, so open them again.
- **Typing** works from a keyboard. Input methods that compose text (such as
  for Japanese or Chinese) do not reach Sue yet.

What the browser version leaves out, because a web page cannot do it, or
because it runs as plain files with no server behind it:

- **Your installed fonts.** Text uses Sue's own fonts.
- **Typing on phones and tablets.** The on-screen keyboard does not come up,
  so use a computer to write.
- **Work in the background.** The Converter and other long jobs make the page
  wait until they finish.
- **Show in Folder and opening other programs.**
- **Files outside the browser.** Sue sees only the files you choose or drop;
  your other files and folders stay private, and anything you do not save
  or download stays only in the browser.
- **Updates and packages.** The page is always the newest Sue from `main`;
  reload to get it.

---

## Built-in package and offline AI manager

Sue has its own **app store**, called the package manager. From it you can add:

- **new tools and studios**,
- **file format support** (codecs: the code that teaches Sue to open a new kind
  of file),
- **looks and themes** (rices, see below),
- **AI models that run 100% on your own device**, for example:
  - voice-to-text transcription,
  - splitting a song into vocals, drums, bass and other instruments,
  - sharpening and enlarging images,
  - local helpers for coding and for understanding pictures.

**No cloud subscriptions and no uploading your files to someone else's
computer.** Everything runs on your machine, and works with the internet off.

Updates are **all-or-nothing**. If an update fails or doesn't work right, Sue
goes back to the version you had, as if nothing happened. Every package is
checked to make sure it hasn't been tampered with before it is installed.

---

## Interactive ricing (`.suerice`)

"Ricing" is the hobby of customizing exactly how your setup looks and feels.
In Sue you can change **every border, gap, color, font, animation, spring and
keyboard shortcut**.

All of it lives in a plain text file ending in **`.suerice`**. You can edit it in
any text editor, but you never have to. Sue reads the file and builds the
controls for it in **Settings**:

- on/off options become **switches**,
- numbers with a range become **sliders** with their value beside them,
- colors become **swatches**: click one and a color wheel opens next to it.
  Turn the ring for the hue, pick in the square for how strong and how light,
  slide the bar for how see-through, or click one of the grays below. You
  never type a color code. Letting go of a drag is one `Ctrl+Z` step,
- short lists of choices become **side-by-side buttons**, longer ones
  **drop-downs**,
- shortcuts become **key caps**, one for each set of keys: click one and
  press the new keys, `×` takes one away and `+` adds another, as many as
  you like. Sue's own shortcuts and every studio's (Sue Paint's, tools and
  brush keys included, Sue Writer's and the Image Viewer's) are all under
  Keymaps, and the menus show what you chose.

**Appearance & Rice** also offers ready-made themes (Void, Graphite, Paper and
Gray) that change every color at once, and one `Ctrl+Z` brings yours back.

Each studio can wear its own theme (Settings, Studios). Sue Paint starts in
**Gray**, the mid grays painting apps use so your colors read true. Set it to
**Sue theme** and it follows Sue's own colors, so changing them changes every
studio that uses the Sue theme at once; pick another preset; or choose
**Custom** and color Sue Paint with its own color wheels. The side panels
always keep Sue's theme. They are
all black, white and gray: Sue's look stays out of the way of your work. A
color you pick yourself stays; only the tinted colors that very old releases
shipped with are reset to the monochrome default (Sue keeps a copy of your old
file).

A line like this in your `.suerice` file:

```toml
padding = 8   # [2..24] Space inside bars, pills and controls (px)
```

shows up in Settings as a slider from 2 to 24 labelled "Padding", with the
description "Space inside bars, pills and controls (px)" under it. Move the
slider and the file updates. Edit the file and the slider moves the next time
Sue reads it. You can share your `.suerice` file with friends or download other
people's looks from the package manager.

---

## Runs on anything

Sue is built to be **light when it can be and powerful when it should be**:

- When nothing on screen is changing, Sue **draws nothing**, so it uses next to
  no processor or graphics power. Old laptops stay cool and batteries last.
- On older or low-power machines, Sue skips animations and jumps straight to
  the end result, so it stays quick.
- On phones in battery-saver mode, Sue lowers its frame rate to save power.
- On high-end machines, Sue uses all of it: smooth 120 Hz and faster motion on
  springs, at the full refresh rate of your screen.
- On computers with two graphics chips, Sue draws with the one your screen is
  plugged into, so an idle window really stays idle. You can pick the other
  chip under **Display & Graphics** in Settings; the change applies the next
  time Sue starts.

Sue picks the right mode for your device by itself. You can always change it
in Settings.
