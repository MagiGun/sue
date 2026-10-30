# Clip Studio Paint's 3D features, as a spec for Sue Paint

A precise description of how Clip Studio Paint's 3D works, so Sue Paint
can build its own 3D from it. Everything here comes from the official
user guide for Ver. 5.0 (help.clip-studio.com/en-us, the 3D chapter
`660_3d` and the Convert to lines and tones page in the filters chapter),
the Tool Setting Guide (clip-studio.com, the Object tool's pages) and
Clip Studio's tips pages; the sources are listed at the end. The names
are Clip Studio's own, in English. Where the manual gives no number, no
number is invented: the doc says so.

For the audience: whoever builds Sue Paint's 3D (it started in
`crates/sue-paint/src/three.rs`, on the shared `Geometry` model).
Sue Paint's current 3D is a cube, sphere, cylinder, cone or plane, OBJ
import, camera drag, and Convert to Lines and Tones; this is what to
grow it into.

## 1. 3D layers

**What makes one.** Layer menu > New Layer > 3D Layer, or File menu >
Import > 3D data, or dragging a 3D material from the Material palette
onto the canvas (a new 3D layer is made unless a 3D layer is selected;
then the material joins that layer). One 3D layer holds one 3D space
with many materials in it. A 3D layer's thumbnail shows the layer's
camera view, and the canvas shows that view; selecting the layer with
the Object tool draws a grid, which is the floor of the 3D space.

**The camera.** The camera is an object of the layer; the Object list
shows it among the layer's contents (it can be locked like any object).
Camera moves: drag to rotate the camera, shift-drag or two-finger swipe
to translate it, mouse wheel or right-button drag, or a pinch, to move
back and forth. The Object launcher's movement manipulator (section 2)
has dedicated camera modes. The camera view can be centered on the
selection with "Focus on editing target".

- Presets: "Specify camera angle from preset" on the Object launcher,
  or Tool Settings > Camera > Angle > Preset, offers Front, Back, Left,
  Right, Top and Bottom views (the All Sides View palette, EX only,
  shows the layer from four of these sides at once).
- Camera category (Tool Settings or Advanced Tool Settings): "Duplicate
  current camera" makes a second camera on the layer and switches to it;
  "Current camera" lists the layer's cameras, each with a lock to keep
  the angle, a name editable by double tap, and Delete. "Camera
  position" has sliders under Angle for numeric adjustment.
- Lens category: "Lens shift" moves the camera's frame without changing
  the angle to the object; "Reset adjustment" puts it back.
- Projection: a layer can be in perspective projection or parallel
  projection; importing onto a parallel-projection layer switches
  panoramas to perspective.

**Lighting.** Lights are objects of the layer too, shown in the Object
list: two directional lights (Directional Light 1 and 2; 2 is hidden
until selected in the Object list) and ambient light.

- Direction: drag the "Light Source" ball in Tool Settings > Light
  Source; the ball moves Directional Light 1. Its color (Color settings
  dialog) and intensity are sliders beside it.
- Ambient light: color from a single color, an image file (.clip, BMP,
  JPEG, PNG, WebP, TIFF, Targa, .psd, .psb), an image material, or a
  panorama material on the layer; then an intensity slider.
- Shadows: "Cast shadows" turns a material's shadow on or off. The
  layer's "Shadow appearance" picks Off, Cast on ground only, Cast on
  3D models only, or Cast on ground and models. The Rendering settings
  dialog (section 7) has the layer switch: "Apply light source", then
  a pull-down with "Cast shadows down" (straight down, ignoring the
  light) or "Follow light source". Preferences > 3D sets the Shadow
  appearance of new layers, with an extra "Ver. 1.12.6 and earlier"
  mode that casts straight down.
- The whole layer's lighting method is in the Rendering settings dialog:
  Gouraud, Phong or Toon.

**How the layer shows.** The 3D layer renders its space through the
current camera: textured or plain shaded, with or without outlines and
texture lines (section 7). "Display settings for editing" (Preferences
category) can be set to Fast, which hides effects, opacity, the
Navigator preview and canvas blending modes while editing, for slow
machines or heavy models.

## 2. The Object tool on a 3D layer

**The object launcher.** A small bar of buttons beside the selection,
per material type. Common to all 3D materials, in order:

1. Select previous 3D object
2. Select next 3D object
3. Object List (opens the Object list category; PRO and EX only)
4. Camera (pick a preset angle; hidden for hand and panorama materials)
5. Focus on editing target (aim the camera at the selection; hidden for
   panorama and 3D background materials)
6. Ground (anchor the material to the floor grid; hidden for panorama
   and 3D background materials; grounds the selected part of an object)

Then, for 3D drawing figures: Register full body pose (as material),
Flip Horizontal, Initial pose, Reset scale, Reset Rotation, Register
body shape (as material), Apply pose to model (with a drop-down holding
Pose Scanner (image), the Technology Preview option, and 3D pose
(Posemaniacs)), Fix joint, Release all locked joints, Change body shape.

For 3D character materials: Register character as material, Flip
Horizontal, Initial pose, Reset scale, Reset Rotation, Enable/disable
physical calculation, Apply pose to model, Fix joint, Release all locked
joints, Face parts, Facial expression list, Hairstyle, Body,
Accessories.

For 3D object materials: Initial pose, Reset scale, Reset Rotation, Save
3D object to Material palette, Material, Layout, Movable parts.

For the 3D head model: Default pose, Reset scale, Reset Rotation,
Register head model as material. For the 3D hand model: Default pose,
Reset scale, Reset, Hand Scanner. For primitives: Reset scale, Reset
Rotation, Register primitive as material. For 3D background materials:
Material, Layout, Movable parts.

**The movement manipulator.** Nine modes at the top of the selection
(tap to switch on, tap again off):

1. Rotate camera
2. Translate camera
3. Move camera back and forth
4. Move on plane
5. Rotate camera view
6. Rotate plane
7. Rotate in 3D space
8. Snap and move
9. Snap to 3D models

Snap to 3D models toggles independently of the others; dragging on a
button performs its action without changing the mode.

**The root manipulator.** Tapping a material with the Object tool shows
it: handles in red, blue and green to move, rotate and resize the whole
material. Its visibility follows Tool Settings > On-screen Manipulator.
It is not shown for panorama or 3D background materials, for parts of
figures or characters (a local manipulator shows instead, section 3), or
for materials on a Ver. 1.5 compatible layer.

**Snapping and the floor.** "Ground" anchors a material to the floor
grid. Advanced Tool Settings > Operation > Snapping picks which actions
snap to other materials (Move, Rotate, Scale, several at once): move
snaps to another material's position, rotate snaps in steps (Rotation
Step, five degrees by default), and scale snaps rings to another
material's size or position.

**The Object list.** Every material, camera, light and group on the
layer, in a list. Tap to select, shift-tap for several; each row has a
visibility eye and a lock (a locked camera, ambient or directional light
keeps its settings); the Duplicate button copies a material on the same
layer; Group makes a folder of selected materials (groups cannot hold
panoramas, 3D background materials, cameras or lights); dragging a child
onto a parent makes a hierarchy, and children follow the parent's move
and rotation. A row with an arrow and a circle-with-slash means the
material cannot be picked on the canvas; tap it to re-enable picking.

**Tool Property and Advanced Tool Settings.** The Object tool's
categories, per selection: Operation (snapping, rotation step, pivot
point for multiple objects: First selected object, Local pivot points,
Midpoint of multiple objects), Object list, Layout, Pose (section 3),
Change body shape (section 3), Head model and Facial features (section
4), Hand model (section 4), Primitive and General primitive (section 5),
Camera, Lens, Light Source, Shadows (section 1), Texture settings (scale
ratio, position, alpha), Normal map, Fog, Panorama, and Preferences:
Rendering settings, Physical settings (skirt physics: Gravity,
Activeness, Bending spring, Ground height), Display settings for
editing, and On-screen Manipulator.

## 3. 3D drawing figures

The default bodies are a male and a female figure (Materials > 3D >
Character; the default figure for new canvases is set in Preferences >
3D). A figure is one material with poseable parts.

**Body shape.** "Change body shape" on the Object launcher, or the
Change body shape category:

- Height and Head to body ratio sliders (typed values too). "Adjust
  head to body ratio with height" makes the head follow height changes;
  moving the ratio alone resizes only the head. The root manipulator
  changes height while keeping the ratio.
- Sex: female or male, for a default-body-shape figure.
- Full body: a two-axis cross dragged in a slider grid; vertical makes
  the shape more or less defined, horizontal leaner or fuller; numbers
  can be typed under the grid; a reset icon restores the defaults.
- Parts: Head, Neck, Shoulders, Torso, Hips, Arms, Hands, Legs, Feet
  (the original blue-lined figures exclude Shoulders and Hips). Arms,
  Legs and Torso split into sub-sections such as upper and lower arm.
  Dragging the cross adjusts a part's length and width; "Maintain ratio"
  keeps proportions; each part resets on its own.
- "Initial Body Shape" returns every slider to the start. A body shape
  is saved with "Save body shape as material" (or Register material) and
  applied by dragging the material onto a figure or onto empty canvas.

**Posing.** Drag a part with the Object tool and the linked bones move
with it (raising the right arm tilts the head and left arm). "Joint
angle limit" keeps bends inside human ranges. "Lock/release selected
joints" freezes the selected part (right-click with a mouse); locked
joints show a blue square; with nothing locked the waist is the default
lock target. "Release all locked joints" frees everything; hips and
chest hold several bones, so a different joint may be needed. Extra red
manipulators sit at the elbow and below the knee, rotating only that
joint.

- Local manipulators: tapping a part shows rings: red twists (X), green
  and blue bend (Y and Z), with the movable range drawn. The pose
  controller (blue circles for waist, hands, head and so on) selects
  parts and ignores locks; tapping a body part returns to the local
  manipulator.
- Hands: the hand controller drags a cross (left closes the fingers,
  right spreads them, up opens the hand, down closes it into one of four
  fist presets); fingers can be locked individually.
- "Straighten pose": tapping Torso (or another part named in the Advanced
  Tool Settings' pose list) makes the neck and torso upright.

**Pose materials.** Materials > 3D > Pose holds full body poses, hand
poses and pose sequences. Apply with "Use 3D pose material" (launcher
drop-down or the Pose category), or drag a pose onto a figure, or onto
empty canvas to make a posed figure (a figure per pose). A hand pose
applies to the selected arm; with the waist, head or center selected it
applies to both hands. Pose sequences come from BVH motion-capture data
and are stepped with the Pose number setting. "Register full body pose",
or, from its drop-down, "Register left hand pose as material" and
"Register right hand pose as material", save poses. Dragging a 3D
character material onto a posed figure swaps the body and keeps the pose.

**Mirror and reset.** "Mirror pose" (Advanced Tool Settings > Pose)
copies one side to the other, for Arms, Hands and Legs (all on by
default), left to right or the reverse. "Flip Horizontal" mirrors the
whole pose. "Initial pose" resets it. "Reset scale" and "Reset Rotation"
put the figure's size and turn back.

**Pose Scanner.** Estimates a pose from an image (BMP, JPEG, PNG, WebP,
TIFF, Targa; the camera or photo library on tablets) and applies it to
the selected figure, or makes a new one. It runs on Clip Studio's
servers, needs a network, uses the largest figure in the image, and does
not reflect fingers or hands. The same controls sit in the Pose category
and File menu > Import. "3D pose (Posemaniacs)" imports poses from
Posemaniacs the same ways.

## 4. The 3D head and hand models

**Head model.** Nine preset head shapes in Materials > 3D > Head, placed
by dragging onto the canvas (or onto a selected head model to replace
it).

- Face Mixer (Head model category): nine face dimensions in a grid
  around a default base face; each tile shows its level number, set by
  slider or number, 100 being that tile exactly. "Limit face mixer"
  keeps strong mixes from distorting the head. A reset icon restores the
  base face.
- Facial features category: parts for head, brows, eyes, ears, nose,
  mouth, neck and head tilt, each with sliders; icons under the profile
  picture edit left or right alone (both together by default); each part
  resets on its own.
- The vertical Direction bar beside the profile sets head tilt: Tilt
  (chin up and down), Angle (side to side) and Side tilt.
- Saving: "Register head model as material" (launcher or Head model
  category). Files saved without Preferences > File > "Save in
  compatibility mode" do not open in Ver. 1.

**Hand model.** Seven preset hand shapes in Materials > 3D > Hand model,
each in left and right, placed by dragging. Hand models do not open in
Ver. 4 or earlier without compatibility mode.

- Adjust parts: for All fingers, Thumb, Index, Middle, Ring, Little
  finger, Palm and Wrist: length and thickness, nail length, width and
  shape, "Hand veins" with "Vein prominence", wrist size and display.
  A hand image picks the part; each resets on its own.
- Hand mixer: blends the seven presets by level (100 with the rest at 0
  is that preset; "Limit hand mixer" stops distortion); a reset icon
  restores the default model.
- "Show hand details" draws a guide texture of the hand's midline, bones
  and joints.
- Poses, two modes switched above the model: Slider mode (handles give
  sliders for Hand: open and close all fingers, Wrist: tilt, Fingers:
  side to side and bend, Fingertips: the second joint side to side and
  bend) and Manipulator mode (drag parts as on a drawing figure). Both
  modes take the Hand Pose setting, pose materials and the Hand Scanner
  (Camera), which tracks a real hand through the camera. "Initial Pose"
  resets; "Flip Horizontal" swaps left and right without changing the
  pose.
- Saving: "Register pose" saves a pose that also fits drawing figures
  and characters; "Register shape as material" saves the hand shape.

## 5. Primitives and 3D objects

**Primitives.** Materials > 3D > Primitive: cube, sphere, prism
(rectangular solid), pyramid, plane and polygon. "Number of divisions"
(General primitive) sets the segmentation; for sphere, prism, pyramid
and polygon it decides the shape. Each has Primitive color (a Color
settings dialog, or a texture from File: .clip, BMP, JPEG, TGA, PNG,
TIFF, PSD, PSB, or an image material through "Add a texture"), with
Alpha (Opaque, Remove with threshold, Semi-transparent), Tiling, Scale
ratio and Position. Plane and polygon primitives rescale to a texture's
aspect ratio on import, and can "Follow camera" (always face the camera;
"Only horizontal" restricts it to horizontal turns). Normal maps load
through the Normal map category (.clip, BMP, JPEG, PNG, WebP, TIFF,
Targa, PSD, PSB). A primitive's UV map exports via Map setting > Export
as a .clip with three layers: Texture, Background Color and UV map (the
guide lines), to edit and load back. "Register primitive as material"
saves a customized one.

**3D objects and backgrounds.** 3D object materials (small objects,
furniture, and the like) can be moved, textured and colored, and
multi-part objects move and resize part by part; 3D background materials
place whole scenes whose parts only move within preset limits. Both have
Material, Layout and Movable parts on their launcher. Panorama materials
(360 degree backgrounds) sit one per layer. 3D image materials keep a
layout of several materials as one.

**Importing.** File menu > Import > 3D data, or drag onto the canvas.
Formats: .cs3c (character), .cs3o (object), .cs3s (background), FBX, OBJ,
LWO, LWS, 6KT, 6KH, GLB, glTF, VRM, and BVH (applied as a pose sequence
with a start and end frame dialog). FBX does not import on Android, and
6KT and 6KH not on tablets. Textures in formats Sue's reader cannot
open: the manual says the file shows without that texture. Normal maps
come with FBX, GLB and glTF. For PBR formats (glTF, VRM and the like) an
"Import 3D File" dialog asks Realistic (metal and plastic reflections) or
Basic (no gloss); the choice is permanent and locks the layer's lighting
method to Realistic, for every material on the layer. VRM imports as a
3D character (VRM0 and VRM1; pose materials work; motion and shape-key
expressions do not), with a dialog showing the model's usage
permissions. Materials can join the selected 3D layer or make a new one.
Incoming materials match an existing three-point perspective ruler, and
one panorama per layer only.

## 6. Convert to lines and tones (EX)

Layer menu > Convert to lines and tones turns the selected 3D layer (or
a raster layer, or a selection) into a folder of line and tone layers.
The dialog:

- Preset pull-down (image presets and 3D presets are separate; edits
  rename it to Custom), "Register to preset" and "Edit preset".
- Preview: a levels bar dragged like a tone curve; nodes add on empty
  space and leave when dragged out; stripes show each shading level's
  share.
- Categories: Lines and Tone for images; 3D lines, Texture lines and
  Tone for 3D, each with an on and off checkbox.
- Lines (image): Mode Gray or Monochrome. Gray: Line thickness,
  Strength (more detects more lines), Direction of detection (arrows;
  a disabled direction detects weakly). Monochrome: Line thickness,
  Line density (larger, longer lines with fewer breaks), Remove
  fragments (larger, faint lines dropped). "Posterize before extracting"
  with a levels slider; "Black fill threshold".
- Tone: "Posterization" with a levels slider; "Grayscale" (plain gray
  instead of dots; disables Type, Angle and Frequency); Type (dot shape;
  Noise brings Size and Factor instead of Frequency); Angle; Frequency.
- 3D lines: Layer type Raster layer or Vector layer; Mode Standard or
  "Compatible with Ver. 4.1 and earlier" (finer detail, noisier); Line
  width; Strength; Brush type (vector layers only); "Emphasize exterior
  outline"; "Adjust accuracy according to scale" (Compatible mode only;
  calmer far from the camera); Depth: a graph of detection strength by
  distance, with Reset and "Apply to outline only"; Smooth (Compatible
  mode only, a levels slider).
- The result: a layer folder with the extracted lines on their own layer
  (raster or vector as chosen) and the tones on theirs.

## 7. Lines and rendering on the 3D layer itself

Before any LT conversion, a 3D layer can draw its own lines. The Layer
Property palette carries the toggles, and the settings live in the
Rendering settings dialog (Preferences category, or Sub Tool Detail >
Environment in older names):

- Outline: "Add outline" on or off, Width, Opacity, "Offset Amount"
  (a polygon offset that decides which outlines buried in other
  polygons still show), and Color.
- Lighting: "Apply light source", Method (Gouraud, Phong, Toon), Shadow;
  the layer-wide lighting pull-down adds "Cast shadows down" or "Follow
  light source" (section 1).
- Texture: "Use texture", and "Alpha test", the opacity threshold under
  which texture pixels are skipped when texture lines are extracted
  (0.5 means below 50 percent opacity is not extracted).
- "Backface culling" omits the backs of polygons. "Clipping planes":
  "Set automatically when resetting camera", Near plane and Far plane
  distances. "Apply to all models" copies the settings to every 3D layer
  on the canvas.
- Texture lines: the lines drawn from the layer's textures, of the same
  family as the LT dialog's Texture lines category; the LT conversion's
  own Texture lines checkbox turns the texture into lines in the result.

## 8. Painting on 3D

Ver. 5 paints directly on 3D primitives, Ver. 2 3D drawing figures and
3D object materials with the ordinary pen and brush tools. Only models
with a UV map take paint. Select the model with the Object tool, then
switch to a drawing tool; only what is visible on the canvas paints, so
turn the model or camera (or the figure's pose) to reach the rest. Brush
blending modes do not apply on 3D primitives; to see a brush's texture
while painting, set Texture settings to Scale ratio 100 and Position 0.
The eraser erases paint; Edit menu > Delete clears it. Paint on a
model registered as a material can no longer be erased (only added to);
registering the whole 3D layer as a 3D image material keeps it
erasable. Textures export: primitives as a flat unfolded UV map image;
object materials as PNG through Advanced Tool Settings > Object >
"Batch export textures" (EX only, and not for default built-in objects,
Assets materials or limited-license materials). Color jitter and color
mixing do not apply on 3D models, and the Dot pen cannot paint on them.

## A first version for Sue Paint, simplest first

1. The 3D layer and its floor grid, with the plane, cube, sphere,
   cylinder, cone primitives already in Sue, grown the CSP set: prism,
   pyramid, polygon, with Number of divisions, Primitive color and a
   texture from File.
2. The Object list as a palette: every object of the layer, visibility
   eyes, locks, Duplicate, Group.
3. The root manipulator (move, rotate, scale) and Ground, then the
   movement manipulator's camera modes with Sue's existing camera drag.
4. Camera presets (Front, Back, Left, Right, Top, Bottom) and the Camera
   category's duplicate, rename, lock and delete.
5. One directional light and ambient light: the light ball, color and
   intensity, Cast shadows and the four Shadow appearance choices, with
   the Rendering settings' lighting method kept to Toon first.
6. The drawing figure: one male and one female body, Height and Head to
   body ratio, drag a part to pose it, joint angle limit, lock and
   release joints, Initial pose and Flip Horizontal.
7. Pose materials (a pose folder, Use 3D pose material, drag onto a
   figure), then Mirror pose, then registering poses.
8. OBJ import (Sue has it) plus the Rendering settings' Outline (Add
   outline, Width, Opacity, Color) on 3D layers.
9. Convert to Lines and Tones grown to the dialog above: presets, the
   3D lines category (layer type, mode, line width, strength, emphasize
   exterior outline), Tone (posterization, grayscale, angle,
   frequency), and the Depth graph last.
10. Then the body shape sliders and Face Mixer, the hand model, painting
    on 3D, and the remaining import formats.

## Sources

- User guide, 3D chapter (660_3d): 3D Tools; Editing a 3D material;
  Set camera angle; All Sides View palette; Place and use 3D materials;
  Useful features for 3D materials; Setting light source and shadows;
  Adjusting a 3D drawing figure body shape; Posing 3D drawing figures;
  3D Head Models; 3D Hand Models; Changing the shape and texture of 3D
  primitives; Importing 3D Files; Painting on 3D materials.
- Filters chapter (390_filters): Convert to lines and tones (EX only).
- Tool Setting Guide (clip-studio.com), Object tool: Pose; Preference
  (the Rendering settings dialog).
- help.clip-studio.com/en-us, the guide for Ver. 5.0; the CLIP STUDIO
  START guide's camera settings palette, for the camera preset buttons.
