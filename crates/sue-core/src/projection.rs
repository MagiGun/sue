//! Non-destructive views of an entity.

use std::fmt;

use crate::ComponentKind;

/// A way of viewing an entity in a studio. Each projection has exactly one
/// native component kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Projection {
    /// Text documents and notes (`StringBuffer`).
    Document,
    /// Audio (`PCMFloat1D`).
    Audio,
    /// Vector art and sketches (`VectorSpline2D`).
    Vector2D,
    /// Raster images and thumbnails (`RasterTile2D`).
    Raster2D,
    /// 3D meshes (`MeshBuffer3D`).
    Mesh3D,
    /// Placement on the timeline (`TimeSpan`).
    Timeline,
    /// Code and scene graphs (`ASTNode`).
    Code,
}

impl Projection {
    /// Every projection, in bit order.
    pub const ALL: [Self; 7] = [
        Self::Document,
        Self::Audio,
        Self::Vector2D,
        Self::Raster2D,
        Self::Mesh3D,
        Self::Timeline,
        Self::Code,
    ];

    /// The component kind this projection reads.
    #[must_use]
    pub const fn native_kind(self) -> ComponentKind {
        match self {
            Self::Document => ComponentKind::StringBuffer,
            Self::Audio => ComponentKind::PCMFloat1D,
            Self::Vector2D => ComponentKind::VectorSpline2D,
            Self::Raster2D => ComponentKind::RasterTile2D,
            Self::Mesh3D => ComponentKind::MeshBuffer3D,
            Self::Timeline => ComponentKind::TimeSpan,
            Self::Code => ComponentKind::ASTNode,
        }
    }

    /// The projection whose native kind is `kind`.
    #[must_use]
    pub const fn of_kind(kind: ComponentKind) -> Self {
        match kind {
            ComponentKind::StringBuffer => Self::Document,
            ComponentKind::PCMFloat1D => Self::Audio,
            ComponentKind::VectorSpline2D => Self::Vector2D,
            ComponentKind::RasterTile2D => Self::Raster2D,
            ComponentKind::MeshBuffer3D => Self::Mesh3D,
            ComponentKind::TimeSpan => Self::Timeline,
            ComponentKind::ASTNode => Self::Code,
        }
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Document => "Document",
            Self::Audio => "Audio",
            Self::Vector2D => "Vector2D",
            Self::Raster2D => "Raster2D",
            Self::Mesh3D => "Mesh3D",
            Self::Timeline => "Timeline",
            Self::Code => "Code",
        }
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

impl fmt::Display for Projection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The set of projections an entity currently supports, as a 16-bit mask.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ProjectionMask(u16);

impl ProjectionMask {
    /// No projections.
    pub const EMPTY: Self = Self(0);

    /// The raw bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// `true` if `projection` is in the set.
    #[must_use]
    pub const fn contains(self, projection: Projection) -> bool {
        self.0 & projection.bit() != 0
    }

    /// The set plus `projection`.
    #[must_use]
    pub const fn with(self, projection: Projection) -> Self {
        Self(self.0 | projection.bit())
    }

    /// The set minus `projection`.
    #[must_use]
    pub const fn without(self, projection: Projection) -> Self {
        Self(self.0 & !projection.bit())
    }

    /// Adds `projection`.
    pub fn insert(&mut self, projection: Projection) {
        *self = self.with(projection);
    }

    /// Removes `projection`.
    pub fn remove(&mut self, projection: Projection) {
        *self = self.without(projection);
    }

    /// `true` if the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Number of projections in the set.
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The projections in the set, in bit order.
    pub fn iter(self) -> impl Iterator<Item = Projection> {
        Projection::ALL
            .into_iter()
            .filter(move |&projection| self.contains(projection))
    }
}

impl FromIterator<Projection> for ProjectionMask {
    fn from_iter<I: IntoIterator<Item = Projection>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, Self::with)
    }
}

impl fmt::Display for ProjectionMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }
        for (i, projection) in self.iter().enumerate() {
            if i > 0 {
                f.write_str("|")?;
            }
            f.write_str(projection.name())?;
        }
        Ok(())
    }
}

impl fmt::Debug for ProjectionMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProjectionMask({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_projections_are_inverse() {
        for projection in Projection::ALL {
            assert_eq!(Projection::of_kind(projection.native_kind()), projection);
        }
    }

    #[test]
    fn mask_set_operations() {
        let mut mask = ProjectionMask::EMPTY.with(Projection::Document);
        mask.insert(Projection::Timeline);
        assert!(mask.contains(Projection::Document));
        assert!(mask.contains(Projection::Timeline));
        assert!(!mask.contains(Projection::Audio));
        assert_eq!(mask.len(), 2);
        assert_eq!(mask.to_string(), "Document|Timeline");
        mask.remove(Projection::Document);
        assert_eq!(mask.iter().collect::<Vec<_>>(), vec![Projection::Timeline]);
        assert_eq!(ProjectionMask::EMPTY.to_string(), "none");
    }

    #[test]
    fn collects_from_iterator() {
        let mask: ProjectionMask = [Projection::Code, Projection::Document]
            .into_iter()
            .collect();
        assert_eq!(mask.bits(), 0b100_0001);
    }
}
