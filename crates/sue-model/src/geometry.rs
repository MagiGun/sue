//! Geometry and transforms. 3D modeling, sculpting, CAD and BIM, vector
//! illustration, 2D rigging, game levels and point clouds all describe
//! shapes as points joined into primitives, with named attributes on each
//! domain (a normal per point, a UV per vertex, a material per face, a
//! layer name for the whole thing). [`Geometry`] stores exactly that, in
//! double precision so CAD stays exact at building scale. [`Transform`]
//! places any entity in 2D or 3D space.

use std::collections::BTreeMap;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, CoreError, MeshBuffer3D};

/// What a primitive's vertices mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimKind {
    /// A closed polygon (a face).
    Polygon,
    /// An open polyline.
    Polyline,
    /// A cubic Bezier chain, `3n + 1` vertices, open or closed.
    Bezier {
        /// The end joins the start.
        closed: bool,
    },
}

/// One primitive: a run of vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prim {
    /// First vertex.
    pub start: u32,
    /// Number of vertices.
    pub count: u32,
    /// What it is.
    pub kind: PrimKind,
}

/// Where an attribute lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    /// One value per point.
    Point,
    /// One value per vertex (a point's use in a primitive).
    Vertex,
    /// One value per primitive.
    Primitive,
    /// One value for the whole geometry.
    Detail,
}

/// Attribute values, `width` numbers per element.
#[derive(Clone, Debug, PartialEq)]
pub enum AttributeData {
    /// 32-bit floats (normals, UVs, colors, weights).
    Float {
        /// Numbers per element.
        width: u8,
        /// The numbers.
        values: Vec<f32>,
    },
    /// 32-bit integers (ids, material indices, flags).
    Int {
        /// Numbers per element.
        width: u8,
        /// The numbers.
        values: Vec<i32>,
    },
    /// Text (names, layer, part numbers).
    Text(Vec<String>),
}

impl AttributeData {
    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Float { width, values } => values.len() / usize::from((*width).max(1)),
            Self::Int { width, values } => values.len() / usize::from((*width).max(1)),
            Self::Text(values) => values.len(),
        }
    }

    /// `true` if there are no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Points, primitives and attributes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    points: Vec<[f64; 3]>,
    vertices: Vec<u32>,
    prims: Vec<Prim>,
    attributes: BTreeMap<(Domain, String), AttributeData>,
}

impl Geometry {
    /// Empty geometry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a point, returning its index.
    ///
    /// # Panics
    /// Past `u32::MAX` points.
    pub fn add_point(&mut self, position: [f64; 3]) -> u32 {
        self.points.push(position);
        u32::try_from(self.points.len() - 1).expect("too many points")
    }

    /// Adds a primitive over existing points.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] for an unknown point, too few vertices
    /// or a Bezier chain without `3n + 1` vertices.
    pub fn add_prim(&mut self, kind: PrimKind, points: &[u32]) -> Result<u32, CoreError> {
        if points
            .iter()
            .any(|point| *point as usize >= self.points.len())
        {
            return Err(CoreError::InvalidBuffer("primitive names a missing point"));
        }
        let minimum = match kind {
            PrimKind::Polygon => 3,
            PrimKind::Polyline | PrimKind::Bezier { .. } => 2,
        };
        if points.len() < minimum {
            return Err(CoreError::InvalidBuffer("primitive has too few vertices"));
        }
        if matches!(kind, PrimKind::Bezier { .. }) && !(points.len() - 1).is_multiple_of(3) {
            return Err(CoreError::InvalidBuffer(
                "a Bezier chain needs 3n + 1 vertices",
            ));
        }
        let start = u32::try_from(self.vertices.len())
            .map_err(|_| CoreError::InvalidBuffer("too many vertices"))?;
        self.vertices.extend_from_slice(points);
        self.prims.push(Prim {
            start,
            count: points.len() as u32,
            kind,
        });
        Ok(self.prims.len() as u32 - 1)
    }

    /// Point positions.
    #[must_use]
    pub fn points(&self) -> &[[f64; 3]] {
        &self.points
    }

    /// Point positions, mutably (the count is fixed).
    pub fn points_mut(&mut self) -> &mut [[f64; 3]] {
        &mut self.points
    }

    /// The primitives.
    #[must_use]
    pub fn prims(&self) -> &[Prim] {
        &self.prims
    }

    /// The points of primitive `prim`.
    #[must_use]
    pub fn prim_points(&self, prim: &Prim) -> &[u32] {
        &self.vertices[prim.start as usize..(prim.start + prim.count) as usize]
    }

    /// Number of elements in `domain`.
    #[must_use]
    pub fn count(&self, domain: Domain) -> usize {
        match domain {
            Domain::Point => self.points.len(),
            Domain::Vertex => self.vertices.len(),
            Domain::Primitive => self.prims.len(),
            Domain::Detail => 1,
        }
    }

    /// Sets an attribute, returning the previous one.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if it does not have one element per
    /// member of `domain`.
    pub fn set_attribute(
        &mut self,
        domain: Domain,
        name: impl Into<String>,
        data: AttributeData,
    ) -> Result<Option<AttributeData>, CoreError> {
        let width_ok = match &data {
            AttributeData::Float { width, values } => {
                *width > 0 && values.len().is_multiple_of(usize::from(*width))
            }
            AttributeData::Int { width, values } => {
                *width > 0 && values.len().is_multiple_of(usize::from(*width))
            }
            AttributeData::Text(_) => true,
        };
        if !width_ok || data.len() != self.count(domain) {
            return Err(CoreError::InvalidBuffer(
                "attribute size does not match its domain",
            ));
        }
        Ok(self.attributes.insert((domain, name.into()), data))
    }

    /// The attribute `name` on `domain`.
    #[must_use]
    pub fn attribute(&self, domain: Domain, name: &str) -> Option<&AttributeData> {
        self.attributes.get(&(domain, name.to_owned()))
    }

    /// Every attribute.
    pub fn attributes(&self) -> impl Iterator<Item = (Domain, &str, &AttributeData)> {
        self.attributes
            .iter()
            .map(|((domain, name), data)| (*domain, name.as_str(), data))
    }

    /// The axis-aligned bounds, if there are points.
    #[must_use]
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.points.first()?;
        Some(
            self.points
                .iter()
                .fold((first, first), |(low, high), point| {
                    (
                        [
                            low[0].min(point[0]),
                            low[1].min(point[1]),
                            low[2].min(point[2]),
                        ],
                        [
                            high[0].max(point[0]),
                            high[1].max(point[1]),
                            high[2].max(point[2]),
                        ],
                    )
                }),
        )
    }

    /// Moves every point by `transform`.
    pub fn transform(&mut self, transform: &Transform) {
        for point in &mut self.points {
            *point = transform.apply(*point);
        }
    }

    /// The polygons as a GPU triangle mesh (fan triangulation, so convex
    /// faces are exact). Polylines and curves are skipped.
    #[must_use]
    pub fn to_mesh(&self) -> MeshBuffer3D {
        let positions = self
            .points
            .iter()
            .map(|point| point.map(|value| value as f32))
            .collect();
        let mut indices = Vec::new();
        for prim in self
            .prims
            .iter()
            .filter(|prim| prim.kind == PrimKind::Polygon)
        {
            let points = self.prim_points(prim);
            for pair in points[1..].windows(2) {
                indices.extend([points[0], pair[0], pair[1]]);
            }
        }
        MeshBuffer3D::new(positions, indices).unwrap_or_default()
    }
}

fn domain_tag(domain: Domain) -> u8 {
    match domain {
        Domain::Point => 0,
        Domain::Vertex => 1,
        Domain::Primitive => 2,
        Domain::Detail => 3,
    }
}

impl ComponentType for Geometry {
    const NAME: &'static str = "sue.geometry";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.f64s(self.points.as_flattened());
        out.len(self.vertices.len());
        self.vertices.iter().for_each(|vertex| out.u32(*vertex));
        out.len(self.prims.len());
        for prim in &self.prims {
            out.u32(prim.start);
            out.u32(prim.count);
            out.u8(match prim.kind {
                PrimKind::Polygon => 0,
                PrimKind::Polyline => 1,
                PrimKind::Bezier { closed: false } => 2,
                PrimKind::Bezier { closed: true } => 3,
            });
        }
        out.len(self.attributes.len());
        for ((domain, name), data) in &self.attributes {
            out.u8(domain_tag(*domain));
            out.str(name);
            match data {
                AttributeData::Float { width, values } => {
                    out.u8(0);
                    out.u8(*width);
                    out.f32s(values);
                }
                AttributeData::Int { width, values } => {
                    out.u8(1);
                    out.u8(*width);
                    out.len(values.len());
                    values
                        .iter()
                        .for_each(|value| out.varint_signed(i64::from(*value)));
                }
                AttributeData::Text(values) => {
                    out.u8(2);
                    out.len(values.len());
                    values.iter().for_each(|value| out.str(value));
                }
            }
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let flat = input.f64s()?;
        if flat.len() % 3 != 0 {
            return Err(input.error("point coordinates are not triples"));
        }
        let mut geometry = Self {
            points: flat.as_chunks::<3>().0.to_vec(),
            ..Self::default()
        };
        let count = input.len(4)?;
        geometry.vertices = (0..count).map(|_| input.u32()).collect::<Result<_, _>>()?;
        if geometry
            .vertices
            .iter()
            .any(|vertex| *vertex as usize >= geometry.points.len())
        {
            return Err(input.error("a vertex names a missing point"));
        }
        for _ in 0..input.len(9)? {
            let (start, count) = (input.u32()?, input.u32()?);
            let kind = match input.u8()? {
                0 => PrimKind::Polygon,
                1 => PrimKind::Polyline,
                2 => PrimKind::Bezier { closed: false },
                3 => PrimKind::Bezier { closed: true },
                other => return Err(input.error(format!("unknown primitive {other}"))),
            };
            if u64::from(start) + u64::from(count) > geometry.vertices.len() as u64 {
                return Err(input.error("a primitive runs past the vertices"));
            }
            geometry.prims.push(Prim { start, count, kind });
        }
        for _ in 0..input.len(3)? {
            let domain = match input.u8()? {
                0 => Domain::Point,
                1 => Domain::Vertex,
                2 => Domain::Primitive,
                3 => Domain::Detail,
                other => return Err(input.error(format!("unknown domain {other}"))),
            };
            let name = input.str()?;
            let data = match input.u8()? {
                0 => AttributeData::Float {
                    width: input.u8()?,
                    values: input.f32s()?,
                },
                1 => {
                    let width = input.u8()?;
                    let count = input.len(1)?;
                    let values = (0..count)
                        .map(|_| {
                            let value = input.varint_signed()?;
                            i32::try_from(value).map_err(|_| input.error("integer overflows"))
                        })
                        .collect::<Result<_, _>>()?;
                    AttributeData::Int { width, values }
                }
                2 => {
                    let count = input.len(1)?;
                    AttributeData::Text((0..count).map(|_| input.str()).collect::<Result<_, _>>()?)
                }
                other => return Err(input.error(format!("unknown attribute type {other}"))),
            };
            geometry
                .set_attribute(domain, name, data)
                .map_err(|error| input.error(error.to_string()))?;
        }
        Ok(geometry)
    }

    fn heap_bytes(&self) -> usize {
        size_of_val(self.points.as_slice())
            + size_of_val(self.vertices.as_slice())
            + size_of_val(self.prims.as_slice())
    }
}

/// Translation, rotation (a unit quaternion `x, y, z, w`) and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Translation.
    pub translation: [f64; 3],
    /// Rotation as a unit quaternion `[x, y, z, w]`.
    pub rotation: [f64; 4],
    /// Scale per axis.
    pub scale: [f64; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

fn quat_mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let p = quat_mul(
        quat_mul(q, [v[0], v[1], v[2], 0.0]),
        [-q[0], -q[1], -q[2], q[3]],
    );
    [p[0], p[1], p[2]]
}

impl Transform {
    /// No change.
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };

    /// A move.
    #[must_use]
    pub fn translate(x: f64, y: f64, z: f64) -> Self {
        Self {
            translation: [x, y, z],
            ..Self::IDENTITY
        }
    }

    /// A rotation of `radians` about the unit `axis`.
    #[must_use]
    pub fn rotate(axis: [f64; 3], radians: f64) -> Self {
        let (sin, cos) = (radians / 2.0).sin_cos();
        Self {
            rotation: [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos],
            ..Self::IDENTITY
        }
    }

    /// A 2D placement: position, rotation about Z and uniform scale.
    #[must_use]
    pub fn planar(x: f64, y: f64, radians: f64, scale: f64) -> Self {
        Self {
            translation: [x, y, 0.0],
            scale: [scale, scale, 1.0],
            ..Self::rotate([0.0, 0.0, 1.0], radians)
        }
    }

    /// Maps a point: scale, then rotate, then translate.
    #[must_use]
    pub fn apply(&self, point: [f64; 3]) -> [f64; 3] {
        let scaled = [
            point[0] * self.scale[0],
            point[1] * self.scale[1],
            point[2] * self.scale[2],
        ];
        let rotated = rotate(self.rotation, scaled);
        [
            rotated[0] + self.translation[0],
            rotated[1] + self.translation[1],
            rotated[2] + self.translation[2],
        ]
    }

    /// `self` after `inner` (a child's transform inside its parent's).
    /// Exact for uniform scale; non-uniform parent scale with rotated
    /// children needs the full matrix ([`Self::matrix`]).
    #[must_use]
    pub fn then_inner(&self, inner: &Self) -> Self {
        Self {
            translation: self.apply(inner.translation),
            rotation: quat_mul(self.rotation, inner.rotation),
            scale: [
                self.scale[0] * inner.scale[0],
                self.scale[1] * inner.scale[1],
                self.scale[2] * inner.scale[2],
            ],
        }
    }

    /// The column-major 4x4 matrix (as GPUs and glTF expect).
    #[must_use]
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let x = rotate(self.rotation, [self.scale[0], 0.0, 0.0]);
        let y = rotate(self.rotation, [0.0, self.scale[1], 0.0]);
        let z = rotate(self.rotation, [0.0, 0.0, self.scale[2]]);
        let t = self.translation;
        [
            [x[0], x[1], x[2], 0.0],
            [y[0], y[1], y[2], 0.0],
            [z[0], z[1], z[2], 0.0],
            [t[0], t[1], t[2], 1.0],
        ]
    }
}

impl ComponentType for Transform {
    const NAME: &'static str = "sue.transform";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        self.translation
            .iter()
            .chain(&self.rotation)
            .chain(&self.scale)
            .for_each(|value| out.f64(*value));
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut values = [0.0; 10];
        for value in &mut values {
            *value = input.f64()?;
        }
        Ok(Self {
            translation: [values[0], values[1], values[2]],
            rotation: [values[3], values[4], values[5], values[6]],
            scale: [values[7], values[8], values[9]],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn geometry_holds_faces_curves_and_attributes() {
        let mut cube_face = Geometry::new();
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let points: Vec<u32> = corners.iter().map(|p| cube_face.add_point(*p)).collect();
        cube_face.add_prim(PrimKind::Polygon, &points).unwrap();
        cube_face
            .add_prim(PrimKind::Polyline, &points[..2])
            .unwrap();
        assert!(cube_face.add_prim(PrimKind::Polygon, &points[..2]).is_err());
        assert!(
            cube_face
                .add_prim(PrimKind::Bezier { closed: false }, &points[..3])
                .is_err()
        );
        cube_face
            .add_prim(PrimKind::Bezier { closed: false }, &points)
            .unwrap();
        cube_face
            .set_attribute(
                Domain::Point,
                "N",
                AttributeData::Float {
                    width: 3,
                    values: [0.0, 0.0, 1.0].repeat(4),
                },
            )
            .unwrap();
        cube_face
            .set_attribute(
                Domain::Primitive,
                "layer",
                AttributeData::Text(vec!["A-WALL".into(); 3]),
            )
            .unwrap();
        cube_face
            .set_attribute(
                Domain::Detail,
                "part",
                AttributeData::Int {
                    width: 1,
                    values: vec![7],
                },
            )
            .unwrap();
        assert!(
            cube_face
                .set_attribute(
                    Domain::Point,
                    "bad",
                    AttributeData::Int {
                        width: 1,
                        values: vec![1]
                    }
                )
                .is_err()
        );
        assert_eq!(cube_face.bounds(), Some(([0.0; 3], [1.0, 1.0, 0.0])));
        let mesh = cube_face.to_mesh();
        assert_eq!(mesh.triangle_count(), 2);

        let mut out = Writer::new();
        ComponentType::encode(&cube_face, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Geometry::decode(1, &mut Reader::new(&bytes)).unwrap(),
            cube_face
        );
    }

    #[test]
    fn transforms_compose() {
        let quarter = Transform::rotate([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
        assert!(close(quarter.apply([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
        let parent = Transform::translate(10.0, 0.0, 0.0).then_inner(&quarter);
        let child = Transform::translate(1.0, 0.0, 0.0);
        let world = parent.then_inner(&child);
        assert!(close(world.apply([0.0; 3]), [10.0, 1.0, 0.0]));
        assert!(close(
            world.apply([0.0; 3]),
            parent.apply(child.apply([0.0; 3]))
        ));
        let m = Transform::planar(5.0, 2.0, 0.0, 2.0).matrix();
        assert_eq!(m[0][0], 2.0);
        assert_eq!(m[3], [5.0, 2.0, 0.0, 1.0]);
        let mut geometry = Geometry::new();
        geometry.add_point([1.0, 0.0, 0.0]);
        geometry.transform(&quarter);
        assert!(close(geometry.points()[0], [0.0, 1.0, 0.0]));
        let mut out = Writer::new();
        ComponentType::encode(&world, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Transform::decode(1, &mut Reader::new(&bytes)).unwrap(),
            world
        );
    }
}
