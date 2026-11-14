//! Sue Writer: typing, the caret, changes, comments, tables.

use super::*;

/// One frame of `size` with `events`, at `now`.
#[test]
fn a_finger_drag_scrolls_sue_writers_page_on_a_phone() {
    let (mut runtime, root) = runtime("writer-touch");
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
            key: "report".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // One frame; where the "Summary" heading was drawn, if it was.
    let mut now = start;
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        now += Duration::from_millis(16);
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        let mut found = None;
        let mut selected = false;
        let mut bar = false;
        let mut tall = 0.0;
        for clipped in &output.shapes {
            let mut look = |shape: &egui::Shape| {
                if let egui::Shape::Text(text) = shape {
                    let words = text.galley.text();
                    if words.contains("Summary") {
                        found = Some(text.pos.y);
                    }
                    tall = f32::max(tall, text.galley.size().y);
                    bar |= words == "Copy";
                    // The status bar counts what is selected.
                    selected |= words
                        .split('|')
                        .any(|part| part.contains(" of ") && part.contains("words"));
                }
            };
            match &clipped.shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
                shape => look(shape),
            }
        }
        output.drop_without_applying_deltas();
        (found, selected, bar, tall)
    };
    let mut before = None;
    for _ in 0..200 {
        before = step(&mut runtime, Vec::new()).0.or(before);
    }
    let before = before.expect("the report's Summary heading is drawn");
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    let from = Pos2::new(215.0, 600.0);
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Start, from),
            egui::Event::PointerMoved(from),
            egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let mut at = from;
    for _ in 0..10 {
        at.y -= 12.0;
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ],
        );
    }
    // The finger rests before it lifts: no flick.
    for _ in 0..10 {
        step(&mut runtime, Vec::new());
    }
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::End, at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerGone,
        ],
    );
    let mut after = None;
    let mut selected = false;
    for _ in 0..5 {
        let (found, any, _, _) = step(&mut runtime, Vec::new());
        after = found.or(after);
        selected |= any;
    }
    let after = after.expect("still drawn");
    assert!(
        (before - after - 120.0).abs() < 1.0,
        "the page moves as far as the finger, 120 points: {before} then {after}"
    );
    assert!(!selected, "a scroll selects nothing");
    // A long press on the heading selects it, and the finger's drag
    // grows the selection.
    let on = Pos2::new(90.0, after + 8.0);
    let mut press = vec![
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
        step(&mut runtime, std::mem::take(&mut press));
    }
    let to = on + vec2(0.0, 60.0);
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Move, to),
            egui::Event::PointerMoved(to),
        ],
    );
    // A phone's lift takes the pointer away too.
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::End, to),
            egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerGone,
        ],
    );
    let (_, selected, _, _) = step(&mut runtime, Vec::new());
    assert!(selected, "a long press and a drag select");
    // The bar over the selection: Cut, Copy, Paste, Select All.
    let (_, _, bar, tall) = step(&mut runtime, Vec::new());
    assert!(bar, "the Copy bar shows over the selection");
    // Two fingers spread apart zoom the page in.
    let finger = |id, phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force: None,
    };
    let (mut a, mut b) = (Pos2::new(160.0, 400.0), Pos2::new(260.0, 400.0));
    step(
        &mut runtime,
        vec![
            finger(1, egui::TouchPhase::Start, a),
            egui::Event::PointerMoved(a),
            egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            finger(2, egui::TouchPhase::Start, b),
        ],
    );
    for _ in 0..15 {
        a.x -= 5.0;
        b.x += 5.0;
        step(
            &mut runtime,
            vec![
                finger(1, egui::TouchPhase::Move, a),
                egui::Event::PointerMoved(a),
                finger(2, egui::TouchPhase::Move, b),
            ],
        );
    }
    step(
        &mut runtime,
        vec![
            finger(1, egui::TouchPhase::End, a),
            finger(2, egui::TouchPhase::End, b),
            egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerGone,
        ],
    );
    let mut zoomed = 0.0;
    for _ in 0..3 {
        let (_, _, _, now) = step(&mut runtime, Vec::new());
        if now > 0.0 {
            zoomed = now;
        }
    }
    assert!(
        zoomed > tall * 1.5,
        "a pinch zooms the page in: its tallest text was {tall}, then {zoomed}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sue_writers_caret_glides_after_typing_then_asks_for_no_frames() {
    let (mut runtime, root) = runtime("smooth-caret");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(
        ShellRequest::Create {
            applet: AppletId::new(sue_writer::ID).unwrap(),
            key: "novel".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // One frame `dt` seconds after the last; how soon egui wants the next.
    let mut now = start;
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>, dt: f64| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(ctx.input(|input| input.time) + dt),
            ..egui::RawInput::default()
        };
        now += Duration::from_secs_f64(dt);
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        let delay = output
            .viewport_output
            .values()
            .next()
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        output.drop_without_applying_deltas();
        delay
    };
    // Let the studio come in and egui's own fades finish.
    for _ in 0..300 {
        step(&mut runtime, Vec::new(), 0.008);
    }
    let typed = |text: &str| vec![egui::Event::Text(text.to_owned())];

    // A word typed: egui answers any input with a frame at once; the
    // gliding caret keeps them coming for its 0.1 s, and no longer.
    assert_eq!(step(&mut runtime, typed("Once"), 0.008), Duration::ZERO);
    let mut gliding = 0;
    while step(&mut runtime, Vec::new(), 0.008) == Duration::ZERO {
        gliding += 1;
        assert!(gliding < 100, "the caret never lands");
    }
    assert!((9..=15).contains(&gliding), "{gliding} frames");
    // At rest only its blink wakes egui, later.
    let blink = step(&mut runtime, Vec::new(), 0.008);
    assert!(
        blink > Duration::from_millis(200) && blink <= Duration::from_millis(530),
        "{blink:?}"
    );
    // After a while it rests lit, and nothing asks for frames.
    step(&mut runtime, Vec::new(), sue_type::caret::BLINK_FOR + 1.0);
    let idle = step(&mut runtime, Vec::new(), 0.008);
    assert!(
        idle > Duration::from_secs(60),
        "egui asks for frames every {idle:?} ({:?})",
        ctx.repaint_causes()
    );

    // Smooth Caret off in Settings: the caret jumps, no frames to glide.
    runtime
        .engine
        .world
        .rice
        .set(sue_wm::SMOOTH_CARET_KEY, sue_rice::RiceValue::Bool(false))
        .unwrap();
    step(&mut runtime, Vec::new(), 0.008);
    assert_eq!(step(&mut runtime, typed(" upon"), 0.008), Duration::ZERO);
    // (egui still draws a frame or two as the words count goes up.)
    let mut jumped = 0;
    while step(&mut runtime, Vec::new(), 0.008) == Duration::ZERO {
        jumped += 1;
        assert!(jumped < 100);
    }
    assert!(
        jumped <= 2 && gliding >= jumped + 8,
        "{jumped} and {gliding}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_writer_makes_a_novel_types_undoes_takes_a_picture_and_compiles() {
    let (mut runtime, root) = runtime("sue-writer");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let writer = AppletId::new(sue_writer::ID).unwrap();
    // A novel from the Sue menu's New: made and opened.
    runtime.request(
        ShellRequest::Create {
            applet: writer.clone(),
            key: "novel".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    assert_eq!(runtime.shell.page(), Some(&writer));
    let arena = &runtime.engine.world.arena;
    let novel = arena
        .iter()
        .find(|(_, entity)| sue_model::manuscript::is_manuscript(entity))
        .map(|(_, entity)| entity.id())
        .expect("a manuscript");
    let draft =
        sue_model::manuscript::folder(arena, novel, sue_model::manuscript::role::DRAFT).unwrap();
    let scene = sue_model::manuscript::walk(arena, draft)
        .into_iter()
        .map(|(id, _)| id)
        .find(|id| {
            arena
                .get_by_id(*id)
                .is_ok_and(sue_model::manuscript::is_document)
        })
        .unwrap();
    let text = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .get_by_id(scene)
            .unwrap()
            .string_buffer()
            .unwrap()
            .as_str()
            .to_owned()
    };

    // Typing goes into the first scene (the editor has the keyboard).
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![egui::Event::Text("It was a dark night.".to_owned())]
        } else {
            Vec::new()
        }
    });
    assert_eq!(text(&runtime), "It was a dark night.\n");
    // A pause ends the burst of typing.
    runtime.typing.flush(&mut runtime.engine.history);
    // Ctrl+B with nothing selected: what is typed next is bold.
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![key(egui::Key::B, egui::Modifiers::COMMAND)],
        2 => vec![egui::Event::Text(" Bold.".to_owned())],
        _ => Vec::new(),
    });
    assert_eq!(text(&runtime), "It was a dark night. Bold.\n");
    let marks = runtime
        .engine
        .world
        .arena
        .get_by_id(scene)
        .unwrap()
        .ext::<sue_model::TextMarks>()
        .cloned()
        .unwrap();
    assert!(
        marks.covers(21..26, sue_model::mark_kind::BOLD),
        "{marks:?}"
    );
    // Undo takes the burst back, its formatting too.
    runtime.typing.flush(&mut runtime.engine.history);
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(text(&runtime), "It was a dark night.\n");

    // A Sue Paint picture sent to Sue Writer goes into the text.
    let picture = runtime
        .with_context(&mut NoTextures, start, |cx| {
            sue_paint::sample_picture(cx, 64, 48)
        })
        .unwrap();
    runtime.request(
        ShellRequest::SendEntity {
            to: writer.clone(),
            purpose: "figure".to_owned(),
            entity: picture,
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let marks = runtime
        .engine
        .world
        .arena
        .get_by_id(scene)
        .unwrap()
        .ext::<sue_model::TextMarks>()
        .cloned()
        .unwrap();
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::EMBED
                && mark.value == sue_core::Value::Entity(picture)),
        "the picture is in the text"
    );

    // Sue Writer's menus sit in the top bar after the Sue button; the
    // Sue menu offers the pin.
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    let first = runtime.shell.sue_menu_labels(&[]);
    assert!(
        first
            .iter()
            .any(|label| label == "Open Sue Writer at Launch"),
        "{first:?}"
    );
    runtime.shell.dismiss_overlays();

    // Compile to Markdown and to PDF, as the system's dialog answers.
    for (format, ext) in [("md", "md"), ("pdf", "pdf"), ("docx", "docx")] {
        let home = runtime.engine.catalog.folders["home"].clone();
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join(format!("novel.{ext}"));
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(writer.clone()),
                    &format!("compile.{format}"),
                    "Export",
                    "novel",
                ),
                path: path.clone(),
            },
            &mut NoTextures,
            start,
        );
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|_| panic!("{format} was written: {:?}", runtime.shell.message()));
        if format == "md" {
            let markdown = String::from_utf8(bytes).unwrap();
            assert!(markdown.contains("Chapter 1"), "{markdown}");
            assert!(markdown.contains("It was a dark night."));
        } else if format == "pdf" {
            assert!(bytes.starts_with(b"%PDF"));
        }
    }
    // Opening a Word file anywhere in Sue brings it into Sue Writer.
    let docs = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::manuscript::is_document(entity))
            .count()
    };
    let before = docs(&runtime);
    let word = runtime.engine.catalog.folders["home"].join("novel.docx");
    runtime.open_path(&word, &mut NoTextures, start);
    assert_eq!(
        docs(&runtime),
        before + 1,
        "the Word file is a document now"
    );
    assert!(
        runtime.engine.world.arena.iter().any(|(_, entity)| entity
            .string_buffer()
            .is_some_and(|text| text.as_str().contains("It was a dark night."))
            && entity.name() == "novel"),
        "with its text"
    );
    assert_eq!(runtime.shell.page(), Some(&writer));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_writer_changes_case_keeping_formatting_and_undoes_it() {
    let (mut runtime, root) = runtime("sue-writer-case");
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
            vec![egui::Event::Text("the storm came".to_owned())]
        } else {
            Vec::new()
        }
    });
    runtime.typing.flush(&mut runtime.engine.history);
    let text = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .find_map(|(_, entity)| {
                entity
                    .string_buffer()
                    .map(|text| text.as_str().to_owned())
                    .filter(|text| text.contains("storm"))
            })
            .unwrap_or_default()
    };
    assert_eq!(text(&runtime), "the storm came\n");
    // Nothing selected: the word at the caret changes.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: writer.clone(),
            command: "format.case.upper".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    assert_eq!(text(&runtime), "the storm CAME\n");
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }]
        } else {
            Vec::new()
        }
    });
    assert_eq!(text(&runtime), "the storm came\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_writer_records_changes_and_accepts_them() {
    let (mut runtime, root) = runtime("sue-writer-changes");
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
            vec![egui::Event::Text("The cat".to_owned())]
        } else {
            Vec::new()
        }
    });
    runtime.typing.flush(&mut runtime.engine.history);
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
    let key = |key: egui::Key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let doc = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .find_map(|(_, entity)| {
                let text = entity.string_buffer()?.as_str().to_owned();
                text.starts_with("The")
                    .then(|| (text, entity.ext::<sue_model::TextMarks>().cloned()))
            })
            .unwrap()
    };
    command(&mut runtime, "review.track");
    // Typed while recording: inserted. Backspace takes an insertion out
    // for good, then strikes "t" of the old text through.
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![egui::Event::Text("s sat".to_owned())],
        2 => vec![key(egui::Key::Backspace), key(egui::Key::Backspace)],
        3 => vec![key(egui::Key::Home)],
        4 => vec![key(egui::Key::Delete)],
        _ => Vec::new(),
    });
    let (text, marks) = doc(&runtime);
    let marks = marks.unwrap();
    assert_eq!(text, "The cats s\n", "{marks:?}");
    let changes = sue_model::changes::changes(&marks);
    assert!(
        changes
            .iter()
            .any(|tracked| tracked.inserted && tracked.range == (7..10))
    );
    assert!(
        changes
            .iter()
            .any(|tracked| !tracked.inserted && tracked.range == (0..1))
    );
    assert_eq!(changes[0].change.author, "Author");
    // Accept All keeps the insertion and drops the struck "T".
    command(&mut runtime, "review.accept_all");
    let (text, marks) = doc_any(&runtime);
    assert_eq!(text, "he cats s\n");
    assert!(sue_model::changes::changes(&marks).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_writer_comments_go_on_words_and_come_off_with_undo() {
    let (mut runtime, root) = runtime("sue-writer-comments");
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
            vec![egui::Event::Text("A dark and stormy night".to_owned())]
        } else {
            Vec::new()
        }
    });
    runtime.typing.flush(&mut runtime.engine.history);
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
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    // New Comment on the word at the caret, typed and sent with
    // Ctrl+Enter.
    command(&mut runtime, "review.comment");
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![egui::Event::Text("Too much?".to_owned())],
        2 => vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        _ => Vec::new(),
    });
    let comments = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| entity.class() == sue_model::comments::COMMENT)
            .map(|(_, entity)| entity.id())
            .collect::<Vec<_>>()
    };
    let found = comments(&runtime);
    assert_eq!(found.len(), 1, "a comment was made");
    let thread = sue_model::comments::thread(&runtime.engine.world.arena, found[0]).unwrap();
    assert_eq!(thread.first.text, "Too much?");
    let marked = runtime.engine.world.arena.iter().any(|(_, entity)| {
        entity.ext::<sue_model::TextMarks>().is_some_and(|marks| {
            marks.marks().iter().any(|mark| {
                mark.kind == sue_model::mark_kind::COMMENT
                    && mark.value == sue_core::Value::Entity(found[0])
            })
        })
    });
    assert!(marked, "its words carry it");
    // Delete it from the Review menu; undo brings it back.
    command(&mut runtime, "review.delete_comment");
    assert!(comments(&runtime).is_empty());
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(comments(&runtime), found);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_writer_tables_take_typing_grow_with_tab_undo_and_export() {
    let (mut runtime, root) = runtime("sue-writer-tables");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let writer = AppletId::new(sue_writer::ID).unwrap();
    runtime.request(
        ShellRequest::Create {
            applet: writer.clone(),
            key: "novel".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
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
    command(&mut runtime, "table.insert.2x2");
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let table_of = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
            .and_then(|(_, entity)| entity.ext::<sue_model::Table>().cloned())
            .expect("a table in the arena")
    };
    assert_eq!(table_of(&runtime).size(), (2, 2));
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    // Typing goes into the first cell; Tab moves on, and Tab in the last
    // cell adds a row.
    frames(&mut runtime, &ctx, size, start, |frame| match frame {
        1 => vec![egui::Event::Text("Name".to_owned())],
        2 => vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        3 => vec![egui::Event::Text("Qty".to_owned())],
        4 => vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        5 => vec![egui::Event::Text("Bolt".to_owned())],
        6 => vec![
            key(egui::Key::Tab, egui::Modifiers::NONE),
            key(egui::Key::Tab, egui::Modifiers::NONE),
        ],
        _ => Vec::new(),
    });
    let table = table_of(&runtime);
    assert_eq!(table.text(0, 0), "Name");
    assert_eq!(table.text(0, 1), "Qty");
    assert_eq!(table.text(1, 0), "Bolt");
    assert_eq!(table.size(), (3, 2), "Tab in the last cell adds a row");
    // Undo takes the new row back, then the typing in the cell before.
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(table_of(&runtime).size(), (2, 2));
    frames(&mut runtime, &ctx, size, start, |frame| {
        if frame == 1 {
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
        } else {
            Vec::new()
        }
    });
    assert_eq!(
        table_of(&runtime).text(1, 0),
        "",
        "{:?}",
        table_of(&runtime)
    );
    assert_eq!(table_of(&runtime).text(0, 1), "Qty");
    // A column after the caret's, from the Table menu.
    command(&mut runtime, "table.column_right");
    assert_eq!(table_of(&runtime).size(), (2, 3));
    // The table goes out in a web page and a Word file.
    for (format, needle) in [("html", "<td>"), ("docx", ""), ("odt", "")] {
        let home = runtime.engine.catalog.folders["home"].clone();
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join(format!("tables.{format}"));
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(writer.clone()),
                    &format!("compile.{format}"),
                    "Export",
                    "tables",
                ),
                path: path.clone(),
            },
            &mut NoTextures,
            start,
        );
        let bytes = std::fs::read(&path).unwrap();
        if format == "html" {
            let html = String::from_utf8(bytes).unwrap();
            assert!(html.contains(needle) && html.contains("Qty"), "{html}");
        } else {
            assert!(bytes.starts_with(b"PK"));
        }
    }
    // The Word file opens with its table a table again.
    let tables = |runtime: &Runtime| {
        runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
            .count()
    };
    let before = tables(&runtime);
    let word = runtime.engine.catalog.folders["home"].join("tables.docx");
    runtime.open_path(&word, &mut NoTextures, start);
    assert_eq!(
        tables(&runtime),
        before + 1,
        "the Word file's table came back"
    );
    // So does an OpenDocument file's.
    let odt = runtime.engine.catalog.folders["home"].join("tables.odt");
    runtime.open_path(&odt, &mut NoTextures, start);
    assert_eq!(
        tables(&runtime),
        before + 2,
        "the OpenDocument table came back"
    );
    let _ = std::fs::remove_dir_all(&root);
}
