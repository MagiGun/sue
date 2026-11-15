//! The canvas grid's settings (View, Grid Settings): how far apart its
//! lines are, how many parts each cell is divided into, and where it
//! starts. They are the picture's, kept as its properties, so a file
//! keeps its grid and a change to them is a step of its history.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, Value};

use crate::doc::Edit;

/// The gap between the grid's main lines, in pixels (`Float`).
pub const GAP: &str = "paint.grid.gap";
/// How many parts each cell is divided into (`Int`, 1 for none).
pub const DIVISIONS: &str = "paint.grid.divisions";
/// Where the grid starts: 0 the top left corner, 1 the middle (`Int`).
pub const ORIGIN: &str = "paint.grid.origin";

/// The smallest and largest gap a grid takes.
pub const GAPS: std::ops::RangeInclusive<f64> = 2.0..=4096.0;
/// The most parts a cell is divided into.
pub const MOST_DIVISIONS: u32 = 16;

/// A picture's grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub gap: f32,
    pub divisions: u32,
    /// 0 the top left corner, 1 the middle of the picture.
    pub origin: u8,
}

impl Default for Grid {
    /// The grid Sue drew before it had settings.
    fn default() -> Self {
        Self {
            gap: 64.0,
            divisions: 1,
            origin: 0,
        }
    }
}

impl Grid {
    /// The grid of `picture`, the default where it has no settings.
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Self {
        let Ok(entity) = arena.get_by_id(picture) else {
            return Self::default();
        };
        let default = Self::default();
        Self {
            gap: entity
                .property(GAP)
                .and_then(Value::as_f64)
                .map_or(default.gap, |gap| {
                    gap.clamp(*GAPS.start(), *GAPS.end()) as f32
                }),
            divisions: entity
                .property(DIVISIONS)
                .and_then(Value::as_i64)
                .map_or(default.divisions, |count| {
                    count.clamp(1, i64::from(MOST_DIVISIONS)) as u32
                }),
            origin: entity
                .property(ORIGIN)
                .and_then(Value::as_i64)
                .map_or(default.origin, |origin| u8::from(origin == 1)),
        }
    }

    /// The gap between the finest lines: what a stroke snaps to.
    #[must_use]
    pub fn step(&self) -> f32 {
        self.gap / self.divisions.max(1) as f32
    }

    /// Where the grid starts in a picture of `size`, in its pixels.
    #[must_use]
    pub fn origin_in(&self, size: [u32; 2]) -> [f32; 2] {
        if self.origin == 1 {
            [size[0] as f32 / 2.0, size[1] as f32 / 2.0]
        } else {
            [0.0, 0.0]
        }
    }

    /// The edit that gives `picture` this grid, or `None` when it has it.
    #[must_use]
    pub fn apply(&self, arena: &Arena, picture: EntityId) -> Option<Edit> {
        if Self::of(arena, picture) == *self {
            return None;
        }
        Some(
            Edit::new(tr!("grid-settings"))
                .then(SetProperty::new(picture, GAP, f64::from(self.gap)))
                .then(SetProperty::new(
                    picture,
                    DIVISIONS,
                    i64::from(self.divisions.clamp(1, MOST_DIVISIONS)),
                ))
                .then(SetProperty::new(picture, ORIGIN, i64::from(self.origin))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{NewPicture, create};
    use sue_core::UndoStack;

    #[test]
    fn a_picture_keeps_its_grid_and_undo_gives_the_old_one_back() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 200,
            height: 100,
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(Grid::of(&arena, picture), Grid::default());
        let grid = Grid {
            gap: 50.0,
            divisions: 5,
            origin: 1,
        };
        assert_eq!(grid.step(), 10.0);
        assert_eq!(grid.origin_in([200, 100]), [100.0, 50.0]);
        let edit = grid.apply(&arena, picture).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(Grid::of(&arena, picture), grid);
        assert!(grid.apply(&arena, picture).is_none(), "nothing to change");
        history.undo(&mut arena).unwrap();
        assert_eq!(Grid::of(&arena, picture), Grid::default());
        history.redo(&mut arena).unwrap();
        assert_eq!(Grid::of(&arena, picture), grid);
    }
}
