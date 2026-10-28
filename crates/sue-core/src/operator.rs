//! Operators and presets. Every action a feature offers (normalize audio,
//! resize an image, extrude a face, export a PDF) is an [`Operator`]: an id,
//! a label, a parameter schema and a function that plans an undoable
//! transaction. The command palette, menus, keyboard shortcuts, macros,
//! scripts and batch processing all run operators through one
//! [`OperatorRegistry`], so a feature added later is reachable everywhere at
//! once.
//!
//! A [`Preset`] is a named set of parameter values for one operator (or one
//! file format's export options). Presets live in a [`PresetLibrary`] that
//! saves to bytes, so they can be shared between projects and users.

use std::collections::BTreeMap;
use std::fmt;

use crate::codec::{CodecError, Reader, Writer};
use crate::{
    EntityId, NodeId, Properties, PropertyDef, ReversibleTransaction, TxnError, UndoStack,
};

/// The plan an operator returns: one undoable step.
pub type Plan<T> = Box<dyn ReversibleTransaction<T>>;

/// An action on a target (the arena, the world) and a selection.
pub trait Operator<T>: Send + Sync {
    /// Unique, namespaced id, e.g. `audio.normalize`.
    fn id(&self) -> &str;
    /// Label for menus and the palette.
    fn label(&self) -> &str;
    /// Menu group, e.g. `Audio`.
    fn category(&self) -> &str {
        "General"
    }
    /// The parameters it takes, with defaults and ranges.
    fn params(&self) -> Vec<PropertyDef> {
        Vec::new()
    }
    /// `true` if it can run on `selection` now.
    fn available(&self, _target: &T, _selection: &[EntityId]) -> bool {
        true
    }
    /// Plans the change without making it.
    ///
    /// # Errors
    /// Why the operator cannot run with these inputs.
    fn plan(
        &self,
        target: &T,
        selection: &[EntityId],
        params: &Properties,
    ) -> Result<Plan<T>, TxnError>;
}

type Available<T> = Box<dyn Fn(&T, &[EntityId]) -> bool + Send + Sync>;
type Planner<T> =
    Box<dyn Fn(&T, &[EntityId], &Properties) -> Result<Plan<T>, TxnError> + Send + Sync>;

/// An operator built from closures, for features that do not need a type.
pub struct FnOperator<T> {
    id: String,
    label: String,
    category: String,
    params: Vec<PropertyDef>,
    available: Available<T>,
    plan: Planner<T>,
}

impl<T> FnOperator<T> {
    /// An operator that plans with `plan`.
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        plan: impl Fn(&T, &[EntityId], &Properties) -> Result<Plan<T>, TxnError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            category: "General".to_owned(),
            params: Vec::new(),
            available: Box::new(|_, _| true),
            plan: Box::new(plan),
        }
    }

    /// In a menu group.
    #[must_use]
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// With a parameter.
    #[must_use]
    pub fn param(mut self, def: PropertyDef) -> Self {
        self.params.push(def);
        self
    }

    /// Only available when `available` says so.
    #[must_use]
    pub fn when(
        mut self,
        available: impl Fn(&T, &[EntityId]) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.available = Box::new(available);
        self
    }
}

impl<T> Operator<T> for FnOperator<T> {
    fn id(&self) -> &str {
        &self.id
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn category(&self) -> &str {
        &self.category
    }

    fn params(&self) -> Vec<PropertyDef> {
        self.params.clone()
    }

    fn available(&self, target: &T, selection: &[EntityId]) -> bool {
        (self.available)(target, selection)
    }

    fn plan(
        &self,
        target: &T,
        selection: &[EntityId],
        params: &Properties,
    ) -> Result<Plan<T>, TxnError> {
        (self.plan)(target, selection, params)
    }
}

/// Every operator, by id.
pub struct OperatorRegistry<T> {
    operators: BTreeMap<String, Box<dyn Operator<T>>>,
}

impl<T> Default for OperatorRegistry<T> {
    fn default() -> Self {
        Self {
            operators: BTreeMap::new(),
        }
    }
}

impl<T> fmt::Debug for OperatorRegistry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.operators.keys()).finish()
    }
}

impl<T> OperatorRegistry<T> {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an operator, replacing one with the same id.
    pub fn register(&mut self, operator: impl Operator<T> + 'static) {
        self.operators
            .insert(operator.id().to_owned(), Box::new(operator));
    }

    /// The operator with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&dyn Operator<T>> {
        self.operators.get(id).map(AsRef::as_ref)
    }

    /// Every operator, by id.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Operator<T>> {
        self.operators.values().map(AsRef::as_ref)
    }

    /// Operators that can run on `selection` now.
    pub fn available<'a>(
        &'a self,
        target: &'a T,
        selection: &'a [EntityId],
    ) -> impl Iterator<Item = &'a dyn Operator<T>> + 'a {
        self.iter()
            .filter(move |operator| operator.available(target, selection))
    }

    /// The parameters `id` would run with: its defaults, overridden by
    /// `params`, each value validated.
    ///
    /// # Errors
    /// An unknown operator or a rejected value.
    pub fn resolve_params(&self, id: &str, params: &Properties) -> Result<Properties, TxnError> {
        let operator = self
            .get(id)
            .ok_or_else(|| TxnError::new(format!("no operator {id}")))?;
        let mut resolved = Properties::new();
        for def in operator.params() {
            let value = params.get(&def.key).cloned().unwrap_or(def.default.clone());
            def.validate(&value).map_err(TxnError::new)?;
            resolved.set(def.key, value);
        }
        for (key, value) in params.iter() {
            if resolved.get(key).is_none() {
                resolved.set(key, value.clone());
            }
        }
        Ok(resolved)
    }

    /// Plans and applies `id`, recording it in `history`.
    ///
    /// # Errors
    /// An unknown or unavailable operator, a rejected parameter or the
    /// transaction's own error. Nothing changes on error.
    pub fn run(
        &self,
        id: &str,
        target: &mut T,
        history: &mut UndoStack<T>,
        selection: &[EntityId],
        params: &Properties,
    ) -> Result<NodeId, TxnError> {
        let operator = self
            .get(id)
            .ok_or_else(|| TxnError::new(format!("no operator {id}")))?;
        if !operator.available(target, selection) {
            return Err(TxnError::new(format!(
                "{} is not available here",
                operator.label()
            )));
        }
        let params = self.resolve_params(id, params)?;
        let plan = operator.plan(target, selection, &params)?;
        history.apply_boxed(target, plan)
    }
}

/// Named parameter values for one operator or format.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    /// Name, e.g. `Podcast voice`.
    pub name: String,
    /// The operator or format id it applies to, e.g. `audio.compress`.
    pub target: String,
    /// The values.
    pub values: Properties,
}

impl Preset {
    /// A preset.
    pub fn new(name: impl Into<String>, target: impl Into<String>, values: Properties) -> Self {
        Self {
            name: name.into(),
            target: target.into(),
            values,
        }
    }

    /// Writes the preset.
    pub fn encode(&self, out: &mut Writer) {
        out.str(&self.name);
        out.str(&self.target);
        self.values.encode(out);
    }

    /// Reads a preset.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            name: input.str()?,
            target: input.str()?,
            values: Properties::decode(input)?,
        })
    }
}

/// A collection of presets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresetLibrary {
    presets: Vec<Preset>,
}

impl PresetLibrary {
    /// An empty library.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a preset, replacing one with the same target and name.
    pub fn add(&mut self, preset: Preset) -> Option<Preset> {
        let existing = self
            .presets
            .iter()
            .position(|old| old.target == preset.target && old.name == preset.name);
        match existing {
            Some(index) => Some(std::mem::replace(&mut self.presets[index], preset)),
            None => {
                self.presets.push(preset);
                None
            }
        }
    }

    /// Removes a preset.
    pub fn remove(&mut self, target: &str, name: &str) -> Option<Preset> {
        let index = self
            .presets
            .iter()
            .position(|preset| preset.target == target && preset.name == name)?;
        Some(self.presets.remove(index))
    }

    /// The preset `name` for `target`.
    #[must_use]
    pub fn get(&self, target: &str, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|preset| preset.target == target && preset.name == name)
    }

    /// Every preset for `target`.
    pub fn for_target<'a>(&'a self, target: &'a str) -> impl Iterator<Item = &'a Preset> + 'a {
        self.presets
            .iter()
            .filter(move |preset| preset.target == target)
    }

    /// Every preset.
    pub fn iter(&self) -> impl Iterator<Item = &Preset> {
        self.presets.iter()
    }

    /// Adds every preset of `other` (theirs win on a name clash).
    pub fn merge(&mut self, other: Self) {
        for preset in other.presets {
            self.add(preset);
        }
    }

    /// Writes the library.
    pub fn encode(&self, out: &mut Writer) {
        out.len(self.presets.len());
        for preset in &self.presets {
            preset.encode(out);
        }
    }

    /// Reads a library.
    ///
    /// # Errors
    /// If the bytes are malformed.
    pub fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = input.len(3)?;
        let presets = (0..count)
            .map(|_| Preset::decode(input))
            .collect::<Result<_, _>>()?;
        Ok(Self { presets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::txn::SetProperty;
    use crate::{Arena, Entity, StringBuffer, Value};

    fn registry() -> OperatorRegistry<Arena> {
        let mut registry = OperatorRegistry::new();
        registry.register(
            FnOperator::new(
                "audio.gain",
                "Set gain",
                |_: &Arena, selection: &[EntityId], params: &Properties| {
                    let id = *selection
                        .first()
                        .ok_or_else(|| TxnError::new("select something"))?;
                    let gain = params.f64("audio.gain").unwrap_or_default();
                    Ok(Box::new(SetProperty::new(id, "audio.gain", gain)) as Plan<Arena>)
                },
            )
            .category("Audio")
            .param(PropertyDef::new("audio.gain", "Gain (dB)", -3.0).range(-60.0, 12.0))
            .when(|_, selection| !selection.is_empty()),
        );
        registry
    }

    #[test]
    fn operators_validate_plan_and_undo() {
        let registry = registry();
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("take", StringBuffer::new()));
        let id = arena.get(handle).unwrap().id();
        let mut history = UndoStack::new();
        assert_eq!(registry.available(&arena, &[]).count(), 0);
        assert_eq!(registry.available(&arena, &[id]).count(), 1);
        registry
            .run(
                "audio.gain",
                &mut arena,
                &mut history,
                &[id],
                &Properties::new(),
            )
            .unwrap();
        assert_eq!(
            arena.get(handle).unwrap().property("audio.gain"),
            Some(&Value::Float(-3.0))
        );
        let loud = Properties::new().with("audio.gain", 50.0);
        assert!(
            registry
                .run("audio.gain", &mut arena, &mut history, &[id], &loud)
                .is_err()
        );
        assert!(
            registry
                .run("nope", &mut arena, &mut history, &[id], &loud)
                .is_err()
        );
        history.undo(&mut arena).unwrap();
        assert!(arena.get(handle).unwrap().property("audio.gain").is_none());
    }

    #[test]
    fn presets_replace_by_name_and_round_trip() {
        let mut library = PresetLibrary::new();
        library.add(Preset::new(
            "Quiet",
            "audio.gain",
            Properties::new().with("audio.gain", -12.0),
        ));
        library.add(Preset::new(
            "Loud",
            "audio.gain",
            Properties::new().with("audio.gain", 6.0),
        ));
        let old = library.add(Preset::new(
            "Quiet",
            "audio.gain",
            Properties::new().with("audio.gain", -18.0),
        ));
        assert!(old.is_some());
        library.add(Preset::new(
            "Web",
            "export.jpeg",
            Properties::new().with("quality", 80),
        ));
        assert_eq!(library.for_target("audio.gain").count(), 2);
        assert_eq!(
            library
                .get("audio.gain", "Quiet")
                .unwrap()
                .values
                .f64("audio.gain"),
            Some(-18.0)
        );
        let mut out = Writer::new();
        library.encode(&mut out);
        let bytes = out.into_bytes();
        let back = PresetLibrary::decode(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, library);
        let resolved = registry()
            .resolve_params(
                "audio.gain",
                &library.get("audio.gain", "Loud").unwrap().values,
            )
            .unwrap();
        assert_eq!(resolved.f64("audio.gain"), Some(6.0));
        assert!(library.remove("audio.gain", "Loud").is_some());
    }
}
