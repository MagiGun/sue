# Animation data in a picture

This page describes how Sue Paint keeps an animation inside a picture, so
that a person or another program can read and write it. Everything here
is ordinary `.sue` data: properties, relations and the shared models of
`sue-model` (`Sequence`, `Animation`). Nothing needs a new chunk, and a
Sue that does not know a key keeps it as it is (Rule 3).

The code is `crates/sue-picture/src/anim.rs` with its parts in
`crates/sue-picture/src/anim/` and the camera in
`crates/sue-picture/src/camera.rs`.

## The timeline

A picture with a timeline carries a `Sequence` extension. Its rate is the
frame rate; each track has a kind (a string) and properties.

| Track kind | What it stands for | Track property |
| --- | --- | --- |
| `sue.paint.cels` | an animation folder; each clip is a key that shows one cel from its frame until the next key | `anim.folder`: the folder entity |
| `sue.paint.layer` | a layer or folder; it shows only inside its clips | `anim.layer`: the layer entity |
| `sue.paint.camera` | a 2D camera folder; its keys are on the folder (below) | `anim.folder`: the camera folder |
| `sue.paint.audio` | a sound; each clip plays an audio entity from its start | none |

A key that shows nothing is a clip with the property `anim.blank` set to
true. An audio clip may carry `audio.volume` (0 to 2, 1 when missing).

A track of an animation folder may carry `anim.track.labels`: its track
labels as `frame:kind` pairs joined by `;`, the kinds being `inbetween`,
`inbetween-done` and `reverse-sheet`. A kind Sue does not know is kept.

A layer with no `sue.paint.layer` track shows on every frame, so a
timeline made by an older Sue plays as it did.

## Properties of the picture

| Key | Value | Meaning |
| --- | --- | --- |
| `anim.length` | Int | the timeline's length in frames |
| `anim.timeline.name` | Text | the timeline's name |
| `anim.timeline.order` | Int | its place in the timeline picker |
| `anim.scene`, `anim.shot` | Int | scene and shot numbers |
| `anim.division` | Int | a division line every so many frames |
| `anim.play.start`, `anim.play.end` | Int | the first and last frame played (from 0) |
| `anim.labels` | Text | timeline labels, one `frame:name` line each |
| `anim.playback` | Text | playback settings, `name=0` or `name=1` pairs joined by `,`: `real_time`, `render_first`, `quality`, `camera` (Render 2D camera) |
| `anim.output` | Text | the output frame (below) |
| `anim.onion` | Text | onion skin settings, as Sue 0.0.75 wrote them |

The output frame reads `width,height;top,bottom,left,right;safe` with an
optional `;width,height,anchor,x,y` for the overflow frame. `safe` is 1
when the title-safe area shows. The anchor is one of nine points, 0 at
the top left, row by row to 8 at the bottom right; `x` and `y` move the
output frame from there.

### Several timelines

The timeline shown is the picture's own. Each other timeline is an entity
of class `sue.paint.timeline`, tied to the picture by the relation
`sue.paint.timeline`, holding its own `Sequence` and the keys that travel
with a timeline (`anim.length`, `anim.timeline.name`, `anim.scene`,
`anim.shot`, `anim.division`, `anim.play.start`, `anim.play.end`,
`anim.labels`, `anim.timeline.order`). Switching swaps the two.

## Animation folders and cels

An animation folder is a layer folder (`sue.folder`) with `folder.animation`
set to true. Its cels are the layers right in it by the relation
`sue.paint.member`, layer folders included: a folder cel shows and hides
as a whole.

## Layer keyframes

Any layer or folder may carry an `Animation` extension whose curves are
keyed by these names. They count while the layer's `anim.keyframes` is
true.

| Curve | Meaning |
| --- | --- |
| `layer.position.x`, `layer.position.y` | moved, in picture pixels |
| `layer.scale` | scaled, 1 as it is |
| `layer.rotation` | turned, in degrees clockwise |
| `layer.center.x`, `layer.center.y` | the centre it turns and scales about |
| `layer.opacity` | its opacity, 0 to 1, in place of its own |

Each key holds how it moves on to the next one: Hold (constant), Linear,
or Smooth (a cubic Bezier whose handles Sue sets from the keys around it
each time a key changes).

## The 2D camera

A camera folder is a layer folder with `folder.camera` set to true. Its
`Animation` extension keys `camera.x` and `camera.y` (the middle of its
view), `camera.scale`, `camera.angle` (radians), `camera.pivot.x` and
`camera.pivot.y` (the centre of rotation; the view's middle when
missing) and `camera.opacity`. A value not keyed is read from the
folder's property of the same name.
