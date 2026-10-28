# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue v1.0 "Faustian Enigma" already opens its own window with the Hub, where
> you pick a studio, over the infinite canvas described below. You can write,
> paint in Sue Paint, listen, split and level audio, view pictures and
> raw bytes, line things up on the timeline, browse files, play music, convert
> files and keep notes. Everything you change can be undone. Your quick note is
> saved as you go, and `Ctrl+S` saves everything on the canvas as one project
> file. The big studios (music, video, 3D) and a web browser built into the
> window are added in the releases that follow. Sue reads your `.suerice` file when it
> starts (or when you press Reload in Settings), so close Sue or reload after you
> edit the file by hand.

---

## What Sue is

Sue is **one small app that can become many apps**.

Most days you'll use it for everyday jobs:

- a **fast file explorer** for browsing your folders and drives,
- a **music and media player** for your MP3s and videos,
- a **notes app** for quick thoughts and lists,
- a **file converter**: drop a file in, get it out in another format.

When you need more, one click turns Sue into a full studio:

| Studio | What you do there |
| --- | --- |
| Document processor | Write and lay out letters, reports, books |
| Vector & raster studio | Draw, paint, edit photos, design logos |
| Music studio (DAW) | Record, arrange and mix songs |
| Video editor | Cut, color and caption videos |
| CAD / 3D modeler | Design parts and build 3D scenes |
| Code editor (IDE) | Write and run programs |
| Game engine | Build and play games |

Every studio works on **the same files at the same time**. Put a song on the
timeline and it is the same song you see on the canvas. Nothing is exported,
imported or converted between them.

---

## How the screen works

Sue's window is one big **infinite canvas**: a dark board with a faint dot grid
that goes on forever in every direction. Everything you work on sits on it as a
card. Out of the box Sue is black, white and grays only, so your own pictures
and words are the only color you see (you can color it yourself in Settings).

```text
┌───┬──────────────────────────────────────────────────────────┬───┐
│ F │               [ Hub  Command (Tab) ]                     │ I │
│ P │                                                          │ H │
│ C │       ┌ Welcome ──────────────────────────────┐          │   │
│ N │       │ # Welcome to Sue                      │          │   │
│   │       │ text you can type                     │          │   │
│   │       │                                       │          │   │
│   │       └───────────────────────────────────────┘          │   │
│ ⚙ │                                                          │   │
├───┴──────────────────────────────────────────────────────────┴───┤
│ Hub │ Canvas  Welcome              1 entity  Undo 0  Balanced  Idle │
└──────────────────────────────────────────────────────────────────┘
```

### The Hub

Sue opens with the **Hub** in front of the canvas: one card for each place
you can work. **Canvas** is the endless surface with every file of your
project; **Sue Paint** is the painting studio, and more studios join it as
they are built. Click one to go there, or press `Esc` to stay on the canvas.
The **Hub** button at the top of the canvas and at the far left of the status
bar (also inside every studio) brings it back. If you would rather open
straight into the canvas or a studio, pick it under Settings, General,
Startup.

### Moving around

- **Zoom** with the mouse wheel, from 10% to 3200%. The point under the pointer
  stays put, so you zoom right into what you are looking at.
- **Pan** by dragging with the middle mouse button, or hold **Space** and drag.
- Everything moves on springs: nothing jumps, it glides into place and then
  stops. When nothing moves, Sue draws nothing at all.

### The Sue button

At the very top left, in the canvas and in every studio, is the **Sue**
button. It opens the Sue menu, a round menu like the right-click one, made
for whatever you are working on. It leads to:

- the open studio's own wheel (in Sue Paint: new, open, import as a layer,
  export, reset the palettes, shortcuts);
- **Send To**: the thing you are working on, into another studio or tool,
  with no exporting and importing. A picture from Sue Paint goes onto the
  canvas or into the Converter; a song goes to the Player; a picture file on
  the canvas opens in Sue Paint; any file shows in the File Manager. Studios
  that come later (a video editor, say) appear here by themselves;
- **File**: a new file, open a file, the File Manager, the Converter, adding
  music;
- **Project**: open, save and save as;
- **Studios**: the Hub, the canvas and every studio;
- the command palette and Settings.

### The top pill

A floating bar at the top of the canvas with just two buttons:

- **Hub** shows every studio you can choose.
- **Command (Tab)** opens the command palette.

To work on a card, just click it. New files are made from the marking menu
(right-click), described below.

### The selection pill

Click a card and a second floating bar springs up right above it, with only the
tools that card needs:

- **Documents:** H1, H2, Bold, Monospace, a smaller or larger text size, and
  Project to Timeline. Headings and bold stay as marks in the text (`#`, `**`),
  so the text is always plain and portable.
- **Audio:** Play or Pause, Gain down and up, Split (at the playhead, or in the
  middle), Normalize and Open in Timeline. None of these change the recording
  itself: the two halves of a split share the very same sound.
- **Pictures:** 1:1 Zoom (one picture pixel per screen pixel), Pixel Grid,
  Re-encode .jxl and Inspect Hex.

Click empty canvas and the pill fades away. Drag a card by its title to move
it, or by its bottom right corner to resize it. Double-click a title to switch
to that card's mode.

### The marking menu and the command palette

**Right-click** anywhere on the canvas: a round menu grows around the pointer
with New File, Open File, Sue Paint, Hub, Command Palette and Reset Camera. Click a slice, or hold the right button, drag toward a slice and let
go.

**New File** opens a second wheel with the kinds of files Sue makes:
Document, Picture, Audio and Code. Each of those opens a third wheel with the
exact kind: a blank document, notes, a to-do list, a letter or a report; an
illustration, a print page, a Full HD screen, a square post, a transparent
sprite or a custom size; a tone or silence; a Rust, Python,
JavaScript, HTML, CSS, JSON or shell file. The new file appears where you
right-clicked. A new picture opens straight in Sue Paint. Click the middle of a
wheel to go back to the one before.

Press **Tab** (or `/`) for the **command palette**: start typing and it finds
any action, mode or tool by a few letters, such as "new notes" for a new
notes document.

### The timeline

Project to Timeline (or Toggle Timeline in the command palette) slides the
**timeline** up from the bottom. It has a Document Track, a Paint/Visual Track and an Audio Track. Each
block on it *is* the card on the canvas, not a copy. Drag blocks to move them,
click the ruler to move the playhead, and press **Space** (or the Play button)
to play: audio blocks sound as the playhead passes them.

### Drawers on the sides

The thin **rails** at the far left and far right hold one icon per tool. Click
one to slide its drawer open; click it again to tuck it away.

- **File Manager** (left): your folders, in tabs (`+` opens another tab on the
  same folder, `×` closes one), each with its own Back and Forward. Show a
  folder as a **list** (name, how long ago it changed, size) or as **cards**,
  where pictures show a small preview. Sort by name, date, size or kind.
  Search the folder as you type, or turn on **Subfolders** to search
  everything under it. The path bar shows the folder you are in: copy it,
  or type any folder and press Enter to go there. Click a file to select it
  (its full path appears, with Copy Path, Rename, Show in Explorer and
  Open); double-click to open it on the canvas. **Save Folder** keeps a
  folder under Saved, next to your project, home folder, Desktop,
  Documents, Downloads, Pictures, Music, Videos and drives. **New Folder**
  makes one, and **Open in Explorer** shows the folder in your system's file
  manager. "Open the large File Manager" shows it all in a big window.
  Hidden system clutter stays hidden.
- **Player** (left): your music library (MP3, FLAC, WAV, Ogg, M4A and more),
  playing while you work anywhere in Sue. It works like the music apps you
  know: the song playing with its cover and a heart to like it; Shuffle,
  Previous, a big Play and Pause, Next and Repeat (the queue, or one song);
  a bar to jump around the song; and the volume with a mute button. Browse
  **Songs**, **Albums** (made from the songs' tags, or put together by hand
  with New Album), **Artists**, **Liked** songs, **Playlists** and the
  **Queue**, and search them all by title, artist, album or genre. Each
  song's `+` (or a right-click) plays it next, adds it to the queue or to a
  playlist, likes it, or shows it in Explorer. Playlists and your own albums
  can be renamed, given a cover picture, reordered and deleted. Add songs
  with **Add Music** or, in the File Manager, pick a song or a whole folder
  and click **Add to Player**. Your library is yours alone: it is kept by
  Sue, not in your projects. Under Settings, Audio & DSP, Player you can
  turn on shuffle and repeat, going on to the next song, evening out
  loudness between songs, loading the next song early, and cover pictures.
  "Open the Large Player" shows it in a big window with a sidebar, and the
  command palette has Play or Pause, Next Song and Previous Song.
- **Convert** (left): turn any file into another kind. Add files with **Add
  Files** (one file or a whole folder, through the File Manager), with the
  File Manager's **Convert** button, or **Add from the Canvas**. For each
  kind of file in the list a drop-down says what it becomes:
  - pictures: PNG, JPEG, JPEG XL, WebP, TIFF, BMP, GIF, TGA, QOI, ICO or PPM,
    with a new size (scale, width, height, fit inside, exact) and how to
    resample, turning, mirroring, brightness, contrast, saturation, grays
    only, filling transparency with a color you pick on the wheel, and the
    JPEG quality or JPEG XL effort;
  - sounds: FLAC, WAV or AIFF, at a new sample rate, mono or stereo, 16 or
    24-bit (or 32-bit float WAV), louder or quieter (or normalized), with
    the start or end cut, faded in or out, sped up or slowed down, or
    backwards;
  - text: tidied (line endings, trailing spaces, tabs), Markdown as an HTML
    page, and tables between CSV, TSV and JSON (JSON can also be
    re-indented);
  - anything at all: Base64 text, a hex dump, a byte map picture, or a
    plain copy.

  Choose where the results go (next to each file, or a folder you pick),
  how they are named (`{name}-small`, say), and whether to keep both or
  replace a file of the same name; the file being converted is never
  written over. Click Convert: big files convert in the background, the
  list says where each one went (click it to see it in Explorer), and the
  status bar names the folder.
- **Notes** (left): jot something down. Notes save themselves. Above the
  note: Undo and Redo, **Save As** (to a text file, through the File
  Manager), **Open** (put a text file into the note; Undo brings the old note
  back), Copy All, Clear, and smaller or larger text; below it, how many
  words and characters it has. Drag to select past the top or bottom of the
  note and it scrolls along, so the selection keeps growing.
- **Inspector** (right): the details of the selected card, and a place to
  change them: type a new name; change its place and size on the canvas
  (drag a number or type it); change its values, such as a document's text
  size or a sound's gain; edit a document's or code's text right there; and
  change, add or remove the facts of its file, such as a song's title or a
  picture's notes. It also shows which views it has (Document, Timeline and
  so on). Every change is one `Ctrl+Z` step.
- **History** (right): every change you made, newest at the top, as a tree you
  can click to go back (or forward again) to any moment.

The rails and drawers stay with you inside every studio too, so your files,
notes and history are always one click away.

You can also drop any file anywhere on the window to put it on the canvas.

Whenever Sue saves or exports a file (a project, a picture from Sue Paint, a
re-encoded JPEG XL, an exported rice), the status bar says the file's name and
the folder it went to. Click that message to see the file, selected, in your
system's file manager.

### Text, everywhere

Every place you type (documents, code, notes, search boxes) works the same: drag
to select, double-click a word, `Ctrl+A` to select all, `Ctrl+C`, `Ctrl+X` and
`Ctrl+V` to copy, cut and paste, and `Ctrl+Z` and `Ctrl+Y` to undo and redo.
Undo is one history for everything: typing, moving a card, a brush stroke or a
Settings slider all go back in the order you did them.

### Settings

The gear at the bottom of the left rail (or `Ctrl+,`) opens **Settings** as a
floating window. A list of categories runs down the left (General, Appearance &
Rice, Display & Graphics, Audio & DSP, Files & Codecs, Packages & Updates,
Keymaps, MCP), and the right side shows only that category. Type in "Filter
settings..." to search all of them.

Under Display & Graphics, **Performance Preference** sets how smooth and how
frugal Sue is (Auto picks for your machine). **GPU preference** chooses which
graphics card draws the window: Auto uses the one your screen is plugged into,
Integrated the power-saving one and Discrete the fast one. Sue shows which card
it is drawing on and every card it found; the choice takes effect when Sue
starts, so press **Restart Sue** after changing it. **MCP** (connecting Sue to
AI tools) is not yet available.

---

## Sue Paint: painting and illustration

Click **Sue Paint** in the Hub (or find it with `Tab`) and the space between the
side rails becomes a painting studio. If you have used a painting app, it will feel
familiar:

- **Menus along the top:** File, Edit, Story, Animation, Layer, Select, View,
  Filter, Window and Help. Everything Sue Paint can do is in them. Items shown
  in gray are planned but not built yet (comic pages, animation, vector and text
  layers, rulers and a few more), so you can see what is coming.
- **Tools on the left:** pen, pencil, brush, airbrush, eraser, fill bucket,
  gradient, shapes, selections, eyedropper, hand and zoom. Each tool has
  ready-made **sub tools** (a G-pen, a mapping pen, a watercolor brush, a soft
  airbrush) whose size, opacity, hardness, smoothing and pen pressure you can
  tune. Your changes stick to that sub tool.
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
  The timeline and Animation Cels already carry their buttons, waiting for
  animation.

`Ctrl+N` makes a new canvas: pick a preset or a paper size, a unit, a
resolution and a paper color. `Ctrl+O` opens a PNG, JPEG or JPEG XL picture,
and File, then Export Single Layer, saves your picture as one again. Both
open the Sue File Manager, showing only the pictures it can read; type a file
name to save, or click System Dialog to use your system's own window
instead. `Alt+I`
lists every shortcut.

Your pictures are part of your project: `Ctrl+S` saves them with everything
else, and every stroke can be undone with `Ctrl+Z`. When you click **Back to
Canvas**, the picture is waiting on the canvas as a card; double-click it (or
use "Open in Sue Paint") to keep painting.

Going in and out is smooth: opening Sue Paint, the canvas zooms into the
picture's card, and then the menus and palettes slide in from the edges around
it. Leaving, they slide back out, the picture
settles into its card, and the canvas zooms out again. How fast this happens
follows the Motion settings (Settings, Display & Graphics): a faster animation
speed shortens it, and "Instant transitions" makes it immediate.

---

## Sue Writer: writing and publishing

**Sue Writer** is where you write, from a letter to a trilogy. Open it from
the Hub, the top pill, the Sue button, or New File's Writing wheel, which
starts a project from a template: a blank document, a novel, a short story, a
screenplay, a non-fiction book, an essay or paper, a poetry collection, a
letter or a report.

It looks like a word processor: pages that fit the window, a toolbar with
paragraph styles, every font installed on your computer, sizes, bold, italic,
underline, strikethrough, raised and lowered text, colors, highlights,
alignment, lists, indents and line spacing. The usual keys work: `Ctrl+B`,
`Ctrl+I`, `Ctrl+U`, `Ctrl+E` to center, `Ctrl+J` to justify, `Ctrl+Alt+1` for
a heading, `Ctrl+Enter` for a new page, `Ctrl+K` for a link, `Ctrl+F` to find
and `Ctrl+H` to replace. Enter after a heading goes on in body text, Enter on
an empty list item ends the list, and Tab moves a list item in. In a
screenplay, Enter and Tab move between scene headings, action, characters,
parentheticals and dialogue, each at its proper indent, typed in capitals
where it should be. Quotes turn curly as you type (Settings, Studios).

For long work it is also a writing studio:

- **The binder** on the left holds your chapters and scenes, research and a
  trash. Click a folder to read all its scenes one after another; drag to
  reorder; right-click for the rest.
- **The corkboard** shows each item as an index card with its synopsis,
  label color and status; **the outliner** lists them in a table.
- **The inspector** on the right keeps each scene's synopsis, notes, label,
  status, word target and keywords, and **snapshots**: take one before a big
  rewrite and put it back whenever you like.
- **Targets**: a word target for the book and for each scene, and a daily
  goal; the status bar counts as you go, and Writing Statistics shows the
  words you wrote each day.
- **Composition mode** (`F11`) leaves nothing but your words on a dark page,
  the line you are on kept in the middle. `Esc` comes back.
- **Find and replace** across one scene or the whole project.

When you are done, **Compile** (`Ctrl+Shift+E`) puts the book together, with
chapter titles from your folders, scene breaks and a title page, and saves it
as a PDF (set in your fonts, with bookmarks and page numbers), a Word
document, an OpenDocument file, an EPUB e-book, a web page, RTF, Markdown or
plain text. `Ctrl+P` goes straight to PDF.

Sue Writer works with the rest of Sue. A project sits on the canvas as a
card. Pictures from Sue Paint go straight into your text with the Sue menu's
Send To, and any text on the canvas opens in Sue Writer the same way. Open a
Word file anywhere in Sue and it comes into Sue Writer, formatting and all.
Everything you do can be undone, and it all saves inside your project.

---

## Projects: one file for everything

Press `Ctrl+S` to save your work as a **project**, a single `.sue` file. It
holds everything on the canvas: your documents, pictures, sounds and whatever
else you made, how they are linked, and where each card sits. The first time,
Sue asks where to save; after that `Ctrl+S` just saves. "Save Project As" in the
command palette picks a new file. Choosing where to save, and `Ctrl+O`, both
open the Sue File Manager. Open a project like any other file (the File
Manager, `Ctrl+O`, or drop it on the window) and the canvas comes back exactly as
you left it. Your quick note is yours alone and never goes into projects.

A few things make projects safe to keep for years:

- **Nothing is lost.** If a project holds something this copy of Sue cannot
  show yet (made by a newer Sue, or by a feature you have not installed), Sue
  keeps it untouched and saves it back as it was. The same goes for files Sue
  cannot open yet, such as videos and PDFs: they sit on the canvas as cards
  that keep the original file inside and can be saved back out unchanged.
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

Sue runs on **Windows, Linux, macOS, Android and iOS**. It is the same app with
the same files and the same tools on each.

- **On a computer**, the drawers push the canvas aside, and you work with the
  mouse and keyboard.
- **On a tablet**, the drawers slide over the canvas so your drawing area stays
  as big as possible. A pen gets pressure-sensitive strokes.
- **On a phone**, the drawers slide in from the edges and the timeline becomes a
  **sheet** you pull up from the bottom, within reach of your thumb.

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
- shortcuts become **key caps**: click one and press the new keys.

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
padding = 8   # [2..24] Space inside drawers, pills and controls (px)
```

shows up in Settings as a slider from 2 to 24 labelled "Padding", with the
description "Space inside drawers, pills and controls (px)" under it. Move the
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
