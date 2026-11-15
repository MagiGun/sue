//! Tools and sub tools.
//!
//! The tool palette lists [`Tool`]s; each tool has [`SubTool`]s, ready-made
//! settings such as a G-pen or a soft airbrush, picked in the Sub Tool
//! palette and tuned in the Tool Property palette.
//!
//! How the tool palette is arranged is data: a [`Toolbox`] holds
//! [`ToolSlot`]s (the buttons of the tool palette) in order, each with
//! [`SubToolGroup`]s (the tabs of the Sub Tool palette) of sub tools. A sub
//! tool can become a tool of its own and go back, groups and tools move,
//! and the whole arrangement reads from and writes to a text file
//! ([`layout`]). Sue Paint's own arrangement is such a file too.

pub mod layout;

use crate::brush::{AntiAlias, Tip};
use crate::fill::{Exclude, Refer, Target};
use crate::gradient::{Edge, Shape};
use crate::gradient_map::{GradientMap, StopColor};
use crate::select::Aspect;
use sue_model::BlendMode;

/// The Fill tool's Close and fill sub tool's mode.
pub const CLOSE_AND_FILL: u8 = 3;
/// The Fill tool's Paint unpainted area sub tool's mode.
pub const PAINT_UNPAINTED: u8 = 4;
/// The Gradient tool's Contour line fill sub tools' mode.
pub const CONTOUR_FILL: u8 = 3;
/// The Figure tool's Lasso fill sub tool's mode.
pub const LASSO_FILL: u8 = 7;
/// The Figure tool's Polyline sub tool's mode.
pub const POLYLINE: u8 = 8;
/// The Figure tool's Curve sub tool's mode: a drag lays the ends, then
/// the pointer bends the line and a click sets it.
pub const CURVE: u8 = 9;
/// The Figure tool's Continuous Curve sub tool's mode: a click at each
/// point, the line smoothed through them.
pub const CONTINUOUS_CURVE: u8 = 10;
/// The Figure tool's Bezier Curve sub tool's mode: a click lays an anchor,
/// a drag from it pulls out its handles.
pub const BEZIER_CURVE: u8 = 11;
/// The Figure tool's Polygon sub tool's mode: a regular polygon of
/// [`SubTool::corners`] corners dragged out.
pub const POLYGON: u8 = 12;

/// The Balloon tool's sub tools, by mode (0 is the ellipse balloon, 1 the
/// tail).
pub const ROUNDED_BALLOON: u8 = 2;
pub const CLOUD_BALLOON: u8 = 3;
pub const FLASH_BALLOON: u8 = 4;
pub const BALLOON_PEN: u8 = 5;
pub const THOUGHT_TAIL: u8 = 6;
pub const ELLIPSE_BALLOON: u8 = 0;
pub const CURVE_BALLOON: u8 = 7;
pub const POLYLINE_BALLOON: u8 = 8;
/// The Frame Border tool's polyline frame (0 is the rectangle, 1 the
/// division).
pub const POLYLINE_FRAME: u8 = 2;
/// The Operation tool's Light table and Edit timeline sub tools, by
/// mode (0 is Object, 1 Select layer).
pub const OPERATION_LIGHT_TABLE: u8 = 2;
pub const OPERATION_EDIT_TIMELINE: u8 = 3;
/// What the Object sub tool picks (Selectable object), as bits of
/// [`SubTool::objects`]: vector lines, raster layers (it moves them), the
/// comic parts (frame borders and effect lines), 3D layers, the camera.
pub const PICKS_VECTOR: u8 = 1;
pub const PICKS_RASTER: u8 = 2;
pub const PICKS_COMIC: u8 = 4;
pub const PICKS_3D: u8 = 8;
pub const PICKS_CAMERA: u8 = 16;
/// The Figure tool's effect lines, by mode.
pub const SATURATED_LINES: u8 = 5;
pub const SPEED_LINES: u8 = 6;

/// A kind of tool: what its sub tools do. The tool palette shows
/// [`ToolSlot`]s, which are these, or sub tools made into tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tool {
    /// Zooms the view in or out.
    Zoom,
    /// Scrolls or rotates the view.
    Hand,
    /// Moves the current layer.
    MoveLayer,
    /// Draws selections.
    Select,
    /// Selects areas of similar color.
    AutoSelect,
    /// Picks a color.
    Eyedropper,
    /// Crisp ink lines.
    Pen,
    /// Grainy pencil lines.
    Pencil,
    /// Paint brushes.
    Brush,
    /// Soft sprays.
    Airbrush,
    /// Pattern brushes.
    Decoration,
    /// Removes pixels.
    Eraser,
    /// Smudges and blurs.
    Blend,
    /// Fills areas.
    Fill,
    /// Fills with gradients.
    Gradient,
    /// Lines and shapes.
    Figure,
    /// Text.
    Text,
    /// Speech balloons.
    Balloon,
    /// Comic panel borders.
    Frame,
    /// Rulers and guides.
    Ruler,
    /// Tidies vector lines.
    CorrectLine,
    /// Selects and edits objects.
    Operation,
}

impl Tool {
    /// The palette, top to bottom.
    pub const ALL: [Self; 22] = [
        Self::Zoom,
        Self::Hand,
        Self::Operation,
        Self::MoveLayer,
        Self::Select,
        Self::AutoSelect,
        Self::Eyedropper,
        Self::Pen,
        Self::Pencil,
        Self::Brush,
        Self::Airbrush,
        Self::Decoration,
        Self::Eraser,
        Self::Blend,
        Self::Fill,
        Self::Gradient,
        Self::Figure,
        Self::Frame,
        Self::Ruler,
        Self::Text,
        Self::Balloon,
        Self::CorrectLine,
    ];

    /// Its stable name in saved data (tool layouts): never translated,
    /// never changed.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Zoom => "zoom",
            Self::Hand => "hand",
            Self::MoveLayer => "move_layer",
            Self::Select => "selection",
            Self::AutoSelect => "auto_select",
            Self::Eyedropper => "eyedropper",
            Self::Pen => "pen",
            Self::Pencil => "pencil",
            Self::Brush => "brush",
            Self::Airbrush => "airbrush",
            Self::Decoration => "decoration",
            Self::Eraser => "eraser",
            Self::Blend => "blend",
            Self::Fill => "fill",
            Self::Gradient => "gradient",
            Self::Figure => "figure",
            Self::Text => "text",
            Self::Balloon => "balloon",
            Self::Frame => "frame_border",
            Self::Ruler => "ruler",
            Self::CorrectLine => "correct_line",
            Self::Operation => "operation",
        }
    }

    /// The tool named `id` ([`Self::id`]).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.id() == id)
    }

    /// Its name in English, as kept in data and shown through the language
    /// database ([`Self::label`] in the language in use).
    #[must_use]
    pub const fn english(self) -> &'static str {
        match self {
            Self::Zoom => "Zoom",
            Self::Hand => "Hand",
            Self::MoveLayer => "Move Layer",
            Self::Select => "Selection",
            Self::AutoSelect => "Auto Select",
            Self::Eyedropper => "Eyedropper",
            Self::Pen => "Pen",
            Self::Pencil => "Sketch",
            Self::Brush => "Brush",
            Self::Airbrush => "Airbrush",
            Self::Decoration => "Decoration",
            Self::Eraser => "Eraser",
            Self::Blend => "Blend",
            Self::Fill => "Fill",
            Self::Gradient => "Gradient",
            Self::Figure => "Figure",
            Self::Text => "Text",
            Self::Balloon => "Balloon",
            Self::Frame => "Frame Border",
            Self::Ruler => "Ruler",
            Self::CorrectLine => "Correct Line",
            Self::Operation => "Operation",
        }
    }

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Zoom => tr!("zoom"),
            Self::Hand => tr!("hand"),
            Self::MoveLayer => tr!("move-layer"),
            Self::Select => tr!("selection"),
            Self::AutoSelect => tr!("auto-select"),
            Self::Eyedropper => tr!("eyedropper"),
            Self::Pen => tr!("pen"),
            Self::Pencil => tr!("sketch"),
            Self::Brush => tr!("brush"),
            Self::Airbrush => tr!("airbrush"),
            Self::Decoration => tr!("decoration"),
            Self::Eraser => tr!("eraser"),
            Self::Blend => tr!("blend"),
            Self::Fill => tr!("fill"),
            Self::Gradient => tr!("gradient"),
            Self::Figure => tr!("figure"),
            Self::Text => tr!("text"),
            Self::Balloon => tr!("balloon"),
            Self::Frame => tr!("frame-border"),
            Self::Ruler => tr!("ruler"),
            Self::CorrectLine => tr!("correct-line"),
            Self::Operation => tr!("operation"),
        }
    }

    /// The key that picks it.
    #[must_use]
    pub const fn key(self) -> Option<char> {
        match self {
            Self::Hand => Some('H'),
            Self::MoveLayer => Some('K'),
            Self::Select => Some('M'),
            Self::AutoSelect => Some('W'),
            Self::Eyedropper => Some('I'),
            Self::Pen | Self::Pencil => Some('P'),
            Self::Brush | Self::Airbrush | Self::Decoration => Some('B'),
            Self::Eraser => Some('E'),
            Self::Fill | Self::Gradient => Some('G'),
            Self::Figure => Some('U'),
            Self::Text | Self::Balloon => Some('T'),
            Self::Operation => Some('O'),
            Self::Zoom => Some('/'),
            Self::Blend => Some('J'),
            Self::Frame | Self::Ruler => Some('U'),
            Self::CorrectLine => Some('Y'),
        }
    }

    /// `true` if it works in this build.
    #[must_use]
    pub const fn available(self) -> bool {
        true
    }

    /// `true` for tools that paint with a [`Tip`].
    #[must_use]
    pub const fn paints(self) -> bool {
        matches!(
            self,
            Self::Pen
                | Self::Pencil
                | Self::Brush
                | Self::Airbrush
                | Self::Eraser
                | Self::Figure
                | Self::Decoration
        )
    }

    /// The sub tools a fresh studio starts with.
    #[must_use]
    pub fn sub_tools(self) -> Vec<SubTool> {
        let tip = |size: f32, change: fn(&mut Tip)| {
            let mut tip = Tip {
                size,
                ..Tip::default()
            };
            change(&mut tip);
            tip
        };
        let plain = |name: &str, mode: u8| SubTool::new(self, name, mode, Tip::default());

        match self {
            Self::Zoom => vec![plain("Zoom In", 0), plain("Zoom Out", 1)],
            Self::Hand => vec![plain("Hand", 0), plain("Rotate", 1)],
            Self::MoveLayer => vec![plain("Move Layer", 0)],
            // Clip Studio Paint's Selection area sub tools.
            Self::Select => vec![
                plain("Rectangle", 0),
                plain("Ellipse", 1),
                plain("Lasso", 2),
                plain("Polyline", 3),
                SubTool::new(
                    self,
                    "Selection pen",
                    4,
                    tip(20.0, |t| t.size_by.pressure = false),
                ),
                SubTool::new(
                    self,
                    "Erase selection",
                    5,
                    tip(20.0, |t| t.size_by.pressure = false),
                ),
                // A lasso whose selection shrinks to what is drawn in it.
                plain("Shrink selection", crate::select::SHRINK_SELECTION),
            ],
            // Clip Studio Paint's three: the editing layer, every layer,
            // and the reference layers.
            Self::AutoSelect => vec![
                plain("Refer Only Editing Layer", 0).tolerance(0.1),
                plain("Refer Other Layers", 0)
                    .tolerance(0.1)
                    .referring(Refer::All),
                plain("Refer Reference Layers", 0)
                    .tolerance(0.1)
                    .referring(Refer::Reference),
            ],
            Self::Eyedropper => vec![
                plain("Obtain Display Color", 0),
                plain("Obtain Layer Color", 1),
            ],
            // The brushes: Sue's built-in presets (`sue_brush::presets`).
            Self::Pen | Self::Pencil | Self::Brush | Self::Airbrush | Self::Eraser => brushes(self),
            // Clip Studio Paint Ver. 4's order (A25V-22); its Lasso fill
            // is the Figure tool's, placed here by the layout.
            Self::Fill => vec![
                plain("Refer Other Layers", 0)
                    .tolerance(0.1)
                    .referring(Refer::All),
                plain("Refer Only Editing Layer", 0).tolerance(0.1),
                // Encircle lines: the areas they close off fill.
                plain("Close and fill", CLOSE_AND_FILL).tolerance(0.1),
                // Draw round colored areas: the gaps left in them fill.
                plain("Paint unpainted area", PAINT_UNPAINTED),
            ],
            Self::Gradient => gradients(),
            Self::Figure => {
                let line = tip(6.0, |t| {
                    t.size_by.pressure = false;
                });
                vec![
                    SubTool::new(self, "Straight Line", 0, line),
                    SubTool::new(self, "Rectangle", 1, line),
                    SubTool::new(self, "Ellipse", 2, line),
                    SubTool::new(self, "Filled Rectangle", 3, line),
                    SubTool::new(self, "Filled Ellipse", 4, line),
                    // Clip Studio Paint's Saturated line and Speed line,
                    // each on a layer of its own.
                    SubTool::new(self, "Saturated Line", SATURATED_LINES, line),
                    SubTool::new(self, "Speed Line", SPEED_LINES, line),
                    // Draw round an area and it fills.
                    SubTool::new(self, "Lasso fill", LASSO_FILL, line),
                    // A click at each corner; a double-click (or Enter)
                    // ends it, a click on the first closes it.
                    SubTool::new(self, "Polyline", POLYLINE, line),
                    // Drag the ends, move to bend, click.
                    SubTool::new(self, "Curve", CURVE, line),
                    SubTool::new(self, "Continuous Curve", CONTINUOUS_CURVE, line),
                    SubTool::new(self, "Bezier Curve", BEZIER_CURVE, line),
                    SubTool::new(self, "Polygon", POLYGON, line),
                ]
            }
            // Clip Studio Paint's order: rectangle, polyline, then cut.
            Self::Frame => vec![
                plain("Rectangle Frame", 0),
                plain("Polyline frame", POLYLINE_FRAME),
                plain("Frame Border Division", 1),
            ],
            Self::Balloon => {
                let outline = tip(3.0, |t| {
                    t.size_by.pressure = false;
                });
                // Clip Studio Paint's Balloon sub tools.
                vec![
                    SubTool::new(self, "Ellipse Balloon", 0, outline),
                    SubTool::new(self, "Rounded Rectangle Balloon", ROUNDED_BALLOON, outline),
                    SubTool::new(self, "Cloud Balloon", CLOUD_BALLOON, outline),
                    SubTool::new(self, "Flash Balloon", FLASH_BALLOON, outline),
                    SubTool::new(self, "Curve Balloon", CURVE_BALLOON, outline),
                    SubTool::new(self, "Polyline Balloon", POLYLINE_BALLOON, outline),
                    SubTool::new(self, "Balloon Pen", BALLOON_PEN, outline),
                    SubTool::new(self, "Balloon Tail", 1, outline),
                    SubTool::new(self, "Thought Balloon Tail", THOUGHT_TAIL, outline),
                ]
            }
            // Clip Studio Paint's Create ruler group, in its order.
            Self::Ruler => vec![
                plain("Linear Ruler", 5),
                plain("Curve Ruler", 6),
                plain("Figure Ruler", 7),
                plain("Ruler Pen", 8),
                plain("Special Ruler", 2),
                plain("Perspective Ruler", 1),
                SubTool {
                    lines: 2,
                    ..plain("Symmetrical Ruler", 0)
                },
            ],
            // Clip Studio Paint's Text: 48 pixel words, across; and words
            // along a curve drawn first.
            Self::Text => {
                let words = Tip {
                    size: 48.0,
                    ..Tip::default()
                };
                vec![
                    SubTool::new(self, "Text", 0, words),
                    SubTool::new(self, "Text on Curve", 1, words),
                ]
            }
            Self::Blend => {
                let soft = tip(60.0, |t| {
                    t.opacity = 0.6;
                    t.size_by.pressure = false;
                });
                vec![
                    SubTool::new(self, "Blur", 0, soft),
                    SubTool::new(self, "Finger Tip", 1, soft),
                    // One sub tool, its mode picked in Tool Property
                    // (`blend::LiquifyMode`).
                    SubTool::new(
                        self,
                        "Liquify",
                        crate::blend::LiquifyMode::Push.mode(),
                        soft,
                    ),
                    // Alt and click picks where it copies from.
                    SubTool::new(
                        self,
                        "Copy stamp",
                        crate::blend::COPY_STAMP,
                        tip(30.0, |t| t.size_by.pressure = false),
                    ),
                ]
            }
            // Clip Studio Paint's Correct line group, then Remove dust.
            Self::CorrectLine => {
                let brush = |size: f32| {
                    tip(size, |t| {
                        t.size_by.pressure = false;
                    })
                };
                vec![
                    plain("Control point", 0),
                    SubTool {
                        level: 2,
                        amount: 30.0,
                        ..plain("Pinch vector line", 1)
                    },
                    SubTool {
                        level: 2,
                        ..SubTool::new(self, "Simplify vector line", 2, brush(50.0))
                    },
                    SubTool::new(self, "Connect vector lines", 3, brush(50.0)),
                    SubTool {
                        amount: 1.0,
                        ..SubTool::new(self, "Adjust line width", 4, brush(40.0))
                    },
                    plain("Redraw vector line", 5).stabilization(6),
                    SubTool::new(self, "Redraw vector line width", 6, brush(10.0)),
                    SubTool {
                        amount: 10.0,
                        ..plain("Remove dust", 7)
                    },
                ]
            }
            // Clip Studio Paint's Operation group: Light table drags the
            // light table's layers, Edit timeline steps the frames.
            Self::Operation => vec![
                plain("Object", 0),
                plain("Select layer", 1),
                plain("Light table", OPERATION_LIGHT_TABLE),
                plain("Edit timeline", OPERATION_EDIT_TIMELINE),
            ],
            Self::Decoration => brushes(self),
        }
    }
}

/// The built-in brushes of `tool` as its sub tools: each preset's name,
/// tip, stabilization and correction, in the preset list's order. A
/// decoration's mode is its place in that list.
/// The Gradient tool's sub tools: Sue's own gradients, then Contour line
/// fill's. Each holds its stops, shape and edge process; the colors of
/// its own are Sue's, none taken from another program.
fn gradients() -> Vec<SubTool> {
    use crate::gradient_map::Stop;
    let made = |name: &str, map: GradientMap, shape: Shape, edge: Edge| SubTool {
        gradient: map,
        gradient_shape: shape,
        gradient_edge: edge,
        ..SubTool::new(Tool::Gradient, name, 0, Tip::default())
    };
    let own = |stops: &[Stop]| GradientMap::of(stops).unwrap_or_default();
    let fade = GradientMap::sourced((StopColor::Drawing, 255), (StopColor::Drawing, 0));
    let main_sub = GradientMap::sourced((StopColor::Main, 255), (StopColor::Sub, 255));
    let mut glow = GradientMap::sourced((StopColor::Drawing, 255), (StopColor::Drawing, 0));
    if let Some(index) = glow.add_at(0.35)
        && let Some(stop) = glow.stop_mut(index)
    {
        stop.color = StopColor::Drawing;
        stop.alpha = 150;
    }
    vec![
        made(
            "Drawing Color to Transparent",
            fade,
            Shape::Linear,
            Edge::Extend,
        ),
        made("Main to Sub Color", main_sub, Shape::Linear, Edge::Extend),
        made("Radial", fade, Shape::Circle, Edge::Extend),
        made("Ellipse", fade, Shape::Ellipse, Edge::Extend),
        made("Soft Glow", glow, Shape::Circle, Edge::Clip),
        made("Repeating Bands", main_sub, Shape::Linear, Edge::Mirror),
        made(
            "Dusk",
            own(&[
                Stop::new(0.0, [24, 20, 60]),
                Stop::new(0.45, [150, 60, 110]),
                Stop::new(0.8, [240, 150, 90]),
                Stop::new(1.0, [255, 236, 190]),
            ]),
            Shape::Linear,
            Edge::Extend,
        ),
        made(
            "Sea",
            own(&[
                Stop::new(0.0, [6, 20, 48]),
                Stop::new(0.55, [30, 110, 160]),
                Stop::new(1.0, [210, 245, 250]),
            ]),
            Shape::Linear,
            Edge::Extend,
        ),
        made(
            "Forest",
            own(&[
                Stop::new(0.0, [10, 28, 20]),
                Stop::new(0.5, [60, 120, 70]),
                Stop::new(1.0, [220, 236, 180]),
            ]),
            Shape::Linear,
            Edge::Extend,
        ),
        made(
            "Spectrum",
            own(&[
                Stop::new(0.0, [230, 40, 40]),
                Stop::new(0.2, [240, 200, 40]),
                Stop::new(0.4, [60, 190, 80]),
                Stop::new(0.6, [40, 180, 220]),
                Stop::new(0.8, [70, 70, 220]),
                Stop::new(1.0, [200, 60, 200]),
            ]),
            Shape::Linear,
            Edge::Extend,
        ),
        // Contour line fill: a click in an area lays the gradient from its
        // outline inward.
        SubTool {
            mode: CONTOUR_FILL,
            tolerance: 0.1,
            ..made("Contour Fill", fade, Shape::Linear, Edge::Extend)
        },
        SubTool {
            mode: CONTOUR_FILL,
            tolerance: 0.1,
            contour_width: 12.0,
            ..made("Contour Bands", main_sub, Shape::Linear, Edge::Mirror)
        },
    ]
}

fn brushes(tool: Tool) -> Vec<SubTool> {
    let key = match tool {
        Tool::Pen => "pen",
        Tool::Pencil => "pencil",
        Tool::Brush => "brush",
        Tool::Airbrush => "airbrush",
        Tool::Decoration => "decoration",
        _ => "eraser",
    };
    sue_brush::presets::of_tool(key)
        .iter()
        .enumerate()
        .map(|(index, preset)| {
            let mode = if tool == Tool::Decoration {
                u8::try_from(index).unwrap_or(u8::MAX)
            } else {
                0
            };
            let mut tip = Tip::from_text(&sue_brush::presets::legacy_text(preset));
            // A tip picture is one of the decoration shapes, known by name.
            if let sue_model::brush::TipSource::Images { pictures, .. } = &preset.tip
                && let Some(sue_model::brush::PictureRef::Named(name)) = pictures.first()
            {
                tip.shape = crate::decoration::shape(name);
            }
            let input = &preset.input;
            SubTool {
                stabilization: input.stabilization,
                adjust_by_speed: input.by_speed,
                correction: input.post_correction,
                flick: input.flick,
                smart_shape: input.smart_shape,
                kind: sue_brush::presets::vector_erase(preset),
                ..SubTool::new(tool, &preset.name, mode, tip)
            }
        })
        .collect()
}

/// A ready-made setting of a tool.
#[derive(Clone, Debug, PartialEq)]
pub struct SubTool {
    /// Its tool.
    pub tool: Tool,
    /// What people read, e.g. `G-Pen`.
    pub name: String,
    /// What it does within its tool (a shape, a direction, a reference).
    pub mode: u8,
    /// Its brush, for painting tools.
    pub tip: Tip,
    /// Pointer smoothing, 0 to 100.
    pub stabilization: u32,
    /// Stabilization loosens as the pen speeds up.
    pub adjust_by_speed: bool,
    /// Post correction when the pen lifts, 0 (none) to 10.
    pub correction: u32,
    /// A quick stroke carries on into a fine point when the pen lifts.
    pub flick: bool,
    /// How different a color may be and still be filled or selected, 0 to 1.
    pub tolerance: f32,
    /// A symmetrical ruler's number of lines, 1 to 32, counting the
    /// mirrored ones as Clip Studio Paint does.
    pub lines: u32,
    /// A symmetrical ruler mirrors across its line (Line symmetry).
    pub mirror: bool,
    /// A fill's Close gap level, 0 (none) to 4 (strongest).
    pub gap: u32,
    /// A fill's Area scaling, in pixels: above nothing under the lines,
    /// below nothing back from them.
    pub spread: i32,
    /// Fill and Auto Select look at more than the editing layer (Refer
    /// multiple); which layers is [`Self::refer`].
    pub refer_multiple: bool,
    /// Which layers Fill and Auto Select look at while
    /// [`Self::refer_multiple`] is on ([`Self::referred`] says what they
    /// look at).
    pub refer: Refer,
    /// Layers Fill and Auto Select leave out while they look at more than
    /// one.
    pub exclude: Exclude,
    /// Fill and Auto Select take only pixels joined to the one clicked
    /// (Apply to connected pixels only); off, every like pixel.
    pub connected_only: bool,
    /// How Fill and Auto Select soften the edge of their area.
    pub area_aa: AntiAlias,
    /// The shape a selection or figure drag keeps (Aspect).
    pub aspect: Aspect,
    /// The Rotate sub tool turns in steps of this many degrees (Snap
    /// angle), and Shift snaps by it; a Figure or Ruler line runs in
    /// steps of it (Step of angle); 0 turns and runs freely.
    pub angle_step: f32,
    /// Liquify acts only inside the selection (Only refer to editing
    /// area).
    pub editing_area_only: bool,
    /// Its settings are locked: Tool Property shows them and changes
    /// nothing ([`Toolbox::settings_mut`]), and Reset leaves it.
    pub locked: bool,
    /// What a ruler sub tool lays or does: the Special Ruler's kind
    /// (`rulers::Special::NAMES`) or the Perspective Ruler's process (0
    /// add a vanishing point, 1 delete one); for an eraser, how it erases
    /// vector lines (`vector::Erase::NAMES`).
    pub kind: u8,
    /// A perspective ruler keeps its eye level where it is while its
    /// vanishing points move (Fix eye level).
    pub fix_eye_level: bool,
    /// A Correct Line level, 1 to 5 (Pinch level, Simplify).
    pub level: u32,
    /// A Correct Line amount: Pinch's effect range and Adjust line width's
    /// width, in pixels, or Remove dust's dust size.
    pub amount: f32,
    /// Adjust line width processes the whole line; Simplify vector line
    /// deletes short lines; the Text tool centers its lines.
    pub whole: bool,
    /// The Text tool's font family (empty: Sue's own).
    pub font: String,
    /// The Text tool's letter spacing, in ems.
    pub tracking: f32,
    /// The Text tool's line spacing, times the font's own (0: its own).
    pub leading: f32,
    /// The Text tool's justification: 0 start, 1 middle, 2 end.
    pub justify: u8,
    /// Holding the pen still at a stroke's end makes it a clean shape
    /// (Smart Shape).
    pub smart_shape: bool,
    /// The Figure tool's Polygon: how many corners, 3 to 32.
    pub corners: u32,
    /// The Figure tool's rectangles and polygons: how round their
    /// corners are, in picture pixels (Roundness of corner; 0 sharp).
    pub roundness: f32,
    /// What the Object sub tool picks, bits of `PICKS_*` (Selectable
    /// object).
    pub objects: u8,
    /// What the Object sub tool does on a raster layer where the click
    /// lands on no paint (Operation of transparent part): 0 moves the
    /// layer all the same, 1 picks the layer painted there and moves it,
    /// 2 nothing.
    pub transparent: u8,
    /// Move layer moves the layer painted where the click lands, not the
    /// current one (Move the object at the clicked position).
    pub move_clicked: bool,
    /// The layer [`Self::move_clicked`] moves becomes the current one
    /// (Moved layers become selected).
    pub move_select: bool,
    /// The settings, by their stable names, that Tool Property leaves
    /// out (the closed eyes of Sub Tool Detail, which shows every one).
    pub hidden: Vec<String>,
    /// The Gradient tool's stops ([`GradientMap::with_drawing`] fills in
    /// the drawing colors as it is laid).
    pub gradient: GradientMap,
    /// The Gradient tool's shape.
    pub gradient_shape: Shape,
    /// What the Gradient tool does before its start and past its end
    /// (Edge process).
    pub gradient_edge: Edge,
    /// An Ellipse gradient's short axis against its long one.
    pub gradient_aspect: f32,
    /// The Gradient tool makes a gradient layer that stays editable
    /// instead of painting on the editing layer (Where to create).
    pub new_layer: bool,
    /// How the Gradient tool's colors mix with the pixels under them
    /// (Blending mode).
    pub blend: BlendMode,
    /// Contour line fill: how many pixels in from the outline the
    /// gradient reaches its end (0: the middle of the area).
    pub contour_width: f32,
    /// Close and fill: which of the closed areas fill, judged on the
    /// editing layer (Target color).
    pub close_target: Target,
    /// A rectangle or ellipse selection turns with the pointer after the
    /// drag until a click fixes it (Adjust angle after fixed).
    pub adjust_angle: bool,
}

impl SubTool {
    /// A sub tool.
    #[must_use]
    pub fn new(tool: Tool, name: &str, mode: u8, tip: Tip) -> Self {
        Self {
            tool,
            name: name.to_owned(),
            mode,
            tip,
            stabilization: 5,
            adjust_by_speed: true,
            correction: 0,
            flick: false,
            tolerance: 0.0,
            lines: 1,
            mirror: true,
            gap: 0,
            spread: 0,
            refer_multiple: false,
            refer: Refer::All,
            exclude: Exclude::default(),
            connected_only: true,
            area_aa: AntiAlias::None,
            aspect: Aspect::Free,
            angle_step: 0.0,
            editing_area_only: false,
            locked: false,
            kind: 0,
            fix_eye_level: true,
            level: 2,
            amount: 0.0,
            whole: false,
            font: String::new(),
            tracking: 0.0,
            leading: 0.0,
            justify: 0,
            smart_shape: true,
            corners: 5,
            roundness: 0.0,
            objects: PICKS_VECTOR | PICKS_RASTER | PICKS_COMIC | PICKS_3D | PICKS_CAMERA,
            transparent: 0,
            move_clicked: false,
            move_select: true,
            hidden: Vec::new(),
            gradient: GradientMap::sourced((StopColor::Drawing, 255), (StopColor::Drawing, 0)),
            gradient_shape: Shape::Linear,
            gradient_edge: Edge::Extend,
            gradient_aspect: 0.5,
            new_layer: false,
            blend: BlendMode::Normal,
            contour_width: 0.0,
            close_target: Target::All,
            adjust_angle: false,
        }
    }

    /// The same with another post correction.
    #[must_use]
    pub fn correction(mut self, level: u32) -> Self {
        self.correction = level.min(10);
        self
    }

    /// The same, carrying quick strokes on into a point.
    #[must_use]
    pub fn flick(mut self) -> Self {
        self.flick = true;
        self
    }

    /// The same with another stabilization.
    #[must_use]
    pub fn stabilization(mut self, level: u32) -> Self {
        self.stabilization = level;
        self
    }

    /// The same with another tolerance.
    #[must_use]
    pub fn tolerance(mut self, tolerance: f32) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// The same, looking at the layers `refer` names (Refer multiple on,
    /// unless it is [`Refer::Editing`]).
    #[must_use]
    pub fn referring(mut self, refer: Refer) -> Self {
        self.set_referred(refer);
        self
    }

    /// Looks at the layers `refer` names from now on: Refer multiple goes
    /// off for [`Refer::Editing`], and on with that choice for the others.
    pub fn set_referred(&mut self, refer: Refer) {
        self.refer_multiple = refer != Refer::Editing;
        if self.refer_multiple {
            self.refer = refer;
        }
    }

    /// The layers Fill and Auto Select look at.
    #[must_use]
    pub fn referred(&self) -> Refer {
        if self.refer_multiple {
            self.refer
        } else {
            Refer::Editing
        }
    }

    /// How Fill and Auto Select take their area.
    #[must_use]
    pub fn area(&self) -> crate::fill::AreaSettings {
        crate::fill::AreaSettings {
            tolerance: self.tolerance,
            gap: self.gap,
            spread: self.spread,
            connected_only: self.connected_only,
            antialias: self.area_aa,
        }
    }

    /// The Gradient tool's gradient as it is laid now: the main, sub and
    /// drawing colors filled in where its stops take them.
    #[must_use]
    pub fn gradient_look(
        &self,
        main: [u8; 3],
        sub: [u8; 3],
        drawing: [u8; 3],
    ) -> crate::gradient::Look {
        crate::gradient::Look {
            map: self.gradient.with_drawing(main, sub, drawing),
            shape: self.gradient_shape,
            edge: self.gradient_edge,
            aspect: self.gradient_aspect.clamp(crate::gradient::MIN_ASPECT, 1.0),
        }
    }

    /// Sub tool names Sue Paint has changed, the old name first: sub tools
    /// kept under an old name are found by it, in layouts and by Reset.
    pub const RENAMED: [(&'static str, &'static str); 6] = [
        // Names too close to another program's product names (risk R16).
        ("Real G-Pen", "Grainy G-pen"),
        ("Milli pen", "Fineliner"),
        // Liquify became one sub tool with modes.
        ("Liquify: Push", "Liquify"),
        ("Liquify: Expand", "Liquify"),
        ("Liquify: Pinch", "Liquify"),
        ("Liquify: Twirl", "Liquify"),
    ];

    /// What sub tool name `name` is called now.
    #[must_use]
    pub fn renamed(name: &str) -> &str {
        Self::RENAMED
            .iter()
            .find(|(old, _)| *old == name)
            .map_or(name, |(_, new)| new)
    }

    /// The built-in sub tool of `tool` that `name` is, or is a numbered
    /// copy of ("G-pen 2"), under its name now: the longest match wins.
    #[must_use]
    pub fn builtin(tool: Tool, name: &str) -> Option<Self> {
        let matches = |original: &str, name: &str| {
            name == original || name.starts_with(&format!("{original} "))
        };
        tool.sub_tools()
            .into_iter()
            .filter(|original| {
                matches(&original.name, name)
                    || Self::RENAMED
                        .iter()
                        .any(|(old, new)| *new == original.name && matches(old, name))
            })
            .max_by_key(|original| original.name.len())
    }
}

/// A tab of the Sub Tool palette: sub tools kept together under a name,
/// as Clip Studio Paint's sub tool groups are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubToolGroup {
    /// What people read: for a built-in group its English name, shown
    /// through the language database; else the name someone gave it.
    pub name: String,
    /// How many of its tool's sub tools, in order from where the group
    /// before it ends, it holds (at least one).
    pub len: usize,
}

/// A button of the tool palette: groups of sub tools under one name, as
/// the tools of Clip Studio Paint's tool palette are. A built-in tool is
/// one of these; a sub tool dragged onto the tool palette becomes another,
/// and a tool dragged into a group goes back.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSlot {
    /// What people read: for a built-in tool its English name, shown
    /// through the language database; else the name someone gave it.
    pub name: String,
    /// The tool whose icon it shows: for a built-in tool the tool itself,
    /// for one made from a sub tool that sub tool's tool.
    pub kind: Tool,
    /// The key that picks it, or none.
    pub key: Option<char>,
    subs: Vec<SubTool>,
    groups: Vec<SubToolGroup>,
    picked: usize,
}

impl ToolSlot {
    /// A tool of one group holding `subs` (at least one).
    fn new(name: &str, kind: Tool, key: Option<char>, group: &str, subs: Vec<SubTool>) -> Self {
        let len = subs.len();
        Self {
            name: name.to_owned(),
            kind,
            key,
            subs,
            groups: vec![SubToolGroup {
                name: group.to_owned(),
                len,
            }],
            picked: 0,
        }
    }

    /// Its sub tools, every group's, in order.
    #[must_use]
    pub fn subs(&self) -> &[SubTool] {
        &self.subs
    }

    /// Its groups, in order.
    #[must_use]
    pub fn groups(&self) -> &[SubToolGroup] {
        &self.groups
    }

    /// The index of the picked sub tool.
    #[must_use]
    pub fn picked(&self) -> usize {
        self.picked
    }

    /// The sub tools of group `group`, as indices into [`Self::subs`].
    #[must_use]
    pub fn group_range(&self, group: usize) -> std::ops::Range<usize> {
        let start: usize = self.groups.iter().take(group).map(|g| g.len).sum();
        let len = self.groups.get(group).map_or(0, |g| g.len);
        start..start + len
    }

    /// The group sub tool `index` is in.
    #[must_use]
    pub fn group_of(&self, index: usize) -> usize {
        let mut end = 0;
        for (group, found) in self.groups.iter().enumerate() {
            end += found.len;
            if index < end {
                return group;
            }
        }
        self.groups.len().saturating_sub(1)
    }

    /// `true` if it holds a sub tool of `tool`.
    #[must_use]
    pub fn holds(&self, tool: Tool) -> bool {
        self.subs.iter().any(|sub| sub.tool == tool)
    }

    /// Takes sub tool `index` out, dropping its group if it empties.
    fn take(&mut self, index: usize) -> SubTool {
        let group = self.group_of(index);
        self.groups[group].len -= 1;
        if self.groups[group].len == 0 {
            self.groups.remove(group);
        }
        let sub = self.subs.remove(index);
        if self.picked > index || self.picked >= self.subs.len() {
            self.picked = self.picked.saturating_sub(1);
        }
        sub
    }

    /// Puts `sub` in group `group` at place `at` within it (clamped).
    fn put(&mut self, group: usize, at: usize, sub: SubTool) -> usize {
        let range = self.group_range(group);
        let index = range.start + at.min(range.len());
        self.subs.insert(index, sub);
        self.groups[group].len += 1;
        if self.picked >= index && self.subs.len() > 1 {
            self.picked += 1;
        }
        index
    }

    /// Keeps the picked index inside the list.
    fn settle(&mut self) {
        self.picked = self.picked.min(self.subs.len().saturating_sub(1));
    }
}

/// The tool palette as data: every tool with its groups of sub tools, in
/// order, which tool is current and which sub tool each has picked.
///
/// The functions that take a [`Tool`] act on that tool's tool palette
/// entry: the current one when it is that tool or its picked sub tool is
/// of that tool, else the first entry that is that tool.
#[derive(Clone, Debug, PartialEq)]
pub struct Toolbox {
    slots: Vec<ToolSlot>,
    current: usize,
}

impl Default for Toolbox {
    fn default() -> Self {
        let mut toolbox = Self::catalogue();
        let problems = toolbox.arrange(&layout::ToolLayout::builtin());
        debug_assert!(problems.is_empty(), "{problems:?}");
        toolbox
    }
}

impl Toolbox {
    /// Every built-in sub tool, one tool each, in [`Tool::ALL`]'s order
    /// and with no groups: what [`Self::arrange`] starts from.
    #[must_use]
    pub fn catalogue() -> Self {
        let slots: Vec<ToolSlot> = Tool::ALL
            .iter()
            .map(|tool| {
                let name = tool.english();
                ToolSlot::new(name, *tool, tool.key(), name, tool.sub_tools())
            })
            .collect();
        let current = slots
            .iter()
            .position(|slot| slot.kind == Tool::Pen)
            .unwrap_or(0);
        Self { slots, current }
    }

    /// The tool palette's entries, in order.
    #[must_use]
    pub fn slots(&self) -> &[ToolSlot] {
        &self.slots
    }

    /// The index of the current entry.
    #[must_use]
    pub fn current_slot(&self) -> usize {
        self.current
    }

    /// The current entry: the tool palette button that is lit.
    #[must_use]
    pub fn current_entry(&self) -> &ToolSlot {
        &self.slots[self.current]
    }

    /// Makes entry `slot` current.
    pub fn pick_slot(&mut self, slot: usize) {
        if slot < self.slots.len() {
            self.current = slot;
        }
    }

    /// The first entry with a sub tool that `pick` says yes to.
    #[must_use]
    pub fn slot_holding(&self, pick: impl Fn(&SubTool) -> bool) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.subs.iter().any(&pick))
    }

    /// The entry that is `tool`: the first of that tool with a key (a sub
    /// tool made into a tool has none, so the tool's own button comes
    /// before it), else the first of that tool, else the first holding one
    /// of its sub tools.
    fn home(&self, tool: Tool) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.kind == tool && slot.key.is_some())
            .or_else(|| self.slots.iter().position(|slot| slot.kind == tool))
            .or_else(|| self.slots.iter().position(|slot| slot.holds(tool)))
    }

    /// The entry the functions that take a [`Tool`] act on.
    fn slot_of(&self, tool: Tool) -> usize {
        let current = &self.slots[self.current];
        if current.kind == tool || current.subs[current.picked].tool == tool {
            return self.current;
        }
        self.home(tool).unwrap_or(self.current)
    }

    /// The current tool: the tool of the current entry's picked sub tool.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.current().tool
    }

    /// Picks a tool (if it is available): its first entry.
    pub fn pick(&mut self, tool: Tool) {
        if tool.available()
            && let Some(slot) = self.home(tool)
        {
            self.current = slot;
        }
    }

    /// The sub tools of `tool`'s entry, every group's, in order.
    #[must_use]
    pub fn subs(&self, tool: Tool) -> &[SubTool] {
        &self.slots[self.slot_of(tool)].subs
    }

    /// The index of the picked sub tool of `tool`'s entry.
    #[must_use]
    pub fn picked(&self, tool: Tool) -> usize {
        self.slots[self.slot_of(tool)].picked
    }

    /// Picks sub tool `index` of `tool`'s entry.
    pub fn pick_sub(&mut self, tool: Tool, index: usize) {
        let slot = self.slot_of(tool);
        let slot = &mut self.slots[slot];
        if index < slot.subs.len() {
            slot.picked = index;
        }
    }

    /// Copies sub tool `index` of `tool`'s entry right after it, in its
    /// group, its name with a number, and picks the copy (Duplicate Sub
    /// Tool).
    pub fn duplicate_sub(&mut self, tool: Tool, index: usize) {
        let slot = self.slot_of(tool);
        let slot = &mut self.slots[slot];
        let Some(sub) = slot.subs.get(index).cloned() else {
            return;
        };
        let taken: Vec<&str> = slot.subs.iter().map(|sub| sub.name.as_str()).collect();
        let name = (2..)
            .map(|n| format!("{} {n}", sub.name))
            .find(|name| !taken.contains(&name.as_str()))
            .unwrap_or_else(|| sub.name.clone());
        let group = slot.group_of(index);
        let at = index + 1 - slot.group_range(group).start;
        let copy = SubTool {
            name,
            locked: false,
            ..sub
        };
        slot.picked = slot.put(group, at, copy);
    }

    /// Adds `sub` at the end of a group of its tool's entry and picks it,
    /// making that entry current (the Item Bank's Add, a brush file let
    /// go on the Sub Tool palette): the current entry when it is one of
    /// that tool, into the group of its picked sub tool; else the tool's
    /// own entry, into its last group.
    pub fn add_sub(&mut self, sub: SubTool) {
        let tool = sub.tool;
        let current = (self.slots[self.current].kind == tool).then_some(self.current);
        let Some(slot) = current.or_else(|| self.home(tool)) else {
            self.slots.push(ToolSlot::new(
                tool.english(),
                tool,
                tool.key(),
                tool.english(),
                vec![sub],
            ));
            self.current = self.slots.len() - 1;
            return;
        };
        let entry = &mut self.slots[slot];
        // Into the group shown when the entry is out (a brush file let go
        // on the Sub Tool palette lands where it was let go), else its
        // last.
        let group = if slot == self.current {
            entry.group_of(entry.picked)
        } else {
            entry.groups.len() - 1
        };
        entry.picked = entry.put(group, usize::MAX, sub);
        self.current = slot;
    }

    /// Deletes sub tool `index` of `tool`'s entry; an entry keeps at least
    /// one, and a group that empties goes.
    pub fn delete_sub(&mut self, tool: Tool, index: usize) -> bool {
        let slot = self.slot_of(tool);
        let slot = &mut self.slots[slot];
        if slot.subs.len() <= 1 || index >= slot.subs.len() {
            return false;
        }
        slot.take(index);
        slot.settle();
        true
    }

    /// Puts sub tool `index` of `tool`'s entry back the way Sue Paint
    /// brings it (Reset to Initial Settings); a copy goes back to what it
    /// copied, and a sub tool with a name Sue Paint has since changed to
    /// what it is now called brings. A locked sub tool stays as it is.
    pub fn reset_sub(&mut self, tool: Tool, index: usize) -> bool {
        let slot = self.slot_of(tool);
        let slot = &mut self.slots[slot];
        let Some(sub) = slot.subs.get(index) else {
            return false;
        };
        if sub.locked {
            return false;
        }
        let (kind, name) = (sub.tool, sub.name.clone());
        let Some(original) = SubTool::builtin(kind, &name) else {
            return false;
        };
        slot.subs[index] = SubTool { name, ..original };
        true
    }

    /// The picked sub tool of the current tool.
    #[must_use]
    pub fn current(&self) -> &SubTool {
        let slot = &self.slots[self.current];
        &slot.subs[slot.picked.min(slot.subs.len() - 1)]
    }

    /// The picked sub tool of the current tool, to change its settings.
    /// It does not look at [`SubTool::locked`]; Tool Property asks
    /// [`Self::settings_mut`].
    pub fn current_mut(&mut self) -> &mut SubTool {
        let slot = &mut self.slots[self.current];
        let index = slot.picked.min(slot.subs.len() - 1);
        &mut slot.subs[index]
    }

    /// The picked sub tool of the current tool to change its settings, or
    /// `None` while they are locked.
    pub fn settings_mut(&mut self) -> Option<&mut SubTool> {
        let sub = self.current_mut();
        (!sub.locked).then_some(sub)
    }

    /// Locks or unlocks the settings of the current tool's picked sub tool.
    pub fn lock_settings(&mut self, locked: bool) {
        self.current_mut().locked = locked;
    }

    /// The picked sub tool of `tool`'s entry: one of `tool`'s own when
    /// the picked one is of another tool.
    #[must_use]
    pub fn of(&self, tool: Tool) -> &SubTool {
        let slot = &self.slots[self.slot_of(tool)];
        let picked = &slot.subs[slot.picked.min(slot.subs.len() - 1)];
        if picked.tool == tool {
            return picked;
        }
        slot.subs
            .iter()
            .find(|sub| sub.tool == tool)
            .unwrap_or(picked)
    }

    /// Picks the sub tool before (or, `forward`, after) the current one,
    /// going round (CSP's comma and period).
    pub fn step_sub(&mut self, forward: bool) {
        let slot = &mut self.slots[self.current];
        let count = slot.subs.len().max(1);
        slot.picked = if forward {
            (slot.picked + 1) % count
        } else {
            (slot.picked + count - 1) % count
        };
    }

    /// Picks the next entry with key `key`, starting after the current one
    /// (P switches between the pen and the pencil).
    pub fn cycle(&mut self, key: char) {
        let matching: Vec<usize> = (0..self.slots.len())
            .filter(|at| {
                let slot = &self.slots[*at];
                slot.key == Some(key) && slot.kind.available()
            })
            .collect();
        let next = match matching.iter().position(|at| *at == self.current) {
            Some(at) => matching[(at + 1) % matching.len()],
            None => match matching.first() {
                Some(at) => *at,
                None => return,
            },
        };
        self.current = next;
    }

    // ----- Arranging the tool palette -------------------------------------------------

    /// Moves entry `from` to place `to` (clamped); the current entry stays
    /// current.
    pub fn move_slot(&mut self, from: usize, to: usize) -> bool {
        if from >= self.slots.len() {
            return false;
        }
        let current = self.current;
        let slot = self.slots.remove(from);
        let to = to.min(self.slots.len());
        self.slots.insert(to, slot);
        self.current = moved_index(current, from, to);
        true
    }

    /// Takes entry `slot` off the tool palette with its sub tools (Delete
    /// Tool). The last entry stays; the current one moves to a neighbour.
    pub fn remove_slot(&mut self, slot: usize) -> Option<ToolSlot> {
        if slot >= self.slots.len() || self.slots.len() <= 1 {
            return None;
        }
        let removed = self.slots.remove(slot);
        if self.current > slot || self.current >= self.slots.len() {
            self.current = self.current.saturating_sub(1);
        }
        Some(removed)
    }

    /// Puts `entry` on the tool palette at place `at` (clamped), as a
    /// removed one comes back (Add Tool from Default, undoing Delete
    /// Tool). Returns its place; the current entry stays current.
    pub fn insert_slot(&mut self, at: usize, mut entry: ToolSlot) -> usize {
        if entry.subs.is_empty() {
            return self.current;
        }
        entry.settle();
        let at = at.min(self.slots.len());
        self.slots.insert(at, entry);
        if self.current >= at && self.slots.len() > 1 {
            self.current += 1;
        }
        at
    }

    /// Renames entry `slot`.
    pub fn rename_slot(&mut self, slot: usize, name: &str) -> bool {
        match self.slots.get_mut(slot) {
            Some(found) if !name.trim().is_empty() => {
                name.trim().clone_into(&mut found.name);
                true
            }
            _ => false,
        }
    }

    /// Renames group `group` of entry `slot`.
    pub fn rename_group(&mut self, slot: usize, group: usize, name: &str) -> bool {
        match self
            .slots
            .get_mut(slot)
            .and_then(|found| found.groups.get_mut(group))
        {
            Some(found) if !name.trim().is_empty() => {
                name.trim().clone_into(&mut found.name);
                true
            }
            _ => false,
        }
    }

    /// Moves group `from` of entry `slot` to place `to` among its groups,
    /// its sub tools with it; the picked sub tool stays picked.
    pub fn move_group(&mut self, slot: usize, from: usize, to: usize) -> bool {
        let Some(entry) = self.slots.get_mut(slot) else {
            return false;
        };
        if from >= entry.groups.len() {
            return false;
        }
        let picked = entry.subs[entry.picked].clone();
        let mut runs: Vec<(SubToolGroup, Vec<SubTool>)> = Vec::new();
        let mut subs = std::mem::take(&mut entry.subs).into_iter();
        for group in std::mem::take(&mut entry.groups) {
            let members = subs.by_ref().take(group.len).collect();
            runs.push((group, members));
        }
        let run = runs.remove(from);
        runs.insert(to.min(runs.len()), run);
        for (group, members) in runs {
            entry.groups.push(group);
            entry.subs.extend(members);
        }
        entry.picked = entry
            .subs
            .iter()
            .position(|sub| *sub == picked)
            .unwrap_or(0);
        true
    }

    /// Starts a new group named `name` at sub tool `index` of entry
    /// `slot`: it and the rest of its group form the new group, right
    /// after the old one.
    pub fn split_group(&mut self, slot: usize, index: usize, name: &str) -> bool {
        let Some(entry) = self.slots.get_mut(slot) else {
            return false;
        };
        if index >= entry.subs.len() || name.trim().is_empty() {
            return false;
        }
        let group = entry.group_of(index);
        let range = entry.group_range(group);
        if index == range.start {
            return false;
        }
        entry.groups[group].len = index - range.start;
        entry.groups.insert(
            group + 1,
            SubToolGroup {
                name: name.trim().to_owned(),
                len: range.end - index,
            },
        );
        true
    }

    /// Moves sub tool `index` of entry `from` into group `group` of entry
    /// `to`, at place `at` within the group (clamped). An entry it leaves
    /// empty goes from the tool palette. It stays picked if it was.
    pub fn move_sub(
        &mut self,
        from: usize,
        index: usize,
        to: usize,
        group: usize,
        at: usize,
    ) -> bool {
        let fits = self
            .slots
            .get(from)
            .is_some_and(|slot| index < slot.subs.len())
            && self
                .slots
                .get(to)
                .is_some_and(|slot| group < slot.groups.len());
        if !fits {
            return false;
        }
        let was_picked = self.slots[from].picked == index;
        if from == to {
            let entry = &mut self.slots[from];
            let source = entry.group_of(index);
            let empties = entry.groups[source].len == 1;
            if empties && source == group {
                // Alone in its group: it is there already.
                return true;
            }
            let sub = entry.take(index);
            // Taking it out dropped its group when it was alone in it.
            let group = if empties && source < group {
                group - 1
            } else {
                group
            };
            let placed = entry.put(group, at, sub);
            if was_picked {
                entry.picked = placed;
            }
            entry.settle();
            return true;
        }
        let sub = self.slots[from].take(index);
        let placed = self.slots[to].put(group, at, sub);
        if was_picked {
            self.slots[to].picked = placed;
            if self.current == from {
                self.current = to;
            }
        }
        self.drop_if_empty(from);
        true
    }

    /// Makes sub tool `index` of entry `from` a tool of its own, at place
    /// `at` of the tool palette (Clip Studio Paint's drag onto the tool
    /// palette): one group, named as the sub tool. An entry it leaves
    /// empty goes. Returns the new entry's index.
    pub fn promote_sub(&mut self, from: usize, index: usize, at: usize) -> Option<usize> {
        if self
            .slots
            .get(from)
            .is_none_or(|slot| index >= slot.subs.len())
        {
            return None;
        }
        let was_current = self.current == from && self.slots[from].picked == index;
        let sub = self.slots[from].take(index);
        self.slots[from].settle();
        let name = sub.name.clone();
        let slot = ToolSlot::new(&name, sub.tool, None, &name, vec![sub]);
        let at = at.min(self.slots.len());
        self.slots.insert(at, slot);
        if self.current >= at {
            self.current += 1;
        }
        let from = if from >= at { from + 1 } else { from };
        if was_current {
            self.current = at;
        }
        let gone = self.drop_if_empty(from);
        Some(if gone && from < at { at - 1 } else { at })
    }

    /// Puts every group of entry `from` after the groups of entry `into`,
    /// and takes `from` off the tool palette (a tool made from a sub tool
    /// going back).
    pub fn demote_slot(&mut self, from: usize, into: usize) -> bool {
        if from == into || from >= self.slots.len() || into >= self.slots.len() {
            return false;
        }
        let entry = self.slots.remove(from);
        let into = if into > from { into - 1 } else { into };
        let target = &mut self.slots[into];
        let offset = target.subs.len();
        target.groups.extend(entry.groups);
        target.subs.extend(entry.subs);
        if self.current == from {
            target.picked = offset + entry.picked;
            self.current = into;
        } else if self.current > from {
            self.current -= 1;
        }
        true
    }

    /// Takes entry `slot` off the tool palette when it holds nothing.
    fn drop_if_empty(&mut self, slot: usize) -> bool {
        if !self.slots[slot].subs.is_empty() {
            return false;
        }
        self.slots.remove(slot);
        if self.current > slot || self.current >= self.slots.len() {
            self.current = self.current.saturating_sub(1);
        }
        true
    }

    // ----- Tool layouts ---------------------------------------------------------------

    /// How the tool palette is arranged, as a [`layout::ToolLayout`] a
    /// person can save, share and load.
    #[must_use]
    pub fn layout(&self) -> layout::ToolLayout {
        layout::ToolLayout {
            tools: self
                .slots
                .iter()
                .map(|slot| layout::LayoutTool {
                    name: slot.name.clone(),
                    kind: slot.kind,
                    key: slot.key,
                    groups: slot
                        .groups
                        .iter()
                        .enumerate()
                        .map(|(group, found)| layout::LayoutGroup {
                            name: found.name.clone(),
                            subs: slot.subs[slot.group_range(group)]
                                .iter()
                                .map(|sub| (sub.tool, sub.name.clone()))
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Arranges the tool palette as `layout` says, with the sub tools it
    /// holds now: each sub tool the layout names (by tool and name) goes
    /// where the layout puts it, and the first time only; one it does not
    /// name goes to the end of its tool's entry, so nothing is lost. The
    /// same tool stays current. Returns what in the layout it could not
    /// follow, for people to read.
    pub fn arrange(&mut self, layout: &layout::ToolLayout) -> Vec<String> {
        let mut problems = Vec::new();
        let current_sub = self.current().clone();
        let mut left: Vec<Option<SubTool>> = self
            .slots
            .drain(..)
            .flat_map(|slot| slot.subs)
            .map(Some)
            .collect();
        let mut find = |tool: Tool, name: &str| {
            let renamed = SubTool::renamed(name);
            left.iter_mut()
                .find(|sub| {
                    sub.as_ref().is_some_and(|sub| {
                        sub.tool == tool && (sub.name == name || sub.name == renamed)
                    })
                })
                .and_then(Option::take)
        };
        let mut slots: Vec<ToolSlot> = Vec::new();
        for tool in &layout.tools {
            let mut slot = ToolSlot {
                name: tool.name.clone(),
                kind: tool.kind,
                key: tool.key,
                subs: Vec::new(),
                groups: Vec::new(),
                picked: 0,
            };
            for group in &tool.groups {
                let before = slot.subs.len();
                for (kind, name) in &group.subs {
                    match find(*kind, name) {
                        Some(sub) => slot.subs.push(sub),
                        None => problems.push(format!("{}: {name}", kind.id())),
                    }
                }
                if slot.subs.len() > before {
                    slot.groups.push(SubToolGroup {
                        name: group.name.clone(),
                        len: slot.subs.len() - before,
                    });
                }
            }
            if slot.subs.is_empty() {
                problems.push(format!("{}: {}", tool.kind.id(), tool.name));
            } else {
                slots.push(slot);
            }
        }
        for sub in left.into_iter().flatten() {
            let tool = sub.tool;
            match slots.iter().position(|slot| slot.kind == tool) {
                Some(at) => {
                    let slot = &mut slots[at];
                    let last = slot.groups.len() - 1;
                    slot.put(last, usize::MAX, sub);
                }
                None => slots.push(ToolSlot::new(
                    tool.english(),
                    tool,
                    tool.key(),
                    tool.english(),
                    vec![sub],
                )),
            }
        }
        for slot in &mut slots {
            slot.picked = 0;
        }
        self.slots = slots;
        self.current = self
            .slot_holding(|sub| *sub == current_sub)
            .unwrap_or_else(|| self.home(current_sub.tool).unwrap_or(0));
        if let Some(index) = self.slots[self.current]
            .subs
            .iter()
            .position(|sub| *sub == current_sub)
        {
            self.slots[self.current].picked = index;
        }
        problems
    }
}

/// Where the item that was at `index` is once the item at `from` moved to
/// `to`.
fn moved_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

/// The pen's eraser end, as in Clip Studio Paint: turning the pen over
/// picks the Eraser tool (with its sub tool as last picked), and turning
/// it back picks the tool and sub tool in use before. It switches only
/// between strokes, and a tool picked by hand meanwhile stays picked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EraserEnd {
    /// Whether the eraser end is in use, as last followed.
    on: bool,
    /// The tool palette entry and sub tool to go back to, while the eraser
    /// end is in use and it picked the Eraser.
    before: Option<(usize, usize)>,
}

impl EraserEnd {
    /// Follows the pen: `eraser` says whether its eraser end is in use,
    /// `stroking` whether a stroke (or another gesture) is under way, in
    /// which case nothing changes until it ends. `true` when the tool
    /// changed.
    pub fn follow(&mut self, eraser: bool, stroking: bool, toolbox: &mut Toolbox) -> bool {
        if stroking || eraser == self.on {
            return false;
        }
        self.on = eraser;
        if eraser {
            if toolbox.tool() == Tool::Eraser {
                // On the Eraser already: nothing to go back to.
                self.before = None;
                return false;
            }
            let slot = toolbox.current_slot();
            self.before = Some((slot, toolbox.slots()[slot].picked()));
            toolbox.pick(Tool::Eraser);
            return toolbox.tool() == Tool::Eraser;
        }
        // Back to the tool before, unless another was picked by hand.
        match self.before.take() {
            Some((slot, sub)) if toolbox.tool() == Tool::Eraser => {
                toolbox.pick_slot(slot);
                let entry = &mut toolbox.slots[toolbox.current];
                if sub < entry.subs.len() {
                    entry.picked = sub;
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eraser_end_picks_the_eraser_and_turning_back_the_tool_before() {
        let mut toolbox = Toolbox::default();
        let mut end = EraserEnd::default();
        toolbox.pick(Tool::Brush);
        let brushes = toolbox.subs(Tool::Brush).len();
        toolbox.pick_sub(Tool::Brush, brushes - 1);
        toolbox.pick_sub(Tool::Eraser, toolbox.subs(Tool::Eraser).len() - 1);
        let eraser_sub = toolbox.picked(Tool::Eraser);
        // Mid-stroke nothing changes; once the stroke ends, the Eraser,
        // with its own sub tool kept.
        assert!(!end.follow(true, true, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Brush);
        assert!(end.follow(true, false, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Eraser);
        assert_eq!(toolbox.picked(Tool::Eraser), eraser_sub);
        assert!(!end.follow(true, false, &mut toolbox), "only on a change");
        // Turning back waits for the stroke too, then returns exactly.
        assert!(!end.follow(false, true, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Eraser);
        assert!(end.follow(false, false, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Brush);
        assert_eq!(toolbox.picked(Tool::Brush), brushes - 1);
        // A tool picked by hand while the eraser end is in use stays.
        assert!(end.follow(true, false, &mut toolbox));
        toolbox.pick(Tool::Fill);
        assert!(!end.follow(false, false, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Fill);
        // On the Eraser already, turning back leaves it there.
        toolbox.pick(Tool::Eraser);
        assert!(!end.follow(true, false, &mut toolbox));
        assert!(!end.follow(false, false, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Eraser);
    }

    #[test]
    fn sub_tools_duplicate_delete_and_reset() {
        let mut toolbox = Toolbox::default();
        let first = toolbox.subs(Tool::Pen).len();
        let name = toolbox.subs(Tool::Pen)[0].name.clone();
        toolbox.duplicate_sub(Tool::Pen, 0);
        assert_eq!(toolbox.subs(Tool::Pen).len(), first + 1);
        assert_eq!(toolbox.subs(Tool::Pen)[1].name, format!("{name} 2"));
        assert_eq!(toolbox.picked(Tool::Pen), 1, "the copy is picked");
        toolbox.pick(Tool::Pen);
        toolbox.current_mut().tip.size = 99.0;
        assert!(toolbox.reset_sub(Tool::Pen, 1));
        assert_eq!(
            toolbox.subs(Tool::Pen)[1].tip.size,
            toolbox.subs(Tool::Pen)[0].tip.size,
            "as it came"
        );
        assert_eq!(
            toolbox.subs(Tool::Pen)[1].name,
            format!("{name} 2"),
            "keeping its name"
        );
        assert!(toolbox.delete_sub(Tool::Pen, 1));
        assert_eq!(toolbox.subs(Tool::Pen).len(), first);
        let mut one = Toolbox::default();
        while one.subs(Tool::Text).len() > 1 {
            one.delete_sub(Tool::Text, 0);
        }
        assert!(!one.delete_sub(Tool::Text, 0), "a tool keeps one");
    }

    #[test]
    fn every_tool_has_sub_tools_and_keys_cycle() {
        let mut toolbox = Toolbox::default();
        for tool in Tool::ALL {
            assert!(!toolbox.subs(tool).is_empty(), "{tool:?}");
            assert!(!tool.label().is_empty());
        }
        assert_eq!(toolbox.tool(), Tool::Pen);
        toolbox.cycle('P');
        assert_eq!(toolbox.tool(), Tool::Pencil);
        toolbox.cycle('P');
        assert_eq!(toolbox.tool(), Tool::Pen);
        toolbox.cycle('E');
        assert_eq!(toolbox.tool(), Tool::Eraser);
        assert!(toolbox.current().tip.eraser);
        toolbox.pick(Tool::Decoration);
        assert_eq!(toolbox.tool(), Tool::Decoration, "decorations paint");
        assert!(toolbox.current().tip.shape != 0);
    }

    #[test]
    fn sub_tools_keep_their_own_settings() {
        let mut toolbox = Toolbox::default();
        toolbox.pick(Tool::Brush);
        // Gouache, the first of the Thick paint group.
        toolbox.pick_sub(Tool::Brush, 3);
        assert_eq!(toolbox.current().name, "Gouache");
        toolbox.current_mut().tip.size = 99.0;
        toolbox.pick_sub(Tool::Brush, 0);
        assert_eq!(toolbox.current().tip.size, 80.0, "Round watercolor brush");
        toolbox.pick_sub(Tool::Brush, 3);
        assert_eq!(toolbox.current().tip.size, 99.0);
        toolbox.pick_sub(Tool::Brush, 99);
        assert_eq!(toolbox.picked(Tool::Brush), 3);
        assert_eq!(toolbox.of(Tool::Pen).name, "G-pen");
    }

    /// The names of `slot`'s groups and how many each holds.
    fn groups(toolbox: &Toolbox, slot: usize) -> Vec<(String, usize)> {
        toolbox.slots()[slot]
            .groups()
            .iter()
            .map(|group| (group.name.clone(), group.len))
            .collect()
    }

    fn slot_named(toolbox: &Toolbox, name: &str) -> usize {
        toolbox
            .slots()
            .iter()
            .position(|slot| slot.name == name)
            .unwrap_or_else(|| panic!("no tool {name}"))
    }

    #[test]
    fn the_default_palette_is_clip_studio_paints_order_and_groups() {
        let toolbox = Toolbox::default();
        let names: Vec<&str> = toolbox
            .slots()
            .iter()
            .map(|slot| slot.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Zoom",
                "Hand",
                "Operation",
                "Move Layer",
                "Selection",
                "Auto Select",
                "Eyedropper",
                "Pen",
                "Sketch",
                "Brush",
                "Airbrush",
                "Decoration",
                "Eraser",
                "Blend",
                "Liquify",
                "Fill",
                "Gradient",
                "Figure",
                "Frame Border",
                "Ruler",
                "Text",
                "Balloon",
                "Correct Line",
            ]
        );
        // Every tool but Liquify is the tool of the same name, in
        // Tool::ALL's order.
        let kinds: Vec<Tool> = toolbox
            .slots()
            .iter()
            .filter(|slot| slot.name != "Liquify")
            .map(|slot| slot.kind)
            .collect();
        assert_eq!(kinds, Tool::ALL);
        let pen = slot_named(&toolbox, "Pen");
        assert_eq!(
            groups(&toolbox, pen),
            [("Pen".to_owned(), 7), ("Marker".to_owned(), 4)]
        );
        let figure = slot_named(&toolbox, "Figure");
        assert_eq!(
            groups(&toolbox, figure),
            [
                ("Direct draw".to_owned(), 10),
                ("Saturated Line".to_owned(), 1),
                ("Speed Line".to_owned(), 1)
            ]
        );
        // Fill in Ver. 4's order, Lasso fill among them (A25V-22).
        let fill: Vec<&str> = toolbox
            .subs(Tool::Fill)
            .iter()
            .map(|sub| sub.name.as_str())
            .collect();
        assert_eq!(
            fill,
            [
                "Refer Other Layers",
                "Refer Only Editing Layer",
                "Close and fill",
                "Lasso fill",
                "Paint unpainted area"
            ]
        );
        assert!(
            !toolbox
                .subs(Tool::Figure)
                .iter()
                .any(|sub| sub.name == "Lasso fill"),
            "Lasso fill is not under Figure (A25V-14)"
        );
        // Direct draw ends with Clip Studio Paint's two other curves and
        // its Polygon (A25V-14, FGV-35).
        let direct: Vec<&str> = toolbox
            .subs(Tool::Figure)
            .iter()
            .map(|sub| sub.name.as_str())
            .take(10)
            .collect();
        assert_eq!(
            direct[7..],
            ["Continuous Curve", "Bezier Curve", "Polygon"],
            "{direct:?}"
        );
        // Auto Select's three (CC-2), each looking at its own layers.
        let refers: Vec<Refer> = toolbox
            .subs(Tool::AutoSelect)
            .iter()
            .map(SubTool::referred)
            .collect();
        assert_eq!(refers, [Refer::Editing, Refer::All, Refer::Reference]);
        // Names of Sue's own for the two closest to product names.
        let pens: Vec<&str> = toolbox
            .subs(Tool::Pen)
            .iter()
            .map(|sub| sub.name.as_str())
            .collect();
        assert!(pens.contains(&"Grainy G-pen") && pens.contains(&"Fineliner"));
        assert!(!pens.contains(&"Real G-Pen") && !pens.contains(&"Milli pen"));
    }

    #[test]
    fn tool_names_in_data_are_english_and_stable() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        for tool in Tool::ALL {
            assert_eq!(tool.english(), tool.label(), "{tool:?}");
            assert_eq!(Tool::from_id(tool.id()), Some(tool));
        }
        let mut ids: Vec<&str> = Tool::ALL.iter().map(|tool| tool.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Tool::ALL.len());
        // Built-in names and group names are kept in English and shown
        // through the language database.
        let mut names = layout::ToolLayout::builtin().names();
        for tool in Tool::ALL {
            names.extend(tool.sub_tools().into_iter().map(|sub| sub.name));
        }
        for name in names {
            assert!(
                sue_lang::has_english("paint", &name),
                "{name} has no English message"
            );
        }
    }

    #[test]
    fn liquify_is_a_tool_of_its_own_with_one_sub_tool_and_modes() {
        let mut toolbox = Toolbox::default();
        let liquify = slot_named(&toolbox, "Liquify");
        assert_eq!(toolbox.slots()[liquify].subs().len(), 1);
        assert_eq!(toolbox.slots()[liquify].key, Some('J'), "J, after Blend");
        toolbox.pick_slot(liquify);
        assert_eq!(toolbox.tool(), Tool::Blend);
        assert!(crate::blend::is_liquify(toolbox.current().mode));
        assert_eq!(toolbox.subs(Tool::Blend).len(), 1, "the entry's own list");
        // Every mode is a mode of the one sub tool.
        for mode in crate::blend::LiquifyMode::ALL {
            toolbox.current_mut().mode = mode.mode();
            assert_eq!(
                crate::blend::LiquifyMode::of_mode(toolbox.current().mode),
                Some(mode)
            );
        }
        // Picking Blend goes to the Blend tool, without Liquify.
        toolbox.pick(Tool::Blend);
        assert_eq!(toolbox.current_slot(), slot_named(&toolbox, "Blend"));
        assert!(
            toolbox
                .subs(Tool::Blend)
                .iter()
                .all(|sub| !crate::blend::is_liquify(sub.mode))
        );
        // J goes round Blend and Liquify, as P does the Pen and Sketch.
        toolbox.cycle('J');
        assert_eq!(toolbox.current_slot(), liquify);
        toolbox.cycle('J');
        assert_eq!(toolbox.current_slot(), slot_named(&toolbox, "Blend"));
    }

    #[test]
    fn a_sub_tool_becomes_a_tool_and_goes_back() {
        let mut toolbox = Toolbox::default();
        let pen = slot_named(&toolbox, "Pen");
        let count = toolbox.slots().len();
        let pens = toolbox.slots()[pen].subs().len();
        toolbox.pick(Tool::Pen);
        toolbox.pick_sub(Tool::Pen, 3);
        let turnip = toolbox.current().clone();
        assert_eq!(turnip.name, "Turnip pen");
        // Dragged onto the tool palette, under the Pen.
        let made = toolbox.promote_sub(pen, 3, pen + 1).expect("made");
        assert_eq!(made, pen + 1);
        assert_eq!(toolbox.slots().len(), count + 1);
        let slot = &toolbox.slots()[made];
        assert_eq!(slot.name, "Turnip pen");
        assert_eq!(slot.kind, Tool::Pen);
        assert_eq!(slot.subs(), std::slice::from_ref(&turnip));
        assert_eq!(toolbox.slots()[pen].subs().len(), pens - 1);
        assert_eq!(toolbox.current_slot(), made, "it stays the tool in use");
        assert_eq!(*toolbox.current(), turnip, "with its own settings");
        // The new tool has no key; the Pen still picks with P.
        toolbox.pick(Tool::Pen);
        assert_eq!(toolbox.current_slot(), pen);
        // Back into the Pen, then into its Marker group, as its first.
        toolbox.pick_slot(made);
        assert!(toolbox.demote_slot(made, pen));
        assert_eq!(toolbox.slots().len(), count);
        assert_eq!(toolbox.current_slot(), pen);
        assert_eq!(*toolbox.current(), turnip);
        let groups_now = groups(&toolbox, pen);
        assert_eq!(
            groups_now.last().unwrap().0,
            "Turnip pen",
            "a group of its own"
        );
        let marker = 1;
        let last = toolbox.slots()[pen].subs().len() - 1;
        assert!(toolbox.move_sub(pen, last, pen, marker, 0));
        assert_eq!(
            groups(&toolbox, pen),
            [("Pen".to_owned(), 6), ("Marker".to_owned(), 5)],
            "its emptied group went"
        );
        assert_eq!(toolbox.slots()[pen].subs()[6], turnip);
        assert_eq!(*toolbox.current(), turnip, "still picked");
        // The last sub tool of a tool leaving it takes the tool away.
        let eraser = slot_named(&toolbox, "Eraser");
        let made = toolbox.promote_sub(pen, 0, eraser + 1).unwrap();
        assert!(toolbox.move_sub(made, 0, pen, 0, 0));
        assert_eq!(toolbox.slots().len(), count);
        assert_eq!(toolbox.slots()[pen].subs()[0].name, "G-pen");
    }

    #[test]
    fn tools_and_groups_move_split_and_rename() {
        let mut toolbox = Toolbox::default();
        toolbox.pick(Tool::Fill);
        let fill = toolbox.current_slot();
        let count = toolbox.slots().len();
        assert!(toolbox.move_slot(fill, 0));
        assert_eq!(toolbox.slots()[0].name, "Fill");
        assert_eq!(toolbox.current_slot(), 0, "the tool in use moves with it");
        assert_eq!(toolbox.tool(), Tool::Fill);
        assert!(toolbox.move_slot(0, count));
        assert_eq!(toolbox.slots()[count - 1].name, "Fill");
        assert_eq!(toolbox.current_slot(), count - 1);
        // Split the Pen group at Rough pen, move it first, rename it.
        let pen = slot_named(&toolbox, "Pen");
        toolbox.pick(Tool::Pen);
        toolbox.pick_sub(Tool::Pen, 0);
        assert!(toolbox.split_group(pen, 5, "Rough"));
        assert!(
            !toolbox.split_group(pen, 5, "Again"),
            "already a group's first"
        );
        assert_eq!(
            groups(&toolbox, pen),
            [
                ("Pen".to_owned(), 5),
                ("Rough".to_owned(), 2),
                ("Marker".to_owned(), 4)
            ]
        );
        assert!(toolbox.move_group(pen, 1, 0));
        assert_eq!(toolbox.slots()[pen].subs()[0].name, "Rough pen");
        assert_eq!(toolbox.current().name, "G-pen", "the pick follows");
        assert!(toolbox.rename_group(pen, 0, "Scratchy"));
        assert!(!toolbox.rename_group(pen, 0, "  "));
        assert!(toolbox.rename_slot(pen, "Inks"));
        let layout = toolbox.layout();
        let (read, problems) = layout::ToolLayout::parse(&layout.to_text());
        assert!(problems.is_empty());
        // A fresh toolbox arranged by the saved layout looks the same.
        let mut fresh = Toolbox::default();
        assert!(fresh.arrange(&read).is_empty());
        assert_eq!(fresh.layout(), layout);
    }

    #[test]
    fn sub_tool_edits_keep_groups_whole() {
        let mut toolbox = Toolbox::default();
        let pen = slot_named(&toolbox, "Pen");
        toolbox.pick(Tool::Pen);
        // A copy lands in its original's group.
        toolbox.duplicate_sub(Tool::Pen, 8);
        assert_eq!(
            groups(&toolbox, pen),
            [("Pen".to_owned(), 7), ("Marker".to_owned(), 5)]
        );
        assert_eq!(toolbox.current().name, "Felt pen 2");
        // Deleting a group's every member drops the group.
        let mut one = Toolbox::default();
        let figure = slot_named(&one, "Figure");
        one.pick(Tool::Figure);
        let speed = one.slots()[figure].group_range(2).start;
        assert!(one.delete_sub(Tool::Figure, speed));
        assert_eq!(one.slots()[figure].groups().len(), 2);
        // An Item Bank sub tool lands at the end of the group shown (the
        // Pen's own while its first sub tool is picked).
        let mut bank = Toolbox::default();
        bank.pick(Tool::Pen);
        let mut sub = Tool::Pen.sub_tools()[0].clone();
        sub.name = "From the bank".to_owned();
        bank.add_sub(sub);
        assert_eq!(bank.current().name, "From the bank");
        assert_eq!(bank.tool(), Tool::Pen);
        assert_eq!(groups(&bank, pen)[0], ("Pen".to_owned(), 8));
        assert_eq!(groups(&bank, pen)[1], ("Marker".to_owned(), 4));
    }

    #[test]
    fn locked_settings_hold_and_reset_finds_renamed_sub_tools() {
        let mut toolbox = Toolbox::default();
        toolbox.pick(Tool::Pen);
        toolbox.lock_settings(true);
        assert!(toolbox.settings_mut().is_none());
        assert!(!toolbox.reset_sub(Tool::Pen, 0), "a locked one stays");
        toolbox.lock_settings(false);
        toolbox.settings_mut().expect("unlocked").tip.size = 77.0;
        assert!(toolbox.reset_sub(Tool::Pen, 0));
        assert_eq!(toolbox.current().tip.size, 10.0);
        // A copy kept under an old name resets to the renamed original.
        toolbox.pick_sub(Tool::Pen, 1);
        toolbox.current_mut().name = "Real G-Pen 2".to_owned();
        toolbox.current_mut().tip.size = 50.0;
        assert!(toolbox.reset_sub(Tool::Pen, 1));
        assert_eq!(toolbox.current().name, "Real G-Pen 2", "keeping its name");
        assert_eq!(toolbox.current().tip.size, 10.0);
        // A copy is not locked even when its original is.
        toolbox.lock_settings(true);
        toolbox.duplicate_sub(Tool::Pen, 1);
        assert!(!toolbox.current().locked);
    }

    #[test]
    fn the_eraser_end_goes_back_to_a_tool_made_from_a_sub_tool() {
        let mut toolbox = Toolbox::default();
        let mut end = EraserEnd::default();
        let liquify = slot_named(&toolbox, "Liquify");
        toolbox.pick_slot(liquify);
        assert!(end.follow(true, false, &mut toolbox));
        assert_eq!(toolbox.tool(), Tool::Eraser);
        assert!(end.follow(false, false, &mut toolbox));
        assert_eq!(toolbox.current_slot(), liquify, "not the Blend tool");
    }

    #[test]
    fn an_added_sub_tool_joins_the_current_entry_of_its_tool() {
        let mut toolbox = Toolbox::default();
        let pen = slot_named(&toolbox, "Pen");
        let own = toolbox.promote_sub(pen, 0, 0).unwrap();
        toolbox.pick_slot(own);
        let tip = toolbox.current().tip;
        toolbox.add_sub(SubTool::new(Tool::Pen, "Mine", 0, tip));
        assert_eq!(toolbox.current_slot(), own, "the tool out, a pen too");
        assert_eq!(toolbox.current().name, "Mine");
        assert_eq!(toolbox.current_entry().subs().len(), 2);
        // Out of another tool's entry it goes to its tool's home entry.
        toolbox.pick(Tool::Fill);
        toolbox.add_sub(SubTool::new(Tool::Pen, "Also mine", 0, tip));
        let home = slot_named(&toolbox, "Pen");
        assert_eq!(toolbox.current_slot(), home);
        assert_eq!(toolbox.current().name, "Also mine");
        assert_ne!(home, own);
        // Picking the Pen picks the Pen tool's own button, not the pen
        // made into a tool before it.
        toolbox.pick(Tool::Fill);
        toolbox.pick(Tool::Pen);
        assert_eq!(toolbox.current_slot(), home);
    }

    #[test]
    fn a_removed_tool_comes_back_where_it_was() {
        let mut toolbox = Toolbox::default();
        let fill = slot_named(&toolbox, "Fill");
        toolbox.pick(Tool::Pen);
        let pen = toolbox.current_slot();
        let before = toolbox.clone();
        let removed = toolbox.remove_slot(fill).unwrap();
        assert_eq!(removed.name, "Fill");
        assert_eq!(toolbox.slots().len(), before.slots().len() - 1);
        assert_eq!(toolbox.current_entry().name, "Pen", "the Pen stays out");
        assert_eq!(toolbox.insert_slot(fill, removed), fill);
        assert_eq!(toolbox, before);
        assert_eq!(toolbox.current_slot(), pen);
        // The current entry going leaves a neighbour current.
        toolbox.pick_slot(fill);
        toolbox.remove_slot(fill).unwrap();
        assert!(toolbox.current_slot() < toolbox.slots().len());
        // The last entry stays.
        let mut one = Toolbox::default();
        while one.slots().len() > 1 {
            one.remove_slot(0).unwrap();
        }
        assert!(one.remove_slot(0).is_none());
    }

    #[test]
    fn a_new_sub_tool_hides_nothing_in_tool_property() {
        for slot in Toolbox::default().slots() {
            assert!(slot.subs().iter().all(|sub| sub.hidden.is_empty()));
        }
    }

    #[test]
    fn an_added_sub_tool_lands_in_the_group_shown() {
        let mut toolbox = Toolbox::default();
        toolbox.pick(Tool::Pen);
        let pen = toolbox.current_slot();
        assert!(
            toolbox.current_entry().groups().len() > 1,
            "the Pen has groups"
        );
        // The first group is shown: the new sub tool ends it, not the last.
        toolbox.pick_sub(Tool::Pen, 0);
        let tip = toolbox.current().tip;
        toolbox.add_sub(SubTool::new(Tool::Pen, "Dropped", 0, tip));
        let entry = toolbox.current_entry();
        assert_eq!(toolbox.current_slot(), pen);
        assert_eq!(entry.subs()[entry.picked()].name, "Dropped");
        assert_eq!(entry.group_of(entry.picked()), 0);
        assert_eq!(entry.picked(), entry.group_range(0).end - 1);
    }
}
