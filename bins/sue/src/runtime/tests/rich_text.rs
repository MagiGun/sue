//! The rich text editor's chrome, as Sue Writer shows it: the colors of
//! what it marks on the page (spelling, tracked changes, comments, the
//! selection and its handles) come from the theme, the spelling menu is
//! Sue's own context menu, and the bar over a finger's selection is made
//! of Sue's buttons with their tips.

use super::*;
use egui::epaint::ColorMode;
use egui::text::{CCursor, CharIndex};
use sue_type::Ink;
use sue_type::layout::PageMarks;

/// The colors Sue's rich text editor drew its marks in before they came
/// from the theme: a blue selection and handles, a red spelling wave,
/// blue and red tracked changes, a yellow comment. None may come back.
const OLD_COLORS: [[u8; 3]; 5] = [
    [0x3a, 0x7b, 0xd5],
    [0xd0, 0x30, 0x30],
    [0x1f, 0x6f, 0xc4],
    [0xc0, 0x39, 0x2b],
    [0xf2, 0xc0, 0x3a],
];

/// Every shape of `output`, groups opened.
fn shapes(output: &egui::FullOutput) -> Vec<egui::Shape> {
    fn open(shape: &egui::Shape, into: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| open(shape, into)),
            shape => into.push(shape.clone()),
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        open(&clipped.shape, &mut found);
    }
    found
}

/// Every color the shapes of `output` are drawn in, text sections
/// included.
fn colors(shapes: &[egui::Shape]) -> Vec<egui::Color32> {
    let mut found = Vec::new();
    for shape in shapes {
        match shape {
            egui::Shape::Text(text) => {
                for section in &text.galley.job.sections {
                    found.push(section.format.color);
                    found.push(section.format.background);
                }
            }
            egui::Shape::Path(path) => {
                if let ColorMode::Solid(color) = path.stroke.color {
                    found.push(color);
                }
                found.push(path.fill);
            }
            egui::Shape::LineSegment { stroke, .. } => found.push(stroke.color),
            egui::Shape::Rect(rect) => {
                found.push(rect.fill);
                found.push(rect.stroke.color);
            }
            egui::Shape::Circle(circle) => found.push(circle.fill),
            _ => {}
        }
    }
    found
}

/// The document's galley: the text shape whose text starts with `start`.
fn doc_text(shapes: &[egui::Shape], start: &str) -> Option<egui::epaint::TextShape> {
    shapes.iter().find_map(|shape| match shape {
        egui::Shape::Text(text) if text.galley.text().starts_with(start) => Some(text.clone()),
        _ => None,
    })
}

/// Where the character `index` of `text` sits on screen, at the middle
/// of its line.
fn char_at(text: &egui::epaint::TextShape, index: usize) -> Pos2 {
    let place = text.galley.pos_from_cursor(CCursor {
        index: CharIndex(index),
        prefer_next_row: false,
    });
    text.pos + vec2(place.min.x + 2.0, place.center().y)
}

/// The text section of `text` that holds `words`: its format.
fn section_of(text: &egui::epaint::TextShape, words: &str) -> egui::TextFormat {
    let job = &text.galley.job;
    job.sections
        .iter()
        .find(|section| {
            job.text[section.byte_range.start.0..section.byte_range.end.0].contains(words)
        })
        .map(|section| section.format.clone())
        .unwrap_or_else(|| panic!("a section holds {words:?}"))
}

/// The first document of the arena: its text and marks.
fn document(runtime: &Runtime) -> (String, sue_model::TextMarks) {
    runtime
        .engine
        .world
        .arena
        .iter()
        .find_map(|(_, entity)| {
            let text = entity.string_buffer()?.as_str().to_owned();
            text.starts_with("The").then(|| {
                (
                    text,
                    entity
                        .ext::<sue_model::TextMarks>()
                        .cloned()
                        .unwrap_or_default(),
                )
            })
        })
        .expect("the document")
}

/// A press and release of `button` at `at`, over three frames.
fn press(at: Pos2, button: egui::PointerButton) -> impl FnMut(usize) -> Vec<egui::Event> {
    move |frame: usize| match frame {
        0 => vec![egui::Event::PointerMoved(at)],
        1 => vec![egui::Event::PointerButton {
            pos: at,
            button,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        2 => vec![egui::Event::PointerButton {
            pos: at,
            button,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        _ => Vec::new(),
    }
}

/// One frame of `size` with `events`, its shapes.
fn step(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    size: egui::Vec2,
    now: Instant,
    events: Vec<egui::Event>,
) -> Vec<egui::Shape> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        events,
        time: Some(ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let found = shapes(&output);
    output.drop_without_applying_deltas();
    found
}

#[test]
fn rich_text_marks_come_from_the_theme_and_the_spelling_menu_picks_with_undo() {
    let (mut runtime, root) = runtime("rich-text-marks");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let writer = AppletId::new(sue_writer::ID).unwrap();
    runtime.request(
        ShellRequest::Create {
            applet: writer.clone(),
            key: "blank".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![egui::Event::Text("The mornnig sat".to_owned())]
        } else {
            Vec::new()
        }
    });
    runtime.typing.flush(&mut runtime.engine.history);
    let marks = PageMarks::resolve(runtime.shell.theme(), &Ink::paper());
    let drawn = step(&mut runtime, &ctx, size, start, Vec::new());
    // The wave under "mornnig" is drawn in the theme's color for it.
    let text = doc_text(&drawn, "The mornnig").expect("the text is drawn");
    let under = char_at(&text, 4).x - 3.0..char_at(&text, 11).x;
    let wave = drawn.iter().any(|shape| match shape {
        egui::Shape::Path(path) => {
            path.stroke.color == ColorMode::Solid(marks.misspelt)
                && path.points.len() > 2
                && path.points.iter().all(|point| under.contains(&point.x))
        }
        _ => false,
    });
    assert!(
        wave,
        "a wave in {:?} under the misspelled word",
        marks.misspelt
    );
    // A secondary press on it opens Sue's menu with what it may have
    // meant; picking "morning" mends it.
    let before = document(&runtime);
    let on = char_at(&text, 7);
    frames(
        &mut runtime,
        &ctx,
        size,
        start,
        press(on, egui::PointerButton::Secondary),
    );
    let mut row = None;
    let mut menu = Vec::new();
    for _ in 0..40 {
        menu = step(&mut runtime, &ctx, size, start, Vec::new());
        row = menu.iter().find_map(|shape| match shape {
            egui::Shape::Text(text) if text.galley.text() == "morning" => {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        });
        if row.is_some() {
            break;
        }
    }
    let row = row.expect("the spelling menu offers \"morning\"");
    let words: Vec<String> = menu
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect();
    for wanted in ["Ignore All", "Add to Dictionary"] {
        assert!(
            words.iter().any(|word| word == wanted),
            "{wanted} in the menu"
        );
    }
    frames(
        &mut runtime,
        &ctx,
        size,
        start,
        press(row, egui::PointerButton::Primary),
    );
    runtime.typing.flush(&mut runtime.engine.history);
    let after = document(&runtime);
    assert_eq!(after.0, "The morning sat\n");
    // Undo puts the word and its marks back; redo mends it again.
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(document(&runtime), before);
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(document(&runtime), after);
    // Tracked changes and a comment, in the marks' colors.
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: writer.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    command(&mut runtime, "review.track");
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![egui::Event::Text(" down".to_owned())],
        2 => vec![key(egui::Key::Home, egui::Modifiers::NONE)],
        3 => vec![key(egui::Key::Delete, egui::Modifiers::NONE)],
        _ => Vec::new(),
    });
    runtime.typing.flush(&mut runtime.engine.history);
    command(&mut runtime, "review.comment");
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![egui::Event::Text("Short?".to_owned())],
        2 => vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        _ => Vec::new(),
    });
    let drawn = step(&mut runtime, &ctx, size, start, Vec::new());
    let text = doc_text(&drawn, "The morning").expect("the text is drawn");
    let inserted = section_of(&text, "down");
    assert_eq!(inserted.color, marks.inserted, "inserted text");
    assert!(
        inserted.underline.width > 0.0,
        "inserted text is underlined"
    );
    let deleted = section_of(&text, "T");
    assert_eq!(deleted.color, marks.deleted, "deleted text");
    assert!(
        deleted.strikethrough.width > 0.0,
        "deleted text is struck through"
    );
    let commented = text
        .galley
        .job
        .sections
        .iter()
        .any(|section| section.format.background == marks.comment);
    assert!(commented, "the commented word lies on the comment's wash");
    // In Sue's monochrome themes the marks are told apart by shape, and
    // none of the old fixed colors is drawn anywhere.
    for color in colors(&drawn) {
        let rgb = [color.r(), color.g(), color.b()];
        assert!(
            !(color.a() > 0 && OLD_COLORS.contains(&rgb)),
            "{color:?} is one of the old fixed colors"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rich_text_touch_bar_buttons_name_their_keys() {
    let (mut runtime, root) = runtime("rich-text-touch-bar");
    let _ = runtime.engine.world.rice.set(
        "display.interface",
        sue_rice::RiceValue::Text("phone".to_owned()),
    );
    let ctx = egui::Context::default();
    let size = vec2(430.0, 932.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(
        ShellRequest::Create {
            applet: AppletId::new(sue_writer::ID).unwrap(),
            key: "blank".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![egui::Event::Text("The cat sat down".to_owned())]
        } else {
            Vec::new()
        }
    });
    runtime.typing.flush(&mut runtime.engine.history);
    let mut drawn = Vec::new();
    for _ in 0..30 {
        drawn = step(&mut runtime, &ctx, size, start, Vec::new());
    }
    let text = doc_text(&drawn, "The cat").expect("the text is drawn");
    let marks = PageMarks::resolve(runtime.shell.theme(), &Ink::paper());
    // A long press on "cat" selects it, and the handles and the bar show.
    let on = char_at(&text, 5);
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    let mut down = vec![
        touch(egui::TouchPhase::Start, on),
        egui::Event::PointerMoved(on),
        egui::Event::PointerButton {
            pos: on,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    for _ in 0..70 {
        step(&mut runtime, &ctx, size, start, std::mem::take(&mut down));
    }
    step(
        &mut runtime,
        &ctx,
        size,
        start,
        vec![
            touch(egui::TouchPhase::End, on),
            egui::Event::PointerButton {
                pos: on,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerGone,
        ],
    );
    let mut copy = None;
    let mut handles = false;
    for _ in 0..10 {
        let drawn = step(&mut runtime, &ctx, size, start, Vec::new());
        handles |= drawn.iter().any(
            |shape| matches!(shape, egui::Shape::Circle(circle) if circle.fill == marks.handle),
        );
        copy = drawn
            .iter()
            .find_map(|shape| match shape {
                egui::Shape::Text(text) if text.galley.text() == "Copy" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .or(copy);
    }
    assert!(handles, "the handles are drawn in {:?}", marks.handle);
    let copy = copy.expect("the bar shows Copy");
    // Resting on Copy shows its tip with its keys.
    let mut tip = false;
    step(
        &mut runtime,
        &ctx,
        size,
        start,
        vec![egui::Event::PointerMoved(copy)],
    );
    for _ in 0..80 {
        let drawn = step(&mut runtime, &ctx, size, start, Vec::new());
        tip |= drawn.iter().any(|shape| {
            matches!(shape, egui::Shape::Text(text) if text.galley.text().contains("Ctrl+C"))
        });
        if tip {
            break;
        }
    }
    assert!(tip, "Copy's tip names Ctrl+C");
    let _ = std::fs::remove_dir_all(&root);
}
