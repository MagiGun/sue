//! The undoable world: the data arena plus the live rice, under one history.

use sue_core::{Arena, ReversibleTransaction, TxnError};
use sue_rice::RiceSchema;

/// Everything the global undo tree can change: arena data and theme settings.
/// One `UndoStack<World>` makes `Ctrl+Z` undo note edits and slider moves alike.
#[derive(Debug, Default)]
pub struct World {
    /// The shared data arena.
    pub arena: Arena,
    /// The live rice.
    pub rice: RiceSchema,
}

/// Lifts an arena transaction onto the [`World`].
#[derive(Debug)]
pub struct ArenaEdit<X>(pub X);

impl<X: ReversibleTransaction<Arena>> ReversibleTransaction<World> for ArenaEdit<X> {
    fn label(&self) -> String {
        self.0.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.apply(&mut world.arena)
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.revert(&mut world.arena)
    }
}

/// Lifts a rice transaction onto the [`World`].
#[derive(Debug)]
pub struct RiceEdit<X>(pub X);

impl<X: ReversibleTransaction<RiceSchema>> ReversibleTransaction<World> for RiceEdit<X> {
    fn label(&self) -> String {
        self.0.label()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.apply(&mut world.rice)
    }

    fn revert(&mut self, world: &mut World) -> Result<(), TxnError> {
        self.0.revert(&mut world.rice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::txn::InsertText;
    use sue_core::{StringBuffer, UndoStack};
    use sue_rice::{RiceValue, SetRiceValue};

    #[test]
    fn one_history_undoes_data_and_theme_edits_in_order() {
        let mut world = World {
            rice: RiceSchema::parse("[layout]\ngap = 8 # [0..64]\n").unwrap(),
            ..World::default()
        };
        let note = world.arena.spawn("note", StringBuffer::new());
        let mut history = UndoStack::new();
        history
            .apply(&mut world, ArenaEdit(InsertText::new(note, 0, "hi")))
            .unwrap();
        history
            .apply(
                &mut world,
                RiceEdit(SetRiceValue::new("layout.gap", RiceValue::Int(20))),
            )
            .unwrap();
        assert_eq!(world.rice.get_i64("layout.gap"), Some(20));
        history.undo(&mut world).unwrap();
        assert_eq!(world.rice.get_i64("layout.gap"), Some(8));
        history.undo(&mut world).unwrap();
        assert_eq!(
            world
                .arena
                .get(note)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str(),
            ""
        );
        history.redo(&mut world).unwrap();
        assert_eq!(history.history(), vec!["Insert 2 character(s)"]);
    }
}
