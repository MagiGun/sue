# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue v1.0 "Faustian Enigma" already opens its own window with the layout
> below: Home with pinning; Files, Player, Convert and Notes on the left;
> Inspector, Browser and History on the right; a Text Studio, an Image Viewer
> and a Hex Inspector in the middle; and Settings with eight categories and a
> search box. Everything you change can be undone and is saved as you go. Drop
> a picture, a song, a text file or any other file on the window, and Sue opens
> it in the right place. The big studios, the timeline, and a web browser built
> into the window are added in the releases that follow; for now, web pages
> open in your usual browser. Sue reads your `.suerice` file when it starts (or
> when you press Reload in Settings), so close Sue or reload after you edit the
> file by hand.

---

## What Sue is

Sue is **one small app that can become many apps**.

Most days you'll use it for everyday jobs:

- a **fast file explorer** for browsing your folders and drives,
- a **music and media player** for your MP3s and videos,
- a **notes app** for quick thoughts and lists,
- a **file converter**: drop a file in, get it out in another format,
- a **web browser** for looking things up while you work.

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

Every studio works on **the same files at the same time**. Open a song in the
music studio and its waveform also shows up in the video editor's timeline.
Nothing is exported, imported or converted between them.

---

## How the screen works

Sue's window looks like a studio workstation: panels sit edge to edge, with
thin lines between them and no wasted space.

```text
┌───────────────────────────────────────────────────────────────────┐
│ File Edit Workspace Tools Settings │ Home │ Notes.md x │ +     (gear) │
├────┬────────────┬──────────────────────────────┬────────────┬────┤
│ R  │ LEFT DOCK  │        CENTER STAGE          │ RIGHT DOCK │ R  │
│ A  │ Files      │   (changes to fit your task) │ Inspector  │ A  │
│ I  │ Player     │                              │ Browser    │ I  │
│ L  │ Convert    ├──────────────────────────────┤ History    │ L  │
│    │ Notes      │  TIMELINE (only for audio,   │            │    │
│    │            │  video and animation)        │            │    │
├────┴────────────┴──────────────────────────────┴────────────┴────┤
│ Notes.md                       3 entities   Undo 4   Balanced   Idle │
└───────────────────────────────────────────────────────────────────┘
```

- The **header** along the top holds the menus, your **tabs** and the gear that
  opens Settings. The small panel icons at its corners show or hide the docks.
- The thin **rails** at the far left and far right hold one icon per tool.
  Click an icon to show that tool in its dock; click it again to tuck the dock
  away.
- The **status bar** at the bottom says what you are working on, and shows
  "Idle" whenever nothing is moving on screen.

### Home and Pin-to-Boot

The first tab is always **Home**. It shows all your installed **Studios** (the
big creative tools) and **Inspectors** (viewers for one kind of file) as cards
with a short description. Click one to open it in a new tab. A new tab (`+`)
starts on Home too.

Maybe you only ever use Sue as a CAD tool, or only as an MP3 player. Then
**pin** that mode with the "Pin as boot" switch on its card. From then on Sue
opens straight into it when it starts. Unpin it whenever you like.

### Left Dock: your daily utilities

The left side holds the tools you reach for all day:

- **Files**: browse your folders and open any file with one click. Sue looks
  at what is inside the file and opens it in the right place.
- **Player**: play music (MP3, FLAC, WAV, Ogg, M4A) while you work in any other
  mode, with a playlist, a position bar and a volume slider.
- **Convert**: turn a picture into JPEG XL or PNG, or a song into WAV. The new
  file is saved next to the original.
- **Notes**: jot something down without leaving what you're doing. Notes save
  themselves.

You can also drop a file anywhere on the window to open it.

### Center Stage: the workspace that changes shape

The middle of the screen **adapts to your task**. It's a page when you write, a
canvas when you draw, a 3D view when you model, and a code editor when you code.

The **timeline** along the bottom stays hidden until you work with something
that moves through time: audio, video or animation. Then it slides up on its own.

### Right Dock: reference and control

The right side keeps your references and controls nearby:

- **Inspector**: shows the details of whatever is open in the middle: the
  words and lines of a text, the size and colors of a picture, the numbers
  under the cursor of the Hex Inspector.
- **Browser**: look things up. Type an address or pick a bookmark; for now
  pages open in your usual web browser.
- **History**: every change you made, as a list you can click to go back (or
  forward again) to any moment.

### Settings

The gear in the header (or `Ctrl+,`) opens **Settings** as its own tab. A list
of categories runs down the left (General, Appearance & Rice, Display &
Graphics, Audio & DSP, Files & Codecs, Local AI, Packages & Updates, Keymaps),
and the page on the right shows only that category. Start typing anywhere to
search all of them.

---

## One app on every device

Sue runs on **Windows, Linux, macOS, Android and iOS**. It is the same app with
the same files and the same tools on each.

- **On a computer**, Sue arranges its panels side by side like tiles, and you
  can show, hide and resize them with the mouse or keyboard.
- **On a tablet**, the side panels slide out of the way so your drawing area is
  as big as possible. A pen gets pressure-sensitive strokes and round pop-up
  menus.
- **On a phone**, the side panels become **drawers** you swipe in from the edge
  of the screen. The timeline becomes a **sheet** you pull up from the bottom,
  within reach of your thumb. One screen shows at a time, and you step back
  with a swipe.

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
In Sue you can change **every border, gap, color, font, animation, shader
(visual effect) and keyboard shortcut**.

All of it lives in a plain text file ending in **`.suerice`**. You can edit it in
any text editor, but you never have to. Sue reads the file and builds the
controls for it in **Settings**:

- on/off options become **switches**,
- numbers with a range become **sliders**,
- colors become **color swatches** with their hex code,
- short lists of choices become **side-by-side buttons**, longer ones
  **drop-downs**,
- shortcuts become **key caps**: click one and press the new keys.

**Appearance & Rice** also offers ready-made themes (Graphite, Midnight and
Daylight) that change every color at once, and one `Ctrl+Z` brings yours back.

A line like this in your `.suerice` file:

```toml
gap = 8   # [0..64] Space between panels
```

shows up in Settings as a slider from 0 to 64 labelled "Gap", with the
description "Space between panels" under it. Move the slider and the file
updates. Edit the file and the slider moves the next time Sue reads it. You can
share your `.suerice` file with friends or download other people's looks from
the package manager.

---

## Runs on anything

Sue is built to be **light when it can be and powerful when it should be**:

- When nothing on screen is changing, Sue **draws nothing**, so it uses next to
  no processor or graphics power. Old laptops stay cool and batteries last.
- On older or low-power machines, Sue skips animations and jumps straight to
  the end result, so it stays quick.
- On phones in battery-saver mode, Sue lowers its frame rate to save power.
- On computers with two graphics chips, Sue draws with the one your screen is
  plugged into, so an idle window really stays idle. You can pick the other
  chip under **Display & Graphics** in Settings; the change applies the next
  time Sue starts.
- On high-end machines, Sue uses all of it: smooth 240 Hz+ motion, spring
  animations, and multi-layer visual effects.

Sue picks the right mode for your device by itself. You can always change it
in Settings.
