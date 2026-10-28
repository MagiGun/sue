//! The parsed rice: typed entries, two-way binding and format-preserving
//! write-back.

use std::collections::HashMap;
use std::ops::Range;

use sue_core::{ReversibleTransaction, TxnError};

use crate::parser::{self, Line};
use crate::{Keybind, Rgba, RiceDeclaration, RiceDiagnostic, RiceError, RiceParameter, RiceValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineState {
    /// Written back byte for byte.
    Clean,
    /// Only the value span is replaced.
    ValueChanged,
    /// The whole line is regenerated (its type was replaced).
    Rewrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    File {
        line: usize,
        value_span: Range<usize>,
        state: LineState,
    },
    Registered,
}

/// One parameter of a [`RiceSchema`].
#[derive(Clone, Debug, PartialEq)]
pub struct RiceEntry {
    path: String,
    section: String,
    key: String,
    parameter: RiceParameter,
    description: Option<String>,
    origin: Origin,
}

impl RiceEntry {
    /// Dotted path, e.g. `layout.gap`. This is the widget binding.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Table the entry belongs to (`""` for top-level keys).
    #[must_use]
    pub fn section(&self) -> &str {
        &self.section
    }

    /// Key within its table.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The typed parameter.
    #[must_use]
    pub fn parameter(&self) -> &RiceParameter {
        &self.parameter
    }

    /// Tooltip text.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Human label derived from the key: `border_width` → "Border width".
    #[must_use]
    pub fn label(&self) -> String {
        humanize(self.key.rsplit('.').next().unwrap_or(&self.key))
    }

    /// 1-based source line, if the entry came from a file.
    #[must_use]
    pub fn line(&self) -> Option<usize> {
        match self.origin {
            Origin::File { line, .. } => Some(line + 1),
            Origin::Registered => None,
        }
    }

    fn mark_changed(&mut self) {
        if let Origin::File { state, .. } = &mut self.origin
            && *state == LineState::Clean
        {
            *state = LineState::ValueChanged;
        }
    }

    /// The entry as a fresh `key = value  # metadata description` line.
    fn render(&self) -> String {
        let metadata = self.parameter.metadata();
        let comment: Vec<&str> = [metadata.as_str(), self.description.as_deref().unwrap_or("")]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        let line = format!("{} = {}", self.key, self.parameter.literal());
        if comment.is_empty() {
            line
        } else {
            format!("{line}  # {}", comment.join(" "))
        }
    }

    /// The entry written over its original source line.
    fn render_over(&self, raw: &str) -> String {
        match &self.origin {
            Origin::File {
                state: LineState::Clean,
                ..
            } => raw.to_owned(),
            Origin::File {
                state: LineState::ValueChanged,
                value_span,
                ..
            } => format!(
                "{}{}{}",
                &raw[..value_span.start],
                self.parameter.literal(),
                &raw[value_span.end..]
            ),
            Origin::File {
                state: LineState::Rewrite,
                ..
            }
            | Origin::Registered => self.render(),
        }
    }
}

/// Turns an identifier into a label: `left_dock-width` → "Left dock width".
pub(crate) fn humanize(id: &str) -> String {
    let spaced = id.replace(['_', '-', '.'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn join_path(section: &str, key: &str) -> String {
    if section.is_empty() {
        key.to_owned()
    } else {
        format!("{section}.{key}")
    }
}

/// What [`RiceSchema::register`] did with a declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// The key was missing and the default was added.
    Inserted,
    /// The user's value was kept (and re-clamped to the declared range).
    KeptUserValue,
    /// The user's value had the wrong type and was replaced by the default.
    ReplacedIncompatible,
}

/// A parsed `.suerice` document, bound two ways: to the Settings GUI through
/// [`SettingsTree`](crate::SettingsTree) and back to text through
/// [`to_suerice`](Self::to_suerice).
#[derive(Clone, Debug, Default)]
pub struct RiceSchema {
    lines: Vec<String>,
    entries: Vec<RiceEntry>,
    index: HashMap<String, usize>,
    sections: Vec<String>,
    anchors: HashMap<String, usize>,
    diagnostics: Vec<RiceDiagnostic>,
    revision: u64,
}

impl RiceSchema {
    /// An empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a `.suerice` document.
    ///
    /// # Errors
    /// Syntax errors, unsupported TOML, metadata that does not fit its value,
    /// and duplicate keys or tables, each with its line number. Out-of-range
    /// values are clamped and reported in [`diagnostics`](Self::diagnostics).
    pub fn parse(source: &str) -> Result<Self, RiceError> {
        let mut schema = Self::default();
        let mut section = String::new();
        for (index, raw) in source.lines().enumerate() {
            let line = index + 1;
            match parser::parse_line(raw, line)? {
                Line::Blank => {}
                Line::Section(name) => {
                    if schema.anchors.contains_key(&name) {
                        return Err(RiceError::Duplicate { line, path: name });
                    }
                    schema.note_section(&name);
                    schema.anchors.insert(name.clone(), index);
                    section = name;
                }
                Line::Entry(parsed) => {
                    let path = join_path(&section, &parsed.key);
                    if schema.index.contains_key(&path) {
                        return Err(RiceError::Duplicate { line, path });
                    }
                    let (parameter, note) = RiceParameter::infer(parsed.literal, &parsed.metadata)
                        .map_err(|message| RiceError::Metadata { line, message })?;
                    if let Some(message) = note {
                        schema.diagnostics.push(RiceDiagnostic {
                            line: Some(line),
                            path: path.clone(),
                            message,
                        });
                    }
                    schema.note_section(&section);
                    schema.anchors.insert(section.clone(), index);
                    schema.push_entry(RiceEntry {
                        path,
                        section: section.clone(),
                        key: parsed.key,
                        parameter,
                        description: parsed.metadata.description,
                        origin: Origin::File {
                            line: index,
                            value_span: parsed.value_span,
                            state: LineState::Clean,
                        },
                    });
                }
            }
            schema.lines.push(raw.to_owned());
        }
        Ok(schema)
    }

    fn note_section(&mut self, section: &str) {
        if !self.sections.iter().any(|known| known == section) {
            self.sections.push(section.to_owned());
        }
    }

    fn push_entry(&mut self, entry: RiceEntry) {
        self.index.insert(entry.path.clone(), self.entries.len());
        self.entries.push(entry);
    }

    /// Every entry, in file order followed by registration order.
    #[must_use]
    pub fn entries(&self) -> &[RiceEntry] {
        &self.entries
    }

    /// Number of parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` if there are no parameters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Table names in order of first appearance.
    #[must_use]
    pub fn sections(&self) -> &[String] {
        &self.sections
    }

    /// Recoverable problems found while loading or registering.
    #[must_use]
    pub fn diagnostics(&self) -> &[RiceDiagnostic] {
        &self.diagnostics
    }

    /// Increases on every change, so views can detect staleness.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The entry at `path`.
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<&RiceEntry> {
        self.index.get(path).map(|&index| &self.entries[index])
    }

    /// The parameter at `path`.
    #[must_use]
    pub fn parameter(&self, path: &str) -> Option<&RiceParameter> {
        self.entry(path).map(RiceEntry::parameter)
    }

    /// The value at `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<RiceValue> {
        self.parameter(path).map(RiceParameter::value)
    }

    /// A toggle's state.
    #[must_use]
    pub fn get_bool(&self, path: &str) -> Option<bool> {
        match self.parameter(path)? {
            RiceParameter::Bool { value } => Some(*value),
            _ => None,
        }
    }

    /// An integer.
    #[must_use]
    pub fn get_i64(&self, path: &str) -> Option<i64> {
        match self.parameter(path)? {
            RiceParameter::IntRange { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// A number, from either a float or an integer parameter.
    #[must_use]
    pub fn get_f64(&self, path: &str) -> Option<f64> {
        match self.parameter(path)? {
            RiceParameter::FloatRange { value, .. } => Some(*value),
            RiceParameter::IntRange { value, .. } => Some(*value as f64),
            _ => None,
        }
    }

    /// A color.
    #[must_use]
    pub fn get_color(&self, path: &str) -> Option<Rgba> {
        match self.parameter(path)? {
            RiceParameter::ColorHex { value } => Some(*value),
            _ => None,
        }
    }

    /// A key chord.
    #[must_use]
    pub fn get_keybind(&self, path: &str) -> Option<Keybind> {
        match self.parameter(path)? {
            RiceParameter::Keybind { value } => Some(*value),
            _ => None,
        }
    }

    /// Text, a choice, or a shader path.
    #[must_use]
    pub fn get_str(&self, path: &str) -> Option<&str> {
        match self.parameter(path)? {
            RiceParameter::EnumChoice { value, .. }
            | RiceParameter::ShaderPath { value }
            | RiceParameter::Text { value } => Some(value),
            _ => None,
        }
    }

    /// Sets the value at `path` and returns the old one.
    ///
    /// # Errors
    /// [`RiceError::UnknownKey`], or the parameter's type and range errors.
    pub fn set(&mut self, path: &str, value: RiceValue) -> Result<RiceValue, RiceError> {
        let &index = self
            .index
            .get(path)
            .ok_or_else(|| RiceError::UnknownKey(path.to_owned()))?;
        let entry = &mut self.entries[index];
        let old = entry.parameter.set(path, value)?;
        entry.mark_changed();
        self.revision += 1;
        Ok(old)
    }

    /// Registers a declared parameter (LAW 3). A missing key gets the default.
    /// A present key keeps the user's value if its type is compatible
    /// (re-clamped to the declared range). An incompatible value is replaced
    /// and reported.
    ///
    /// # Errors
    /// [`RiceError::InvalidPath`] unless the path is `section.key` made of bare
    /// keys.
    pub fn register(
        &mut self,
        declaration: &RiceDeclaration,
    ) -> Result<RegisterOutcome, RiceError> {
        let invalid = || RiceError::InvalidPath(declaration.path.clone());
        let path = parser::normalize_key(&declaration.path, 0).map_err(|_| invalid())?;
        let (section, key) = path.rsplit_once('.').ok_or_else(invalid)?;

        if let Some(&index) = self.index.get(&path) {
            let entry = &mut self.entries[index];
            if entry.parameter.same_type(&declaration.default) {
                let previous = entry.parameter.value();
                // The declaration's range and choices are authoritative.
                let metadata_changed = entry.parameter.metadata() != declaration.default.metadata();
                let mut adopted = declaration.default.clone();
                let note = adopted.set_clamped(&path, previous.clone())?;
                let value_changed = adopted.value() != previous;
                entry.parameter = adopted;
                if entry.description.is_none() {
                    entry.description = Some(declaration.description.clone());
                }
                if metadata_changed {
                    if let Origin::File { state, .. } = &mut entry.origin {
                        *state = LineState::Rewrite;
                    }
                } else if value_changed {
                    entry.mark_changed();
                }
                if metadata_changed || value_changed {
                    self.revision += 1;
                }
                if let Some(message) = note {
                    let line = entry.line();
                    self.diagnostics.push(RiceDiagnostic {
                        line,
                        path,
                        message,
                    });
                }
                return Ok(RegisterOutcome::KeptUserValue);
            }
            let message = format!(
                "expected {}, found {}; reset to the default",
                declaration.default.type_name(),
                entry.parameter.type_name()
            );
            entry.parameter = declaration.default.clone();
            entry.description = Some(declaration.description.clone());
            if let Origin::File { state, .. } = &mut entry.origin {
                *state = LineState::Rewrite;
            }
            let line = entry.line();
            self.diagnostics.push(RiceDiagnostic {
                line,
                path,
                message,
            });
            self.revision += 1;
            return Ok(RegisterOutcome::ReplacedIncompatible);
        }

        let entry = RiceEntry {
            path: path.clone(),
            section: section.to_owned(),
            key: key.to_owned(),
            parameter: declaration.default.clone(),
            description: Some(declaration.description.clone()),
            origin: Origin::Registered,
        };
        self.note_section(section);
        self.push_entry(entry);
        self.revision += 1;
        Ok(RegisterOutcome::Inserted)
    }

    /// Registers every declaration.
    ///
    /// # Errors
    /// The first [`register`](Self::register) error.
    pub fn register_all(&mut self, declarations: &[RiceDeclaration]) -> Result<(), RiceError> {
        for declaration in declarations {
            self.register(declaration)?;
        }
        Ok(())
    }

    /// Writes the schema back as `.suerice` text. Untouched lines are kept byte
    /// for byte; edited values are spliced into their original span; registered
    /// defaults are added at the end of their table, or in a new table.
    #[must_use]
    pub fn to_suerice(&self) -> String {
        let mut by_line: HashMap<usize, &RiceEntry> = HashMap::new();
        let mut registered: Vec<(&str, Vec<&RiceEntry>)> = Vec::new();
        for entry in &self.entries {
            match entry.origin {
                Origin::File { line, .. } => {
                    by_line.insert(line, entry);
                }
                Origin::Registered => {
                    match registered
                        .iter_mut()
                        .find(|(section, _)| *section == entry.section)
                    {
                        Some((_, group)) => group.push(entry),
                        None => registered.push((entry.section.as_str(), vec![entry])),
                    }
                }
            }
        }
        let anchored: HashMap<usize, &str> = self
            .anchors
            .iter()
            .map(|(section, &line)| (line, section.as_str()))
            .collect();
        let emit = |out: &mut String, group: &[&RiceEntry]| {
            for entry in group {
                out.push_str(&entry.render());
                out.push('\n');
            }
        };

        let mut out = String::new();
        for (index, raw) in self.lines.iter().enumerate() {
            match by_line.get(&index) {
                Some(entry) => out.push_str(&entry.render_over(raw)),
                None => out.push_str(raw),
            }
            out.push('\n');
            if let Some(section) = anchored.get(&index)
                && let Some((_, group)) = registered.iter().find(|(name, _)| name == section)
            {
                emit(&mut out, group);
            }
        }
        for (section, group) in &registered {
            if self.anchors.contains_key(*section) {
                continue;
            }
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push('\n');
            }
            out.push_str(&format!("[{section}]\n"));
            emit(&mut out, group);
        }
        out
    }
}

/// Sets one rice value as an undoable transaction: the bridge from a Settings
/// widget edit back into the schema.
#[derive(Clone, Debug)]
pub struct SetRiceValue {
    path: String,
    value: RiceValue,
    previous: Option<RiceValue>,
}

impl SetRiceValue {
    /// Sets `path` to `value`.
    pub fn new(path: impl Into<String>, value: RiceValue) -> Self {
        Self {
            path: path.into(),
            value,
            previous: None,
        }
    }
}

impl ReversibleTransaction<RiceSchema> for SetRiceValue {
    fn label(&self) -> String {
        format!("Set {} to {}", self.path, self.value)
    }

    fn apply(&mut self, schema: &mut RiceSchema) -> Result<(), TxnError> {
        self.previous = Some(schema.set(&self.path, self.value.clone())?);
        Ok(())
    }

    fn revert(&mut self, schema: &mut RiceSchema) -> Result<(), TxnError> {
        let previous = self
            .previous
            .clone()
            .ok_or_else(|| TxnError::new("SetRiceValue reverted before it was applied"))?;
        schema.set(&self.path, previous)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_RICE;
    use sue_core::UndoStack;

    const SAMPLE: &str = "\
# Sample rice
[layout]
gap = 8            # [0..64] Space between panes
corner_radius = 6.0 # [0.0..24.0]

[colors]
accent = \"#c9a227\" # Focus ring
";

    #[test]
    fn default_rice_parses_cleanly() {
        let schema = RiceSchema::parse(DEFAULT_RICE).unwrap();
        assert!(
            schema.diagnostics().is_empty(),
            "{:?}",
            schema.diagnostics()
        );
        assert!(schema.len() >= 20);
        assert_eq!(schema.sections()[0], "profile");
        assert_eq!(schema.get_i64("layout.gap"), Some(8));
        assert_eq!(schema.get_f64("motion.spring_stiffness"), Some(170.0));
        assert_eq!(schema.get_str("profile.hardware"), Some("auto"));
        assert_eq!(
            schema.get_keybind("keys.redo").unwrap().to_string(),
            "Ctrl+Shift+Z"
        );
        assert!(matches!(
            schema.parameter("shaders.background"),
            Some(RiceParameter::ShaderPath { .. })
        ));
    }

    #[test]
    fn untouched_documents_round_trip_byte_for_byte() {
        for source in [SAMPLE, DEFAULT_RICE] {
            let normalized = source.replace("\r\n", "\n");
            assert_eq!(RiceSchema::parse(source).unwrap().to_suerice(), normalized);
        }
    }

    #[test]
    fn edits_splice_only_the_value() {
        let mut schema = RiceSchema::parse(SAMPLE).unwrap();
        assert_eq!(
            schema.set("layout.gap", RiceValue::Int(12)),
            Ok(RiceValue::Int(8))
        );
        schema
            .set("colors.accent", RiceValue::Color(Rgba::new(1, 2, 3, 255)))
            .unwrap();
        let text = schema.to_suerice();
        let gap_line = SAMPLE.lines().nth(2).unwrap().replacen("= 8", "= 12", 1);
        assert!(text.contains(&format!("{gap_line}\n")), "{text}");
        assert!(text.contains("accent = \"#010203\" # Focus ring\n"));
        assert!(text.contains("corner_radius = 6.0 # [0.0..24.0]\n"));
        assert_eq!(schema.revision(), 2);
    }

    #[test]
    fn invalid_edits_are_rejected() {
        let mut schema = RiceSchema::parse(SAMPLE).unwrap();
        assert!(matches!(
            schema.set("layout.gap", RiceValue::Int(99)),
            Err(RiceError::OutOfRange { .. })
        ));
        assert!(matches!(
            schema.set("layout.nope", RiceValue::Int(1)),
            Err(RiceError::UnknownKey(_))
        ));
        assert!(matches!(
            schema.set("colors.accent", RiceValue::Text("teal".into())),
            Err(RiceError::InvalidColor(_))
        ));
        assert_eq!(schema.revision(), 0);
    }

    #[test]
    fn out_of_range_file_values_are_clamped_with_diagnostics() {
        let schema = RiceSchema::parse("[layout]\ngap = 900 # [0..64]\n").unwrap();
        assert_eq!(schema.get_i64("layout.gap"), Some(64));
        assert_eq!(schema.diagnostics()[0].line, Some(2));
    }

    #[test]
    fn registration_inserts_keeps_and_replaces() {
        let mut schema = RiceSchema::parse("[layout]\ngap = 30 # [0..64]\nmode = 3\n").unwrap();
        let gap = RiceDeclaration::new(
            "layout.gap",
            RiceParameter::IntRange {
                value: 8,
                min: Some(0),
                max: Some(16),
            },
            "Gap",
        );
        assert_eq!(schema.register(&gap), Ok(RegisterOutcome::KeptUserValue));
        assert_eq!(schema.get_i64("layout.gap"), Some(16));
        let mode = RiceDeclaration::new(
            "layout.mode",
            RiceParameter::EnumChoice {
                value: "tiling".into(),
                options: vec!["tiling".into(), "floating".into()],
            },
            "Window mode",
        );
        assert_eq!(
            schema.register(&mode),
            Ok(RegisterOutcome::ReplacedIncompatible)
        );
        let border = RiceDeclaration::new(
            "layout.border",
            RiceParameter::IntRange {
                value: 2,
                min: Some(0),
                max: Some(8),
            },
            "Border",
        );
        assert_eq!(schema.register(&border), Ok(RegisterOutcome::Inserted));
        let fps = RiceDeclaration::new(
            "hud.show_fps",
            RiceParameter::Bool { value: false },
            "FPS counter",
        );
        assert_eq!(schema.register(&fps), Ok(RegisterOutcome::Inserted));
        assert!(
            schema
                .register(&RiceDeclaration::new(
                    "rootkey",
                    RiceParameter::Bool { value: true },
                    ""
                ))
                .is_err()
        );

        let text = schema.to_suerice();
        assert_eq!(
            text,
            "[layout]\ngap = 16  # [0..16] Gap\nmode = \"tiling\"  # {tiling|floating} Window mode\n\
             border = 2  # [0..8] Border\n\n[hud]\nshow_fps = false  # FPS counter\n"
        );
        let reparsed = RiceSchema::parse(&text).unwrap();
        for entry in schema.entries() {
            assert_eq!(
                reparsed.parameter(entry.path()),
                Some(entry.parameter()),
                "{}",
                entry.path()
            );
        }
        assert_eq!(schema.diagnostics().len(), 2);
    }

    #[test]
    fn duplicate_keys_and_tables_report_lines() {
        assert_eq!(
            RiceSchema::parse("[a]\nx = 1\nx = 2\n").unwrap_err(),
            RiceError::Duplicate {
                line: 3,
                path: "a.x".into()
            }
        );
        assert!(matches!(
            RiceSchema::parse("[a]\n[b]\n[a]\n"),
            Err(RiceError::Duplicate { line: 3, .. })
        ));
    }

    #[test]
    fn edits_are_undoable_transactions() {
        let mut schema = RiceSchema::parse(SAMPLE).unwrap();
        let mut history = UndoStack::new();
        history
            .apply(
                &mut schema,
                SetRiceValue::new("layout.gap", RiceValue::Int(20)),
            )
            .unwrap();
        assert_eq!(schema.get_i64("layout.gap"), Some(20));
        history.undo(&mut schema).unwrap();
        assert_eq!(schema.get_i64("layout.gap"), Some(8));
        assert!(
            history
                .apply(
                    &mut schema,
                    SetRiceValue::new("layout.gap", RiceValue::Int(-1))
                )
                .is_err()
        );
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn labels_are_humanized() {
        let schema = RiceSchema::parse("[layout]\nleft_dock-width = 1\n").unwrap();
        assert_eq!(
            schema.entry("layout.left_dock-width").unwrap().label(),
            "Left dock width"
        );
        assert_eq!(humanize(""), "");
    }
}
