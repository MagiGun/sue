//! The undoable world: the data arena and the live rice under one history.
//! Every change to either is a [`ReversibleTransaction`] that stores its
//! inverse delta (backbone rule 2).

use sue_core::codec::CodecError;
use sue_core::{Arena, AssetHandle, ComponentRegistry, ReversibleTransaction, SueFile, TxnError};
use sue_rice::RiceSchema;

/// Everything the global undo tree can change.
/// One `UndoStack<World>` makes `Ctrl+Z` undo typing, a brush stroke and a
/// slider in Settings alike.
#[derive(Debug, Default)]
pub struct World {
    /// The shared data arena.
    pub arena: Arena,
    /// The live rice.
    pub rice: RiceSchema,
}

impl World {
    /// Writes the document (entities and relations) into `file`, keeping
    /// every other chunk it already has, so chunks this build does not know
    /// (an older build's canvas layout, say) survive untouched.
    pub fn write_into(&self, file: &mut SueFile) {
        file.set_arena(&self.arena);
    }

    /// Replaces the document with the one in `file`; the rice is untouched.
    /// Old handles go stale (never reused for the new entities), so the
    /// caller must start a new undo history.
    ///
    /// # Errors
    /// If the file is malformed; the world is unchanged then.
    pub fn load_file(
        &mut self,
        file: &SueFile,
        registry: &ComponentRegistry,
    ) -> Result<(), CodecError> {
        let mut loaded = file.arena(registry)?;
        let current: Vec<AssetHandle> = self.arena.iter().map(|(handle, _)| handle).collect();
        for handle in current {
            let _ = self.arena.despawn(handle);
        }
        let kinds: Vec<String> = loaded.relations().tree_kinds().map(str::to_owned).collect();
        for kind in kinds {
            self.arena.add_tree_kind(kind);
        }
        let relations: Vec<_> = loaded.relations().iter().collect();
        let handles: Vec<AssetHandle> = loaded.iter().map(|(handle, _)| handle).collect();
        for handle in handles {
            if let Ok(entity) = loaded.despawn(handle) {
                self.arena.insert(entity);
            }
        }
        for relation in relations {
            let _ = self.arena.relate(relation);
        }
        Ok(())
    }
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

/// Several world transactions as one undo step (the kernel's generic
/// [`sue_core::txn::Batch`]).
pub type Batch = sue_core::txn::Batch<World>;

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::txn::{InsertText, Rename};
    use sue_core::{Chunk, StringBuffer, UndoStack};
    use sue_rice::{RiceValue, SetRiceValue};

    #[test]
    fn projects_save_and_open_and_keep_chunks_this_build_does_not_know() {
        let mut world = World::default();
        let doc = world.arena.spawn("doc", StringBuffer::from("shared"));
        let doc_id = world.arena.get(doc).unwrap().id();
        let mut file = SueFile::new();
        // An older build's canvas layout, and anything newer, stay as they
        // are: opaque bytes this build carries along.
        for tag in [*b"SCNE", *b"XTRA"] {
            file.set(Chunk {
                tag,
                version: 1,
                payload: vec![7, 1],
            });
        }
        world.write_into(&mut file);
        let bytes = file.to_bytes();

        let mut other = World::default();
        let scratch = other.arena.spawn("scratch", StringBuffer::new());
        let reopened = SueFile::from_bytes(&bytes).unwrap();
        other
            .load_file(&reopened, &ComponentRegistry::new())
            .unwrap();
        assert_eq!(other.arena.len(), 1, "only the document");
        assert!(!other.arena.contains(scratch));
        let loaded = other.arena.handle_of(doc_id).unwrap();
        assert_ne!(loaded, doc, "new handles never alias old ones");
        assert_eq!(
            other
                .arena
                .get(loaded)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str(),
            "shared"
        );
        // Saved again, the unknown chunks are still there, byte for byte.
        let mut again = reopened;
        other.write_into(&mut again);
        let again = SueFile::from_bytes(&again.to_bytes()).unwrap();
        for tag in [*b"SCNE", *b"XTRA"] {
            assert_eq!(again.chunk(tag).unwrap().payload, [7, 1]);
        }
        // An empty project leaves nothing.
        other
            .load_file(&SueFile::new(), &ComponentRegistry::new())
            .unwrap();
        assert!(other.arena.is_empty());
    }

    #[test]
    fn one_history_undoes_data_and_theme_edits_in_order() {
        let mut world = World {
            rice: RiceSchema::parse("[layout]\ngap = 8 # [0..64]\n").unwrap(),
            ..World::default()
        };
        let handle = world.arena.spawn("Note", StringBuffer::new());
        let id = world.arena.get(handle).unwrap().id();
        let mut history = UndoStack::new();
        history
            .apply(&mut world, ArenaEdit(InsertText::new(handle, 0, "hi")))
            .unwrap();
        history
            .apply(&mut world, ArenaEdit(Rename::new(id, "Letter")))
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
        assert_eq!(world.arena.get(handle).unwrap().name(), "Note");
        history.undo(&mut world).unwrap();
        let text = |world: &World| {
            world
                .arena
                .get(handle)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str()
                .to_owned()
        };
        assert_eq!(text(&world), "");
        // Redo brings back the same buffer.
        history.redo(&mut world).unwrap();
        history.redo(&mut world).unwrap();
        assert_eq!(text(&world), "hi");
        assert_eq!(world.arena.get(handle).unwrap().name(), "Letter");
    }

    #[test]
    fn batches_are_one_step_and_atomic() {
        let mut world = World::default();
        let handle = world.arena.spawn("clip", StringBuffer::new());
        let id = world.arena.get(handle).unwrap().id();
        let mut history = UndoStack::new();
        let both = Batch::new("Both")
            .then(ArenaEdit(InsertText::new(handle, 0, "ab")))
            .then(ArenaEdit(Rename::new(id, "Named")));
        history.apply(&mut world, both).unwrap();
        history.undo(&mut world).unwrap();
        assert_eq!(world.arena.get(handle).unwrap().name(), "clip");
        assert!(
            world
                .arena
                .get(handle)
                .unwrap()
                .string_buffer()
                .unwrap()
                .is_empty()
        );
        // A failing step leaves nothing behind.
        let gone = world.arena.spawn("gone", StringBuffer::new());
        world.arena.despawn(gone).unwrap();
        let broken = Batch::new("Broken")
            .then(ArenaEdit(Rename::new(id, "Half")))
            .then(ArenaEdit(InsertText::new(gone, 0, "x")));
        assert!(history.apply(&mut world, broken).is_err());
        assert_eq!(world.arena.get(handle).unwrap().name(), "clip");
    }
}
