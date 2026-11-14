//! Tidying plain text on its way out: line endings, spaces at the ends of
//! lines, and tabs. A pure function of its inputs.

/// Line endings for written text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endings {
    /// `\n`, as on Linux and macOS.
    Lf,
    /// `\r\n`, as on Windows.
    CrLf,
}

/// Everything an export can tidy about plain text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextOptions {
    /// Line endings.
    pub endings: Endings,
    /// Drop spaces at the ends of lines.
    pub trim_trailing: bool,
    /// Replace tabs with this many spaces (0 keeps them).
    pub tab_spaces: u8,
}

/// The text tidied: line endings, trailing spaces, tabs. A last line
/// keeps its line end, or its lack of one.
#[must_use]
pub fn tidy(text: &str, options: &TextOptions) -> String {
    let end = match options.endings {
        Endings::Lf => "\n",
        Endings::CrLf => "\r\n",
    };
    let ends = text.ends_with('\n');
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut line = if options.tab_spaces > 0 {
                line.replace('\t', &" ".repeat(usize::from(options.tab_spaces)))
            } else {
                line.to_owned()
            };
            if options.trim_trailing {
                line.truncate(line.trim_end().len());
            }
            line
        })
        .collect();
    let mut out = lines.join(end);
    if ends {
        out.push_str(end);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_tidied_as_the_options_say() {
        let all = TextOptions {
            endings: Endings::CrLf,
            trim_trailing: true,
            tab_spaces: 2,
        };
        assert_eq!(tidy("a  \n\tb\n", &all), "a\r\n  b\r\n");
        // Line endings alone: spaces and tabs stay, as Markdown needs.
        let endings = |endings| TextOptions {
            endings,
            trim_trailing: false,
            tab_spaces: 0,
        };
        assert_eq!(
            tidy("one  \r\n\ttwo\r\n", &endings(Endings::Lf)),
            "one  \n\ttwo\n"
        );
        assert_eq!(
            tidy("one\ntwo\r\nthree", &endings(Endings::CrLf)),
            "one\r\ntwo\r\nthree"
        );
        // Tabs become spaces before the line ends are trimmed, so a line
        // of tabs alone becomes empty.
        let tabs = TextOptions {
            endings: Endings::Lf,
            trim_trailing: true,
            tab_spaces: 4,
        };
        assert_eq!(tidy("\tx\ty \n\t\t\n", &tabs), "    x    y\n\n");
        // Tidying twice changes nothing more, and nothing stays nothing.
        let once = tidy("a \t\r\nb", &tabs);
        assert_eq!(tidy(&once, &tabs), once);
        assert_eq!(tidy("", &all), "");
    }
}
