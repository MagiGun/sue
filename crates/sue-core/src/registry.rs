//! Open components. Besides the seven built-in buffers, an entity carries
//! any number of extension components, one per type name. A feature defines
//! a type once ([`ComponentType`]: a name, a version, an encoding) and
//! registers it in the [`ComponentRegistry`]; every other feature, the file
//! format and the undo engine then handle it without knowing what it is.
//!
//! A component Sue does not know (a newer version, or a feature that is not
//! installed) loads as an [`OpaqueComponent`] and saves back byte for byte.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;

use crate::EntityId;
use crate::codec::{CodecError, Reader, Writer};

/// An extension component, as the arena stores it.
pub trait Component: Any + Send + Sync + fmt::Debug {
    /// The registered type name, e.g. `sue.table`.
    fn type_name(&self) -> &str;
    /// The encoding version written by [`Self::encode`].
    fn version(&self) -> u32;
    /// Writes the component.
    fn encode(&self, out: &mut Writer);
    /// A boxed copy.
    fn clone_box(&self) -> Box<dyn Component>;
    /// For downcasting.
    fn as_any(&self) -> &dyn Any;
    /// For downcasting, mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Approximate heap footprint, in bytes.
    fn heap_bytes(&self) -> usize;
    /// Points every entity link through `map` (used when entities are copied
    /// or merged into another document, which gives them new ids).
    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId);
}

impl Clone for Box<dyn Component> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Two components are equal when they have the same type and encoding.
#[must_use]
pub fn components_equal(a: &dyn Component, b: &dyn Component) -> bool {
    if a.type_name() != b.type_name() || a.version() != b.version() {
        return false;
    }
    let (mut left, mut right) = (Writer::new(), Writer::new());
    a.encode(&mut left);
    b.encode(&mut right);
    left.as_bytes() == right.as_bytes()
}

/// A statically known component type. Implement this; the arena-facing
/// [`Component`] comes for free.
pub trait ComponentType: Clone + Send + Sync + fmt::Debug + 'static {
    /// Unique, namespaced, never changed once shipped, e.g. `sue.table`.
    const NAME: &'static str;
    /// The current encoding version. Bump it when the encoding changes and
    /// teach [`Self::decode`] to read every older version.
    const VERSION: u32;
    /// Writes the current version.
    fn encode(&self, out: &mut Writer);
    /// Reads any version up to [`Self::VERSION`].
    ///
    /// # Errors
    /// If the bytes are malformed.
    fn decode(version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError>;
    /// Approximate heap footprint, in bytes.
    fn heap_bytes(&self) -> usize {
        0
    }
    /// Points every entity link through `map`.
    fn remap_entities(&mut self, _map: &mut dyn FnMut(EntityId) -> EntityId) {}
}

impl<T: ComponentType> Component for T {
    fn type_name(&self) -> &str {
        T::NAME
    }

    fn version(&self) -> u32 {
        T::VERSION
    }

    fn encode(&self, out: &mut Writer) {
        ComponentType::encode(self, out);
    }

    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn heap_bytes(&self) -> usize {
        ComponentType::heap_bytes(self)
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        ComponentType::remap_entities(self, map);
    }
}

/// A component whose type is not registered here, or whose version is newer
/// than this build understands. It is kept as bytes and saved unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueComponent {
    /// The type name.
    pub type_name: String,
    /// The encoding version.
    pub version: u32,
    /// The encoding.
    pub bytes: Vec<u8>,
}

impl Component for OpaqueComponent {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn encode(&self, out: &mut Writer) {
        out.raw(&self.bytes);
    }

    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn heap_bytes(&self) -> usize {
        self.bytes.len()
    }

    fn remap_entities(&mut self, _map: &mut dyn FnMut(EntityId) -> EntityId) {}
}

type Decoder = fn(u32, &mut Reader<'_>) -> Result<Box<dyn Component>, CodecError>;

fn decode_boxed<T: ComponentType>(
    version: u32,
    input: &mut Reader<'_>,
) -> Result<Box<dyn Component>, CodecError> {
    Ok(Box::new(T::decode(version, input)?))
}

struct Registration {
    version: u32,
    decode: Decoder,
}

/// Every extension component type this build understands.
#[derive(Default)]
pub struct ComponentRegistry {
    types: BTreeMap<&'static str, Registration>,
}

impl fmt::Debug for ComponentRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.types.keys()).finish()
    }
}

impl ComponentRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `T`. Registering the same name twice keeps the last.
    pub fn register<T: ComponentType>(&mut self) -> &mut Self {
        self.types.insert(
            T::NAME,
            Registration {
                version: T::VERSION,
                decode: decode_boxed::<T>,
            },
        );
        self
    }

    /// `true` if `name` is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    /// Every registered name.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.types.keys().copied()
    }

    /// Decodes a component. Unknown types, and versions newer than the
    /// registered one, come back as an [`OpaqueComponent`].
    ///
    /// # Errors
    /// If a known type fails to decode its bytes.
    pub fn decode(
        &self,
        name: &str,
        version: u32,
        bytes: &[u8],
    ) -> Result<Box<dyn Component>, CodecError> {
        match self.types.get(name) {
            Some(registration) if version <= registration.version => {
                let mut input = Reader::new(bytes);
                let component = (registration.decode)(version, &mut input)?;
                if !input.is_empty() {
                    return Err(
                        input.error(format!("{name} left {} bytes unread", input.remaining()))
                    );
                }
                Ok(component)
            }
            _ => Ok(Box::new(OpaqueComponent {
                type_name: name.to_owned(),
                version,
                bytes: bytes.to_vec(),
            })),
        }
    }

    /// Turns an opaque component into its real type once that type is
    /// registered (a feature installed after the file was opened).
    ///
    /// # Errors
    /// If the bytes do not decode.
    pub fn upgrade(&self, component: Box<dyn Component>) -> Result<Box<dyn Component>, CodecError> {
        match component.as_any().downcast_ref::<OpaqueComponent>() {
            Some(opaque) if self.contains(&opaque.type_name) => {
                self.decode(&opaque.type_name, opaque.version, &opaque.bytes)
            }
            _ => Ok(component),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Tempo {
        bpm: f64,
        beats: u32,
    }

    impl ComponentType for Tempo {
        const NAME: &'static str = "test.tempo";
        const VERSION: u32 = 2;

        fn encode(&self, out: &mut Writer) {
            out.f64(self.bpm);
            out.u32(self.beats);
        }

        fn decode(version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
            let bpm = input.f64()?;
            // Version 1 had no time signature.
            let beats = if version >= 2 { input.u32()? } else { 4 };
            Ok(Self { bpm, beats })
        }
    }

    fn encoded(component: &dyn Component) -> Vec<u8> {
        let mut out = Writer::new();
        component.encode(&mut out);
        out.into_bytes()
    }

    #[test]
    fn known_types_decode_old_versions_and_unknown_types_survive() {
        let mut registry = ComponentRegistry::new();
        registry.register::<Tempo>();
        let tempo = Tempo {
            bpm: 120.0,
            beats: 3,
        };
        let bytes = encoded(&tempo);
        let back = registry.decode("test.tempo", 2, &bytes).unwrap();
        assert_eq!(back.as_any().downcast_ref::<Tempo>(), Some(&tempo));

        let old = 90.0_f64.to_le_bytes();
        let back = registry.decode("test.tempo", 1, &old).unwrap();
        assert_eq!(back.as_any().downcast_ref::<Tempo>().unwrap().beats, 4);

        let future = registry.decode("test.tempo", 3, &[1, 2]).unwrap();
        assert!(future.as_any().is::<OpaqueComponent>());
        assert_eq!(encoded(future.as_ref()), [1, 2]);

        let unknown = registry.decode("other.thing", 1, &[7]).unwrap();
        assert_eq!(unknown.type_name(), "other.thing");
        assert!(components_equal(unknown.as_ref(), unknown.clone().as_ref()));
        assert!(registry.decode("test.tempo", 2, &[0; 3]).is_err());
    }

    #[test]
    fn opaque_components_upgrade_when_their_type_arrives() {
        let tempo = Tempo {
            bpm: 60.0,
            beats: 4,
        };
        let opaque: Box<dyn Component> = Box::new(OpaqueComponent {
            type_name: "test.tempo".into(),
            version: 2,
            bytes: encoded(&tempo),
        });
        let mut registry = ComponentRegistry::new();
        let still = registry.upgrade(opaque.clone()).unwrap();
        assert!(still.as_any().is::<OpaqueComponent>());
        registry.register::<Tempo>();
        let upgraded = registry.upgrade(opaque).unwrap();
        assert_eq!(upgraded.as_any().downcast_ref::<Tempo>(), Some(&tempo));
    }
}
