//! Tools and sub tools.
//!
//! The tool palette lists [`Tool`]s; each tool has [`SubTool`]s, ready-made
//! settings such as a G-pen or a soft airbrush, picked in the Sub Tool
//! palette and tuned in the Tool Property palette. Tools that are not built
//! yet stay in the palette, grayed out.

use crate::brush::{AntiAlias, Tip};

/// A tool of the tool palette.
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
            Self::Brush | Self::Airbrush => Some('B'),
            Self::Eraser => Some('E'),
            Self::Fill | Self::Gradient => Some('G'),
            Self::Figure => Some('U'),
            Self::Text | Self::Balloon => Some('T'),
            Self::Operation => Some('O'),
            Self::Zoom => Some('/'),
            Self::Blend => Some('J'),
            Self::Frame | Self::Ruler => Some('U'),
            Self::CorrectLine => Some('Y'),
            _ => None,
        }
    }

    /// `true` if it works in this build.
    #[must_use]
    pub const fn available(self) -> bool {
        !matches!(self, Self::Decoration)
    }

    /// `true` for tools that paint with a [`Tip`].
    #[must_use]
    pub const fn paints(self) -> bool {
        matches!(
            self,
            Self::Pen | Self::Pencil | Self::Brush | Self::Airbrush | Self::Eraser | Self::Figure
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
            Self::Zoom => vec![plain(&tr!("zoom-in-2"), 0), plain(&tr!("zoom-out-2"), 1)],
            Self::Hand => vec![plain(&tr!("hand"), 0), plain(&tr!("rotate"), 1)],
            Self::MoveLayer => vec![plain(&tr!("move-layer"), 0)],
            // Clip Studio Paint's Selection area sub tools.
            Self::Select => vec![
                plain(&tr!("rectangle"), 0),
                plain(&tr!("ellipse"), 1),
                plain(&tr!("lasso"), 2),
                plain(&tr!("polyline"), 3),
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
            ],
            Self::AutoSelect => vec![
                plain(&tr!("refer-only-editing-layer"), 0).tolerance(0.1),
                plain(&tr!("refer-other-layers"), 1).tolerance(0.1),
            ],
            Self::Eyedropper => vec![
                plain(&tr!("obtain-display-color-2"), 0),
                plain(&tr!("obtain-layer-color-2"), 1),
            ],
            // Clip Studio Paint's Pen tool: its sub tools in its order. Pen
            // pressure thins them; none tapers on its own, so a mouse draws
            // them at full size from the first point.
            Self::Pen => vec![
                SubTool::new(self, "G-pen", 0, tip(8.0, |_| {}))
                    .stabilization(6)
                    .correction(2)
                    .flick(),
                SubTool::new(
                    self,
                    "Real G-Pen",
                    0,
                    tip(8.0, |t| {
                        t.size_by.pressure_min = 0.05;
                        t.grain = 0.08;
                        t.aa = AntiAlias::Medium;
                    }),
                )
                .stabilization(6)
                .correction(1),
                SubTool::new(
                    self,
                    "Mapping pen",
                    0,
                    tip(4.0, |t| t.size_by.pressure_min = 0.05),
                )
                .stabilization(8)
                .correction(3)
                .flick(),
                SubTool::new(
                    self,
                    "Turnip pen",
                    0,
                    tip(14.0, |t| {
                        t.size_by.pressure_min = 0.3;
                        t.aa = AntiAlias::Medium;
                    }),
                )
                .stabilization(4)
                .correction(1)
                .flick(),
                SubTool::new(
                    self,
                    "Textured pen",
                    0,
                    tip(10.0, |t| {
                        t.size_by.pressure_min = 0.2;
                        t.grain = 0.35;
                    }),
                )
                .stabilization(4),
                SubTool::new(
                    self,
                    "Rough pen",
                    0,
                    tip(10.0, |t| {
                        t.size_by.pressure_min = 0.2;
                        t.grain = 0.6;
                        t.hardness = 0.8;
                    }),
                )
                .stabilization(3),
                SubTool::new(
                    self,
                    "Calligraphy",
                    0,
                    tip(16.0, |t| {
                        t.size_by.pressure_min = 0.15;
                        t.size_by.velocity = true;
                        t.size_by.velocity_min = 0.4;
                        t.aa = AntiAlias::Strong;
                    }),
                )
                .stabilization(6),
                SubTool::new(
                    self,
                    "Milli pen",
                    0,
                    tip(2.0, |t| t.size_by.pressure = false),
                )
                .stabilization(6)
                .correction(1),
                SubTool::new(
                    self,
                    "Felt pen",
                    0,
                    tip(8.0, |t| {
                        t.size_by.pressure = false;
                        t.opacity = 0.9;
                        t.hardness = 0.8;
                        t.aa = AntiAlias::Strong;
                    }),
                )
                .stabilization(4),
                SubTool::new(
                    self,
                    "Dot pen",
                    0,
                    tip(1.0, |t| {
                        t.size_by.pressure = false;
                        t.aa = AntiAlias::None;
                        t.at_least_one = true;
                    }),
                ),
                SubTool::new(
                    self,
                    "Flat marker",
                    0,
                    tip(12.0, |t| {
                        t.size_by.pressure = false;
                        t.opacity = 0.85;
                        t.aa = AntiAlias::Strong;
                    }),
                )
                .stabilization(3),
            ],
            // Clip Studio Paint's Sketch tool (once the Pencil tool).
            Self::Pencil => vec![
                SubTool::new(
                    self,
                    "Pencil",
                    0,
                    tip(6.0, |t| {
                        t.grain = 0.45;
                        t.hardness = 0.85;
                        t.size_by.pressure_min = 0.4;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(3),
                SubTool::new(
                    self,
                    "Mechanical pencil",
                    0,
                    tip(3.0, |t| {
                        t.grain = 0.3;
                        t.size_by.pressure = false;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(3),
                SubTool::new(
                    self,
                    "Charcoal",
                    0,
                    tip(20.0, |t| {
                        t.grain = 0.8;
                        t.hardness = 0.5;
                        t.size_by.pressure_min = 0.5;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(2),
                SubTool::new(
                    self,
                    "Crayon",
                    0,
                    tip(14.0, |t| {
                        t.grain = 0.65;
                        t.hardness = 0.75;
                        t.size_by.pressure_min = 0.6;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(2),
                SubTool::new(
                    self,
                    "Pastel",
                    0,
                    tip(24.0, |t| {
                        t.grain = 0.5;
                        t.hardness = 0.4;
                        t.size_by.pressure_min = 0.6;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(2),
                SubTool::new(
                    self,
                    "Chalk",
                    0,
                    tip(22.0, |t| {
                        t.grain = 0.75;
                        t.hardness = 0.6;
                        t.size_by.pressure_min = 0.7;
                        t.opacity_by.pressure = true;
                    }),
                )
                .stabilization(2),
            ],
            Self::Brush => vec![
                SubTool::new(
                    self,
                    "Watercolor",
                    0,
                    tip(40.0, |t| {
                        t.hardness = 0.3;
                        t.build_up = true;
                        t.flow = 0.15;
                        t.opacity = 0.8;
                        t.mix = 0.35;
                    }),
                )
                .stabilization(4),
                SubTool::new(
                    self,
                    "Opaque Watercolor",
                    0,
                    tip(30.0, |t| {
                        t.hardness = 0.7;
                        t.build_up = true;
                        t.flow = 0.4;
                        t.mix = 0.5;
                    }),
                )
                .stabilization(4),
                SubTool::new(
                    self,
                    "Gouache",
                    0,
                    tip(30.0, |t| {
                        t.size_by.pressure_min = 0.6;
                        t.aa = AntiAlias::Medium;
                        t.flow = 0.9;
                        t.mix = 0.2;
                    }),
                )
                .stabilization(4),
                SubTool::new(
                    self,
                    "Ink Brush",
                    0,
                    tip(20.0, |t| {
                        t.size_by.pressure_min = 0.02;
                        t.aa = AntiAlias::Medium;
                        t.taper_start = 20.0;
                        t.taper_end = 40.0;
                    }),
                )
                .stabilization(6)
                .correction(2)
                .flick(),
            ],
            Self::Airbrush => vec![
                SubTool::new(
                    self,
                    "Soft",
                    0,
                    tip(80.0, |t| {
                        t.hardness = 0.0;
                        t.build_up = true;
                        t.flow = 0.06;
                        t.size_by.pressure = false;
                        t.opacity_by.pressure = true;
                    }),
                ),
                SubTool::new(
                    self,
                    "Strong",
                    0,
                    tip(60.0, |t| {
                        t.hardness = 0.3;
                        t.build_up = true;
                        t.flow = 0.2;
                        t.size_by.pressure = false;
                    }),
                ),
                SubTool::new(
                    self,
                    "Fine",
                    0,
                    tip(24.0, |t| {
                        t.hardness = 0.2;
                        t.build_up = true;
                        t.flow = 0.15;
                    }),
                ),
            ],
            Self::Eraser => vec![
                SubTool::new(
                    self,
                    "Hard",
                    0,
                    tip(20.0, |t| {
                        t.eraser = true;
                        t.size_by.pressure = false;
                    }),
                )
                .stabilization(2),
                SubTool::new(
                    self,
                    "Soft",
                    0,
                    tip(60.0, |t| {
                        t.eraser = true;
                        t.hardness = 0.2;
                        t.build_up = true;
                        t.flow = 0.3;
                        t.size_by.pressure = false;
                    }),
                )
                .stabilization(2),
                SubTool::new(
                    self,
                    "Rough",
                    0,
                    tip(30.0, |t| {
                        t.eraser = true;
                        t.grain = 0.6;
                    }),
                )
                .stabilization(2),
                // Clip Studio Paint's Vector eraser: up to intersection.
                SubTool {
                    kind: 1,
                    ..SubTool::new(
                        self,
                        "Vector",
                        0,
                        tip(10.0, |t| {
                            t.eraser = true;
                            t.size_by.pressure = false;
                        }),
                    )
                    .stabilization(2)
                },
            ],
            Self::Fill => vec![
                plain(&tr!("refer-only-editing-layer"), 0).tolerance(0.1),
                plain(&tr!("refer-other-layers"), 1).tolerance(0.1),
            ],
            Self::Gradient => vec![
                plain(&tr!("drawing-color-to-transparent"), 0),
                plain(&tr!("main-to-sub-color"), 1),
                plain(&tr!("radial"), 2),
            ],
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
                ]
            }
            Self::Frame => vec![
                plain(&tr!("rectangle-frame"), 0),
                plain(&tr!("frame-border-division"), 1),
            ],
            Self::Balloon => {
                let outline = tip(3.0, |t| {
                    t.size_by.pressure = false;
                });
                vec![
                    SubTool::new(self, "Ellipse Balloon", 0, outline),
                    SubTool::new(self, "Balloon Tail", 1, outline),
                ]
            }
            // Clip Studio Paint's Create ruler group, in its order.
            Self::Ruler => vec![
                plain(&tr!("linear-ruler"), 5),
                plain(&tr!("curve-ruler"), 6),
                plain(&tr!("figure-ruler"), 7),
                plain(&tr!("ruler-pen"), 8),
                plain(&tr!("special-ruler-2"), 2),
                plain(&tr!("perspective-ruler"), 1),
                SubTool {
                    lines: 2,
                    ..plain(&tr!("symmetrical-ruler"), 0)
                },
            ],
            Self::Text => vec![plain(&tr!("text"), 0)],
            Self::Blend => {
                let soft = tip(60.0, |t| {
                    t.opacity = 0.6;
                    t.size_by.pressure = false;
                });
                vec![
                    SubTool::new(self, "Blur", 0, soft),
                    SubTool::new(self, "Finger Tip", 1, soft),
                    SubTool::new(self, "Liquify: Push", 2, soft),
                    SubTool::new(self, "Liquify: Expand", 3, soft),
                    SubTool::new(self, "Liquify: Pinch", 4, soft),
                    SubTool::new(self, "Liquify: Twirl", 5, soft),
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
                    plain(&tr!("control-point"), 0),
                    SubTool {
                        level: 2,
                        amount: 30.0,
                        ..plain(&tr!("pinch-vector-line"), 1)
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
                    plain(&tr!("redraw-vector-line"), 5).stabilization(6),
                    SubTool::new(self, "Redraw vector line width", 6, brush(10.0)),
                    SubTool {
                        amount: 10.0,
                        ..plain(&tr!("remove-dust-2"), 7)
                    },
                ]
            }
            // Clip Studio Paint's Operation group.
            Self::Operation => vec![plain(&tr!("object"), 0), plain(&tr!("select-layer"), 1)],
            Self::Decoration => {
                vec![plain(&self.label(), 0)]
            }
        }
    }
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
    /// A fill's Area scaling, in pixels under the lines.
    pub spread: u32,
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
    /// deletes short lines.
    pub whole: bool,
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
            kind: 0,
            fix_eye_level: true,
            level: 2,
            amount: 0.0,
            whole: false,
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
}

/// Every tool's sub tools and which one is picked.
#[derive(Clone, Debug, PartialEq)]
pub struct Toolbox {
    sets: Vec<(Tool, Vec<SubTool>, usize)>,
    tool: Tool,
}

impl Default for Toolbox {
    fn default() -> Self {
        Self {
            sets: Tool::ALL
                .iter()
                .map(|tool| (*tool, tool.sub_tools(), 0))
                .collect(),
            tool: Tool::Pen,
        }
    }
}

impl Toolbox {
    /// The current tool.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Picks a tool (if it is available).
    pub fn pick(&mut self, tool: Tool) {
        if tool.available() {
            self.tool = tool;
        }
    }

    fn set(&self, tool: Tool) -> &(Tool, Vec<SubTool>, usize) {
        self.sets
            .iter()
            .find(|(t, _, _)| *t == tool)
            .unwrap_or(&self.sets[0])
    }

    fn set_mut(&mut self, tool: Tool) -> &mut (Tool, Vec<SubTool>, usize) {
        let index = self
            .sets
            .iter()
            .position(|(t, _, _)| *t == tool)
            .unwrap_or(0);
        &mut self.sets[index]
    }

    /// The sub tools of `tool`.
    #[must_use]
    pub fn subs(&self, tool: Tool) -> &[SubTool] {
        &self.set(tool).1
    }

    /// The index of the picked sub tool of `tool`.
    #[must_use]
    pub fn picked(&self, tool: Tool) -> usize {
        self.set(tool).2
    }

    /// Picks sub tool `index` of `tool`.
    pub fn pick_sub(&mut self, tool: Tool, index: usize) {
        let set = self.set_mut(tool);
        if index < set.1.len() {
            set.2 = index;
        }
    }

    /// Copies sub tool `index` of `tool` right after it, its name with a
    /// number, and picks the copy (Duplicate Sub Tool).
    pub fn duplicate_sub(&mut self, tool: Tool, index: usize) {
        let set = self.set_mut(tool);
        let Some(sub) = set.1.get(index).cloned() else {
            return;
        };
        let taken: Vec<String> = set.1.iter().map(|sub| sub.name.clone()).collect();
        let name = (2..)
            .map(|n| format!("{} {n}", sub.name))
            .find(|name| !taken.contains(name))
            .unwrap_or_else(|| sub.name.clone());
        set.1.insert(index + 1, SubTool { name, ..sub });
        set.2 = index + 1;
    }

    /// Adds `sub` at the end of its tool's sub tools and picks it (the
    /// Item Bank's Add).
    pub fn add_sub(&mut self, sub: SubTool) {
        let tool = sub.tool;
        let set = self.set_mut(tool);
        set.1.push(sub);
        set.2 = set.1.len() - 1;
        self.pick(tool);
    }

    /// Deletes sub tool `index` of `tool`; a tool keeps at least one.
    pub fn delete_sub(&mut self, tool: Tool, index: usize) -> bool {
        let set = self.set_mut(tool);
        if set.1.len() <= 1 || index >= set.1.len() {
            return false;
        }
        set.1.remove(index);
        set.2 = set.2.min(set.1.len() - 1);
        true
    }

    /// Puts sub tool `index` of `tool` back the way Sue Paint brings it
    /// (Reset to Initial Settings); a copy goes back to what it copied.
    pub fn reset_sub(&mut self, tool: Tool, index: usize) -> bool {
        let set = self.set_mut(tool);
        let Some(sub) = set.1.get(index) else {
            return false;
        };
        let name = sub.name.clone();
        let Some(original) = tool
            .sub_tools()
            .into_iter()
            .filter(|original| {
                name == original.name || name.starts_with(&format!("{} ", original.name))
            })
            .max_by_key(|original| original.name.len())
        else {
            return false;
        };
        set.1[index] = SubTool { name, ..original };
        true
    }

    /// The picked sub tool of the current tool.
    #[must_use]
    pub fn current(&self) -> &SubTool {
        let (_, subs, index) = self.set(self.tool);
        &subs[(*index).min(subs.len() - 1)]
    }

    /// The picked sub tool of the current tool, to change its settings.
    pub fn current_mut(&mut self) -> &mut SubTool {
        let tool = self.tool;
        let (_, subs, index) = self.set_mut(tool);
        let index = (*index).min(subs.len() - 1);
        &mut subs[index]
    }

    /// The picked sub tool of `tool`.
    #[must_use]
    pub fn of(&self, tool: Tool) -> &SubTool {
        let (_, subs, index) = self.set(tool);
        &subs[(*index).min(subs.len() - 1)]
    }

    /// Picks the sub tool before (or, `forward`, after) the current one,
    /// going round (CSP's comma and period).
    pub fn step_sub(&mut self, forward: bool) {
        let tool = self.tool;
        let set = self.set_mut(tool);
        let count = set.1.len().max(1);
        set.2 = if forward {
            (set.2 + 1) % count
        } else {
            (set.2 + count - 1) % count
        };
    }

    /// Picks the next tool sharing `key`, starting after the current one
    /// (P switches between the pen and the pencil).
    pub fn cycle(&mut self, key: char) {
        let matching: Vec<Tool> = Tool::ALL
            .iter()
            .copied()
            .filter(|tool| tool.key() == Some(key) && tool.available())
            .collect();
        let next = match matching.iter().position(|tool| *tool == self.tool) {
            Some(at) => matching[(at + 1) % matching.len()],
            None => match matching.first() {
                Some(tool) => *tool,
                None => return,
            },
        };
        self.tool = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            toolbox.tool(),
            Tool::Eraser,
            "unavailable tools stay unpicked"
        );
    }

    #[test]
    fn sub_tools_keep_their_own_settings() {
        let mut toolbox = Toolbox::default();
        toolbox.pick(Tool::Brush);
        toolbox.pick_sub(Tool::Brush, 2);
        assert_eq!(toolbox.current().name, "Gouache");
        toolbox.current_mut().tip.size = 99.0;
        toolbox.pick_sub(Tool::Brush, 0);
        assert_eq!(toolbox.current().tip.size, 40.0);
        toolbox.pick_sub(Tool::Brush, 2);
        assert_eq!(toolbox.current().tip.size, 99.0);
        toolbox.pick_sub(Tool::Brush, 99);
        assert_eq!(toolbox.picked(Tool::Brush), 2);
        assert_eq!(toolbox.of(Tool::Pen).name, "G-pen");
    }
}
