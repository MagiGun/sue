//! 3D layers, as in Clip Studio Paint: a layer that shows a 3D model (one
//! of Sue Paint's shapes, or a model imported from a file) as the camera
//! sees it, drawn shaded, as line art, or both, and turned into lines and
//! tones to draw over.
//!
//! A 3D layer is a picture layer of class [`CLASS`] linked by [`REF`] to a
//! model entity (class `sue.model`, its shape the shared [`Geometry`]); the
//! camera, light and look are its properties, and its pixels are the model
//! drawn by a small software renderer (a depth buffer, flat shading, and
//! lines where the depth jumps or the surface folds).

use sue_core::txn::{ReplaceComponent, SetProperty, SpawnEntity};
use sue_core::{
    Arena, CHILD, Entity, EntityId, MeshBuffer3D, PixelFormat, REF, RasterTile2D, Relation, Value,
};
use sue_model::{Geometry, PrimKind};

use crate::doc::Edit;

/// The class of a 3D layer.
pub const CLASS: &str = "sue.paint.layer3d";
/// How far round the model the camera is, in degrees (`Float`).
pub const YAW: &str = "view3d.yaw";
/// How far above it, in degrees (`Float`).
pub const PITCH: &str = "view3d.pitch";
/// How far away, in model sizes (`Float`).
pub const DISTANCE: &str = "view3d.distance";
/// The camera's field of view, in degrees (`Float`).
pub const FOV: &str = "view3d.fov";
/// Where the light comes from, in degrees round the model (`Float`).
pub const LIGHT: &str = "view3d.light";
/// How it is drawn: 0 shaded, 1 lines, 2 shaded with lines (`Int`).
pub const STYLE: &str = "view3d.style";
/// The lines' width in pixels (`Float`).
pub const LINE_WIDTH: &str = "view3d.line_width";
/// How sharp a fold must be to draw a line, in degrees (`Float`).
pub const CREASE: &str = "view3d.crease";
/// How far the camera has moved across and down, in model sizes (`Float`).
pub const PAN_X: &str = "view3d.pan_x";
/// See [`PAN_X`].
pub const PAN_Y: &str = "view3d.pan_y";
/// How strong the light is, 0 to 2 (`Float`).
pub const INTENSITY: &str = "view3d.intensity";
/// How much light reaches the parts the light does not, 0 to 1 (`Float`).
pub const AMBIENT: &str = "view3d.ambient";
/// Whether the model casts shadows on itself (`Bool`).
pub const SHADOWS: &str = "view3d.shadows";
/// The lines' color, `0xRRGGBB` (`Int`).
pub const LINE_COLOR: &str = "view3d.line_color";
/// The lines' opacity, 0 to 1 (`Float`).
pub const LINE_OPACITY: &str = "view3d.line_opacity";

/// What people read for the styles, in order.
pub const STYLES: [&str; 3] = ["Shaded", "Line art", "Shaded with lines"];

/// Sue Paint's own shapes.
pub const SHAPES: [&str; 7] = [
    "Cube", "Sphere", "Cylinder", "Cone", "Plane", "Prism", "Pyramid",
];

/// Clip Studio Paint's camera presets, in its order: where the camera
/// looks from, as a turn round and a height in degrees.
pub const CAMERAS: [(&str, f32, f32); 6] = [
    ("Front", 0.0, 0.0),
    ("Back", 180.0, 0.0),
    ("Left", -90.0, 0.0),
    ("Right", 90.0, 0.0),
    ("Top", 0.0, 89.0),
    ("Bottom", 0.0, -89.0),
];

/// How a 3D layer looks at its model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3D {
    /// Degrees round.
    pub yaw: f32,
    /// Degrees above.
    pub pitch: f32,
    /// How far away, in model sizes.
    pub distance: f32,
    /// Field of view, degrees.
    pub fov: f32,
    /// Light, degrees round.
    pub light: f32,
    /// 0 shaded, 1 lines, 2 both.
    pub style: u8,
    /// Line width, pixels.
    pub line_width: f32,
    /// Crease angle, degrees.
    pub crease: f32,
    /// The camera moved across and down.
    pub pan: [f32; 2],
    /// How strong the light is.
    pub intensity: f32,
    /// How much light the unlit parts get.
    pub ambient: f32,
    /// The model casts shadows on itself.
    pub shadows: bool,
    /// The lines' color.
    pub line_color: [u8; 3],
    /// The lines' opacity.
    pub line_opacity: f32,
}

impl Default for View3D {
    fn default() -> Self {
        Self {
            yaw: 35.0,
            pitch: 25.0,
            distance: 5.0,
            fov: 40.0,
            light: 45.0,
            style: 2,
            line_width: 2.0,
            crease: 40.0,
            pan: [0.0; 2],
            intensity: 1.0,
            ambient: 0.25,
            shadows: false,
            line_color: [0; 3],
            line_opacity: 1.0,
        }
    }
}

impl View3D {
    /// The view of the 3D layer `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let number = |key: &str, or: f32| {
            entity
                .property(key)
                .and_then(Value::as_f64)
                .map_or(or, |value| value as f32)
        };
        let still = Self::default();
        Self {
            yaw: number(YAW, still.yaw),
            pitch: number(PITCH, still.pitch).clamp(-89.0, 89.0),
            distance: number(DISTANCE, still.distance).max(0.5),
            fov: number(FOV, still.fov).clamp(5.0, 150.0),
            light: number(LIGHT, still.light),
            style: entity
                .property(STYLE)
                .and_then(Value::as_i64)
                .map_or(still.style, |value| value.clamp(0, 2) as u8),
            line_width: number(LINE_WIDTH, still.line_width).max(0.0),
            crease: number(CREASE, still.crease).clamp(1.0, 179.0),
            pan: [number(PAN_X, 0.0), number(PAN_Y, 0.0)],
            intensity: number(INTENSITY, still.intensity).clamp(0.0, 2.0),
            ambient: number(AMBIENT, still.ambient).clamp(0.0, 1.0),
            shadows: entity
                .property(SHADOWS)
                .and_then(Value::as_bool)
                .unwrap_or(still.shadows),
            line_color: entity
                .property(LINE_COLOR)
                .and_then(Value::as_i64)
                .map_or(still.line_color, |rgb| {
                    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                }),
            line_opacity: number(LINE_OPACITY, still.line_opacity).clamp(0.0, 1.0),
        }
    }

    /// The view from Clip Studio Paint's camera preset `index` in
    /// [`CAMERAS`], centered again, the rest kept.
    #[must_use]
    pub fn preset(self, index: usize) -> Self {
        let (_, yaw, pitch) = CAMERAS[index.min(CAMERAS.len() - 1)];
        Self {
            yaw,
            pitch,
            pan: [0.0; 2],
            ..self
        }
    }

    /// The view as property edits on `layer`.
    #[must_use]
    pub fn edits(&self, layer: EntityId) -> Vec<SetProperty> {
        vec![
            SetProperty::new(layer, YAW, f64::from(self.yaw)),
            SetProperty::new(layer, PITCH, f64::from(self.pitch)),
            SetProperty::new(layer, DISTANCE, f64::from(self.distance)),
            SetProperty::new(layer, FOV, f64::from(self.fov)),
            SetProperty::new(layer, LIGHT, f64::from(self.light)),
            SetProperty::new(layer, STYLE, i64::from(self.style)),
            SetProperty::new(layer, LINE_WIDTH, f64::from(self.line_width)),
            SetProperty::new(layer, CREASE, f64::from(self.crease)),
            SetProperty::new(layer, PAN_X, f64::from(self.pan[0])),
            SetProperty::new(layer, PAN_Y, f64::from(self.pan[1])),
            SetProperty::new(layer, INTENSITY, f64::from(self.intensity)),
            SetProperty::new(layer, AMBIENT, f64::from(self.ambient)),
            SetProperty::new(layer, SHADOWS, self.shadows),
            SetProperty::new(
                layer,
                LINE_COLOR,
                i64::from(self.line_color[0]) << 16
                    | i64::from(self.line_color[1]) << 8
                    | i64::from(self.line_color[2]),
            ),
            SetProperty::new(layer, LINE_OPACITY, f64::from(self.line_opacity)),
        ]
    }
}

/// `true` if `layer` is a 3D layer.
#[must_use]
pub fn is_3d(arena: &Arena, layer: EntityId) -> bool {
    arena
        .get_by_id(layer)
        .is_ok_and(|entity| entity.class() == CLASS)
}

/// The model a 3D layer shows.
#[must_use]
pub fn model_of(arena: &Arena, layer: EntityId) -> Option<EntityId> {
    arena.relations().targets(layer, REF).into_iter().next()
}

/// One of Sue Paint's shapes, by its number in [`SHAPES`].
#[must_use]
pub fn shape(kind: usize) -> Geometry {
    let mut geometry = Geometry::new();
    let face = |geometry: &mut Geometry, points: &[[f64; 3]]| {
        let ids: Vec<u32> = points
            .iter()
            .map(|point| geometry.add_point(*point))
            .collect();
        let _ = geometry.add_prim(PrimKind::Polygon, &ids);
    };
    let round = |steps: usize, at: usize| {
        let theta = at as f64 / steps as f64 * std::f64::consts::TAU;
        (theta.cos(), theta.sin())
    };
    match kind {
        1 => {
            // A sphere of 24 by 16 faces.
            let (around, up) = (24, 16);
            let at = |i: usize, j: usize| {
                let phi = j as f64 / up as f64 * std::f64::consts::PI;
                let (c, s) = round(around, i);
                [phi.sin() * c, phi.cos(), phi.sin() * s]
            };
            for j in 0..up {
                for i in 0..around {
                    face(
                        &mut geometry,
                        &[at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)],
                    );
                }
            }
        }
        2 | 3 | 5 | 6 => {
            // A cylinder (or a cone) of 32 sides with its caps; a prism of
            // 3 sides, a pyramid of 4.
            let sides = match kind {
                5 => 3,
                6 => 4,
                _ => 32,
            };
            let top = if kind == 3 || kind == 6 { 0.0 } else { 1.0 };
            for i in 0..sides {
                let (c0, s0) = round(sides, i);
                let (c1, s1) = round(sides, i + 1);
                face(
                    &mut geometry,
                    &[
                        [c0, -1.0, s0],
                        [c1, -1.0, s1],
                        [c1 * top, 1.0, s1 * top],
                        [c0 * top, 1.0, s0 * top],
                    ],
                );
            }
            let bottom: Vec<[f64; 3]> = (0..sides)
                .rev()
                .map(|i| {
                    let (c, s) = round(sides, i);
                    [c, -1.0, s]
                })
                .collect();
            face(&mut geometry, &bottom);
            if top > 0.0 {
                let cap: Vec<[f64; 3]> = (0..sides)
                    .map(|i| {
                        let (c, s) = round(sides, i);
                        [c, 1.0, s]
                    })
                    .collect();
                face(&mut geometry, &cap);
            }
        }
        4 => face(
            &mut geometry,
            &[
                [-1.0, 0.0, -1.0],
                [-1.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 0.0, -1.0],
            ],
        ),
        _ => {
            // A cube, its faces wound outwards.
            let c = |x: f64, y: f64, z: f64| [x, y, z];
            for quad in [
                [
                    c(-1., -1., 1.),
                    c(1., -1., 1.),
                    c(1., 1., 1.),
                    c(-1., 1., 1.),
                ],
                [
                    c(1., -1., -1.),
                    c(-1., -1., -1.),
                    c(-1., 1., -1.),
                    c(1., 1., -1.),
                ],
                [
                    c(1., -1., 1.),
                    c(1., -1., -1.),
                    c(1., 1., -1.),
                    c(1., 1., 1.),
                ],
                [
                    c(-1., -1., -1.),
                    c(-1., -1., 1.),
                    c(-1., 1., 1.),
                    c(-1., 1., -1.),
                ],
                [
                    c(-1., 1., 1.),
                    c(1., 1., 1.),
                    c(1., 1., -1.),
                    c(-1., 1., -1.),
                ],
                [
                    c(-1., -1., -1.),
                    c(1., -1., -1.),
                    c(1., -1., 1.),
                    c(-1., -1., 1.),
                ],
            ] {
                face(&mut geometry, &quad);
            }
        }
    }
    geometry
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalized(a: [f32; 3]) -> [f32; 3] {
    let length = dot(a, a).sqrt();
    if length > 0.0 {
        [a[0] / length, a[1] / length, a[2] / length]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// What the renderer saw at each pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct Buffers {
    /// Width and height.
    pub size: [u32; 2],
    /// How far away the surface is (infinite where there is none).
    pub depth: Vec<f32>,
    /// Its normal, facing the camera.
    pub normal: Vec<[f32; 3]>,
    /// How lit it is, 0 to 1.
    pub light: Vec<f32>,
}

impl Buffers {
    /// `true` where the model covers the pixel.
    #[must_use]
    pub fn covered(&self, index: usize) -> bool {
        self.depth[index].is_finite()
    }
}

/// How the camera of a 3D layer sees: where the model's middle is and how
/// it is scaled into the box the camera orbits, the camera's place and
/// axes, and its lens.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    middle: [f32; 3],
    scale: f32,
    eye: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
    focal: f32,
    size: [f32; 2],
}

impl Camera {
    /// The camera `view` makes on a `size` canvas for a model whose
    /// middle and largest extent are `frame`.
    #[must_use]
    pub fn new(view: &View3D, size: [u32; 2], frame: ([f32; 3], f32)) -> Self {
        let (middle, extent) = frame;
        let (yaw, pitch) = (view.yaw.to_radians(), view.pitch.to_radians());
        let eye = [
            view.distance * pitch.cos() * yaw.sin(),
            view.distance * pitch.sin(),
            view.distance * pitch.cos() * yaw.cos(),
        ];
        let forward = normalized(sub([0.0; 3], eye));
        let right = normalized(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        let eye =
            [0, 1, 2].map(|axis| eye[axis] - right[axis] * view.pan[0] + up[axis] * view.pan[1]);
        Self {
            middle,
            scale: 2.0 / extent.max(1e-6),
            eye,
            right,
            up,
            forward,
            focal: (size[1] as f32 / 2.0) / (view.fov.to_radians() / 2.0).tan(),
            size: [size[0] as f32, size[1] as f32],
        }
    }

    /// The camera for `mesh`, framed by its own bounds.
    #[must_use]
    pub fn fitting(mesh: &MeshBuffer3D, view: &View3D, size: [u32; 2]) -> Self {
        let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for p in mesh.positions() {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        if !min[0].is_finite() {
            return Self::new(view, size, ([0.0; 3], 2.0));
        }
        let middle = [0, 1, 2].map(|axis| (min[axis] + max[axis]) / 2.0);
        let extent = (0..3)
            .map(|axis| max[axis] - min[axis])
            .fold(0.0_f32, f32::max);
        Self::new(view, size, (middle, extent))
    }

    /// `p` (model units) in the world the camera orbits, and in the
    /// camera's own axes (across, up, away).
    fn see(&self, p: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let world = [0, 1, 2].map(|axis| (p[axis] - self.middle[axis]) * self.scale);
        let rel = sub(world, self.eye);
        (
            world,
            [
                dot(rel, self.right),
                dot(rel, self.up),
                dot(rel, self.forward),
            ],
        )
    }

    /// Camera axes to canvas pixels, with the distance kept.
    fn screen(&self, c: [f32; 3]) -> [f32; 3] {
        [
            self.size[0] / 2.0 + c[0] / c[2] * self.focal,
            self.size[1] / 2.0 - c[1] / c[2] * self.focal,
            c[2],
        ]
    }

    /// Where model point `p` shows on the canvas, in pixels; `None` behind
    /// the camera.
    #[must_use]
    pub fn project(&self, p: [f32; 3]) -> Option<[f32; 2]> {
        let (_, c) = self.see(p);
        (c[2] > 0.05).then(|| {
            let s = self.screen(c);
            [s[0], s[1]]
        })
    }
}

/// The model `mesh` as `view` sees it on a `size` canvas: the depth,
/// normal and light of every pixel. The model is fitted to a box 2 wide
/// round the middle first, so any model shows whole.
#[must_use]
pub fn render_buffers(mesh: &MeshBuffer3D, view: &View3D, size: [u32; 2]) -> Buffers {
    render_framed(mesh, view, size, None)
}

/// [`render_buffers`], framed by `frame` (a middle and a size) instead of
/// the model's own bounds when given, so a model that moves (a posed
/// figure) keeps its scale.
#[must_use]
pub fn render_framed(
    mesh: &MeshBuffer3D,
    view: &View3D,
    size: [u32; 2],
    frame: Option<([f32; 3], f32)>,
) -> Buffers {
    let (w, h) = (size[0] as usize, size[1] as usize);
    let mut buffers = Buffers {
        size,
        depth: vec![f32::INFINITY; w * h],
        normal: vec![[0.0; 3]; w * h],
        light: vec![0.0; w * h],
    };
    let positions = mesh.positions();
    if positions.is_empty() || w == 0 || h == 0 {
        return buffers;
    }
    let lens = match frame {
        Some(frame) => Camera::new(view, size, frame),
        None => Camera::fitting(mesh, view, size),
    };
    let eye = lens.eye;
    let light_angle = view.light.to_radians();
    let light = normalized([light_angle.sin(), 0.8, light_angle.cos()]);
    let camera = |p: [f32; 3]| lens.see(p);
    let project = |c: [f32; 3]| lens.screen(c);
    let shadow = view.shadows.then(|| ShadowMap::new(mesh, &lens, light));
    for triangle in mesh.indices().as_chunks::<3>().0 {
        let [(a_world, a), (b_world, b), (c_world, c)] =
            [0, 1, 2].map(|k| camera(positions[triangle[k] as usize]));
        if a[2] < 0.05 || b[2] < 0.05 || c[2] < 0.05 {
            continue;
        }
        let mut normal = normalized(cross(sub(b_world, a_world), sub(c_world, a_world)));
        // Facing the camera either way (models are not always closed).
        let toward = sub(eye, a_world);
        if dot(normal, toward) < 0.0 {
            normal = [-normal[0], -normal[1], -normal[2]];
        }
        let facing = dot(normal, light).max(0.0);
        let [pa, pb, pc] = [project(a), project(b), project(c)];
        let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
        if area.abs() < 1e-6 {
            continue;
        }
        let x0 = pa[0].min(pb[0]).min(pc[0]).floor().max(0.0) as usize;
        let x1 = (pa[0].max(pb[0]).max(pc[0]).ceil().max(0.0) as usize).min(w);
        let y0 = pa[1].min(pb[1]).min(pc[1]).floor().max(0.0) as usize;
        let y1 = (pa[1].max(pb[1]).max(pc[1]).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let wa = ((pb[0] - px) * (pc[1] - py) - (pb[1] - py) * (pc[0] - px)) / area;
                let wb = ((pc[0] - px) * (pa[1] - py) - (pc[1] - py) * (pa[0] - px)) / area;
                let wc = 1.0 - wa - wb;
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                // Depth interpolated as one over the distance, as perspective
                // asks.
                let inverse = wa / pa[2] + wb / pb[2] + wc / pc[2];
                let depth = 1.0 / inverse;
                let at = y * w + x;
                if depth < buffers.depth[at] {
                    let sunlit = shadow.as_ref().map_or(1.0, |shadow| {
                        let world = [0, 1, 2].map(|axis| {
                            (wa / pa[2] * a_world[axis]
                                + wb / pb[2] * b_world[axis]
                                + wc / pc[2] * c_world[axis])
                                * depth
                        });
                        shadow.lit(world)
                    });
                    buffers.depth[at] = depth;
                    buffers.normal[at] = normal;
                    buffers.light[at] = (view.ambient
                        + (1.0 - view.ambient) * view.intensity * facing * sunlit)
                        .clamp(0.0, 1.0);
                }
            }
        }
    }
    buffers
}

/// The model as the light sees it, straight down its rays: the nearest
/// surface to the light across a grid, so a point further along a ray is
/// in shadow.
struct ShadowMap {
    across: [f32; 3],
    up: [f32; 3],
    toward: [f32; 3],
    min: [f32; 2],
    cell: f32,
    nearest: Vec<f32>,
    bias: f32,
}

impl ShadowMap {
    const SIZE: usize = 512;

    fn new(mesh: &MeshBuffer3D, lens: &Camera, toward: [f32; 3]) -> Self {
        let mut across = cross(toward, [0.0, 1.0, 0.0]);
        if dot(across, across) < 1e-6 {
            across = [1.0, 0.0, 0.0];
        }
        let across = normalized(across);
        let up = cross(across, toward);
        let worlds: Vec<[f32; 3]> = mesh.positions().iter().map(|p| lens.see(*p).0).collect();
        let flat = |p: [f32; 3]| [dot(p, across), dot(p, up), dot(p, toward)];
        let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        let (mut near, mut far) = (f32::NEG_INFINITY, f32::INFINITY);
        for p in &worlds {
            let q = flat(*p);
            for axis in 0..2 {
                min[axis] = min[axis].min(q[axis]);
                max[axis] = max[axis].max(q[axis]);
            }
            near = near.max(q[2]);
            far = far.min(q[2]);
        }
        let cell = ((max[0] - min[0]).max(max[1] - min[1]) / (Self::SIZE - 1) as f32).max(1e-6);
        let mut map = Self {
            across,
            up,
            toward,
            min,
            cell,
            nearest: vec![f32::NEG_INFINITY; Self::SIZE * Self::SIZE],
            bias: (near - far).max(1e-3) * 0.01 + cell * 1.5,
        };
        let n = Self::SIZE as f32;
        for triangle in mesh.indices().as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|k| {
                let q = flat(worlds[triangle[k] as usize]);
                [(q[0] - min[0]) / cell, (q[1] - min[1]) / cell, q[2]]
            });
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area.abs() < 1e-9 {
                continue;
            }
            let x0 = a[0].min(b[0]).min(c[0]).floor().clamp(0.0, n) as usize;
            let x1 = (a[0].max(b[0]).max(c[0]).ceil() + 1.0).clamp(0.0, n) as usize;
            let y0 = a[1].min(b[1]).min(c[1]).floor().clamp(0.0, n) as usize;
            let y1 = (a[1].max(b[1]).max(c[1]).ceil() + 1.0).clamp(0.0, n) as usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f32, y as f32);
                    let wa = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
                    let wb = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
                    let wc = 1.0 - wa - wb;
                    if wa < -1e-4 || wb < -1e-4 || wc < -1e-4 {
                        continue;
                    }
                    let height = wa * a[2] + wb * b[2] + wc * c[2];
                    let at = y * Self::SIZE + x;
                    map.nearest[at] = map.nearest[at].max(height);
                }
            }
        }
        map
    }

    /// 1 where the light reaches world point `p`, 0 in shadow, softened
    /// over the four nearest cells.
    fn lit(&self, p: [f32; 3]) -> f32 {
        let x = (dot(p, self.across) - self.min[0]) / self.cell;
        let y = (dot(p, self.up) - self.min[1]) / self.cell;
        let height = dot(p, self.toward);
        let mut light = 0.0;
        for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (cx, cy) = ((x + dx - 0.5).round(), (y + dy - 0.5).round());
            let inside = cx >= 0.0 && cy >= 0.0 && cx < Self::SIZE as f32 && cy < Self::SIZE as f32;
            let blocked =
                inside && self.nearest[cy as usize * Self::SIZE + cx as usize] > height + self.bias;
            if !blocked {
                light += 0.25;
            }
        }
        light
    }
}

/// How much line covers each pixel: where the model's outline is, where the
/// depth jumps, and where the surface folds more than `crease` degrees,
/// `width` pixels wide.
#[must_use]
pub fn lines(buffers: &Buffers, width: f32, crease: f32) -> Vec<f32> {
    let [w, h] = buffers.size.map(|value| value as usize);
    let fold = crease.to_radians().cos();
    let mut edge = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let at = y * w + x;
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx >= w || ny >= h {
                    continue;
                }
                let next = ny * w + nx;
                let (here, there) = (buffers.covered(at), buffers.covered(next));
                let line = match (here, there) {
                    (true, true) => {
                        let (a, b) = (buffers.depth[at], buffers.depth[next]);
                        (a - b).abs() > 0.04 * a.min(b)
                            || dot(buffers.normal[at], buffers.normal[next]) < fold
                    }
                    (true, false) | (false, true) => true,
                    _ => false,
                };
                if line {
                    // On the nearer side, so outlines sit on the model.
                    let mark = if !there || (here && buffers.depth[at] <= buffers.depth[next]) {
                        at
                    } else {
                        next
                    };
                    edge[mark] = 1.0;
                }
            }
        }
    }
    if width <= 1.0 {
        return edge;
    }
    let grown = crate::effects::Edge::build(
        |x, y| {
            if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                0.0
            } else {
                edge[y as usize * w + x as usize]
            }
        },
        [0, 0, buffers.size[0], buffers.size[1]],
        (width - 1.0) / 2.0,
    );
    (0..w * h)
        .map(|at| grown.at((at % w) as u32, (at / w) as u32))
        .collect()
}

/// The layer's pixels: the model shaded in gray, as lines, or both, on
/// clear.
#[must_use]
pub fn draw(buffers: &Buffers, view: &View3D) -> Option<RasterTile2D> {
    let [w, h] = buffers.size;
    let line = (view.style > 0).then(|| lines(buffers, view.line_width, view.crease));
    let color = view.line_color.map(|c| f32::from(c) / 255.0);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for at in 0..(w * h) as usize {
        let ink = line.as_ref().map_or(0.0, |line| line[at]) * view.line_opacity;
        let (rgb, alpha) = if view.style != 1 && buffers.covered(at) {
            let gray = buffers.light[at];
            (color.map(|c| gray + (c - gray) * ink), 1.0)
        } else {
            (color, ink)
        };
        pixels.extend_from_slice(&[
            (rgb[0] * 255.0).round() as u8,
            (rgb[1] * 255.0).round() as u8,
            (rgb[2] * 255.0).round() as u8,
            (alpha * 255.0).round() as u8,
        ]);
    }
    RasterTile2D::from_bytes(w, h, PixelFormat::Rgba8, pixels).ok()
}

/// The model of 3D layer `layer` as `view` sees it on a `size` canvas.
#[must_use]
pub fn buffers_of(
    arena: &Arena,
    layer: EntityId,
    view: &View3D,
    size: [u32; 2],
) -> Option<Buffers> {
    let model = arena.get_by_id(model_of(arena, layer)?).ok()?;
    if crate::figure::is_figure(model) {
        let pose = crate::figure::Pose::of(model);
        let body = crate::figure::Body::of(model);
        return Some(render_framed(
            &crate::figure::mesh(&pose, &body),
            view,
            size,
            Some(crate::figure::FRAME),
        ));
    }
    let mesh = model.ext::<Geometry>()?.to_mesh();
    Some(render_buffers(&mesh, view, size))
}

/// The edit that makes a 3D layer named `name` showing `model` (an
/// existing model entity, or a new one to spawn), right above `below`.
///
/// # Errors
/// If the picture has no size.
pub fn new_layer(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: &str,
    model: Entity,
) -> Result<(Edit, EntityId), String> {
    let size = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .ok_or(tr!("the-picture-has-no-size"))?;
    let figure = crate::figure::is_figure(&model);
    let view = if figure {
        View3D {
            yaw: 18.0,
            pitch: 6.0,
            distance: 4.4,
            ..View3D::default()
        }
    } else {
        View3D::default()
    };
    let buffers = if figure {
        let pose = crate::figure::Pose::of(&model);
        render_framed(
            &crate::figure::mesh(&pose, &crate::figure::Body::of(&model)),
            &view,
            size,
            Some(crate::figure::FRAME),
        )
    } else {
        let mesh = model
            .ext::<Geometry>()
            .map(Geometry::to_mesh)
            .unwrap_or_default();
        render_buffers(&mesh, &view, size)
    };
    let raster = draw(&buffers, &view).ok_or(tr!("cannot-draw-the-model"))?;
    let model_id = model.id();
    let mut layer = Entity::new(name, raster).with_class(CLASS);
    let id = layer.id();
    let stack = crate::doc::layers(arena, picture);
    let order = below
        .and_then(|below| stack.iter().position(|info| info.id == below))
        .map_or_else(
            || arena.relations().next_order(picture, CHILD),
            |index| {
                sue_core::OrderKey::between(
                    Some(&stack[index].order),
                    stack
                        .get(index + 1)
                        .map(|next| &next.order)
                        .filter(|next| **next > stack[index].order),
                )
            },
        );
    for (key, value) in [
        (YAW, Value::Float(f64::from(view.yaw))),
        (PITCH, Value::Float(f64::from(view.pitch))),
        (DISTANCE, Value::Float(f64::from(view.distance))),
        (STYLE, Value::Int(i64::from(view.style))),
    ] {
        layer = layer.with_property(key, value);
    }
    let edit = Edit::new(tr!("new-3d-layer-2", name = name))
        .then(SpawnEntity::new(model))
        .then(
            SpawnEntity::new(layer)
                .with_relation(Relation::new(picture, CHILD, id).ordered(order))
                .with_relation(Relation::new(id, REF, model_id)),
        );
    Ok((edit, id))
}

/// The edit that sets 3D layer `layer` to `view` and draws it again, as
/// one step named `label`.
#[must_use]
pub fn set_view(arena: &Arena, layer: EntityId, view: &View3D, label: &str) -> Option<Edit> {
    let raster = crate::doc::raster(arena, layer)?;
    let size = [raster.width(), raster.height()];
    let buffers = buffers_of(arena, layer, view, size)?;
    let drawn = draw(&buffers, view)?;
    let mut edit = Edit::new(label);
    for set in view.edits(layer) {
        edit = edit.then(set);
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(buffers: &Buffers) -> usize {
        (0..buffers.depth.len())
            .filter(|at| buffers.covered(*at))
            .count()
    }

    #[test]
    fn shapes_render_with_outlines_and_folds() {
        let size = [120, 90];
        let cube = shape(0).to_mesh();
        assert_eq!(cube.indices().len(), 36, "twelve triangles");
        let view = View3D::default();
        let buffers = render_buffers(&cube, &view, size);
        let area = covered(&buffers);
        assert!(area > 1500 && area < 120 * 90 / 2, "{area}");
        assert!(buffers.covered(60 + 45 * 120), "the middle");
        assert!(!buffers.covered(0), "the corner is clear");
        // Closer is bigger.
        let near = render_buffers(
            &cube,
            &View3D {
                distance: 2.2,
                ..view
            },
            size,
        );
        assert!(covered(&near) > area);
        // Lines: the outline and the cube's edges, not its flat faces.
        let line = lines(&buffers, 1.0, 40.0);
        let inked = line.iter().filter(|value| **value > 0.0).count();
        assert!(inked > 150 && inked < area / 2, "{inked}");
        // Each style draws.
        for style in 0..3 {
            let drawn = draw(&buffers, &View3D { style, ..view }).unwrap();
            let alpha = |x: u32, y: u32| drawn.bytes().unwrap()[((y * 120 + x) * 4 + 3) as usize];
            assert_eq!(alpha(0, 0), 0);
            if style != 1 {
                assert_eq!(alpha(60, 45), 255, "shaded inside");
            }
        }
        // A sphere has an outline but no folds inside.
        let sphere = render_buffers(&shape(1).to_mesh(), &view, size);
        let line = lines(&sphere, 1.0, 40.0);
        let middle = (40..50).flat_map(|y| (55..65).map(move |x| y * 120 + x));
        assert!(
            middle.clone().all(|at| line[at] == 0.0),
            "smooth in the middle"
        );
        assert!(line.iter().any(|value| *value > 0.0));
        for (kind, name) in SHAPES.iter().enumerate() {
            assert!(!shape(kind).to_mesh().indices().is_empty(), "{name}");
        }
    }

    #[test]
    fn shadows_fall_and_lines_take_their_color() {
        // A cube standing on a wide floor, lit from above.
        let mut geometry = shape(0);
        let floor: Vec<u32> = [
            [-4.0, -1.0, -4.0],
            [-4.0, -1.0, 4.0],
            [4.0, -1.0, 4.0],
            [4.0, -1.0, -4.0],
        ]
        .iter()
        .map(|point| geometry.add_point(*point))
        .collect();
        geometry.add_prim(PrimKind::Polygon, &floor).unwrap();
        let mesh = geometry.to_mesh();
        let view = View3D {
            pitch: 40.0,
            distance: 3.0,
            light: 180.0,
            style: 0,
            ..View3D::default()
        };
        let size = [160, 120];
        let mean = |buffers: &Buffers| {
            let lit: Vec<f32> = (0..buffers.light.len())
                .filter(|at| buffers.covered(*at))
                .map(|at| buffers.light[at])
                .collect();
            lit.iter().sum::<f32>() / lit.len() as f32
        };
        let open = render_buffers(&mesh, &view, size);
        let shaded = render_buffers(
            &mesh,
            &View3D {
                shadows: true,
                ..view
            },
            size,
        );
        assert_eq!(covered(&open), covered(&shaded), "the same model");
        assert!(mean(&shaded) < mean(&open) - 0.005, "a shadow fell");
        let ambient = render_buffers(
            &mesh,
            &View3D {
                ambient: 1.0,
                ..view
            },
            size,
        );
        assert!(
            ambient
                .light
                .iter()
                .all(|light| *light == 0.0 || *light == 1.0)
        );
        // Red lines at half opacity.
        let drawn = draw(
            &open,
            &View3D {
                style: 1,
                line_color: [255, 0, 0],
                line_opacity: 0.5,
                ..view
            },
        )
        .unwrap();
        let bytes = drawn.bytes().unwrap();
        let inked: Vec<&[u8]> = bytes.chunks(4).filter(|pixel| pixel[3] > 0).collect();
        assert!(!inked.is_empty());
        assert!(
            inked
                .iter()
                .all(|pixel| pixel[0] == 255 && pixel[1] == 0 && pixel[3] <= 128)
        );
    }

    #[test]
    fn camera_presets_look_from_each_side() {
        let view = View3D {
            pan: [0.3, 0.2],
            ..View3D::default()
        };
        let front = view.preset(0);
        assert_eq!((front.yaw, front.pitch, front.pan), (0.0, 0.0, [0.0; 2]));
        assert_eq!(view.preset(1).yaw, 180.0);
        assert_eq!(view.preset(4).pitch, 89.0);
        assert_eq!(view.preset(5).pitch, -89.0);
        assert_eq!(view.preset(3).distance, view.distance, "the rest kept");
        // Every view is a property, and comes back.
        let mut arena = Arena::new();
        let layer = Entity::empty("3D", CLASS);
        let id = layer.id();
        arena.insert_keep_id(layer).unwrap();
        let look = View3D {
            shadows: true,
            line_color: [10, 20, 30],
            intensity: 1.5,
            ..front
        };
        for mut set in look.edits(id) {
            sue_core::ReversibleTransaction::apply(&mut set, &mut arena).unwrap();
        }
        assert_eq!(View3D::of(arena.get_by_id(id).unwrap()), look);
    }

    #[test]
    fn a_3d_layer_shows_its_model_and_turns_as_one_step() {
        let mut arena = Arena::new();
        let (mut edit, picture_id) = crate::doc::create(&crate::doc::NewPicture {
            width: 100,
            height: 80,
            ..crate::doc::NewPicture::default()
        })
        .unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let model = Entity::empty("Cube", sue_model::class::MODEL).with_ext(shape(0));
        let model_id = model.id();
        let below = crate::doc::layers(&arena, picture_id)
            .last()
            .map(|info| info.id);
        let (mut edit, layer) = new_layer(&arena, picture_id, below, "Cube", model).unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        assert!(is_3d(&arena, layer));
        assert_eq!(model_of(&arena, layer), Some(model_id));
        let before = crate::doc::raster(&arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec();
        assert!(before.chunks(4).any(|pixel| pixel[3] > 0), "the cube shows");
        let turned = View3D {
            yaw: 80.0,
            ..View3D::of(arena.get_by_id(layer).unwrap())
        };
        let mut turn = set_view(&arena, layer, &turned, "Turn Camera").unwrap();
        sue_core::ReversibleTransaction::apply(&mut turn, &mut arena).unwrap();
        assert_eq!(View3D::of(arena.get_by_id(layer).unwrap()).yaw, 80.0);
        assert_ne!(
            crate::doc::raster(&arena, layer).unwrap().bytes().unwrap(),
            &before[..]
        );
        sue_core::ReversibleTransaction::revert(&mut turn, &mut arena).unwrap();
        assert_eq!(
            crate::doc::raster(&arena, layer).unwrap().bytes().unwrap(),
            &before[..],
            "undone"
        );
    }
}
