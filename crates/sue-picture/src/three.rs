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
    Arena, Entity, EntityId, MeshBuffer3D, PixelFormat, REF, RasterTile2D, Relation, Value,
};
use sue_model::{Geometry, PrimKind};

use crate::doc::Edit;

mod surface;
pub use surface::{Relief, Surface, frame_of_points, light_of, render_surface, smooth_normals};

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
/// The light turns with the camera, as if carried on it (`Bool`, Clip
/// Studio Paint 5.0's Follow camera).
pub const FOLLOW_LIGHT: &str = "view3d.follow_light";
/// How thick the fog is at the model's foot, 0 (none) to 1 (`Float`,
/// Clip Studio Paint 5.1's height-based fog).
pub const FOG: &str = "view3d.fog";
/// How high the fog reaches, 0 (the model's foot) to 1 (its top)
/// (`Float`).
pub const FOG_HEIGHT: &str = "view3d.fog_height";
/// Manga Perspective: how much more the parts nearest the camera grow, as
/// in a drawn foreshortening, 0 (off) to 1 (`Float`). The model's middle
/// keeps its size; the lens widens and the camera comes nearer.
pub const MANGA: &str = "view3d.manga";
/// How high the light stands, in degrees above the ground (`Float`).
pub const LIGHT_HEIGHT: &str = "view3d.light_height";
/// The model casts a shadow on the floor (`Bool`).
pub const FLOOR: &str = "view3d.floor_shadow";

/// Where the object stands on its 3D layer: across, up and toward the
/// camera's front, in the units of its shape (`Float`, on the model).
pub const OBJECT_X: &str = "object.x";
/// See [`OBJECT_X`].
pub const OBJECT_Y: &str = "object.y";
/// See [`OBJECT_X`].
pub const OBJECT_Z: &str = "object.z";
/// How far the object is turned round its up axis, in degrees (`Float`).
pub const OBJECT_YAW: &str = "object.yaw";
/// How far it leans forward or back, in degrees (`Float`).
pub const OBJECT_PITCH: &str = "object.pitch";
/// How far it tilts to the side, in degrees (`Float`).
pub const OBJECT_ROLL: &str = "object.roll";
/// How large it is, 1 as made (`Float`).
pub const OBJECT_SCALE: &str = "object.scale";

/// The angles an object can be set to at once: a name and a turn round,
/// a lean and a tilt, in degrees.
pub const ANGLES: [(&str, [f32; 3]); 6] = [
    ("Facing front", [0.0, 0.0, 0.0]),
    ("Facing back", [180.0, 0.0, 0.0]),
    ("Facing left", [90.0, 0.0, 0.0]),
    ("Facing right", [-90.0, 0.0, 0.0]),
    ("Lying face up", [0.0, -90.0, 0.0]),
    ("Lying face down", [0.0, 90.0, 0.0]),
];

/// Where an object of a 3D layer stands, how it is turned and how large it
/// is (Clip Studio Paint's Object scale, Position and Angle). It lives on
/// the model entity, so every 3D layer showing the model shows it placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Across, up and forward.
    pub position: [f32; 3],
    /// Degrees round (about up), leaning (about across) and tilting (about
    /// forward), applied in that order.
    pub turn: [f32; 3],
    /// How large, 1 as made.
    pub scale: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            turn: [0.0; 3],
            scale: 1.0,
        }
    }
}

impl Placement {
    /// The sizes people may choose.
    pub const SCALES: std::ops::RangeInclusive<f32> = 0.05..=20.0;

    /// The placement of model `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let number = |key: &str, or: f32| {
            entity
                .property(key)
                .and_then(Value::as_f64)
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .unwrap_or(or)
        };
        Self {
            position: [
                number(OBJECT_X, 0.0),
                number(OBJECT_Y, 0.0),
                number(OBJECT_Z, 0.0),
            ],
            turn: [
                number(OBJECT_YAW, 0.0),
                number(OBJECT_PITCH, 0.0),
                number(OBJECT_ROLL, 0.0),
            ],
            scale: number(OBJECT_SCALE, 1.0).clamp(*Self::SCALES.start(), *Self::SCALES.end()),
        }
    }

    /// `true` when it leaves the object as made.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// The turn as a matrix (columns are where x, y and z go).
    #[must_use]
    pub fn matrix(&self) -> [[f32; 3]; 3] {
        let [yaw, pitch, roll] = self.turn.map(f32::to_radians);
        let (sy, cy) = yaw.sin_cos();
        let (sx, cx) = pitch.sin_cos();
        let (sz, cz) = roll.sin_cos();
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        let mul = |a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]| {
            [0, 1, 2]
                .map(|row| [0, 1, 2].map(|column| (0..3).map(|k| a[row][k] * b[k][column]).sum()))
        };
        mul(&mul(&ry, &rx), &rz)
    }

    /// Point `p` of the object as made, placed.
    #[must_use]
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let m = self.matrix();
        let s = p.map(|value| value * self.scale);
        [0, 1, 2]
            .map(|row| self.position[row] + m[row][0] * s[0] + m[row][1] * s[1] + m[row][2] * s[2])
    }

    /// Its properties and values, in a fixed order.
    #[must_use]
    pub fn values(&self) -> [(&'static str, f64); 7] {
        [
            (OBJECT_X, f64::from(self.position[0])),
            (OBJECT_Y, f64::from(self.position[1])),
            (OBJECT_Z, f64::from(self.position[2])),
            (OBJECT_YAW, f64::from(self.turn[0])),
            (OBJECT_PITCH, f64::from(self.turn[1])),
            (OBJECT_ROLL, f64::from(self.turn[2])),
            (OBJECT_SCALE, f64::from(self.scale)),
        ]
    }

    /// The placement as property edits on `model`.
    #[must_use]
    pub fn edits(&self, model: EntityId) -> Vec<SetProperty> {
        self.values()
            .into_iter()
            .map(|(key, value)| SetProperty::new(model, key, value))
            .collect()
    }

    /// `mesh` placed.
    #[must_use]
    pub fn mesh(&self, mesh: &MeshBuffer3D) -> MeshBuffer3D {
        if self.is_identity() {
            return mesh.clone();
        }
        let positions = mesh.positions().iter().map(|p| self.apply(*p)).collect();
        MeshBuffer3D::new(positions, mesh.indices().to_vec()).unwrap_or_else(|_| mesh.clone())
    }
}

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
    /// The light turns with the camera.
    pub follow_light: bool,
    /// The fog's thickness at the foot, 0 to 1.
    pub fog: f32,
    /// How high the fog reaches, 0 to 1 of the model's height.
    pub fog_height: f32,
    /// Manga Perspective, 0 (off) to 1.
    pub manga: f32,
    /// How high the light stands, in degrees above the ground.
    pub light_height: f32,
    /// The model casts a shadow on the floor.
    pub floor: bool,
    /// Draw quickly rather than finely (while something is dragged);
    /// never kept with the layer.
    pub fast: bool,
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
            follow_light: false,
            fog: 0.0,
            fog_height: 0.5,
            manga: 0.0,
            light_height: Self::LIGHT_HEIGHT,
            floor: false,
            fast: false,
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
            follow_light: entity
                .property(FOLLOW_LIGHT)
                .and_then(Value::as_bool)
                .unwrap_or(still.follow_light),
            fog: number(FOG, still.fog).clamp(0.0, 1.0),
            fog_height: number(FOG_HEIGHT, still.fog_height).clamp(0.0, 1.0),
            manga: number(MANGA, still.manga).clamp(0.0, 1.0),
            light_height: number(LIGHT_HEIGHT, still.light_height).clamp(-89.0, 89.0),
            floor: entity
                .property(FLOOR)
                .and_then(Value::as_bool)
                .unwrap_or(still.floor),
            fast: false,
        }
    }

    /// How high the light stands unless set, in degrees: the height Sue
    /// Paint's light has always had.
    pub const LIGHT_HEIGHT: f32 = 38.659_81;

    /// The widest lens Manga Perspective opens to, in degrees.
    pub const MANGA_FOV: f32 = 90.0;

    /// The field of view and distance the camera really has: as set, or,
    /// with Manga Perspective, a lens opened toward [`Self::MANGA_FOV`]
    /// from as much nearer as keeps the model's middle the same size.
    #[must_use]
    pub fn lens(&self) -> (f32, f32) {
        let manga = self.manga.clamp(0.0, 1.0);
        if manga <= 0.0 || self.fov >= Self::MANGA_FOV {
            return (self.fov, self.distance);
        }
        let fov = self.fov + (Self::MANGA_FOV - self.fov) * manga;
        let half = |degrees: f32| (degrees.to_radians() / 2.0).tan();
        (fov, self.distance * half(self.fov) / half(fov))
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
        self.values()
            .into_iter()
            .map(|(key, value)| SetProperty::new(layer, key, value))
            .collect()
    }

    /// The view as the properties and values of its 3D layer.
    #[must_use]
    pub fn values(&self) -> Vec<(&'static str, Value)> {
        vec![
            (YAW, Value::from(f64::from(self.yaw))),
            (PITCH, Value::from(f64::from(self.pitch))),
            (DISTANCE, Value::from(f64::from(self.distance))),
            (FOV, Value::from(f64::from(self.fov))),
            (LIGHT, Value::from(f64::from(self.light))),
            (STYLE, Value::from(i64::from(self.style))),
            (LINE_WIDTH, Value::from(f64::from(self.line_width))),
            (CREASE, Value::from(f64::from(self.crease))),
            (PAN_X, Value::from(f64::from(self.pan[0]))),
            (PAN_Y, Value::from(f64::from(self.pan[1]))),
            (INTENSITY, Value::from(f64::from(self.intensity))),
            (AMBIENT, Value::from(f64::from(self.ambient))),
            (SHADOWS, Value::from(self.shadows)),
            (
                LINE_COLOR,
                Value::from(
                    i64::from(self.line_color[0]) << 16
                        | i64::from(self.line_color[1]) << 8
                        | i64::from(self.line_color[2]),
                ),
            ),
            (LINE_OPACITY, Value::from(f64::from(self.line_opacity))),
            (FOLLOW_LIGHT, Value::from(self.follow_light)),
            (FOG, Value::from(f64::from(self.fog))),
            (FOG_HEIGHT, Value::from(f64::from(self.fog_height))),
            (MANGA, Value::from(f64::from(self.manga))),
            (LIGHT_HEIGHT, Value::from(f64::from(self.light_height))),
            (FLOOR, Value::from(self.floor)),
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
    /// How much fog is in front of it, 0 to 1.
    pub fog: Vec<f32>,
    /// How much of the pixel the surface covers, 0 to 1, so edges are
    /// soft (empty when every covered pixel is covered whole).
    pub cover: Vec<f32>,
    /// How much guide line lies on the surface at the pixel (empty when
    /// the model has none).
    pub guide: Vec<f32>,
    /// How dark the model's shadow on the floor is at the pixel (empty
    /// without one).
    pub floor: Vec<f32>,
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
        // Manga Perspective: a wider lens from nearer, so the middle keeps
        // its size and what comes toward the camera grows.
        let (fov, distance) = view.lens();
        let eye = [
            distance * pitch.cos() * yaw.sin(),
            distance * pitch.sin(),
            distance * pitch.cos() * yaw.cos(),
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
            focal: (size[1] as f32 / 2.0) / (fov.to_radians() / 2.0).tan(),
            size: [size[0] as f32, size[1] as f32],
        }
    }

    /// The camera for `mesh`, framed by its own bounds.
    #[must_use]
    pub fn fitting(mesh: &MeshBuffer3D, view: &View3D, size: [u32; 2]) -> Self {
        Self::new(view, size, frame_of(mesh))
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

    /// The model point the canvas pixel `pixel` shows, as far from the
    /// camera as model point `like` is: where a drag of `like` across the
    /// screen takes it.
    #[must_use]
    pub fn unproject(&self, pixel: [f32; 2], like: [f32; 3]) -> [f32; 3] {
        let depth = self.see(like).1[2].max(0.05);
        let across = (pixel[0] - self.size[0] / 2.0) / self.focal * depth;
        let up = -(pixel[1] - self.size[1] / 2.0) / self.focal * depth;
        [0, 1, 2].map(|axis| {
            let world = self.eye[axis]
                + self.right[axis] * across
                + self.up[axis] * up
                + self.forward[axis] * depth;
            world / self.scale + self.middle[axis]
        })
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

/// The middle and largest extent of `mesh`, which a camera frames it by
/// (a box 2 wide round the middle when it has no points).
#[must_use]
pub fn frame_of(mesh: &MeshBuffer3D) -> ([f32; 3], f32) {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in mesh.positions() {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    if !min[0].is_finite() {
        return ([0.0; 3], 2.0);
    }
    let middle = [0, 1, 2].map(|axis| (min[axis] + max[axis]) / 2.0);
    let extent = (0..3)
        .map(|axis| max[axis] - min[axis])
        .fold(0.0_f32, f32::max);
    (middle, extent)
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
        fog: vec![0.0; w * h],
        cover: Vec::new(),
        guide: Vec::new(),
        floor: Vec::new(),
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
    // Carried on the camera, the light keeps its place on the screen as
    // the camera goes round.
    let light = light_of(view);
    // Height-based fog: thickest at the model's foot, gone at its height.
    let (low, high) = positions
        .iter()
        .map(|p| lens.see(*p).0[1])
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), y| {
            (low.min(y), high.max(y))
        });
    let fog_top = low + (high - low) * view.fog_height;
    let fog_at = |y: f32| {
        if view.fog <= 0.0 || fog_top <= low {
            return 0.0;
        }
        (view.fog * ((fog_top - y) / (fog_top - low)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    };
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
                    let world = [0, 1, 2].map(|axis| {
                        (wa / pa[2] * a_world[axis]
                            + wb / pb[2] * b_world[axis]
                            + wc / pc[2] * c_world[axis])
                            * depth
                    });
                    let sunlit = shadow.as_ref().map_or(1.0, |shadow| shadow.lit(world));
                    buffers.fog[at] = fog_at(world[1]);
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
        let worlds: Vec<[f32; 3]> = mesh.positions().iter().map(|p| lens.see(*p).0).collect();
        Self::from_points(&worlds, mesh.indices(), toward)
    }

    /// The map of triangles `indices` over `worlds` (points in the world
    /// the camera orbits), seen from the light at `toward`.
    fn from_points(worlds: &[[f32; 3]], indices: &[u32], toward: [f32; 3]) -> Self {
        let mut across = cross(toward, [0.0, 1.0, 0.0]);
        if dot(across, across) < 1e-6 {
            across = [1.0, 0.0, 0.0];
        }
        let across = normalized(across);
        let up = cross(across, toward);
        let flat = |p: [f32; 3]| [dot(p, across), dot(p, up), dot(p, toward)];
        let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        let (mut near, mut far) = (f32::NEG_INFINITY, f32::INFINITY);
        for p in worlds {
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
        for triangle in indices.as_chunks::<3>().0 {
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
        // Fog whitens what is behind it, lines too.
        let fog = buffers.fog.get(at).copied().unwrap_or(0.0);
        let ink = line.as_ref().map_or(0.0, |line| line[at]) * view.line_opacity * (1.0 - fog);
        let (rgb, alpha) = if view.style != 1 && buffers.covered(at) {
            // Guide lines lie on the surface, a darker gray.
            let guide = buffers.guide.get(at).copied().unwrap_or(0.0);
            let lit = buffers.light[at] * (1.0 - 0.6 * guide);
            let gray = lit + (1.0 - lit) * fog;
            // A soft edge: the pixel shows as much as the model covers.
            let cover = buffers.cover.get(at).copied().unwrap_or(1.0);
            (
                color.map(|c| gray + (c - gray) * ink),
                cover + (1.0 - cover) * ink,
            )
        } else {
            // The model's shadow on the floor, where the model is not.
            let floor = buffers.floor.get(at).copied().unwrap_or(0.0) * 0.38;
            let alpha = ink + (1.0 - ink) * floor;
            if alpha <= 0.0 {
                (color, 0.0)
            } else {
                (color.map(|c| c * ink / alpha), alpha)
            }
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
    render_model(model, view, size, &Placement::of(model))
}

/// Model `model` placed as `place` says, as `view` sees it on a `size`
/// canvas. A figure is framed by its body; any other shape by its own
/// bounds as made, so placing it moves it on the canvas rather than the
/// camera after it.
#[must_use]
pub fn render_model(
    model: &Entity,
    view: &View3D,
    size: [u32; 2],
    place: &Placement,
) -> Option<Buffers> {
    if crate::figure::is_figure(model) {
        let pose = crate::figure::Pose::of(model);
        let body = crate::figure::Body {
            place: *place,
            ..crate::figure::Body::of(model)
        };
        return Some(crate::figure::render(&pose, &body, view, size));
    }
    let mesh = model.ext::<Geometry>()?.to_mesh();
    // A model from a file is drawn smooth, its folds kept.
    if model
        .property(crate::model3d::SMOOTH)
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let placed = if place.is_identity() {
            mesh.clone()
        } else {
            place.mesh(&mesh)
        };
        let surface = Surface::creased(placed.positions(), placed.indices(), 50.0);
        return Some(render_surface(&surface, view, size, frame_of(&mesh)));
    }
    if place.is_identity() {
        return Some(render_buffers(&mesh, view, size));
    }
    Some(render_framed(
        &place.mesh(&mesh),
        view,
        size,
        Some(frame_of(&mesh)),
    ))
}

/// How far below the ground (height 0) the lowest point of model `model`
/// placed as `place` is: what Put on the ground lifts it by.
#[must_use]
pub fn lowest(model: &Entity, place: &Placement) -> Option<f32> {
    let positions: Vec<[f32; 3]> = if crate::figure::is_figure(model) {
        let body = crate::figure::Body {
            place: *place,
            ..crate::figure::Body::of(model)
        };
        crate::figure::mesh(&crate::figure::Pose::of(model), &body)
            .positions()
            .to_vec()
    } else {
        let mesh = model.ext::<Geometry>()?.to_mesh();
        place.mesh(&mesh).positions().to_vec()
    };
    positions.iter().map(|p| p[1]).reduce(f32::min)
}

/// The edit that places the object of 3D layer `layer` as `place` and
/// draws the layer again, as one step named `label`.
#[must_use]
pub fn set_placement(
    arena: &Arena,
    layer: EntityId,
    place: &Placement,
    label: &str,
) -> Option<Edit> {
    let model_id = model_of(arena, layer)?;
    let model = arena.get_by_id(model_id).ok()?;
    let raster = crate::doc::raster(arena, layer)?;
    let view = View3D::of(arena.get_by_id(layer).ok()?);
    let buffers = render_model(model, &view, [raster.width(), raster.height()], place)?;
    let drawn = draw(&buffers, &view)?;
    let mut edit = Edit::new(label);
    for set in place.edits(model_id) {
        edit = edit.then(set);
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
}

/// The edit that sets 3D layer `layer` to `view` and its object to
/// `place` and draws the layer again, as one step named `label` (a drag
/// of Tool Property that moved both).
#[must_use]
pub fn set_view_placed(
    arena: &Arena,
    layer: EntityId,
    view: &View3D,
    place: &Placement,
    label: &str,
) -> Option<Edit> {
    let model_id = model_of(arena, layer)?;
    let model = arena.get_by_id(model_id).ok()?;
    let raster = crate::doc::raster(arena, layer)?;
    let buffers = render_model(model, view, [raster.width(), raster.height()], place)?;
    let drawn = draw(&buffers, view)?;
    let mut edit = Edit::new(label);
    for set in view.edits(layer) {
        edit = edit.then(set);
    }
    for set in place.edits(model_id) {
        edit = edit.then(set);
    }
    Some(edit.then(ReplaceComponent::new(layer, drawn, label)))
}

/// How a new 3D layer looks at a figure of `kind`: a little from the
/// side and above, the light carried on the camera so the figure is lit
/// from wherever it is seen, and a hand from its back, where its
/// knuckles, tendons and veins are.
#[must_use]
pub fn figure_view(kind: crate::figure::Kind) -> View3D {
    let hand = kind == crate::figure::Kind::Hand;
    View3D {
        yaw: if hand { 200.0 } else { 18.0 },
        pitch: if hand { 12.0 } else { 6.0 },
        distance: if hand { 3.4 } else { 4.4 },
        follow_light: true,
        ..View3D::default()
    }
}

/// The edit that makes a 3D layer named `name` showing `model` (an
/// existing model entity, or a new one to spawn) where a new layer goes
/// with `below` picked: right above it in its folder, or in it when it is
/// a folder.
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
    new_layer_viewed(arena, picture, below, name, model, None)
}

/// [`new_layer`], its camera set to `view` when given (a 3D background
/// looked into from where a person would stand) instead of the camera a
/// new figure or model gets.
///
/// # Errors
/// If the picture has no size.
pub fn new_layer_viewed(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    name: &str,
    model: Entity,
    view: Option<View3D>,
) -> Result<(Edit, EntityId), String> {
    let given = view.is_some();
    let size = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .ok_or(tr!("the-picture-has-no-size"))?;
    let figure = crate::figure::is_figure(&model);
    let view = if let Some(view) = view {
        view
    } else if figure {
        figure_view(crate::figure::Body::of(&model).kind)
    } else {
        View3D::default()
    };
    let buffers = render_model(&model, &view, size, &Placement::of(&model))
        .unwrap_or_else(|| render_buffers(&MeshBuffer3D::default(), &view, size));
    let raster = draw(&buffers, &view).ok_or(tr!("cannot-draw-the-model"))?;
    let model_id = model.id();
    let mut layer = Entity::new(name, raster).with_class(CLASS);
    let id = layer.id();
    for (key, value) in [
        (YAW, Value::Float(f64::from(view.yaw))),
        (PITCH, Value::Float(f64::from(view.pitch))),
        (DISTANCE, Value::Float(f64::from(view.distance))),
        (STYLE, Value::Int(i64::from(view.style))),
    ] {
        layer = layer.with_property(key, value);
    }
    if view.follow_light {
        layer = layer.with_property(FOLLOW_LIGHT, Value::from(true));
    }
    // A camera given keeps its lens and its place too.
    if given {
        for (key, value) in [(FOV, view.fov), (PAN_X, view.pan[0]), (PAN_Y, view.pan[1])] {
            layer = layer.with_property(key, Value::Float(f64::from(value)));
        }
    }
    let edit = Edit::new(tr!("new-3d-layer-2", name = name))
        .then(SpawnEntity::new(model))
        .then(
            // Where every new layer goes: in the picked folder, or right
            // above the picked layer in its folder.
            crate::doc::landed(arena, picture, below, layer)
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
    fn the_light_can_follow_the_camera_and_fog_rises_from_the_foot() {
        let size = [120, 90];
        let ball = shape(1).to_mesh();
        // Where the brightest light falls on the screen, across.
        let bright_x = |view: &View3D| {
            let buffers = render_buffers(&ball, view, size);
            let (index, _) = buffers
                .light
                .iter()
                .enumerate()
                .filter(|(at, _)| buffers.covered(*at))
                .fold(
                    (0, -1.0),
                    |best, (at, light)| {
                        if *light > best.1 { (at, *light) } else { best }
                    },
                );
            (index % 120) as i64
        };
        let still = View3D {
            follow_light: false,
            ..View3D::default()
        };
        let turned = |view: View3D| View3D {
            yaw: view.yaw + 90.0,
            ..view
        };
        // Fixed: the lit side moves on the screen as the camera goes round.
        assert!((bright_x(&still) - bright_x(&turned(still))).abs() > 15);
        // Following: it stays where it was.
        let carried = View3D {
            follow_light: true,
            ..still
        };
        assert!((bright_x(&carried) - bright_x(&turned(carried))).abs() <= 3);
        // Fog: thick at the foot, none at the top, and it whitens there.
        let foggy = View3D {
            fog: 1.0,
            fog_height: 0.6,
            style: 0,
            ..View3D::default()
        };
        let buffers = render_buffers(&ball, &foggy, size);
        let rows: Vec<usize> = (0..120 * 90).filter(|at| buffers.covered(*at)).collect();
        let (top, bottom) = (rows[0], rows[rows.len() - 1]);
        assert!(
            buffers.fog[bottom] > 0.5,
            "the foot: {}",
            buffers.fog[bottom]
        );
        assert!(buffers.fog[top] < 0.05, "the top: {}", buffers.fog[top]);
        let drawn = draw(&buffers, &foggy).unwrap();
        let clear = draw(
            &render_buffers(&ball, &View3D { fog: 0.0, ..foggy }, size),
            &foggy,
        )
        .unwrap();
        assert!(drawn.bytes().unwrap()[bottom * 4] > clear.bytes().unwrap()[bottom * 4]);
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

    #[test]
    fn an_object_is_placed_turned_sized_and_put_on_the_ground_as_one_step() {
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
        let (mut edit, layer) = new_layer(&arena, picture_id, None, "Cube", model).unwrap();
        sue_core::ReversibleTransaction::apply(&mut edit, &mut arena).unwrap();
        let pixels = |arena: &Arena| {
            crate::doc::raster(arena, layer)
                .unwrap()
                .bytes()
                .unwrap()
                .to_vec()
        };
        let ink = |bytes: &[u8]| bytes.chunks(4).filter(|p| p[3] > 0).count();
        let before = pixels(&arena);
        let props_before = arena.get_by_id(model_id).unwrap().properties().clone();
        // Placing as made changes nothing it draws.
        let same = set_placement(&arena, layer, &Placement::default(), "Place").unwrap();
        let mut same = same;
        sue_core::ReversibleTransaction::apply(&mut same, &mut arena).unwrap();
        assert_eq!(pixels(&arena), before, "as made draws as before");
        sue_core::ReversibleTransaction::revert(&mut same, &mut arena).unwrap();
        // Half the size draws less; the camera does not follow it.
        let small = Placement {
            scale: 0.5,
            ..Placement::default()
        };
        let mut shrink = set_placement(&arena, layer, &small, "Object Scale").unwrap();
        sue_core::ReversibleTransaction::apply(&mut shrink, &mut arena).unwrap();
        assert!(ink(&pixels(&arena)) * 2 < ink(&before), "smaller");
        assert_eq!(Placement::of(arena.get_by_id(model_id).unwrap()), small);
        sue_core::ReversibleTransaction::revert(&mut shrink, &mut arena).unwrap();
        assert_eq!(pixels(&arena), before, "undone");
        assert_eq!(
            arena.get_by_id(model_id).unwrap().properties(),
            &props_before,
            "no property left behind"
        );
        sue_core::ReversibleTransaction::apply(&mut shrink, &mut arena).unwrap();
        assert_eq!(
            Placement::of(arena.get_by_id(model_id).unwrap()),
            small,
            "redone"
        );
        // A turn about up moves a corner; Put on the ground lifts the cube
        // so its lowest corner touches height 0.
        let turned = Placement {
            turn: [45.0, 30.0, 0.0],
            ..small
        };
        let corner = turned.apply([1.0, 1.0, 1.0]);
        assert!((corner[1] - small.apply([1.0, 1.0, 1.0])[1]).abs() > 0.05);
        let model = arena.get_by_id(model_id).unwrap();
        let low = lowest(model, &turned).unwrap();
        assert!(low < 0.0, "the turned cube reaches below the ground: {low}");
        let grounded = Placement {
            position: [0.0, -low, 0.0],
            ..turned
        };
        assert!(lowest(model, &grounded).unwrap().abs() < 1e-4);
        // Kept on the model, and read back clamped.
        let wild = Entity::empty("Cube", sue_model::class::MODEL)
            .with_property(OBJECT_SCALE, Value::from(500.0))
            .with_property(OBJECT_X, Value::from(f64::NAN));
        let read = Placement::of(&wild);
        assert_eq!(read.scale, 20.0);
        assert_eq!(read.position[0], 0.0);
    }

    #[test]
    fn manga_perspective_keeps_the_middle_and_grows_what_comes_near() {
        let size = [200, 200];
        let frame = ([0.0, 0.0, 0.0], 2.0);
        let plain = View3D {
            yaw: 0.0,
            pitch: 0.0,
            ..View3D::default()
        };
        let manga = View3D {
            manga: 1.0,
            ..plain
        };
        let (fov, distance) = manga.lens();
        assert_eq!(fov, View3D::MANGA_FOV);
        assert!(distance < plain.distance, "nearer");
        let half = |degrees: f32| (degrees.to_radians() / 2.0).tan();
        assert!((distance * half(fov) - plain.distance * half(plain.fov)).abs() < 1e-4);
        assert_eq!(plain.lens(), (plain.fov, plain.distance), "off: as set");
        let see = |view: &View3D, p: [f32; 3]| Camera::new(view, size, frame).project(p).unwrap();
        // A point on the middle's plane shows where it did.
        let side = [0.5, 0.0, 0.0];
        assert!((see(&plain, side)[0] - see(&manga, side)[0]).abs() < 0.5);
        // A point toward the camera shows further out.
        let near = [0.5, 0.0, 0.8];
        let spread = |view: &View3D| see(view, near)[0] - 100.0;
        assert!(spread(&manga) > spread(&plain) * 1.2, "foreshortened");
        // Kept as a property.
        let mut arena = Arena::new();
        let layer = Entity::empty("3D", CLASS);
        let id = layer.id();
        arena.insert_keep_id(layer).unwrap();
        for mut set in (View3D {
            manga: 0.4,
            ..plain
        })
        .edits(id)
        {
            sue_core::ReversibleTransaction::apply(&mut set, &mut arena).unwrap();
        }
        assert!((View3D::of(arena.get_by_id(id).unwrap()).manga - 0.4).abs() < 1e-6);
    }
}
