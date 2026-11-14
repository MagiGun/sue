//! Tool layouts: how the tool palette is arranged, as a text file a person
//! can write, share, install and remove (`docs/tool-layout-format.md`).
//!
//! A layout names tools, their groups and the sub tools in each group, in
//! order. It holds no settings: a sub tool is named by its tool's id and
//! its name, and a toolbox arranged by a layout keeps each sub tool's own
//! settings ([`super::Toolbox::arrange`]). Sue Paint's own arrangement is
//! a layout too, [`ToolLayout::builtin`].

use super::Tool;

/// The first line of a tool layout file.
pub const HEADER: &str = "sue-tool-layout 1";

/// The file name extension of a tool layout.
pub const EXTENSION: &str = "suetools";

/// Sue Paint's own arrangement: Clip Studio Paint's tool palette order and
/// sub tool groups (Ver. 4).
const BUILTIN: &str = include_str!("builtin.suetools");

/// A tool palette arrangement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolLayout {
    /// The tools, top to bottom.
    pub tools: Vec<LayoutTool>,
}

/// One tool of a [`ToolLayout`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutTool {
    /// What people read (English for Sue Paint's own).
    pub name: String,
    /// The tool whose icon it shows.
    pub kind: Tool,
    /// The key that picks it, or none.
    pub key: Option<char>,
    /// Its groups, in order.
    pub groups: Vec<LayoutGroup>,
}

/// One sub tool group of a [`LayoutTool`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutGroup {
    /// What people read (English for Sue Paint's own).
    pub name: String,
    /// Its sub tools, in order, by tool and name.
    pub subs: Vec<(Tool, String)>,
}

impl ToolLayout {
    /// Sue Paint's own arrangement.
    #[must_use]
    pub fn builtin() -> Self {
        let (layout, problems) = Self::parse(BUILTIN);
        debug_assert!(problems.is_empty(), "{problems:?}");
        layout
    }

    /// Reads a layout from `text`: what it could read, and a line for each
    /// thing it could not, saying where (1 is the first line).
    #[must_use]
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let mut layout = Self::default();
        let mut problems = Vec::new();
        let mut header = false;
        for (number, line) in text.lines().enumerate() {
            let number = number + 1;
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            if !header {
                header = true;
                if line.trim() == HEADER {
                    continue;
                }
                if line.starts_with("sue-tool-layout ") {
                    problems.push(format!("{number}: a newer layout; read as far as known"));
                    continue;
                }
                problems.push(format!("{number}: not a tool layout (no \"{HEADER}\")"));
            }
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["tool", name, kind, key] => {
                    let Some(kind) = Tool::from_id(kind) else {
                        problems.push(format!("{number}: no tool {kind}"));
                        continue;
                    };
                    let key = match *key {
                        "-" | "" => None,
                        key if key.chars().count() == 1 => key.chars().next(),
                        key => {
                            problems.push(format!("{number}: not a key {key}"));
                            None
                        }
                    };
                    layout.tools.push(LayoutTool {
                        name: (*name).to_owned(),
                        kind,
                        key,
                        groups: Vec::new(),
                    });
                }
                ["group", name] => match layout.tools.last_mut() {
                    Some(tool) => tool.groups.push(LayoutGroup {
                        name: (*name).to_owned(),
                        subs: Vec::new(),
                    }),
                    None => problems.push(format!("{number}: a group before any tool")),
                },
                ["sub", kind, name] => {
                    let Some(kind) = Tool::from_id(kind) else {
                        problems.push(format!("{number}: no tool {kind}"));
                        continue;
                    };
                    let Some(tool) = layout.tools.last_mut() else {
                        problems.push(format!("{number}: a sub tool before any tool"));
                        continue;
                    };
                    if tool.groups.is_empty() {
                        tool.groups.push(LayoutGroup {
                            name: tool.name.clone(),
                            subs: Vec::new(),
                        });
                    }
                    if let Some(group) = tool.groups.last_mut() {
                        group.subs.push((kind, (*name).to_owned()));
                    }
                }
                _ => problems.push(format!("{number}: not a line of a tool layout")),
            }
        }
        (layout, problems)
    }

    /// The layout as the text [`Self::parse`] reads. Names lose any tab or
    /// line break, which the format keeps for itself.
    #[must_use]
    pub fn to_text(&self) -> String {
        let clean = |name: &str| name.replace(['\t', '\r', '\n'], " ");
        let mut out = format!("{HEADER}\n");
        for tool in &self.tools {
            let key = tool
                .key
                .map_or_else(|| "-".to_owned(), |key| key.to_string());
            out.push_str(&format!(
                "tool\t{}\t{}\t{key}\n",
                clean(&tool.name),
                tool.kind.id()
            ));
            for group in &tool.groups {
                out.push_str(&format!("group\t{}\n", clean(&group.name)));
                for (kind, name) in &group.subs {
                    out.push_str(&format!("sub\t{}\t{}\n", kind.id(), clean(name)));
                }
            }
        }
        out
    }

    /// Every name a person reads in it, tools and groups, for the language
    /// database.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for tool in &self.tools {
            names.push(tool.name.clone());
            names.extend(tool.groups.iter().map(|group| group.name.clone()));
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::super::{SubTool, Toolbox};
    use super::*;

    #[test]
    fn the_builtin_layout_reads_cleanly_and_places_every_sub_tool_once() {
        let (layout, problems) = ToolLayout::parse(BUILTIN);
        assert!(problems.is_empty(), "{problems:?}");
        let mut placed: Vec<(Tool, String)> = layout
            .tools
            .iter()
            .flat_map(|tool| tool.groups.iter())
            .flat_map(|group| group.subs.iter().cloned())
            .collect();
        let count = placed.len();
        placed.sort();
        placed.dedup();
        assert_eq!(placed.len(), count, "no sub tool is placed twice");
        let mut all: Vec<(Tool, String)> = Tool::ALL
            .iter()
            .flat_map(|tool| tool.sub_tools())
            .map(|sub| (sub.tool, sub.name))
            .collect();
        all.sort();
        assert_eq!(placed, all, "every built-in sub tool has its place");
        let mut catalogue = Toolbox::catalogue();
        assert!(catalogue.arrange(&layout).is_empty());
        assert_eq!(catalogue, Toolbox::default());
    }

    #[test]
    fn a_layout_round_trips_through_its_text() {
        let layout = Toolbox::default().layout();
        let text = layout.to_text();
        assert!(text.starts_with(HEADER));
        let (read, problems) = ToolLayout::parse(&text);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(read, layout);
        assert_eq!(read, ToolLayout::builtin());
        // Names keep what the format can hold.
        let mut odd = layout;
        odd.tools[0].name = "Mine\twith a tab".to_owned();
        let (read, _) = ToolLayout::parse(&odd.to_text());
        assert_eq!(read.tools[0].name, "Mine with a tab");
    }

    #[test]
    fn a_hand_written_layout_is_followed_and_what_it_misses_is_kept() {
        let text = "# My layout\nsue-tool-layout 1\n\
            tool\tInk\tpen\tP\n\
            group\tFavourites\n\
            sub\tpen\tTurnip pen\n\
            sub\tbrush\tGouache\n\
            sub\tpen\tNo such pen\n\
            tool\tNothing here\tbrush\t-\n\
            sub\tbrush\tAlso missing\n\
            bogus line\n\
            tool\tX\tno_such_tool\t-\n";
        let (layout, problems) = ToolLayout::parse(text);
        assert_eq!(problems.len(), 2, "{problems:?}");
        let mut toolbox = Toolbox::default();
        let before: usize = toolbox.slots().iter().map(|slot| slot.subs().len()).sum();
        let problems = toolbox.arrange(&layout);
        assert_eq!(
            problems,
            vec![
                "pen: No such pen".to_owned(),
                "brush: Also missing".to_owned(),
                "brush: Nothing here".to_owned()
            ]
        );
        let first = &toolbox.slots()[0];
        assert_eq!(first.name, "Ink");
        assert_eq!(first.key, Some('P'));
        assert_eq!(first.groups()[0].name, "Favourites");
        assert_eq!(first.subs()[0].name, "Turnip pen");
        assert_eq!(first.subs()[1].name, "Gouache");
        // Every sub tool the layout leaves out is still there, at the end
        // of its own tool's entry.
        let after: usize = toolbox.slots().iter().map(|slot| slot.subs().len()).sum();
        assert_eq!(after, before);
        assert!(toolbox.slots().iter().any(|slot| slot.kind == Tool::Eraser));
    }

    #[test]
    fn a_layout_finds_sub_tools_by_their_former_names() {
        let text = "sue-tool-layout 1\ntool\tPens\tpen\tP\nsub\tpen\tReal G-Pen\n";
        let (layout, problems) = ToolLayout::parse(text);
        assert!(problems.is_empty());
        let mut toolbox = Toolbox::default();
        assert!(toolbox.arrange(&layout).is_empty());
        assert_eq!(
            toolbox.slots()[0].subs()[0].name,
            SubTool::renamed("Real G-Pen")
        );
    }

    #[test]
    fn a_newer_or_foreign_file_says_so() {
        let (_, problems) = ToolLayout::parse("sue-tool-layout 9\ntool\tA\tpen\tP\n");
        assert_eq!(problems.len(), 1);
        let (layout, problems) = ToolLayout::parse("hello\n");
        assert_eq!(problems.len(), 2);
        assert!(layout.tools.is_empty());
    }
}
