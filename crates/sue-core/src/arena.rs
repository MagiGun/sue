//! The generational entity arena.

use std::collections::BTreeMap;

use crate::{
    AssetHandle, ComponentData, ComponentKind, CoreError, Projection, ProjectionMask, StringBuffer,
};

/// An asset: a base component plus any number of projection components, at
/// most one per kind, and free-form string metadata (source path, byte length,
/// codec details).
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    name: String,
    base: ComponentKind,
    projections: ProjectionMask,
    components: BTreeMap<ComponentKind, ComponentData>,
    metadata: BTreeMap<String, String>,
}

impl Entity {
    fn new(name: String, base: ComponentData) -> Self {
        let kind = base.kind();
        Self {
            name,
            base: kind,
            projections: ProjectionMask::EMPTY.with(Projection::of_kind(kind)),
            components: BTreeMap::from([(kind, base)]),
            metadata: BTreeMap::new(),
        }
    }

    /// Human-readable name, e.g. a file name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The metadata value for `key`.
    #[must_use]
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    /// Sets a metadata value, returning the previous one. Metadata describes an
    /// asset (where it came from, how it was decoded); it is not edit state.
    pub fn set_meta(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String> {
        self.metadata.insert(key.into(), value.into())
    }

    /// All metadata, ordered by key.
    pub fn metadata(&self) -> impl Iterator<Item = (&str, &str)> {
        self.metadata
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    /// Kind of the base component given at spawn.
    #[must_use]
    pub fn base_kind(&self) -> ComponentKind {
        self.base
    }

    /// Projections currently attached.
    #[must_use]
    pub fn projections(&self) -> ProjectionMask {
        self.projections
    }

    /// The component of `kind`, if attached.
    #[must_use]
    pub fn component(&self, kind: ComponentKind) -> Option<&ComponentData> {
        self.components.get(&kind)
    }

    /// The component of `kind`, mutably. Mutations must come from a
    /// [`ReversibleTransaction`](crate::ReversibleTransaction).
    pub fn component_mut(&mut self, kind: ComponentKind) -> Option<&mut ComponentData> {
        self.components.get_mut(&kind)
    }

    /// All attached components, ordered by kind.
    pub fn components(&self) -> impl Iterator<Item = &ComponentData> {
        self.components.values()
    }

    /// The `StringBuffer` component, if attached.
    #[must_use]
    pub fn string_buffer(&self) -> Option<&StringBuffer> {
        self.component(ComponentKind::StringBuffer)?
            .as_string_buffer()
    }

    /// The `StringBuffer` component, mutably.
    pub fn string_buffer_mut(&mut self) -> Option<&mut StringBuffer> {
        self.component_mut(ComponentKind::StringBuffer)?
            .as_string_buffer_mut()
    }

    /// Approximate heap footprint of every component, in bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.components
            .values()
            .map(ComponentData::heap_bytes)
            .sum()
    }
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u32,
    entity: Option<Entity>,
}

/// Generational slot arena of [`Entity`]s, addressed by [`AssetHandle`].
///
/// `spawn` and `despawn` are O(1). Despawning bumps the slot generation, so old
/// handles go stale instead of aliasing the next occupant. Mutation needs
/// `&mut Arena`. The arena is `Send + Sync`, so any number of threads may read
/// it concurrently.
#[derive(Clone, Debug, Default)]
pub struct Arena {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
}

impl Arena {
    /// An empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty arena with room for `capacity` entities.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            ..Self::default()
        }
    }

    /// Number of live entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live
    }

    /// `true` if no entity is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Creates an entity named `name` whose base component is `base`.
    ///
    /// # Panics
    /// If more than `u32::MAX` slots would be needed.
    pub fn spawn(
        &mut self,
        name: impl Into<String>,
        base: impl Into<ComponentData>,
    ) -> AssetHandle {
        let entity = Entity::new(name.into(), base.into());
        self.live += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.entity = Some(entity);
            return AssetHandle::new(index, slot.generation);
        }
        let index = u32::try_from(self.slots.len()).expect("arena exceeded u32::MAX slots");
        self.slots.push(Slot {
            generation: 1,
            entity: Some(entity),
        });
        AssetHandle::new(index, 1)
    }

    /// Removes an entity and returns it. Every handle to it becomes stale.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn despawn(&mut self, handle: AssetHandle) -> Result<Entity, CoreError> {
        let slot = self
            .slots
            .get_mut(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .ok_or(CoreError::StaleHandle(handle))?;
        let entity = slot.entity.take().ok_or(CoreError::StaleHandle(handle))?;
        self.live -= 1;
        // A slot whose generation is exhausted is retired, never reused.
        if slot.generation < u32::MAX {
            slot.generation += 1;
            self.free.push(handle.index());
        }
        Ok(entity)
    }

    /// `true` if `handle` refers to a live entity.
    #[must_use]
    pub fn contains(&self, handle: AssetHandle) -> bool {
        self.get(handle).is_ok()
    }

    /// The entity behind `handle`.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn get(&self, handle: AssetHandle) -> Result<&Entity, CoreError> {
        self.slots
            .get(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .and_then(|slot| slot.entity.as_ref())
            .ok_or(CoreError::StaleHandle(handle))
    }

    /// The entity behind `handle`, mutably.
    ///
    /// # Errors
    /// [`CoreError::StaleHandle`] if the handle is not live.
    pub fn get_mut(&mut self, handle: AssetHandle) -> Result<&mut Entity, CoreError> {
        self.slots
            .get_mut(handle.index() as usize)
            .filter(|slot| slot.generation == handle.generation())
            .and_then(|slot| slot.entity.as_mut())
            .ok_or(CoreError::StaleHandle(handle))
    }

    /// Borrows `read` shared and `write` exclusively at the same time, without
    /// copying either. This is the primitive behind zero-copy pipelines.
    ///
    /// # Errors
    /// [`CoreError::AliasedHandles`] if both handles are equal, or
    /// [`CoreError::StaleHandle`] if either is not live.
    pub fn split_pair(
        &mut self,
        read: AssetHandle,
        write: AssetHandle,
    ) -> Result<(&Entity, &mut Entity), CoreError> {
        if read == write {
            return Err(CoreError::AliasedHandles(read));
        }
        self.get(read)?;
        self.get(write)?;
        // Two distinct live handles cannot share a slot: a slot has one generation.
        let (read_index, write_index) = (read.index() as usize, write.index() as usize);
        let (reader, writer) = if read_index < write_index {
            let (low, high) = self.slots.split_at_mut(write_index);
            (&low[read_index], &mut high[0])
        } else {
            let (low, high) = self.slots.split_at_mut(read_index);
            (&high[0], &mut low[write_index])
        };
        match (reader.entity.as_ref(), writer.entity.as_mut()) {
            (Some(reader), Some(writer)) => Ok((reader, writer)),
            (None, _) => Err(CoreError::StaleHandle(read)),
            (_, None) => Err(CoreError::StaleHandle(write)),
        }
    }

    /// Checks that `projection` could be attached to `handle` with a component
    /// of `kind`, without changing anything.
    ///
    /// # Errors
    /// The same errors [`Arena::attach_projection`] would return.
    pub fn check_attach(
        &self,
        handle: AssetHandle,
        projection: Projection,
        kind: ComponentKind,
    ) -> Result<(), CoreError> {
        if kind != projection.native_kind() {
            return Err(CoreError::ProjectionMismatch {
                projection,
                found: kind,
            });
        }
        let entity = self.get(handle)?;
        if entity.projections.contains(projection) || entity.components.contains_key(&kind) {
            return Err(CoreError::AlreadyProjected { handle, projection });
        }
        Ok(())
    }

    /// Attaches `projection` by adding `component` next to the existing ones.
    /// Existing buffers are neither copied, moved on the heap, nor mutated.
    ///
    /// # Errors
    /// [`CoreError::ProjectionMismatch`], [`CoreError::AlreadyProjected`] or
    /// [`CoreError::StaleHandle`].
    pub fn attach_projection(
        &mut self,
        handle: AssetHandle,
        projection: Projection,
        component: ComponentData,
    ) -> Result<(), CoreError> {
        let kind = component.kind();
        self.check_attach(handle, projection, kind)?;
        let entity = self.get_mut(handle)?;
        entity.components.insert(kind, component);
        entity.projections.insert(projection);
        Ok(())
    }

    /// Detaches `projection` and returns its component, so the change can be
    /// reverted.
    ///
    /// # Errors
    /// [`CoreError::CannotDetachBase`], [`CoreError::NotProjected`] or
    /// [`CoreError::StaleHandle`].
    pub fn detach_projection(
        &mut self,
        handle: AssetHandle,
        projection: Projection,
    ) -> Result<ComponentData, CoreError> {
        let entity = self.get_mut(handle)?;
        let kind = projection.native_kind();
        if kind == entity.base {
            return Err(CoreError::CannotDetachBase { handle, projection });
        }
        let component = entity
            .components
            .remove(&kind)
            .ok_or(CoreError::NotProjected { handle, projection })?;
        entity.projections.remove(projection);
        Ok(component)
    }

    /// Every live entity with its handle, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (AssetHandle, &Entity)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            let entity = slot.entity.as_ref()?;
            // `spawn` guarantees the slot count fits in u32.
            Some((AssetHandle::new(index as u32, slot.generation), entity))
        })
    }

    /// Handles of every live entity that supports `projection`.
    pub fn with_projection(&self, projection: Projection) -> impl Iterator<Item = AssetHandle> {
        self.iter()
            .filter(move |(_, entity)| entity.projections.contains(projection))
            .map(|(handle, _)| handle)
    }

    /// Approximate heap footprint of all component payloads, in bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.iter().map(|(_, entity)| entity.heap_bytes()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeshBuffer3D, PCMFloat1D, PixelFormat, RasterTile2D, TimeSpan};

    #[test]
    fn spawn_get_despawn() {
        let mut arena = Arena::new();
        let handle = arena.spawn("note.txt", StringBuffer::from("hello"));
        assert_eq!(arena.len(), 1);
        let entity = arena.get(handle).unwrap();
        assert_eq!(entity.name(), "note.txt");
        assert_eq!(entity.base_kind(), ComponentKind::StringBuffer);
        assert_eq!(entity.string_buffer().unwrap().as_str(), "hello");
        let removed = arena.despawn(handle).unwrap();
        assert_eq!(removed.name(), "note.txt");
        assert!(arena.is_empty());
        assert_eq!(arena.get(handle), Err(CoreError::StaleHandle(handle)));
        assert!(arena.despawn(handle).is_err());
    }

    #[test]
    fn reused_slots_get_new_generations() {
        let mut arena = Arena::new();
        let first = arena.spawn("a", StringBuffer::new());
        arena.despawn(first).unwrap();
        let second = arena.spawn("b", StringBuffer::new());
        assert_eq!(first.index(), second.index());
        assert_eq!(second.generation(), first.generation() + 1);
        assert!(!arena.contains(first));
        assert!(arena.contains(second));
        assert!(!arena.contains(AssetHandle::NULL));
    }

    #[test]
    fn split_pair_borrows_two_entities() {
        let mut arena = Arena::new();
        let source = arena.spawn("src", StringBuffer::from("abc"));
        let target = arena.spawn("dst", StringBuffer::new());
        for (read, write) in [(source, target), (target, source)] {
            let (reader, writer) = arena.split_pair(read, write).unwrap();
            let text = reader.string_buffer().unwrap().as_str().to_owned();
            let buffer = writer.string_buffer_mut().unwrap();
            let end = buffer.len();
            buffer.insert(end, &text).unwrap();
        }
        assert_eq!(
            arena.get(target).unwrap().string_buffer().unwrap().as_str(),
            "abc"
        );
        assert_eq!(
            arena.get(source).unwrap().string_buffer().unwrap().as_str(),
            "abcabc"
        );
        assert_eq!(
            arena.split_pair(source, source).unwrap_err(),
            CoreError::AliasedHandles(source)
        );
    }

    #[test]
    fn attaching_a_projection_never_moves_existing_buffers() {
        let mut arena = Arena::new();
        let pcm = PCMFloat1D::new(48_000, 1, vec![0.25; 4096]).unwrap();
        let handle = arena.spawn("take-1.wav", pcm);
        let before = arena
            .get(handle)
            .unwrap()
            .component(ComponentKind::PCMFloat1D)
            .unwrap();
        let address = before.f32_view().unwrap().as_ptr();

        let span = TimeSpan::from_seconds(0.0, 4096.0 / 48_000.0).unwrap();
        arena
            .attach_projection(handle, Projection::Timeline, span.into())
            .unwrap();
        let tile = RasterTile2D::new(64, 64, PixelFormat::Rgba8).unwrap();
        arena
            .attach_projection(handle, Projection::Raster2D, tile.into())
            .unwrap();

        let entity = arena.get(handle).unwrap();
        let after = entity.component(ComponentKind::PCMFloat1D).unwrap();
        assert_eq!(after.f32_view().unwrap().as_ptr(), address);
        assert_eq!(entity.projections().len(), 3);
        assert_eq!(arena.with_projection(Projection::Timeline).count(), 1);
    }

    #[test]
    fn projection_rules_are_enforced() {
        let mut arena = Arena::new();
        let handle = arena.spawn("tile", RasterTile2D::new(2, 2, PixelFormat::Gray8).unwrap());
        assert_eq!(
            arena.attach_projection(handle, Projection::Mesh3D, StringBuffer::new().into()),
            Err(CoreError::ProjectionMismatch {
                projection: Projection::Mesh3D,
                found: ComponentKind::StringBuffer
            })
        );
        arena
            .attach_projection(
                handle,
                Projection::Mesh3D,
                MeshBuffer3D::quad(2.0, 2.0).into(),
            )
            .unwrap();
        assert!(matches!(
            arena.attach_projection(handle, Projection::Mesh3D, MeshBuffer3D::default().into()),
            Err(CoreError::AlreadyProjected { .. })
        ));
        assert!(matches!(
            arena.detach_projection(handle, Projection::Raster2D),
            Err(CoreError::CannotDetachBase { .. })
        ));
        let mesh = arena.detach_projection(handle, Projection::Mesh3D).unwrap();
        assert_eq!(mesh.kind(), ComponentKind::MeshBuffer3D);
        assert!(matches!(
            arena.detach_projection(handle, Projection::Mesh3D),
            Err(CoreError::NotProjected { .. })
        ));
    }

    #[test]
    fn entities_carry_metadata() {
        let mut arena = Arena::new();
        let handle = arena.spawn("photo.jxl", StringBuffer::new());
        let entity = arena.get_mut(handle).unwrap();
        assert_eq!(entity.set_meta("source", "C:/photos/photo.jxl"), None);
        assert_eq!(entity.set_meta("bytes", "1024"), None);
        assert_eq!(entity.set_meta("bytes", "2048").as_deref(), Some("1024"));
        let entity = arena.get(handle).unwrap();
        assert_eq!(entity.meta("bytes"), Some("2048"));
        assert_eq!(entity.metadata().count(), 2);
        assert_eq!(entity.meta("missing"), None);
    }

    #[test]
    fn iterates_live_entities_only() {
        let mut arena = Arena::with_capacity(4);
        let a = arena.spawn("a", StringBuffer::from("1234"));
        let b = arena.spawn("b", StringBuffer::from("56"));
        arena.despawn(a).unwrap();
        let handles: Vec<_> = arena.iter().map(|(handle, _)| handle).collect();
        assert_eq!(handles, vec![b]);
        assert_eq!(arena.heap_bytes(), 2);
    }
}
