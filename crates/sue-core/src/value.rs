//! Dynamic values and properties. Every feature describes its settings,
//! cell contents, node parameters and preset fields with one [`Value`] type,
//! so the Inspector, presets, scripting, spreadsheets and file formats all
//! read and write the same thing. A [`PropertySchema`] describes the
//! properties a feature understands (type, default, range), so a generic UI
//! can edit them without knowing the feature.

use std::collections::BTreeMap;
use std::fmt;

use crate::codec::{CodecError, Reader, Writer};
use crate::{Color, ColorSpace, Dimension, EntityId, Quantity, Time, Unit};

/// A dynamically typed value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// True or false.
    Bool(bool),
    /// A 64-bit integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// Text.
    Text(String),
    /// A 2D vector or point.
    Vec2([f64; 2]),
    /// A 3D vector or point.
    Vec3([f64; 3]),
    /// A 4D vector or quaternion.
    Vec4([f64; 4]),
    /// A managed color.
    Color(Color),
    /// A number with a unit.
    Quantity(Quantity),
    /// A time or duration.
    Time(Time),
    /// A link to another entity, stable across files.
    Entity(EntityId),
    /// An ordered list.
    List(Vec<Value>),
    /// A string-keyed map.
    Map(BTreeMap<String, Value>),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// A value written by a newer Sue, kept byte for byte so saving does not
    /// lose it.
    Opaque {
        /// Its type tag.
        tag: u8,
        /// Its encoding.
        bytes: Vec<u8>,
    },
}

/// The type of a [`Value`], for schemas.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ValueType {
    /// Anything.
    Any,
    /// [`Value::Bool`].
    Bool,
    /// [`Value::Int`].
    Int,
    /// [`Value::Float`].
    Float,
    /// [`Value::Text`].
    Text,
    /// [`Value::Vec2`].
    Vec2,
    /// [`Value::Vec3`].
    Vec3,
    /// [`Value::Vec4`].
    Vec4,
    /// [`Value::Color`].
    Color,
    /// [`Value::Quantity`] of a dimension.
    Quantity(Dimension),
    /// [`Value::Time`].
    Time,
    /// [`Value::Entity`].
    Entity,
    /// [`Value::List`].
    List,
    /// [`Value::Map`].
    Map,
    /// [`Value::Bytes`].
    Bytes,
    /// [`Value::Text`] limited to these choices.
    Choice(Vec<String>),
}

const TAG_BOOL: u8 = 1;
const TAG_INT: u8 = 2;
const TAG_FLOAT: u8 = 3;
const TAG_TEXT: u8 = 4;
const TAG_VEC2: u8 = 5;
const TAG_VEC3: u8 = 6;
const TAG_VEC4: u8 = 7;
const TAG_COLOR: u8 = 8;
const TAG_QUANTITY: u8 = 9;
const TAG_TIME: u8 = 10;
const TAG_ENTITY: u8 = 11;
const TAG_LIST: u8 = 12;
const TAG_MAP: u8 = 13;
const TAG_BYTES: u8 = 14;

impl Value {
    /// The value's type.
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Bool(_) => ValueType::Bool,
            Self::Int(_) => ValueType::Int,
            Self::Float(_) => ValueType::Float,
            Self::Text(_) => ValueType::Text,
            Self::Vec2(_) => ValueType::Vec2,
            Self::Vec3(_) => ValueType::Vec3,
            Self::Vec4(_) => ValueType::Vec4,
            Self::Color(_) => ValueType::Color,
            Self::Quantity(quantity) => ValueType::Quantity(quantity.unit.dimension),
            Self::Time(_) => ValueType::Time,
            Self::Entity(_) => ValueType::Entity,
            Self::List(_) => ValueType::List,
            Self::Map(_) => ValueType::Map,
            Self::Bytes(_) => ValueType::Bytes,
            Self::Opaque { .. } => ValueType::Any,
        }
    }

    /// The boolean, if this is one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// A number: ints, floats and quantities (in their own unit) convert.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            Self::Quantity(quantity) => Some(quantity.value),
            Self::Bool(value) => Some(f64::from(u8::from(*value))),
            _ => None,
        }
    }

    /// An integer: floats round toward zero.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Float(value) if value.is_finite() => Some(*value as i64),
            Self::Bool(value) => Some(i64::from(*value)),
            _ => None,
        }
    }

    /// The text, if this is text.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The linked entity, if this is a link.
    #[must_use]
    pub fn as_entity(&self) -> Option<EntityId> {
        match self {
            Self::Entity(id) => Some(*id),
            _ => None,
        }
    }

    /// `true` if this value fits `value_type`.
    #[must_use]
    pub fn fits(&self, value_type: &ValueType) -> bool {
        match (value_type, self) {
            (ValueType::Any, _) => true,
            (ValueType::Float, Self::Int(_)) => true,
            (ValueType::Choice(choices), Self::Text(text)) => {
                choices.iter().any(|choice| choice == text)
            }
            (ValueType::Quantity(dimension), Self::Quantity(quantity)) => {
                quantity.unit.dimension == *dimension
            }
            (expected, value) => *expected == value.value_type(),
        }
    }

    /// Calls `visit` with every entity this value links to, mutably, so a
    /// copy can point its links at the copied entities.
    pub fn remap_entities(&mut self, visit: &mut dyn FnMut(EntityId) -> EntityId) {
        match self {
            Self::Entity(id) => *id = visit(*id),
            Self::List(items) => items.iter_mut().for_each(|item| item.remap_entities(visit)),
            Self::Map(map) => map.values_mut().for_each(|item| item.remap_entities(visit)),
            _ => {}
        }
    }

    /// Writes the value: a tag, then a length-prefixed payload, so readers
    /// that do not know a tag can skip and keep it.
    pub fn encode(&self, out: &mut Writer) {
        if let Self::Opaque { tag, bytes } = self {
            out.u8(*tag);
            out.bytes(bytes);
            return;
        }
        let tag = match self {
            Self::Bool(_) => TAG_BOOL,
            Self::Int(_) => TAG_INT,
            Self::Float(_) => TAG_FLOAT,
            Self::Text(_) => TAG_TEXT,
            Self::Vec2(_) => TAG_VEC2,
            Self::Vec3(_) => TAG_VEC3,
            Self::Vec4(_) => TAG_VEC4,
            Self::Color(_) => TAG_COLOR,
            Self::Quantity(_) => TAG_QUANTITY,
            Self::Time(_) => TAG_TIME,
            Self::Entity(_) => TAG_ENTITY,
            Self::List(_) => TAG_LIST,
            Self::Map(_) => TAG_MAP,
            Self::Bytes(_) => TAG_BYTES,
            Self::Opaque { .. } => unreachable!("handled above"),
        };
        out.u8(tag);
        out.nested(|out| match self {
            Self::Bool(value) => out.bool(*value),
            Self::Int(value) => out.varint_signed(*value),
            Self::Float(value) => out.f64(*value),
            Self::Text(text) => out.str(text),
            Self::Vec2(v) => v.iter().for_each(|x| out.f64(*x)),
            Self::Vec3(v) => v.iter().for_each(|x| out.f64(*x)),
            Self::Vec4(v) => v.iter().for_each(|x| out.f64(*x)),
            Self::Color(color) => {
                out.str(color.space.name());
                color.rgb.iter().for_each(|x| out.f32(*x));
                out.f32(color.alpha);
            }
            Self::Quantity(quantity) => {
                out.f64(quantity.value);
                out.str(quantity.unit.symbol);
            }
            Self::Time(time) => out.varint_signed(time.0),
            Self::Entity(id) => out.raw(&id.to_bytes()),
            Self::List(items) => {
                out.len(items.len());
                items.iter().for_each(|item| item.encode(out));
            }
            Self::Map(map) => {
                out.len(map.len());
                for (key, item) in map {
                    out.str(key);
                    item.encode(out);
                }
            }
            Self::Bytes(bytes) => out.bytes(bytes),
            Self::Opaque { .. } => {}
        });
    }

    /// Reads a value written by [`Self::encode`].
    ///
    /// # Errors
    /// If the input is truncated or malformed.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let tag = input.u8()?;
        let payload = input.bytes()?;
        let opaque = || Self::Opaque {
            tag,
            bytes: payload.to_vec(),
        };
        let mut body = Reader::new(payload);
        let body = &mut body;
        let value = match tag {
            TAG_BOOL => Self::Bool(body.bool()?),
            TAG_INT => Self::Int(body.varint_signed()?),
            TAG_FLOAT => Self::Float(body.f64()?),
            TAG_TEXT => Self::Text(body.str()?),
            TAG_VEC2 => Self::Vec2([body.f64()?, body.f64()?]),
            TAG_VEC3 => Self::Vec3([body.f64()?, body.f64()?, body.f64()?]),
            TAG_VEC4 => Self::Vec4([body.f64()?, body.f64()?, body.f64()?, body.f64()?]),
            TAG_COLOR => {
                let Some(space) = ColorSpace::from_name(&body.str()?) else {
                    return Ok(opaque());
                };
                Self::Color(Color {
                    rgb: [body.f32()?, body.f32()?, body.f32()?],
                    alpha: body.f32()?,
                    space,
                })
            }
            TAG_QUANTITY => {
                let value = body.f64()?;
                let Some(unit) = Unit::find(&body.str()?) else {
                    return Ok(opaque());
                };
                Self::Quantity(Quantity { value, unit })
            }
            TAG_TIME => Self::Time(Time(body.varint_signed()?)),
            TAG_ENTITY => {
                let mut bytes = [0; 16];
                bytes.copy_from_slice(body.raw(16)?);
                Self::Entity(EntityId::from_bytes(bytes))
            }
            TAG_LIST => {
                let count = body.len(2)?;
                Self::List(
                    (0..count)
                        .map(|_| Self::decode(body))
                        .collect::<Result<_, _>>()?,
                )
            }
            TAG_MAP => {
                let count = body.len(3)?;
                let mut map = BTreeMap::new();
                for _ in 0..count {
                    let key = body.str()?;
                    map.insert(key, Self::decode(body)?);
                }
                Self::Map(map)
            }
            TAG_BYTES => Self::Bytes(body.bytes()?.to_vec()),
            _ => return Ok(opaque()),
        };
        Ok(value)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Text(text) => f.write_str(text),
            Self::Vec2([x, y]) => write!(f, "({x}, {y})"),
            Self::Vec3([x, y, z]) => write!(f, "({x}, {y}, {z})"),
            Self::Vec4([x, y, z, w]) => write!(f, "({x}, {y}, {z}, {w})"),
            Self::Color(color) => {
                let [r, g, b] = color.rgb;
                write!(f, "{}({r}, {g}, {b}, {})", color.space.name(), color.alpha)
            }
            Self::Quantity(quantity) => write!(f, "{quantity}"),
            Self::Time(time) => write!(f, "{}s", time.seconds()),
            Self::Entity(id) => write!(f, "{id}"),
            Self::List(items) => {
                f.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Map(map) => {
                f.write_str("{")?;
                for (index, (key, item)) in map.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {item}")?;
                }
                f.write_str("}")
            }
            Self::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            Self::Opaque { tag, bytes } => write!(f, "<value type {tag}, {} bytes>", bytes.len()),
        }
    }
}

macro_rules! value_from {
    ($($source:ty => $variant:ident),* $(,)?) => {
        $(
            impl From<$source> for Value {
                fn from(value: $source) -> Self {
                    Self::$variant(value.into())
                }
            }
        )*
    };
}

value_from!(
    bool => Bool,
    i64 => Int,
    i32 => Int,
    u32 => Int,
    f64 => Float,
    f32 => Float,
    String => Text,
    &str => Text,
    [f64; 2] => Vec2,
    [f64; 3] => Vec3,
    [f64; 4] => Vec4,
    Color => Color,
    Quantity => Quantity,
    Time => Time,
    EntityId => Entity,
    Vec<Value> => List,
);

/// An entity's (or a node's, or a preset's) named values. Keys are
/// namespaced by the feature that owns them, e.g. `text.font_size` or
/// `noelle.gravity`, so features never collide.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Properties {
    values: BTreeMap<String, Value>,
}

impl Properties {
    /// No properties.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// A number, if `key` holds one.
    #[must_use]
    pub fn f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.as_f64()
    }

    /// Text, if `key` holds text.
    #[must_use]
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// Sets `key`, returning the previous value.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        self.values.insert(key.into(), value.into())
    }

    /// Builder form of [`Self::set`].
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.set(key, value);
        self
    }

    /// Removes `key`, returning its value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.values.remove(key)
    }

    /// Every key and value, ordered by key.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// Every value mutably.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.values.values_mut()
    }

    /// Number of properties.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` if there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Writes every property.
    pub fn encode(&self, out: &mut Writer) {
        out.len(self.values.len());
        for (key, value) in &self.values {
            out.str(key);
            value.encode(out);
        }
    }

    /// Reads properties written by [`Self::encode`].
    ///
    /// # Errors
    /// If the input is malformed.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = input.len(3)?;
        let mut values = BTreeMap::new();
        for _ in 0..count {
            let key = input.str()?;
            values.insert(key, Value::decode(input)?);
        }
        Ok(Self { values })
    }
}

impl FromIterator<(String, Value)> for Properties {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Self {
            values: iter.into_iter().collect(),
        }
    }
}

/// Describes one property a feature understands.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyDef {
    /// Namespaced key, e.g. `audio.gain`.
    pub key: String,
    /// Label for the UI.
    pub label: String,
    /// What it holds.
    pub value_type: ValueType,
    /// The value when unset.
    pub default: Value,
    /// Inclusive numeric range, if any.
    pub range: Option<(f64, f64)>,
    /// One-line help.
    pub help: String,
    /// Group in the Inspector, e.g. `Layout`.
    pub group: String,
}

impl PropertyDef {
    /// A property with a default and no range.
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        default: impl Into<Value>,
    ) -> Self {
        let default = default.into();
        Self {
            key: key.into(),
            label: label.into(),
            value_type: default.value_type(),
            default,
            range: None,
            help: String::new(),
            group: String::new(),
        }
    }

    /// With a numeric range.
    #[must_use]
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.range = Some((min, max));
        self
    }

    /// With an explicit type (e.g. a [`ValueType::Choice`]).
    #[must_use]
    pub fn typed(mut self, value_type: ValueType) -> Self {
        self.value_type = value_type;
        self
    }

    /// With help text.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// In an Inspector group.
    #[must_use]
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    /// Checks `value` against the type and range.
    ///
    /// # Errors
    /// A message for the UI.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        if !value.fits(&self.value_type) {
            return Err(format!(
                "{} expects {:?}, got {value}",
                self.label, self.value_type
            ));
        }
        if let (Some((min, max)), Some(number)) = (self.range, value.as_f64())
            && !(min..=max).contains(&number)
        {
            return Err(format!("{} must be between {min} and {max}", self.label));
        }
        Ok(())
    }
}

/// Every property definition features registered, by key.
#[derive(Clone, Debug, Default)]
pub struct PropertySchema {
    defs: BTreeMap<String, PropertyDef>,
}

impl PropertySchema {
    /// An empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `def`, replacing one with the same key.
    pub fn register(&mut self, def: PropertyDef) {
        self.defs.insert(def.key.clone(), def);
    }

    /// The definition of `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&PropertyDef> {
        self.defs.get(key)
    }

    /// Definitions whose key starts with `prefix` (a feature's namespace).
    pub fn with_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = &'a PropertyDef> + 'a {
        self.defs
            .values()
            .filter(move |def| def.key.starts_with(prefix))
    }

    /// Every definition.
    pub fn iter(&self) -> impl Iterator<Item = &PropertyDef> {
        self.defs.values()
    }

    /// The value of `key` in `properties`, or its registered default.
    #[must_use]
    pub fn resolve<'a>(&'a self, properties: &'a Properties, key: &str) -> Option<&'a Value> {
        properties
            .get(key)
            .or_else(|| self.get(key).map(|def| &def.default))
    }

    /// Checks `value` for `key`. Unknown keys are allowed (features may
    /// store private properties).
    ///
    /// # Errors
    /// If a registered definition rejects the value.
    pub fn validate(&self, key: &str, value: &Value) -> Result<(), String> {
        self.get(key).map_or(Ok(()), |def| def.validate(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(value: &Value) -> Value {
        let mut out = Writer::new();
        value.encode(&mut out);
        let bytes = out.into_bytes();
        let mut input = Reader::new(&bytes);
        let back = Value::decode(&mut input).unwrap();
        assert!(input.is_empty());
        back
    }

    #[test]
    fn every_value_round_trips() {
        let id = EntityId::new();
        let values = vec![
            Value::Bool(true),
            Value::Int(-42),
            Value::Float(1.25),
            Value::Text("héllo".into()),
            Value::Vec2([1.0, 2.0]),
            Value::Vec3([1.0, 2.0, 3.0]),
            Value::Vec4([1.0, 2.0, 3.0, 4.0]),
            Value::Color(Color::srgb8(10, 20, 30)),
            Value::Quantity(Quantity::new(12.5, "mm")),
            Value::Time(Time::SECOND),
            Value::Entity(id),
            Value::Bytes(vec![1, 2, 3]),
        ];
        let mut map = BTreeMap::new();
        map.insert("nested".to_owned(), Value::List(values.clone()));
        for value in values.into_iter().chain([Value::Map(map)]) {
            assert_eq!(round_trip(&value), value);
        }
    }

    #[test]
    fn unknown_tags_are_kept_byte_for_byte() {
        let mut out = Writer::new();
        out.u8(200);
        out.bytes(&[9, 9, 9]);
        let bytes = out.into_bytes();
        let value = Value::decode(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(
            value,
            Value::Opaque {
                tag: 200,
                bytes: vec![9, 9, 9]
            }
        );
        let mut again = Writer::new();
        value.encode(&mut again);
        assert_eq!(again.into_bytes(), bytes);
    }

    #[test]
    fn schemas_validate_and_supply_defaults() {
        let mut schema = PropertySchema::new();
        schema.register(PropertyDef::new("audio.gain", "Gain", 0.0).range(-60.0, 12.0));
        schema.register(
            PropertyDef::new("duct.size", "Size", Quantity::new(200.0, "mm"))
                .typed(ValueType::Quantity(Dimension::Length)),
        );
        schema.register(
            PropertyDef::new("blend", "Blend", "normal")
                .typed(ValueType::Choice(vec!["normal".into(), "multiply".into()])),
        );
        assert!(schema.validate("audio.gain", &Value::Float(-6.0)).is_ok());
        assert!(schema.validate("audio.gain", &Value::Int(3)).is_ok());
        assert!(schema.validate("audio.gain", &Value::Float(20.0)).is_err());
        assert!(
            schema
                .validate("audio.gain", &Value::Text("x".into()))
                .is_err()
        );
        assert!(
            schema
                .validate("duct.size", &Quantity::new(8.0, "in").into())
                .is_ok()
        );
        assert!(
            schema
                .validate("duct.size", &Quantity::new(8.0, "kPa").into())
                .is_err()
        );
        assert!(schema.validate("blend", &"multiply".into()).is_ok());
        assert!(schema.validate("blend", &"screen".into()).is_err());
        assert!(schema.validate("private.key", &Value::Bool(true)).is_ok());
        let properties = Properties::new().with("audio.gain", -3.0);
        assert_eq!(
            schema.resolve(&properties, "audio.gain"),
            Some(&Value::Float(-3.0))
        );
        assert_eq!(
            schema
                .resolve(&Properties::new(), "blend")
                .and_then(Value::as_str),
            Some("normal")
        );
        assert_eq!(schema.with_prefix("audio.").count(), 1);
    }

    #[test]
    fn links_can_be_remapped() {
        let (old, new) = (EntityId::new(), EntityId::new());
        let mut value = Value::List(vec![Value::Entity(old), Value::Int(1)]);
        value.remap_entities(&mut |id| if id == old { new } else { id });
        assert_eq!(value, Value::List(vec![Value::Entity(new), Value::Int(1)]));
    }
}
