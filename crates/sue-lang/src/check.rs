//! Checks a language against English: which messages it lacks, which it
//! has that English no longer does, and which use other variables. What
//! `sue-cli lang check` prints and what the tests require.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

use crate::{ENGLISH, LANGUAGE};

/// A message whose variables differ from English's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableMismatch {
    /// Its namespace.
    pub namespace: String,
    /// Its key.
    pub key: String,
    /// The variables English uses.
    pub english: BTreeSet<String>,
    /// The variables this language uses.
    pub found: BTreeSet<String>,
}

/// How a language stands against English.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The language's tag.
    pub tag: String,
    /// Its name in its own script.
    pub name: String,
    /// `true` while it waits for a native speaker's review.
    pub draft: bool,
    /// Messages English has and this language lacks: namespace, key and
    /// the English source line, ready to translate.
    pub missing: Vec<(String, String, String)>,
    /// Messages this language has and English does not (stale: remove or
    /// rename them).
    pub stale: Vec<(String, String)>,
    /// Messages whose variables differ from English's.
    pub variables: Vec<VariableMismatch>,
    /// Lines that do not parse.
    pub errors: Vec<String>,
    /// Messages English has, and how many of them this language has.
    pub total: usize,
}

impl Report {
    /// `true` with nothing stale, mismatched or broken (missing messages
    /// fall back to English, so they do not fail it).
    #[must_use]
    pub fn sound(&self) -> bool {
        self.stale.is_empty() && self.variables.is_empty() && self.errors.is_empty()
    }

    /// `true` when it has every message English has, and is sound.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.missing.is_empty() && self.sound()
    }

    /// How many of English's messages it has.
    #[must_use]
    pub fn translated(&self) -> usize {
        self.total.saturating_sub(self.missing.len())
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.draft {
            "draft, waiting for a native speaker's review"
        } else {
            "reviewed"
        };
        writeln!(f, "{} ({}): {status}", self.name, self.tag)?;
        writeln!(
            f,
            "  {} of {} messages translated",
            self.translated(),
            self.total
        )?;
        for error in &self.errors {
            writeln!(f, "  error: {error}")?;
        }
        if !self.stale.is_empty() {
            writeln!(f, "  stale (English no longer has them; remove or rename):")?;
            for (namespace, key) in &self.stale {
                writeln!(f, "    {namespace}.ftl: {key}")?;
            }
        }
        for mismatch in &self.variables {
            let list = |set: &BTreeSet<String>| {
                set.iter()
                    .map(|name| format!("${name}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            writeln!(
                f,
                "  variables differ in {}.ftl: {}: English uses [{}], this uses [{}]",
                mismatch.namespace,
                mismatch.key,
                list(&mismatch.english),
                list(&mismatch.found)
            )?;
        }
        if !self.missing.is_empty() {
            writeln!(
                f,
                "  missing (shown in English until translated; copy into the file named and translate):"
            )?;
            let mut last = "";
            for (namespace, _, line) in &self.missing {
                if namespace != last {
                    writeln!(f, "  # {}/{namespace}.ftl", self.tag)?;
                    last = namespace;
                }
                for part in line.lines() {
                    writeln!(f, "  {part}")?;
                }
            }
        }
        Ok(())
    }
}

/// Every message in `sources` (namespace to its files): its source text and
/// its variables. Parse errors go to `errors`.
fn messages(
    sources: &BTreeMap<String, Vec<String>>,
    errors: &mut Vec<String>,
) -> BTreeMap<(String, String), (String, BTreeSet<String>)> {
    let mut found = BTreeMap::new();
    for (namespace, files) in sources {
        for source in files {
            let resource = match fluent_syntax::parser::parse(source.as_str()) {
                Ok(resource) => resource,
                Err((resource, problems)) => {
                    errors.extend(problems.into_iter().map(|problem| {
                        let line = source[..problem.pos.start.min(source.len())]
                            .lines()
                            .count();
                        format!("{namespace}.ftl:{line}: {problem}")
                    }));
                    resource
                }
            };
            for entry in &resource.body {
                if let Entry::Message(message) = entry {
                    let mut variables = BTreeSet::new();
                    if let Some(pattern) = &message.value {
                        collect(pattern, &mut variables);
                    }
                    for attribute in &message.attributes {
                        collect(&attribute.value, &mut variables);
                    }
                    let text = line_of(source, message.id.name);
                    found.insert(
                        (namespace.clone(), message.id.name.to_owned()),
                        (text, variables),
                    );
                }
            }
        }
    }
    found
}

/// The source of message `key` in `source`: its first line and the
/// indented lines after it.
fn line_of(source: &str, key: &str) -> String {
    let mut lines = source.lines();
    let mut out = Vec::new();
    for line in lines.by_ref() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(key)
            && trimmed[key.len()..].trim_start().starts_with('=')
            && !line.starts_with(' ')
        {
            out.push(line);
            break;
        }
    }
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push(line);
        } else {
            break;
        }
    }
    out.join("\n")
}

fn collect(pattern: &Pattern<&str>, variables: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            collect_expression(expression, variables);
        }
    }
}

fn collect_expression(expression: &Expression<&str>, variables: &mut BTreeSet<String>) {
    match expression {
        Expression::Inline(inline) => collect_inline(inline, variables),
        Expression::Select { selector, variants } => {
            collect_inline(selector, variables);
            for variant in variants {
                collect(&variant.value, variables);
            }
        }
    }
}

fn collect_inline(inline: &InlineExpression<&str>, variables: &mut BTreeSet<String>) {
    match inline {
        InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_owned());
        }
        InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                collect_inline(argument, variables);
            }
            for named in &arguments.named {
                collect_inline(&named.value, variables);
            }
        }
        InlineExpression::Placeable { expression } => collect_expression(expression, variables),
        _ => {}
    }
}

/// Every English message: (namespace, key) and its variables.
#[must_use]
pub fn english_keys() -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut errors = Vec::new();
    crate::sources(ENGLISH)
        .map(|(sources, _)| messages(&sources, &mut errors))
        .unwrap_or_default()
        .into_iter()
        .map(|(key, (_, variables))| (key, variables))
        .collect()
}

/// The variables of every message in one Fluent source.
///
/// # Errors
/// The parse errors.
pub fn variables_of(source: &str) -> Result<BTreeMap<String, BTreeSet<String>>, Vec<String>> {
    let mut errors = Vec::new();
    let mut sources = BTreeMap::new();
    sources.insert(String::new(), vec![source.to_owned()]);
    let found = messages(&sources, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(found
        .into_iter()
        .map(|((_, key), (_, variables))| (key, variables))
        .collect())
}

/// How language `tag` stands against English, or `None` if Sue does not
/// have it.
#[must_use]
pub fn check(tag: &str) -> Option<Report> {
    let (sources, english_sources) = crate::sources(tag)?;
    let mut report = Report {
        tag: tag.to_owned(),
        name: crate::name(tag),
        draft: crate::languages()
            .into_iter()
            .any(|info| info.tag == tag && info.draft),
        ..Report::default()
    };
    let mut ignored = Vec::new();
    let english = messages(&english_sources, &mut ignored);
    let own = messages(&sources, &mut report.errors);
    // A language describes itself in its own words: those keys are its own.
    let shared = |namespace: &str| namespace != LANGUAGE;
    report.total = english.keys().filter(|(space, _)| shared(space)).count();
    for ((namespace, key), (text, variables)) in &english {
        if !shared(namespace) {
            continue;
        }
        match own.get(&(namespace.clone(), key.clone())) {
            None => report
                .missing
                .push((namespace.clone(), key.clone(), text.clone())),
            Some((_, found)) if found != variables => report.variables.push(VariableMismatch {
                namespace: namespace.clone(),
                key: key.clone(),
                english: variables.clone(),
                found: found.clone(),
            }),
            Some(_) => {}
        }
    }
    for (namespace, key) in own.keys() {
        if shared(namespace) && !english.contains_key(&(namespace.clone(), key.clone())) {
            report.stale.push((namespace.clone(), key.clone()));
        }
    }
    Some(report)
}
