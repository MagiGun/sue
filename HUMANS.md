# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue v1.0 "Faustian Enigma" already opens its own window, with the Hub and
> pinning, Quick Notes, a Drop Zone, and Settings for every option, which you
> can undo and which are saved as you go. Drop a JPEG XL picture, a text file
> or any other file on the window, and Sue opens it in the right viewer. The
> big studios, the file explorer, the media player, the web browser and the
> timeline are added in the releases that follow. For now, Sue reads your
> `.suerice` file only when it starts, so close Sue before you edit the file by
> hand.

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

Sue's window has four areas. Each one appears only when you need it.

```text
┌────────────┬──────────────────────────────┬──────────────┐
│            │                              │              │
│  LEFT      │        CENTER STAGE          │   RIGHT      │
│  DOCK      │   (changes to fit your task) │   DOCK       │
│            │                              │              │
│  Files     │                              │  Browser     │
│  Player    │                              │  Inspector   │
│  Notes     ├──────────────────────────────┤  Settings    │
│  Converter │  TIMELINE (only for audio,   │              │
│            │  video and animation)        │              │
└────────────┴──────────────────────────────┴──────────────┘
```

### The Launchpad and Pin-to-Boot

Every **new tab** opens to the **Hub**, a home screen that shows all your
installed **Studios** (the big creative tools) and **Utilities** (the small
everyday tools). Click one to open it.

Maybe you only ever use Sue as a CAD tool, or only as an MP3 player. Then
**pin** that mode. From then on Sue opens straight into it when it starts and
skips the Hub. Unpin it whenever you like.

### Left Dock: your daily utilities

The left side holds the tools you reach for all day:

- **File Explorer**: browse, search and preview your files, including instant
  previews of large images.
- **Media Player**: play music and video while you work in any other mode.
- **Quick Notes**: jot something down without leaving what you're doing.
- **Drag-and-Drop Converter**: drop a file onto it and pick the format you want.

### Center Stage: the workspace that changes shape

The middle of the screen **adapts to your task**. It's a page when you write, a
canvas when you draw, a 3D view when you model, and a code editor when you code.

The **timeline** along the bottom stays hidden until you work with something
that moves through time: audio, video or animation. Then it slides up on its own.

### Right Dock: reference and control

The right side keeps your references and controls nearby:

- **Web Browser**: look things up without leaving Sue. Drag pictures, sounds or
  code straight from a website into your project.
- **Inspector**: shows the details of whatever you've selected (size, color,
  length, settings) and lets you change them.
- **Settings**: every option Sue has, in one place.

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

- on/off options become **toggles**,
- numbers with a range become **sliders**,
- colors become **color pickers**,
- lists of choices become **drop-downs**,
- shortcuts become **"press a key" recorders**.

A line like this in your `.suerice` file:

```toml
gap = 8   # [0..64] Space between panels
```

shows up in Settings as a slider from 0 to 64 labelled "Gap", with the
tooltip "Space between panels". Move the slider and the file updates. Edit the
file and the slider moves. You can share your `.suerice` file with friends or
download other people's looks from the package manager.

---

## Runs on anything

Sue is built to be **light when it can be and powerful when it should be**:

- When nothing on screen is changing, Sue **draws nothing**, so it uses next to
  no processor or graphics power. Old laptops stay cool and batteries last.
- On older or low-power machines, Sue skips animations and jumps straight to
  the end result, so it stays quick.
- On phones in battery-saver mode, Sue lowers its frame rate to save power.
- On high-end machines, Sue uses all of it: smooth 240 Hz+ motion, spring
  animations, and multi-layer visual effects.

Sue picks the right mode for your device by itself. You can always change it
in Settings.
