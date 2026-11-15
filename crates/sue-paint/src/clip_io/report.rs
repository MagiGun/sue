//! The conversion report: per layer, what a `.clip` import or export did
//! with it. It is said once, quietly, in the status bar, never in a
//! dialog that stops the work.

/// What became of one layer, from the best to the worst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Outcome {
    /// Converted with nothing lost.
    Exact,
    /// Converted as closely as Sue can (a gray layer painted in color, a
    /// blend mode Sue does not know).
    Approximated,
    /// Shown as Sue can; what Sue cannot show yet is kept as Clip Studio
    /// data and goes back into the file while the layer is unchanged.
    Kept,
    /// Written as its pixels only (text and vector layers).
    Flattened,
    /// Not written: Clip Studio has nothing it could be (a correction
    /// layer Sue made).
    LeftOut,
}

/// One layer's line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The layer's name.
    pub name: String,
    /// What became of it.
    pub outcome: Outcome,
}

/// A whole report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// One line per layer and folder, bottom first.
    pub lines: Vec<Line>,
    /// The file carried a copy of Sue's own project, but its layers were
    /// changed after Sue wrote it, so it opened from its layers.
    pub stale_copy: bool,
    /// The file opened from the copy of Sue's own project it carried.
    pub own_copy: bool,
    /// What the file held beyond its layers had to be left behind (the
    /// canvas changed size).
    pub canvas_dropped: bool,
}

impl Report {
    /// The names of the layers that came out as `outcome`.
    #[must_use]
    pub fn names(&self, outcome: Outcome) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|line| line.outcome == outcome)
            .map(|line| line.name.as_str())
            .collect()
    }

    /// Whether every layer came out exactly.
    #[must_use]
    pub fn exact(&self) -> bool {
        !self.stale_copy
            && !self.canvas_dropped
            && self.lines.iter().all(|line| line.outcome == Outcome::Exact)
    }

    /// The report in one line of the language in use.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.own_copy {
            return tr!("clip-report-own-copy");
        }
        let mut parts = Vec::new();
        if self.stale_copy {
            parts.push(tr!("clip-report-stale-copy"));
        }
        let exact = self.names(Outcome::Exact).len();
        if exact > 0 {
            parts.push(tr!("clip-report-exact", count = exact));
        }
        let approximated = self.names(Outcome::Approximated);
        if !approximated.is_empty() {
            parts.push(tr!(
                "clip-report-approximated",
                names = listed(&approximated)
            ));
        }
        let kept = self.names(Outcome::Kept);
        if !kept.is_empty() {
            parts.push(tr!("clip-report-kept", names = listed(&kept)));
        }
        let flattened = self.names(Outcome::Flattened);
        if !flattened.is_empty() {
            parts.push(tr!("clip-report-flattened", names = listed(&flattened)));
        }
        let left_out = self.names(Outcome::LeftOut);
        if !left_out.is_empty() {
            parts.push(tr!("clip-report-left-out", names = listed(&left_out)));
        }
        if self.canvas_dropped {
            parts.push(tr!("clip-report-canvas-dropped"));
        }
        if parts.is_empty() {
            return tr!("clip-report-empty");
        }
        parts.join("; ")
    }
}

/// Up to three names, then how many more.
fn listed(names: &[&str]) -> String {
    let shown = names
        .iter()
        .take(3)
        .map(|name| {
            if name.is_empty() {
                tr!("clip-report-unnamed")
            } else {
                (*name).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > 3 {
        tr!("clip-report-more", names = shown, count = names.len() - 3)
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, outcome: Outcome) -> Line {
        Line {
            name: name.to_owned(),
            outcome,
        }
    }

    #[test]
    fn the_summary_names_what_was_not_exact() {
        let mut report = Report::default();
        assert_eq!(report.summary(), "nothing to convert");
        report.lines = vec![
            line("Back", Outcome::Exact),
            line("Ink", Outcome::Exact),
            line("Gray", Outcome::Approximated),
            line("Tone", Outcome::Kept),
        ];
        assert!(!report.exact());
        assert_eq!(
            report.summary(),
            "2 layers converted exactly; approximated: Gray; \
             kept as Clip Studio data: Tone"
        );
        report.lines = ["A", "B", "C", "D", "E"]
            .iter()
            .map(|name| line(name, Outcome::LeftOut))
            .collect();
        report.stale_copy = true;
        assert_eq!(
            report.summary(),
            "the copy of the Sue project inside does not match the layers \
             (they were changed after Sue saved it), so it opened from the \
             layers; left out: A, B, C and 2 more"
        );
        report.lines = vec![line("Only", Outcome::Exact)];
        report.stale_copy = false;
        assert!(report.exact());
        assert_eq!(report.summary(), "1 layer converted exactly");
    }
}
