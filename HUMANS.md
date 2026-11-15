# Sue, in Plain English

> "A jack of all trades is a master of none, but oftentimes better than a master of one."

This guide is for people who want to *use* Sue: listeners, writers, artists,
musicians, filmmakers, makers and coders. It has no technical jargon. When a term
needs explaining, it is explained the first time it appears.

> **Where things stand:** This guide describes the experience Sue is built for.
> Sue already opens its own window on the Hub, where you pick a studio.
> You can paint in Sue Paint, where any picture file opens as a canvas, and
> write and publish in Sue Writer. Everything you change can be undone, and
> `Ctrl+S` saves everything you made as one project file. The big studios
> (music, video, 3D) are added in the releases that follow. Sue reads your
> `.suerice` file when it starts (or when you press Reload in Settings), so
> close Sue or reload after you edit the file by hand.

---

## What Sue is

Sue is **one small app that can become many apps**.

One click turns Sue into a full **studio**:

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

Sue's window has a **bar along the top** and the **page** under it. A thin
**status bar** along the bottom is off at first: Settings, Appearance &
Rice, **Status bar** turns it on. What Sue has to tell you (a file saved,
something that went wrong) shows as a **notice** at the bottom of the page,
above the status bar: each wears a sign of what it is (news, done, mind
this, failed), stays a few seconds (longer while the pointer is over it)
and has a button that closes it; the notice of a saved file has **Show in
Folder**. New notices stack under the older ones. Out of the box Sue is black,
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
│  Recent            Saved Projects     Unsaved Work                │
│  ┌──────────────┐  ┌──────────────┐   ┌──────────────────────┐    │
│  │ album  Proj. │  │ album        │   │ Novel   [Recover]    │    │
│  └──────────────┘  └──────────────┘   └──────────────────────┘    │
├───────────────────────────────────────────────────────────────────┤
│ Hub                                    0 entities  Undo 0  Idle   │
└───────────────────────────────────────────────────────────────────┘
```

### The Hub

Sue opens on the **Hub**, your workspace. Down its left side runs a rail
of places: **Home**, **Files**, and any place a studio adds (Sue Paint's
materials). On a phone, or whenever the window is narrow, the rail becomes a
tab bar along the bottom.

**Home** greets you with **New** (a menu of everything every studio can
make, by kind) and **Open** (your computer's open dialog), then:

- **Studios**: a card for each studio (Sue Paint, Sue Writer) with its icon
  and what it is for. Click one to go there: it comes in over the Hub, and a
  studio slides its panels in from the edges. New studios and tools appear
  here by themselves as they are added.
- **Jump Back In**: your newest work as cards with pictures, one click from
  open, and **All Files** to see everything. It shows only once there is
  work, so a first run is not a page of empty boxes.
- **Your Work**: buttons that open Files already sorted: **Recent**,
  **Projects**, **Unsaved Work**, **Favorites**, each with how many it
  holds, and **Folder**, which opens Sue's own folder in your file manager
  where your system has one. Unsaved Work is outlined while there is work
  to recover, so you notice it.

At the top right of every studio card is a round **pin** (it shows when you
point at the card). Pin the studio or tool you use most and Sue opens
straight into it from then on, with no stop at the Hub. Only one can be
pinned: pinning another moves the pin, and clicking the pinned one again
takes it away. `Ctrl+Z` undoes a pin like any other change, and you can
also choose it under Settings, General, Startup.

**Files** shows the work of every studio as cards with pictures (or as a
list): the files and projects you opened or saved lately, and the work Sue
kept for you. Search it, narrow it by studio and by type, sort it by date or
name, mark favorites and sort work under labels of your own. A card's
`>` says what it is, where it is, and offers Show in Folder and Remove from
List (the file itself stays). Sue keeps whatever you have not saved a
minute after you change it, and again when you quit, so a crash or a quick
Quit loses nothing; that kept work has **Recover** on its card (or click
the card), which brings it back in the studio it was made in, and
**Discard**, which asks once more and then throws it away. The `...` menu
of Files rebuilds the thumbnails, forgets files that are gone, and clears
the transfers that are done.

Click a file or project to open it where it belongs: a project opens in the
studio it was saved from. Opening a project while you have unsaved work
does not lose that work: it goes to Unsaved Work. Files that are gone drop
off the list by themselves.

Over the Hub the top bar holds a **search field** (click it, or press
`Tab`, to find any command, setting or studio) and, at its right,
**Transfers** (what Sue opened, saved and copied in this run) and
**Settings**. The arrow keys walk the Hub's places, buttons and cards, and
`Enter` uses the one in focus.

### The Sue button and its menu

At the very top left of the window, always in the same place, is the **Sue**
button (Sue's silhouette). Right after it come the menus of whatever is open:
Sue Paint's File, Edit, Story and the rest, or Sue Writer's, or a tool's. On
the Hub the bar holds the search field and the Transfers and Settings
buttons.

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
  text in Sue Writer. Studios that come later appear here by themselves;
- **Interface**: Automatic (what your device is), Desktop, Tablet or Phone,
  for the whole of Sue at once. On a computer, Tablet and Phone show the
  same buttons, bars and sheets as on a tablet or phone, in your window;
  **Phone Shaped** keeps the phone layout to a phone-wide column in the
  middle. Phones and tablets switch between their two layouts. It works in
  the browser too, and Sue remembers it;
- **Command Palette** (its key stands beside it, as Settings' and Quit
  Sue's do; where the studio you are in keeps that key for itself, none
  is shown), **Check for Updates**, Settings and Quit Sue.

Inside a studio, File and Project are not in the Sue menu: the studio's own
File menu opens and saves. The language Sue speaks and what Sue opens at
launch are chosen in Settings (General, and Language); the Hub's pin and the
command palette pin too.

When you quit with work not saved, Sue asks about each piece in a dialog:
**Save** (or `Enter`), **Do Not Save**, or **Cancel** (or `Escape`), which
keeps Sue open.

The Sue menu and the menus along the top work from the keyboard too: Up and
Down walk the items, Home and End jump to the first and the last, Right
opens a nested menu and Left leaves it (on the bar they go to the menu
beside), Enter or Space runs the item and Escape closes. Typing the start of
an item's name goes to it. A menu taller than the window scrolls instead of
being cut off: with the wheel, by resting the pointer on the arrow at its
top or bottom, or by itself as the keys move.

### Keeping Sue up to date

When Sue starts it looks for a newer release, at most once a day, without
getting in your way: if there is one, a small window in the bottom right
corner says so and shows what is new. **Check for Updates** in the Sue menu
(or in Settings, under Packages & Updates) looks right away. Turn **Check
automatically** off in Settings and Sue only looks when you ask.

Updating follows how you installed Sue:

- **Windows**, installed with the setup program, the .msi or winget:
  **Install and Restart** downloads the new setup program, closes Sue and
  starts it; it replaces the old version in place. Scoop shows its command
  (`scoop update sue`) to copy. The portable zip opens the release page.
- **macOS**: Sue downloads the disk image and opens it; quit Sue and drag
  the new one into Applications.
- **Linux**: an AppImage replaces itself and starts again. The .deb, .rpm,
  Arch package and Flatpak are downloaded to your Downloads folder, with
  the command that installs them to copy. The tarball opens the release
  page.
- **Android**: **Download and Install** fetches the new APK inside Sue and
  hands it to Android's installer, which asks you first and puts the new
  version over the old one, keeping your work. The first time, Android
  asks whether Sue may install apps: turn it on and come back, and the
  update goes on by itself.
- **iOS** opens the release page, and the **browser** version is always
  the newest already.

Everything is downloaded in the background while you keep working, and
checked against the release's list of checksums before it is used.
**Release Notes** opens everything that is new in the release inside Sue,
on a phone as on a computer.

### Sue's windows

On Windows and Linux, Sue draws its windows' frames itself: a dark title
bar with the title in the middle and minimize, maximize and close at the
right, on the main window and on every dialog and floating palette. Drag
the bar to move a window, double-click it to maximize, and drag an edge
or corner to resize. In Sue Paint the title tells what is open, as in
Clip Studio Paint: the picture's name (with `*` while it has changes not
saved), its size, resolution and zoom, and the print profile while View,
Color Profile, Preview is on. macOS keeps its own frame.

### Opening and saving files

Whenever Sue needs a file, or a place to save one, it shows your system's own
Open, Save or folder window, the one every other app uses. Open File (in the
Sue menu, or `Ctrl+O`) and dropping a file anywhere on the window both put
the file where it belongs: a picture opens in Sue Paint as a new canvas, a
brush file (`.sut`, `.abr`, `.gbr`, `.gih`) becomes sub tools there, a
sound file goes on the timeline of the canvas in front, a Word, Markdown or
text file opens in Sue Writer, and a `.sue` project replaces what is open.
A file nothing in Sue opens yet is left alone, and the status bar says so.

Whenever Sue saves or exports a file (a project, a picture from Sue Paint,
an exported rice), the status bar says the file's name and
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
ways: Sue reads the files in it (Sue Paint's Batch import) and each
file Sue makes in it lands there (Android lets you choose any folder but
the top of Download). iPhone and iPad save
first and then ask where in Files the file goes, each time you save; a
folder you choose there is written to directly. Wherever Sue names such a
file (the status bar's "Saved", the Hub's recent work), it names what
you chose, such as "Picture.png in Download", with its folder where the
system says it. A phone has no file manager for Sue to show files in, so
those messages are not clickable there, and Show in Folder is left
out.

### The command palette

Press **Tab** (or `/`) for the **command palette**: start typing and it finds
any action by a few letters, such as "open writer" for Sue Writer or
"new square" for a new square picture. (In Sue Paint, Tab hides the palettes
instead, as in other painting apps; the palette is in the Sue menu.)

Everything moves on springs: nothing jumps, it glides into place and then
stops. When nothing moves, Sue draws nothing at all.

### Text, everywhere

Every place you type (documents, search boxes, names) works the same: drag
to select, double-click a word, `Ctrl+A` to select all, `Ctrl+C`, `Ctrl+X` and
`Ctrl+V` to copy, cut and paste, and `Ctrl+Z` and `Ctrl+Y` to undo and redo.
Each document keeps its own history, as in Clip Studio Paint: undo in one
canvas of Sue Paint (or one story's pages) takes back only that canvas's
steps, never another's, and each studio and tool keeps its own for each file
it works on. Sue's own changes (Settings, the Hub's pin, the language) have
their own history too, undone from the Hub. Sue Paint's Edit, Clear work
history, History forgets the steps of the canvas in front.

### Settings

The Sue menu's **Settings** (or `Ctrl+,`) opens Settings as a dialog over
the page (on a phone it fills the screen); `Escape`, its close button or a
click beside it closes it. A list of categories runs down the left
(General, Language, Appearance, Display and Performance, Sound, Files, a
page for each studio, Keyboard Shortcuts, Extensions and Advanced), and
the right side shows only that category. Rows most people never need wait
under closed titles (the colors, sizes and spacing, animation details); a
click opens one. Type in "Filter settings..." to search all of them; Clip
Studio Paint's names for an option find Sue Paint's setting too
("Brush-type cursor" finds Brush cursor). A setting you changed shows **Reset**
beside it to go back to how Sue comes; **Reset This Page** and **Reset All
Settings** (click it twice) do the same for a page or for everything, and
each reset undoes with `Ctrl+Z`.

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
**Restart Sue** after changing it. Packages & Updates shows the version you
have, **Check automatically** for new releases, and **Check for Updates**. **MCP** (connecting Sue to AI tools) is
not yet available.

### Languages

Sue speaks your language. English, Spanish, Japanese, Arabic, German, French, Italian,
Brazilian Portuguese and Simplified Chinese are available, with new
languages marked as drafts awaiting native-speaker review. The first time it starts it uses your system's language when Sue has
it, and English otherwise. To change it, open the Sue
menu and choose **Language**: pick a language by its own name, or
**System** to follow your system again. Every studio and tool changes at
once; nothing needs a restart.

A studio or tool can also keep a language of its own: say Sue in English
and Sue Paint in Japanese. Settings, **Language** lists Sue and every studio
and tool; each one is **Follow Sue** unless you choose a language for it.
Sue Paint's File, Preferences opens Settings on Sue Paint's own page.

Sue ships in English, with drafts in Spanish and Japanese so far, written
without a native speaker and waiting for one to review them. Anything a
language does not have yet shows in English. Names you type, such as layer
and tool names, stay as you wrote them, and the window's title is always
Sue. CONTRIBUTING.md explains how to add a language or improve one.

---

## Sue Paint: painting and illustration

Click **Sue Paint** in the Hub (or find it with `Tab`) and the page becomes a
painting studio. If you have used a painting app, it will feel familiar.
How close it is to Clip Studio Paint EX is tracked in
[`docs/paint-parity.md`](docs/paint-parity.md), a row for every feature,
with percentages and how to check and update it.

**With no canvas open** Sue Paint shows its workspace as it is: the menu
bar, the palettes and, where the canvas goes, **New...** and **Open...**.
Close the last canvas and nothing else moves; the Window menu still opens
any palette. Your pictures and projects are in the Hub's **Files**, and
File, **Back to the Hub** goes there (the command palette finds it too).

**Materials in the Hub.** Sue Paint adds a **Materials** place to the
Hub's rail, beside Home and Files. It shows every material: Sue Paint's
own tones and patterns, your pictures and your brush tips, under All
materials, by kind, Favorite and Recently used, as large or small cards
or a list. Search by name, choose Sue Paint's own or Added by you, tick
several to make them favorites or delete the ones you added. **Add a
material** copies a PNG picture in. With a canvas open, a click on a
material goes back to Sue Paint and puts it on the canvas (Edit, Undo
takes it off again); right-click and **Details** shows what it is, where
its file is, and deletes one you added (click twice). The place's menu
(the three dots) has **Find Materials Again**, **Rebuild Thumbnails**,
**Show Materials Folder**, **Choose Location of Materials...** and **Use
Sue Paint's Own Folder**. Nothing here goes online.

**Help** in Sue Paint's menu bar links to the basic tips, this guide
online, frequently asked questions, contact and feedback on the Sue
site and GitHub. Sue Paint's colors, where its materials are kept and the pen
pressure curve are rows of Settings, Sue Paint; Transfers is the
Hub's two arrows in the top bar.

**Saving keeps a Sue project.** File, Save keeps a picture in a Sue
project (`.sue`), with everything Sue Paint knows about it. Clip Studio
Paint's `.clip` is a conversion both ways. Sue Paint opens `.clip` files
however they are opened (its own Open, the Sue menu's Open File, a drop on
the window, the Hub's recent files), with folders and their blend modes,
clipping, masks, paper, gray layers and pixels past the canvas's edges;
text and vector layers come in as the pixels Clip Studio last drew for
them, and correction, gradient and fill layers as empty layers in their
places, since Sue cannot redo those effects yet. What Sue cannot show is
kept with each layer: a layer you leave alone goes back into the file
exactly as it came, and a layer you change is written as Sue has it. The
status bar says which layers did not convert exactly. On a canvas opened
from a `.clip`, the first Save asks once: export back to that file, or
keep working in a Sue project beside it. Sue never writes a file you did
not choose, and the window title names the file Save writes. File, Export
Back to the Same .clip writes the canvas into its `.clip` in one step, and
File, Save Duplicate, .clip makes a new one. A canvas opened from a PNG,
JPEG or another picture Sue writes saves back in its format, as in Clip
Studio Paint; once it has more than one layer, Save says the layers will
be merged into the picture (the canvas keeps them) and asks first. A
picture that holds more than Sue Paint shows (frames of an animation,
pages of a TIFF, more than 8 bits a channel, or float pixels) is never
written over: Save keeps it in the Sue project instead. File, Save As can
make a Sue project, a `.clip`, a Photoshop document or a flat picture;
with Settings (Sue Paint, Save format) on Clip Studio Paint, Save on a new
canvas asks where to write a `.clip` instead of a project. A `.clip` that
an older Sue saved opens from the copy of the project it carries, as long
as its layers still match that copy; when they were changed in Clip
Studio since, it opens from its layers and says so.

**On a phone or tablet** Sue Paint with no canvas shows its canvas area
with **New...** and **Open...**. New opens the **New canvas** page:
Illustration, Webtoon, Comic and Animation tabs, each with a row of
templates to swipe through (Create from image, Custom size, your screen's
size; 690 x 20000, 1380 x 20000 and 800 x 10000 px webtoons; A4, A5 and A6
color comics; an animation from a movie, 1:1 or 9:16). Tap one and draw.
**Advanced Settings** opens every setting and **Import** brings in a
picture. Your work and the materials are in the Hub, whose tab bar along
the bottom has Home, Files and Materials.

**Simple Mode** is how Sue Paint draws on a phone or tablet at first: the
canvas alone, with the back arrow (to the Hub), Undo, Redo, your
reference pictures and **...** along the top. Near the bottom are the brush
size and opacity sliders with their numbers; the bar under them swaps
brush and eraser, picks a pen, picks a brush and its settings, opens the
color and the layers. **...** has Export image, Export timelapse, Flip
horizontal view, See reference images, Create animation, Canvas setting,
Preferences, Switch to Studio Mode and Help.

**Studio Mode** is closer to the desktop. A column at the left holds the
menu, the brush and eraser swap, your pen (tap it again for every tool),
the lasso and the object tool, then the brush size and opacity on
vertical sliders and your color. Along the bottom a row of palettes
(navigator, layers, layer property, tool settings, color, materials,
timeline, reference pictures) sits over a command bar: back, save, undo,
redo, transform, select all, copy, cut and paste. The button at the top
right goes fullscreen. The tab at the right edge slides out the modifier
keys: Esc, Ctrl, Shift, Alt and Space stay down until you tap them again,
and T1 to T7 run commands you choose in Settings (Sue Paint, Key T1 and
on). The menu button opens Return to Home, Switch to Simple Mode, App
Settings and every menu. Settings, Sue Paint, Touch mode switches between
the two modes too.

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
  Paint: pen pressure, the pen's speed, its **tilt** (how far it leans) and
  chance can each change them, down to a minimum you choose, along a curve
  you shape by dragging points (press an empty spot to add one, drag one
  out or double-click it to take it away). A mouse, or a pen whose tablet
  does not report tilt, counts as upright, so a setting that follows tilt
  stays at its minimum. Brush Size also offers **Specify by size on
  screen** and **At least 1 pixel**.
- **Pen Pressure Settings** (File menu, or Pen Pressure in the start
  page's Settings): draw on the pad the way you normally draw, choose
  **Check adjusted settings**, and Sue fits the pressure to your hand;
  **Stronger** and **Lighter** fine-tune it, or shape the graph yourself.
  **Done** keeps it for every tool. Out of the box the curve is already a
  little soft, as most hands press tablets, and the first time your pen's
  pressure shows up Sue says where to fit it.
- **Drawing tablets:** pressure works with Windows Ink and with WinTab, so
  Huion, XP-Pen and Wacom tablets work whether or not Windows Ink is turned
  on in their driver. In the browser version, a pen's pressure works too.
  On Linux, the pen draws with its pressure under Wayland (niri, KDE,
  GNOME, Hyprland and others) and under X11, and under Wayland it shows
  the same cursor as the mouse, answers at once even while you only
  hover or draw, and gives every point of a stroke the pressure it had
  there. If your Wayland compositor
  has no tablet support and a pen tablet is connected, Sue opens through
  XWayland so the pen still works. To choose yourself, start Sue with
  `SUE_DISPLAY_SERVER=wayland` or `SUE_DISPLAY_SERVER=x11`.
- **The pen's eraser end:** turn the pen over and Sue Paint picks the
  Eraser (the eraser sub tool you used last); turn it back and you have
  the tool you were drawing with. It switches between strokes, never in
  the middle of one, and if you pick another tool by hand meanwhile, Sue
  keeps your choice. This works where the tablet reports the eraser end:
  under Linux, both Wayland and X11.
- **Clean lines without lag:** pens smooth out the shake of your hand, and
  smooth less the faster you move ("Adjust by speed"). However fast you
  draw, the line never falls more than a short way behind the pen, and when
  you lift the pen it finishes along the way your pen went instead of
  jumping straight there. When you lift the pen, Sue tidies
  the line: it removes the little hook at the end, turns a few fast mouse
  points into a smooth curve, evens out wobbles while keeping sharp corners
  sharp ("Correction"), thins the end into a point ("Taper"), and lets a
  quick flick run out into a fine tail. Watercolor and gouache brushes pick up the color underneath and
  drag it along ("Color mixing"). All of it is in Tool Property.
- **Colors:** a color wheel, sliders, color sets and the colors you recently
  painted with. `X` switches between your main and sub color, `C` takes the
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
- **Dialogs are windows:** on a computer, Sue Paint's dialogs (New,
  Preferences, the filters and the rest) open as windows of their own,
  which you can move anywhere, outside Sue's window too, and resize.
- **Tools as in Clip Studio Paint:** the tool palette at the left has
  Clip Studio's tools in its order; Comic holds the frame border and
  balloon tools and Move the hand, zoom and move layer tools (click again
  for the next). Below them are the main and sub colors, their swap, and
  the transparent color. A tap with a pen leaves a dot the size of the
  brush.
- **Layout Styles:** a layout style is which design Sue Paint's
  workspace follows: which palettes there are and where they sit until
  you move them. Choose it in Window, Layout Style (or Settings, Sue
  Paint). **Clip Studio** is the one built: it lays Sue Paint out as Clip
  Studio Paint does. **Sue Paint (Coming Later)** is listed grayed out for
  a design of Sue's own. A layout style is not a workspace (where you put
  the palettes inside a style, below), nor the interface (desktop, tablet
  or phone, in the Sue menu), nor the theme (colors). Each style keeps
  its own palettes and workspaces, so switching styles and back loses
  nothing.
- **Laid out like Clip Studio Paint:** in the Clip Studio style Sue Paint
  starts with the same frame: the command bar across the top; at the
  left the tools, then Sub Tool, Tool Property and Brush Size stacked as
  three palettes over the six color palettes (Color Wheel, Color Slider,
  Color Set, Intermediate Color, Approximate Color, Color History) as
  tabs; at the right Navigator (with Sub View, Item Bank and Information
  as tabs), Layer Property, and Layer (with History and Auto Action); the
  Timeline folded to an icon under the canvas (click it to open the
  Timeline); and a strip of folded palettes at the far right (Material,
  Animation Cels and Tool Sliders). Every palette is one of its own: the
  Window menu opens each as itself, and none shows in two places.
  Picking a sub tool leaves the Sub Tool list where it is. The arrows at
  the top of a column fold the whole column into that strip. The canvas
  has scrollbars (View, Scroll Bar hides them), with the zoom and turn
  controls beside the bottom one.
- **Launchers:** small bars of buttons under what you are working on, as
  in Clip Studio Paint. Under a transform's box: reset, flip, turn a
  quarter, cancel and OK. Under a text layer's words: edit, smaller,
  larger, vertical and centered. Under the layer's drawing with the
  Operation tool: scale and rotate, flip, its properties and delete. View
  turns each launcher on or off.
- **Quick Filters and Quick Effects:** Filter, Quick Filters (colors such
  as Vivid, Sepia or Night) and Quick Effects (styles such as Glow,
  Halftone or Pixel art) show every look on a small copy of your layer;
  click one, set its strength and see it on the canvas. Apply to all
  works on a merged layer of everything shown, so the layers stay as
  they were.
- **Smart Smoothing:** Edit, Smart Smoothing smooths jagged, pixelated
  line art (a small scan, or lines drawn without anti-aliasing) while
  keeping each line as thick and as dark as it was. Works on a layer of
  lines on clear, or ink on white.
- **Advanced Fill:** Edit, Advanced Fill fills the selection, or the whole
  canvas, in one go: choose the color (main, sub, transparent or any),
  its opacity and blending mode, which colors it fills, and whether it
  fills only areas closed in by lines (with small gaps closed). Area
  scaling tucks the fill under the lines. It remembers its settings.
- **Align a layer:** Edit, Align/Distribute lines the layer's drawing up
  with the canvas's left, right, top or bottom edge, or its middle.
- **Arrange the palettes your way:** drag a palette by its title bar (or
  its tab, or its icon in the strip) and let go where it should be. Sue
  lights up the spot first: the middle of another palette stacks them as
  tabs (drag a tab along the tabs to reorder them), its top or bottom puts
  it above or below, a column's edge or the canvas's left or right edge
  gives it a column of its own, the canvas's bottom puts it under the
  canvas beside the timeline, its top puts it in the row at the top beside
  the command bar, the strip folds it into an icon, and anywhere else
  floats it over the canvas. Drag a tab such as Color Slider, Tool
  Property or History off its group to make it a palette of its own. The command bar (by
  the grip at its left end) and the tool palette move the same way. Drag
  the edge of a column to make it wider (the tool palette then shows its
  tools in several columns), the line between two palettes to share the
  room differently, and the edge of the row under the canvas or at the
  top to make it taller. Each palette's menu (the three lines at its top
  left, on floating palettes too) holds the palette's own commands (the
  Layer palette's new layers, for one), then docks it on either side, in
  a new column on the left or right, at the top or at the bottom, floats
  it over the canvas, pops it out into a window of its own that you can
  move anywhere (outside Sue's window too, onto another screen) and
  resize, or folds it into an icon in a strip at the far right (click the
  icon to see it); its last item, Hide (name) Palette, hides it. Hiding a
  palette, or the X of a floating one, hides it for good: only the Window
  menu shows it again, where it was. Double-click a floating palette's
  title to roll it up. Right-click an icon of the strip to choose Display
  Palette as Tab (it stays open until you click its icon again) or as
  Pop-up (a click elsewhere closes it). The row under the canvas has
  arrows at its right end: one folds it into icons, two close it to a
  thin bar. In a narrow window,
  the widest palettes fold into the strip by themselves and come back when
  there is room. Sue Paint opens with the palettes where you left them
  (Settings, Remember the palette layout, turns that off). Window,
  Workspace keeps layouts by name: Register Workspace saves the one you
  have, its name in the menu brings it back, Manage Workspaces renames and
  deletes them, Use Current Workspace as Default keeps the palettes as they
  are as your default, and Reset to Default puts everything back that way
  (choose it again to get the layout style's own layout, as it was at
  first). The workspace in use is checked, and Reload puts it back as you
  registered it. Illustration and Animation are workspaces Sue Paint
  ships (Animation opens the Timeline and puts Animation Cels beside the
  layers). A workspace keeps the command bar's buttons too. The layouts
  and workspaces are plain text files in Sue Paint's data folder, which
  you can copy, share and edit (`docs/paint-workspace-files.md`).
  `Tab` (Window, Hide All Palettes) hides every palette but the command
  bar, and shows them all again exactly as they were (the item then reads
  Show All Palettes); `Shift+Tab` hides the menus.
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
- **Text on Curve** (the Text tool's second sub tool): draw a curve, then
  type, and the words follow it, each letter turned with it; more lines
  follow it further down.
- **Ruby** (furigana): while typing, select some words, type their
  reading in Tool Property' Ruby field and press Enter (or Set ruby). The
  reading sits small over them, or to their right in vertical text, and
  stays with them as you type around them. Remove ruby takes it away.
- **Smart Shape:** hold the pen still for a moment at the end of a stroke
  and it becomes the clean shape you meant: a straight line, an arc, an
  ellipse or circle, or a polygon such as a rectangle or triangle. Lift the
  pen to keep it. Before you lift, a drag still adjusts it, as in Clip Studio
  Paint: a line's far end follows the pen, and a shape turns and grows around
  its middle. Each pen and brush has it on or off in Tool Property.
- **Text** (T): click and type right on the canvas, as in Clip Studio
  Paint, with a caret that follows the words; Enter starts a new line and
  Esc, Ctrl and Enter, or another tool keeps them as one step. Tool
  Property sets the font (any font on your computer), size, across or in
  vertical columns, centered, letter spacing, line spacing and how lines
  line up (start, middle or end), also for words being typed. Click inside
  a balloon and the words sit in its middle. Click the words again to type
  into them where you clicked.
- **Tone Curve, Levels, Color Balance, Gradient Map** (Edit, Tonal
  Correction): shape the curve by dragging its points, with a live preview.
  With other layers picked too (Ctrl+click in the Layer palette, or Layer,
  Change Selected Layer, Select All Layers), a tonal correction changes
  them all at once, as one step to undo.
- **Grayscale** (Edit, Tonal Correction): one click turns the layer, and
  every other layer picked with it, into grays of the same brightness,
  transparency kept and, with a selection, only inside it. One step to
  undo.
- **Correction layers** (Layer, New Correction Layer): the same corrections
  as a layer that changes everything under it while leaving those layers
  alone. Hide it or lower its opacity to weaken it, and double-click it to
  change its settings.
- **Fill that stays inside:** the fill bucket's **Close gap** keeps the paint
  inside lines with small breaks, and **Area scaling** spreads it a little
  under the lines so no white edge is left. **Close and fill** fills by
  drawing round instead of clicking: every area inside that the lines close
  off fills at once, and the Figure tool's **Lasso fill** fills whatever
  you draw round.
- **Fill by dragging, on frames and masks:** drag the fill bucket across
  the picture and every area the drag crosses fills, as one undo step. In
  an animation, fills and Auto Select see only the cels of the frame
  shown. While a layer's mask is edited, fills, gradients and Edit, Fill
  go to the mask: any color shows the layer there, the transparent color
  hides it. With the transparent color they clear instead of painting.
- **Shrink selection:** the Selection area tool's seventh sub tool. Draw
  round a drawing, roughly, and the selection shrinks to the drawing,
  keeping the holes it closes in.
- **Blend** (J): Blur softens, Finger Tip smudges the colors along your
  stroke, and Liquify pushes the drawing along, swells it, pinches it or
  twirls it under the brush. **Copy stamp** copies one part of a layer to
  another: Alt and click where to copy from, then paint.
- **Puppet warp** (Edit, Transform): click to put pins on a layer, drag one
  and the drawing bends around the others while keeping its shape; Alt and
  click takes a pin away. OK (or Enter) keeps it, Cancel (or Esc) does not.
- **Comics and manga:** File, New, Comic makes a story of pages, bound on the
  right for manga or on the left. The Story menu adds, inserts, duplicates
  and deletes pages and steps through them, and the **Page Manager** shows
  them side by side as spreads: click one to choose it, double-click to open
  it, move it earlier or later. Export All Pages saves every page. File,
  Import, **Batch import** brings every picture in a folder in, one a page
  in name order, such as a stack of scans. Story, Change Story Information
  numbers the pages (the folio), at their outer corners or centered, and
  sets whether page 1 starts alone or facing page 2.
- **Panels, balloons and effects:** Layer, New Layer, Frame Border Folder
  lays out panels with gutters; what you draw on layers under it shows only
  inside the panels. The Frame Border tool draws more panels, or divides one
  when you drag a line across it. The Balloon tool's sub tools, as in Clip
  Studio Paint, draw ellipse, rounded rectangle, cloud (thought) and flash
  (shout) balloons, curve and polyline balloons (click the corners, then
  Enter or click the first corner again), balloons in any outline with the
  Balloon pen, tails, and a thought balloon's trail of bubbles; touching
  balloons and tails join into one outlined shape. The Frame Border tool's
  Polyline frame makes panels of any shape the same way, and the Object
  tool on a frame border layer drags a panel's corner or the whole panel.
  The Figure tool's Saturated Line (drag the clear middle) and Speed Line
  (drag the direction) each draw on an effect line layer that stays
  editable: the Object tool drags its ends, and Tool Property sets how many
  lines, how wide, a new scatter and the drawing color. Filter, Render has focus lines, speed lines and screentone, and
  Layer, New Layer, Tone makes a layer of tone.
- **Animation:** File, New, Animation (or New Animation Cel on any picture)
  starts a timeline. The Timeline palette, laid out as Clip Studio Paint's,
  makes folders and cels, plays and stops (with loop), steps frame by frame
  and zooms; each folder's row shows its cels as pictures, every other
  second is a lighter band and the current frame is a red bar. Edit
  Timeline sets the frame rate and length. Click a frame to go there; double-click to show the current layer from
  there. Onion skin shows the drawing before in blue and after in green;
  Onion Skin Settings choose how many, their colors and how faint they
  get. The **light table** (Animation, Light Table, or the Animation Cels
  palette) shows other layers under the cel you are drawing, to trace
  from: pick the layer, then Register Selected Layer.
  Each **animation folder** is a folder in the Layer palette, with a
  strip of film for its icon, and its cels sit inside it, indented and
  folded away with it, as in Clip Studio Paint. New Animation Cel puts
  the cel in the folder you picked (or the one your cel is in). Drag a
  layer onto the folder to make it a cel; drag a cel out and it leaves
  the timeline. Animations made with Sue 0.0.74 or earlier get their
  folders the first time you open them; Undo takes that back.
  File, Export Animation saves an animated GIF, an animated sticker
  (APNG), an animated WebP, a movie or every frame as a picture.
  Animation, Timeline, **Exposure Sheet** shows the timing as a table, a
  row a frame and a column a folder: a cel's name where it starts, x for
  a blank, | where the one before holds; click a cell to change it. File,
  Export Animation, Exposure Sheet saves it as a CSV file to send or edit,
  and File, Import, Exposure Sheet reads one back.
- **Brushes you downloaded:** Add from downloads at the bottom of the
  Sub Tool opens a file you downloaded and makes its brushes sub tools
  of the tool out: Clip Studio Paint sub tools (.sut), Photoshop brushes
  (.abr), GIMP brushes (.gbr, .gih) or a picture. Size, opacity, spacing,
  spray, ribbon and turn come along; colored tips keep their colors, and
  a brush of several pictures uses them in turn. They are kept for next
  time.
  A Clip Studio Paint sub tool without a picture tip (a round pen, an
  airbrush) comes in too, painting with its own settings.
- **Decorations:** the Decoration tool lays shapes instead of a line.
  Stars, Hearts, Sparkles, Snow, Flowers and Bubbles scatter about the
  stroke at random sizes and turns; Leaves, Chain, Lace, Stitches, Dotted
  line and Dashed line run along it and turn with its curve. Tool Property
  sets how each shape turns, how far it scatters and how much its size
  and turn vary, for any brush with a shape. **Turn with pen tilt** turns
  each shape the way your pen leans, so a flat or chisel tip follows your
  hand as a real one would; with the pen upright, or a mouse, the shape
  turns along the line if that is on, and otherwise stays as set.
- **3D drawing figures:** Layer, New Layer, 3D Drawing Figure puts a
  posable figure on a 3D layer. With the Object tool, drag a body part to
  bend it at its joint (Shift twists it) and drag anywhere else to turn
  the camera; the joints show as dots, the one you are posing lit. Tool
  Settings has pose presets, Flip pose, Reset pose and the picked joint's
  Bend, Twist and Swing, and the body shape: Height and Head to body
  ratio. Convert to Lines and Tones makes line art from
  the pose to draw over; Convert to Vector Lines and Tones makes those
  lines strokes you can reshape with the vector tools.
- **3D hands and heads:** Layer, New Layer, **3D Hand** and **3D Head**
  pose the same way: drag a finger joint to curl it, or the neck or jaw
  to turn, tilt or open the mouth. Their presets include a fist,
  pointing, a peace sign, thumbs up and holding for the hand, and front,
  three-quarter, looking up or down, turned, tilted and an open mouth for
  the head. **Register pose as material** in Tool Property keeps any
  pose in the Material palette's Poses, with a picture of it; double-click
  one to pose the figure, hand or head you have picked, or to add a new
  one in that pose.
- **Pose scanner and Hand scanner** (Tool Property, with a 3D drawing
  figure or a 3D hand picked): choose a photo of a person or a hand, and
  the figure or hand takes the pose it shows, turned to face you as the
  photo did, in one step you can undo. The first time, Sue downloads the
  model that finds the body (MoveNet, 9 MB) or the hand (MediaPipe,
  4 MB) into its models folder; after that it works offline, on the
  computer itself. A whole person, clearly lit, scans best; arms and
  legs pointing at the camera are guessed from how short they look. Sue's
  3D hand is a right hand, so a left hand poses it as its mirror.
- **3D layers:** Layer, New Layer, 3D Layer puts a cube, sphere, cylinder,
  cone, plane, prism or pyramid on the canvas, and File, Import, 3D Data brings in an OBJ
  model. With the Object tool (O), drag to turn the camera, Shift and drag
  to move it, Ctrl and drag to come nearer; Tool Property sets the camera
  (or picks Front, Back, Left, Right, Top or Bottom), the light, its
  intensity, ambient light and cast shadows, the light following the
  camera, fog rising from the foot, whether it draws shaded, as line art
  or both, and the lines' color and opacity. Layer, Convert to
  Lines and Tones makes clean line art and tone from it to draw over.
- **New:** File, New is laid out as Clip Studio Paint's, with its six
  projects: Illustration, Webtoon, Comic, Printing of fanzine (A5 pages),
  Show all comic settings and Animation, each starting at its usual size.
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
- **Layer comps** (Layer, Layer Comps): set your layers as you want them
  (which show, how strongly, how they blend), then New Layer Comp keeps
  that as a named version, such as day and night or with and without the
  background. Choose one and Show to see it again (or double-click it);
  Update, Rename and Delete it; File, Special Export, Export layer comps
  saves each as its own picture.
- **Reference layers** (Layer, Layer Settings, Set as Reference Layer): the
  fill and auto select can look only at them, so you fill on one layer
  between the lines of another. **Rasterize** turns a vector layer into an
  ordinary one, and File, Special Export, **Export vectors** saves the
  lines of the visible vector layers as an SVG drawing, as wide as you drew
  them.
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
  layer there for every project. Settings (or the Hub's Materials menu)
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
the Hub or the Sue menu's Studios. Its start page works like Word's: a blank
document first, then templates shown as little pictures of their first page
(a letter, a cover letter, a resume, a report, an essay or paper, a
newsletter, a memo and meeting notes). Long work is in the **Presets** list
beside them: a novel, a short story, a screenplay, a non-fiction book and a
poetry collection, each with its chapters, research and manuscript
formatting ready. The Sue menu's New offers the same.

Templates are files, so you can add your own. File, **Save as Template**
keeps the open document (its page, fonts, headers and footers, styles and
starter text) as a template, and File, **Templates Folder** opens the folder
they live in. Word (`.docx`, `.dotx`) and LibreOffice (`.odt`, `.ott`)
files you drop there are offered as templates too.

It looks like a word processor: pages that fit the window and one short
toolbar, with what Word and LibreOffice Writer keep at hand: Undo and Redo,
paragraph styles, every font installed on your computer (each one's name
shown in that font in the menu), sizes, bold, italic,
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
where it should be. Quotes turn curly as you type (Settings, Sue Writer).

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
Typing (or Settings, Sue Writer) turns the underlines off. English
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

**Sue Writer on a phone** shows the whole page across the screen, upright or
turned on its side, and pinching zooms in and out (View, Draft View flows
the text to the width of the screen at a size you can read instead, on
paper that fills the screen). Drag a finger to scroll, and pinch with two to zoom. Tap
to put the caret there, double tap to select a word, or press and hold on a
word: it is selected between two handles you can drag, under a bar with Cut,
Copy, Paste and Select All (Copy and Paste use your phone's clipboard, so
text moves to and from other apps). The keyboard stays up while you tap
around the text or the toolbar. The top bar also holds Page View, Insert
Picture, Comments and Save as PDF when there is room, and the formatting row
at the bottom slides sideways to strikethrough, Clear, Link, Picture, Table,
Footnote, Comment and Page Break. A tablet's top bar has Find, Page View,
Picture, Comments, Save as PDF and Table beside the menus. The top bar has the Sue button, a menu button with every
menu, Undo, Redo, the binder and Find. The formatting is a row of big
buttons at the bottom (style, bold, italic, underline, alignment, lists,
indents, font, size and colors; swipe it sideways for the rest), so it sits
right above the keyboard while you type, and tapping one keeps the keyboard
up. When the keyboard comes up, the line you are writing moves into sight
above it, on Android phones and on iPhones alike. The binder covers the
page when you open it and closes when you pick a scene. **On a tablet** the
toolbar's buttons are larger, for fingers.

Pages can be turned on their side: Project Settings, Orientation,
Landscape.

Sue Writer works with the rest of Sue. Pictures from Sue Paint go straight
into your text with the Sue menu's Send To. Open a Word, OpenDocument
(LibreOffice), PDF, Markdown, HTML or plain text file anywhere in Sue and it
comes into Sue Writer, formatting and all (a PDF brings its text, paragraphs
and headings; its pictures and exact layout stay behind); tables come in as tables, heading rows
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

**Sue Paint on a phone or tablet** draws in Simple Mode or Studio Mode
(see Sue Paint, above). Palettes come up from the bottom as sheets, one at
a time; tap above a sheet to put it away.

Everywhere, **two fingers** move, zoom and turn the canvas, a quick
**two-finger tap undoes** and a **three-finger tap redoes**, as in Clip Studio
Paint. A pinch never paints and never undoes: if one finger began a line
before the second came down, that line is taken back, and a tap counts only
when the fingers stay still and the view does not move.

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
opens with nothing to install. It is the same Sue, with the Hub, Sue Paint
and Sue Writer.

- **The layout** follows your device: a computer gets the desktop layout,
  a tablet the tablet one and a phone the phone one. On a phone, turn on
  your browser's Request Desktop Site (or Desktop Site) and reload to get
  the desktop layout. Tablets keep the tablet layout, since their browsers
  ask for desktop sites by themselves. The Sue menu switches layouts too.
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

- **Your installed fonts.** Text uses the two fonts Sue carries, Ubuntu
  Light and Hack.
- **Typing on phones and tablets.** The on-screen keyboard does not come up,
  so use a computer to write.
- **Work in the background.** Long jobs make the page wait until they
  finish.
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
  (the small button at the wheel's corner turns the square into a triangle
  of lightness and saturation in every wheel, and back again),
  slide the bar for how see-through, or click one of the grays below. You
  never type a color code. Letting go of a drag is one `Ctrl+Z` step,
- short lists of choices become **side-by-side buttons**, longer ones
  **drop-downs**,
- shortcuts become **key caps**, one for each set of keys: click one and
  press the new keys, `×` takes one away and `+` adds another, as many as
  you like. Sue's own shortcuts and every studio's (Sue Paint's, tools and
  brush keys included, and Sue Writer's) are all under
  Keyboard Shortcuts, grouped as each studio's menus are, and the menus
  show what you chose. Every command can get a key, not only those that
  ship with one, and the search finds a command by its name or its key.
  A key that two commands hold is marked on both rows with a button that
  takes it from the other one.

**Appearance** also offers ready-made themes (Void, Graphite, Paper and
Gray) that change every color at once, and one `Ctrl+Z` brings yours back.

Each studio can wear its own theme (Settings, the studio's page). Sue Paint starts in
**Gray**, the mid grays painting apps use so your colors read true. Set it to
**Sue theme** and it follows Sue's own colors, so changing them changes every
studio that uses the Sue theme at once; pick another preset; or choose
**Custom** and color Sue Paint with its own color wheels, which appear
below once Custom is chosen. The side panels
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
