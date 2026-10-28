# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue v1.0 "Faustian Enigma" already opens its own window onto the infinite
> canvas described below, with a document, a paint layer and a piece of audio
> on it. You can write, draw, listen, split and level audio, view pictures and
> raw bytes, line things up on the timeline, browse files, play music, convert
> files and keep notes. Everything you change can be undone and is saved as you
> go. The big studios (music, video, 3D) and a web browser built into the window
> are added in the releases that follow. Sue reads your `.suerice` file when it
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
card. Sue is black, white and grays only, so your own pictures and words are the
only color you see.

```text
┌───┬──────────────────────────────────────────────────────────┬───┐
│ F │      [ Canvas  Document  Paint  Timeline  Code / Hex ]    │ I │
│ P │      [ + Text  + Paint  + Audio ]  [ Command (Tab) ]      │ H │
│ C │                                                          │   │
│ N │   ┌ Welcome ─────────┐         [ Brush  Line  Rect ... ] │   │
│   │   │ # Welcome to Sue │         ┌ Sketch ──────────────┐  │   │
│   │   │ text you can type│         │   (you can draw here)│  │   │
│   │   └──────────────────┘         └──────────────────────┘  │   │
│   │                                ┌ Chord ───────────────┐  │   │
│   │                                │ ▁▃▅▇▅▃▁ waveform      │  │   │
│ ⚙ │                                └──────────────────────┘  │   │
├───┴──────────────────────────────────────────────────────────┴───┤
│ Canvas  Sketch                 4 entities  Undo 1  Balanced  Idle │
└──────────────────────────────────────────────────────────────────┘
```

### Moving around

- **Zoom** with the mouse wheel, from 10% to 3200%. The point under the pointer
  stays put, so you zoom right into what you are looking at.
- **Pan** by dragging with the middle mouse button, or hold **Space** and drag.
- Everything moves on springs: nothing jumps, it glides into place and then
  stops. When nothing moves, Sue draws nothing at all.

### The top pill

A floating bar at the top of the canvas:

- **Modes** (Canvas, Document, Paint, Timeline, Code / Hex) glide the camera to
  the thing you want to work on, about a quarter of a second each. Canvas shows
  everything again.
- **+ Text, + Paint, + Audio** put a new document, paint layer or audio clip in
  view.
- **Command (Tab)** opens the command palette.

### The selection pill

Click a card and a second floating bar springs up right above it, with only the
tools that card needs:

- **Documents:** H1, H2, Bold, Monospace, a smaller or larger text size, and
  Project to Timeline. Headings and bold stay as marks in the text (`#`, `**`),
  so the text is always plain and portable.
- **Paint layers:** Brush, Line, Rect, Eraser, a size slider, the gray value
  (click to step from white to black), Export .jxl and Project to Timeline.
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
with New Document, New Paint Layer, New Audio Node, Toggle Timeline, Command
Palette and Reset Camera. Click a slice, or hold the right button, drag toward a
slice and let go.

Press **Tab** (or `/`) for the **command palette**: start typing and it finds
any action, mode or tool by a few letters, such as "np" for New Paint Layer.

### The timeline

Project to Timeline (or the Timeline mode) slides the **timeline** up from the
bottom. It has a Document Track, a Paint/Visual Track and an Audio Track. Each
block on it *is* the card on the canvas, not a copy. Drag blocks to move them,
click the ruler to move the playhead, and press **Space** (or the Play button)
to play: audio blocks sound as the playhead passes them.

### Drawers on the sides

The thin **rails** at the far left and far right hold one icon per tool. Click
one to slide its drawer open; click it again to tuck it away.

- **Files** (left): browse folders, with quick jumps to your project, Desktop,
  Documents, Downloads, Pictures and Music. Click a file to put it on the
  canvas. Hidden system clutter stays hidden.
- **Player** (left): play music (MP3, FLAC, WAV, Ogg, M4A) while you work in any
  mode, with a waveform you can click to jump, and a volume slider.
- **Convert** (left): turn a picture into JPEG XL, PNG or JPEG, or a song into
  WAV. The new file is saved next to the original.
- **Notes** (left): jot something down. Notes save themselves.
- **Inspector** (right): the details of the selected card: its place and size
  on the canvas, which views it has (Document, Timeline and so on), and the
  facts of its file, such as a JPEG XL picture's size and colors.
- **History** (right): every change you made, as a tree you can click to go back
  (or forward again) to any moment.

You can also drop any file anywhere on the window to put it on the canvas.

### Text, everywhere

Every place you type (documents, code, notes, search boxes) works the same: drag
to select, double-click a word, `Ctrl+A` to select all, `Ctrl+C`, `Ctrl+X` and
`Ctrl+V` to copy, cut and paste, and `Ctrl+Z` and `Ctrl+Y` to undo and redo.
Undo is one history for everything: typing, moving a card, a paint stroke or a
Settings slider all go back in the order you did them.

### Settings

The gear at the bottom of the left rail (or `Ctrl+,`) opens **Settings** as a
floating window. A list of categories runs down the left (General, Appearance &
Rice, Display & Graphics, Audio & DSP, Files & Codecs, Local AI, Packages &
Updates, Keymaps), and the right side shows only that category. Type in "Filter
settings..." to search all of them.

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
- colors become **swatches** with their hex code,
- short lists of choices become **side-by-side buttons**, longer ones
  **drop-downs**,
- shortcuts become **key caps**: click one and press the new keys.

**Appearance & Rice** also offers ready-made themes (Void, Graphite and Paper)
that change every color at once, and one `Ctrl+Z` brings yours back. They are
all black, white and gray: Sue's look stays out of the way of your work. If an
older `.suerice` file has a tinted color in it, Sue resets that color to the
monochrome default the next time it starts and keeps a copy of your old file.

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
