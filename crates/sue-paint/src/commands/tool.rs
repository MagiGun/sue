//! The tool and brush keys: Clip Studio Paint's letters for each tool,
//! the sub tool steps, the colors and the brush size.
//!
//! None of them changes the document, so none has an undo pair. Each is
//! one entry with its own run: a key that two or more tools share picks
//! them in turn (P switches between the pen and the pencil). Every one
//! may be recorded by Auto Actions ([`Flags::RECORDABLE`]) and none
//! leaves the canvas ([`Flags::LEAVES_CANVAS`]): a test holds both.

use sue_wm::{AppletContext, Call, Command, Flags};

use crate::color;
use crate::studio::SuePaint;
use crate::tools::Tool;

impl SuePaint {
    /// Runs a command of this group; `false` if `command` is not one.
    /// The keys and the tool commands of scripts and Auto Actions come
    /// here first, before the studio's other commands.
    pub(crate) fn tool_key(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        let Some(entry) = COMMANDS.iter().find(|entry| entry.name == command) else {
            return false;
        };
        (entry.run)(
            self,
            cx,
            Call {
                name: command,
                arg: "",
            },
        );
        true
    }

    /// Changes the brush size of the current sub tool by one step: a
    /// fifth of it, at least one pixel.
    fn step_brush(&mut self, bigger: bool) {
        let tip = &mut self.toolbox.current_mut().tip;
        tip.size = if bigger {
            (tip.size * 1.2).max(tip.size + 1.0)
        } else {
            (tip.size / 1.2).min(tip.size - 1.0)
        }
        .clamp(0.5, 2000.0);
    }
}

/// A command of this group: Auto Actions may record it, and it works
/// while a canvas is open.
const fn c(
    name: &'static str,
    label: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "tool", run)
        .state(super::doc_open)
        .with(Flags::RECORDABLE)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("tool.zoom", "zoom-tool", |paint, _, _| {
        paint.toolbox.cycle('/')
    })
    .keys("/")
    .icon("tool-zoom"),
    c("tool.hand", "hand-tool", |paint, _, _| {
        paint.toolbox.cycle('H')
    })
    .keys("H")
    .icon("tool-hand"),
    // The hand's Rotate sub tool.
    c("tool.rotate", "rotate-tool", |paint, _, _| {
        paint.toolbox.pick(Tool::Hand);
        paint.toolbox.pick_sub(Tool::Hand, 1);
    })
    .keys("R")
    .icon("tool-rotate-view"),
    c("tool.operation", "operation-tool", |paint, _, _| {
        paint.toolbox.cycle('O');
    })
    .keys("O")
    .icon("tool-operation"),
    c("tool.move_layer", "move-layer-tool", |paint, _, _| {
        paint.toolbox.cycle('K');
    })
    .keys("K")
    .icon("tool-move-layer"),
    c("tool.selection", "selection-tool", |paint, _, _| {
        paint.toolbox.cycle('M');
    })
    .keys("M")
    .icon("tool-select"),
    c("tool.auto_select", "auto-select-tool", |paint, _, _| {
        paint.toolbox.cycle('W');
    })
    .keys("W")
    .icon("tool-auto-select"),
    c("tool.eyedropper", "eyedropper-tool", |paint, _, _| {
        paint.toolbox.cycle('I');
    })
    .keys("I")
    .icon("tool-eyedropper"),
    c("tool.pen", "pen-and-pencil-tools", |paint, _, _| {
        paint.toolbox.cycle('P');
    })
    .keys("P")
    .icon("tool-pen"),
    c(
        "tool.brush",
        "brush-airbrush-and-decoration-tools",
        |paint, _, _| paint.toolbox.cycle('B'),
    )
    .keys("B")
    .icon("tool-brush"),
    c("tool.eraser", "eraser-tool", |paint, _, _| {
        paint.toolbox.cycle('E');
    })
    .keys("E")
    .icon("tool-eraser"),
    c("tool.blend", "blend-tool", |paint, _, _| {
        paint.toolbox.cycle('J')
    })
    .keys("J")
    .icon("tool-blend"),
    c("tool.fill", "fill-and-gradient-tools", |paint, _, _| {
        paint.toolbox.cycle('G');
    })
    .keys("G")
    .icon("tool-fill"),
    c(
        "tool.figure",
        "figure-frame-border-ruler-tools",
        |paint, _, _| paint.toolbox.cycle('U'),
    )
    .keys("U")
    .icon("tool-figure"),
    c("tool.text", "text-and-balloon-tools", |paint, _, _| {
        paint.toolbox.cycle('T');
    })
    .keys("T")
    .icon("tool-text"),
    c("tool.correct_line", "correct-line-tool", |paint, _, _| {
        paint.toolbox.cycle('Y');
    })
    .keys("Y")
    .icon("tool-correct-line"),
    // The sub tool list runs top to bottom: the one above, the one below.
    c("tool.previous_sub", "previous-sub-tool", |paint, _, _| {
        paint.toolbox.step_sub(false);
    })
    .keys(",")
    .icon("chevron-up"),
    c("tool.next_sub", "next-sub-tool", |paint, _, _| {
        paint.toolbox.step_sub(true);
    })
    .keys(".")
    .icon("chevron-down"),
    c("color.swap", "swap-main-and-sub-color", |paint, _, _| {
        std::mem::swap(&mut paint.main, &mut paint.sub);
        paint.transparent = false;
        paint.hex = color::hex(paint.drawing_color());
    })
    .keys("X")
    .icon("color-swap"),
    // Between the drawing color and the transparent color. Brushes erase
    // with the transparent color, so the eraser stands for it until the
    // icon set has a checker of its own.
    c(
        "color.transparent",
        "drawing-or-transparent-color",
        |paint, _, _| {
            paint.transparent = !paint.transparent;
        },
    )
    .keys("C")
    .icon("tool-eraser"),
    c("brush.smaller", "smaller-brush", |paint, _, _| {
        paint.step_brush(false);
    })
    .keys("[")
    .icon("minus"),
    c("brush.bigger", "bigger-brush", |paint, _, _| {
        paint.step_brush(true);
    })
    .keys("]")
    .icon("plus"),
];

#[cfg(test)]
mod tests {
    use super::*;

    use sue_core::{Arena, UndoStack};
    use sue_rice::RiceSchema;
    use sue_wm::World;

    /// Runs `work` on a fresh studio with an applet context over an
    /// empty world.
    fn with_studio(work: impl FnOnce(&mut SuePaint, &mut AppletContext<'_>)) {
        let rice = RiceSchema::new();
        let theme = sue_wm::Theme::from_rice(&rice);
        let mut world = World {
            arena: Arena::new(),
            rice,
        };
        let mut history = UndoStack::new();
        let mut typing = sue_wm::Typing::default();
        let mut requests = Vec::new();
        let catalog = sue_wm::Catalog::default();
        let mut textures = sue_platform::NoTextures;
        let mut transport = sue_wm::Transport::default();
        let work_list = sue_wm::WorkList::default();
        let mut transfers = sue_wm::Transfers::default();
        let mut cx = AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: sue_wm::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        let mut paint = SuePaint::default();
        work(&mut paint, &mut cx);
    }

    #[test]
    fn every_entry_has_its_label_key_icon_and_flags() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let registry = super::super::registry();
        for entry in COMMANDS {
            let name = entry.name;
            assert_eq!(entry.group, "tool", "{name}");
            assert!(sue_lang::has("paint", entry.label), "{name}: no label");
            assert!(!entry.chords.is_empty(), "{name}: no default key");
            assert!(
                !entry.default_chords().is_empty(),
                "{name}: {:?} does not parse",
                entry.chords
            );
            assert!(
                sue_wm::icons::Icon::from_name(entry.icon).is_some(),
                "{name}: no icon named {:?}",
                entry.icon
            );
            assert!(!entry.is_family(), "{name}");
            assert!(!entry.flags.has(Flags::HIDDEN), "{name}");
            // The registry answers for it with this very entry.
            let (found, call) = registry.find(name).expect("registered");
            assert_eq!((found.name, found.label), (name, entry.label));
            assert_eq!(call.arg, "", "{name}");
        }
    }

    #[test]
    fn the_group_is_recordable_and_stays_on_the_canvas() {
        // `RECORDABLE` keeps what `recordable()` decided for these names,
        // and no command of the group leaves the canvas.
        for entry in COMMANDS {
            assert!(entry.flags.has(Flags::RECORDABLE), "{}", entry.name);
            assert_eq!(
                entry.flags.has(Flags::RECORDABLE),
                crate::extras::recordable(entry.name),
                "{}",
                entry.name
            );
            assert!(!entry.flags.has(Flags::LEAVES_CANVAS), "{}", entry.name);
        }
    }

    #[test]
    fn a_name_of_another_group_is_not_a_tool_key() {
        with_studio(|paint, cx| {
            let before = paint.toolbox.clone();
            assert!(!paint.tool_key("edit.undo", cx));
            assert!(!paint.tool_key("tool.", cx));
            assert!(!paint.tool_key("tool.nothing", cx));
            assert_eq!(paint.toolbox, before);
        });
    }

    #[test]
    fn each_tool_key_picks_its_tools_in_turn() {
        // The tools each key goes round, in the palette's order: from a
        // tool of another key the first, then each after the current one.
        let cases: [(&str, &[Tool]); 15] = [
            ("tool.zoom", &[Tool::Zoom]),
            ("tool.hand", &[Tool::Hand]),
            ("tool.operation", &[Tool::Operation]),
            ("tool.move_layer", &[Tool::MoveLayer]),
            ("tool.selection", &[Tool::Select]),
            ("tool.auto_select", &[Tool::AutoSelect]),
            ("tool.eyedropper", &[Tool::Eyedropper]),
            ("tool.pen", &[Tool::Pen, Tool::Pencil]),
            (
                "tool.brush",
                &[Tool::Brush, Tool::Airbrush, Tool::Decoration],
            ),
            ("tool.eraser", &[Tool::Eraser]),
            ("tool.blend", &[Tool::Blend]),
            ("tool.fill", &[Tool::Fill, Tool::Gradient]),
            ("tool.figure", &[Tool::Figure, Tool::Frame, Tool::Ruler]),
            ("tool.text", &[Tool::Text, Tool::Balloon]),
            ("tool.correct_line", &[Tool::CorrectLine]),
        ];
        with_studio(|paint, cx| {
            // A fresh studio starts on the pen.
            assert_eq!(paint.toolbox.tool(), Tool::Pen);
            for (command, tools) in cases {
                for round in 0..2 {
                    for tool in tools {
                        assert!(paint.tool_key(command, cx), "{command}");
                        assert_eq!(paint.toolbox.tool(), *tool, "{command}, round {round}");
                    }
                }
            }
        });
    }

    #[test]
    fn r_picks_the_hands_rotate_sub_tool() {
        with_studio(|paint, cx| {
            assert!(paint.tool_key("tool.rotate", cx));
            assert_eq!(paint.toolbox.tool(), Tool::Hand);
            assert_eq!(paint.toolbox.picked(Tool::Hand), 1);
            // H again keeps the hand on the sub tool it had.
            assert!(paint.tool_key("tool.hand", cx));
            assert_eq!(paint.toolbox.picked(Tool::Hand), 1);
        });
    }

    #[test]
    fn comma_and_period_step_through_the_sub_tools() {
        with_studio(|paint, cx| {
            let count = paint.toolbox.subs(Tool::Pen).len();
            assert!(count > 1, "the pen has sub tools to step through");
            assert_eq!(paint.toolbox.picked(Tool::Pen), 0);
            assert!(paint.tool_key("tool.next_sub", cx));
            assert_eq!(paint.toolbox.picked(Tool::Pen), 1);
            // Back up past the first goes round to the last.
            assert!(paint.tool_key("tool.previous_sub", cx));
            assert!(paint.tool_key("tool.previous_sub", cx));
            assert_eq!(paint.toolbox.picked(Tool::Pen), count - 1);
            assert!(paint.tool_key("tool.next_sub", cx));
            assert_eq!(paint.toolbox.picked(Tool::Pen), 0);
        });
    }

    #[test]
    fn x_swaps_the_two_colors_and_c_the_transparent_one() {
        with_studio(|paint, cx| {
            paint.main = [10, 20, 30];
            paint.sub = [200, 210, 220];
            paint.transparent = true;
            assert!(paint.tool_key("color.swap", cx));
            assert_eq!((paint.main, paint.sub), ([200, 210, 220], [10, 20, 30]));
            assert!(!paint.transparent, "X goes back to a color");
            assert_eq!(paint.hex, color::hex(paint.drawing_color()));
            assert!(paint.tool_key("color.transparent", cx));
            assert!(paint.transparent);
            assert!(paint.tool_key("color.transparent", cx));
            assert!(!paint.transparent);
        });
    }

    #[test]
    fn the_brackets_step_the_brush_size_within_its_bounds() {
        with_studio(|paint, cx| {
            paint.toolbox.current_mut().tip.size = 10.0;
            assert!(paint.tool_key("brush.bigger", cx));
            assert!((paint.toolbox.current().tip.size - 12.0).abs() < 1e-4);
            assert!(paint.tool_key("brush.smaller", cx));
            assert!((paint.toolbox.current().tip.size - 10.0).abs() < 1e-4);
            // A small brush steps by at least one pixel.
            paint.toolbox.current_mut().tip.size = 2.0;
            assert!(paint.tool_key("brush.bigger", cx));
            assert!((paint.toolbox.current().tip.size - 3.0).abs() < 1e-4);
            // And never leaves the bounds.
            paint.toolbox.current_mut().tip.size = 1.0;
            assert!(paint.tool_key("brush.smaller", cx));
            assert!((paint.toolbox.current().tip.size - 0.5).abs() < 1e-4);
            paint.toolbox.current_mut().tip.size = 1990.0;
            assert!(paint.tool_key("brush.bigger", cx));
            assert!((paint.toolbox.current().tip.size - 2000.0).abs() < 1e-4);
        });
    }
}
