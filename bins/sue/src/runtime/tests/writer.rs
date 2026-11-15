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
    // The notice that says the novel was made has its time, then the
    // studio comes in and egui's own fades (the notice's too) finish.
    step(&mut runtime, Vec::new(), 5.0);
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
    // Sue menu is Sue's own (the pin lives in Settings and on the Hub).
    runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
    let first = runtime.shell.sue_menu_labels(&[]);
    assert!(
        first.iter().any(|label| label == "Settings")
            && !first.iter().any(|label| label.ends_with("at Launch")),
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

/// Sue Writer at 1440 by 900 with a blank document open, driven a frame
/// at a time; what each frame drew is kept, so clicks find their place by
/// the text drawn there.
struct WriterDriver {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
    texts: Vec<(String, egui::Rect)>,
}

impl WriterDriver {
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        let now = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        runtime.request(
            ShellRequest::Create {
                applet: Self::id(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            now,
        );
        let mut driver = Self {
            runtime,
            ctx,
            now,
            root,
            texts: Vec::new(),
        };
        driver.settle(30);
        driver
    }

    fn id() -> AppletId {
        AppletId::new(sue_writer::ID).unwrap()
    }

    /// The home folder of the test's Sue, made.
    fn home(&self) -> PathBuf {
        let home = self.runtime.engine.catalog.folders["home"].clone();
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        self.texts = text_rects(&output);
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// Where the last frame drew `text`, the lowest first (a dialog sits
    /// over the page, the menu bar above it).
    fn rect_of(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
    }

    /// A click in the middle of `text`, found where the last frame drew it.
    fn click_text(&mut self, text: &str) {
        let at = self
            .rect_of(text)
            .unwrap_or_else(|| {
                let shown: Vec<&String> = self.texts.iter().map(|(text, _)| text).collect();
                panic!("{text:?} is on screen: {shown:?}")
            })
            .center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(4);
    }

    /// Runs one of Sue Writer's commands, as its menus do.
    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: Self::id(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.settle(4);
    }

    /// The system's file dialog answering `purpose` with `path`.
    fn picked(&mut self, purpose: &str, path: &Path, save: bool) {
        let pick = if save {
            FilePick::save(Some(Self::id()), purpose, "Save", "file")
        } else {
            FilePick::open(Some(Self::id()), purpose, "Open")
        };
        self.runtime.request(
            ShellRequest::Picked {
                pick,
                path: path.to_path_buf(),
            },
            &mut NoTextures,
            self.now,
        );
        self.settle(4);
    }

    fn undo(&mut self, redo: bool) {
        let modifiers = if redo {
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::COMMAND
        };
        self.step(vec![key(egui::Key::Z, modifiers)]);
        self.settle(4);
    }

    /// Every table of the arena: its id, its cells' text and heading rows.
    fn tables(&self) -> Vec<(sue_core::EntityId, Vec<Vec<String>>, Option<i64>)> {
        self.runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
            .map(|(_, entity)| {
                let table = entity.ext::<sue_model::Table>().unwrap();
                let (rows, columns) = table.size();
                let cells = (0..rows)
                    .map(|row| (0..columns).map(|column| table.text(row, column)).collect())
                    .collect();
                let heading = entity
                    .property(sue_model::HEADER_ROWS)
                    .and_then(sue_core::Value::as_i64);
                (entity.id(), cells, heading)
            })
            .collect()
    }

    /// Every document: id, text and marks, sorted by id.
    fn documents(&self) -> Vec<(sue_core::EntityId, String, sue_model::TextMarks)> {
        let mut docs: Vec<_> = self
            .runtime
            .engine
            .world
            .arena
            .iter()
            .filter(|(_, entity)| sue_model::manuscript::is_document(entity))
            .map(|(_, entity)| {
                (
                    entity.id(),
                    entity.string_buffer().unwrap().as_str().to_owned(),
                    entity
                        .ext::<sue_model::TextMarks>()
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        docs.sort_by_key(|(id, ..)| *id);
        docs
    }
}

impl Drop for WriterDriver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A CSV opened anywhere in Sue becomes a Sue Writer document holding the
/// table (the first row a heading row, as the dialog asked), and Table,
/// Convert, Export Table As writes it back byte for byte; as TSV too.
/// This is the Writer leg that replaces the Converter's tables.
#[test]
fn sue_writer_opens_a_csv_as_a_table_and_exports_it_back_equal() {
    let mut writer = WriterDriver::new("writer-csv");
    let csv = "Part,Code,Note\r\nBolt,007,\"M8, zinc\"\r\nNut,12,\"say \"\"hi\"\"\"\r\n";
    let path = writer.home().join("parts.csv");
    std::fs::write(&path, csv).unwrap();
    let docs_before = writer.documents().len();
    writer.runtime.open_path(&path, &mut NoTextures, writer.now);
    writer.settle(6);
    assert_eq!(writer.runtime.shell.page(), Some(&WriterDriver::id()));
    // The dialog asks about the heading row before anything is made.
    assert!(writer.tables().is_empty(), "nothing yet");
    assert!(
        writer.rect_of("The first row is a heading row").is_some(),
        "the question is asked"
    );
    writer.click_text("Open");
    let tables = writer.tables();
    assert_eq!(tables.len(), 1, "one table");
    let (table, cells, heading) = &tables[0];
    assert_eq!(cells[0], ["Part", "Code", "Note"]);
    assert_eq!(
        cells[1],
        ["Bolt", "007", "M8, zinc"],
        "text kept as written"
    );
    assert_eq!(cells[2][2], "say \"hi\"");
    assert_eq!(*heading, Some(1), "the first row is a heading row");
    // A document named after the file holds it (a blank project's empty
    // first document takes it, as any opened file).
    assert!(writer.documents().len() >= docs_before);
    assert!(
        writer.runtime.engine.world.arena.iter().any(|(_, entity)| {
            entity.name() == "parts"
                && entity.ext::<sue_model::TextMarks>().is_some_and(|marks| {
                    marks.marks().iter().any(|mark| {
                        mark.kind == sue_model::mark_kind::EMBED
                            && mark.value == sue_core::Value::Entity(*table)
                    })
                })
        }),
        "the document shows the table"
    );
    // The caret is in the table: Export Table As asks where, and writes.
    writer.command("table.export");
    let out = writer.home().join("parts-out.csv");
    writer.picked("table-export", &out, true);
    assert_eq!(
        std::fs::read(&out).unwrap(),
        csv.as_bytes(),
        "the CSV exports back equal: {:?}",
        writer.runtime.shell.message()
    );
    writer.command("table.export");
    let tsv = writer.home().join("parts-out.tsv");
    writer.picked("table-export", &tsv, true);
    assert_eq!(
        std::fs::read_to_string(&tsv).unwrap(),
        "Part\tCode\tNote\r\nBolt\t007\tM8, zinc\r\nNut\t12\t\"say \"\"hi\"\"\"\r\n",
        "in TSV a comma needs no quotes, a quote does"
    );
}

/// Table, Convert, Table from File puts a CSV's table at the caret as one
/// step: Undo takes it away, ids and all, and Redo brings it back. With
/// the box unticked the table has no heading row.
#[test]
fn sue_writer_puts_a_table_from_a_file_at_the_caret_with_one_undo_step() {
    let mut writer = WriterDriver::new("writer-table-file");
    writer.step(vec![egui::Event::Text("Before the table.".to_owned())]);
    writer.settle(4);
    writer
        .runtime
        .typing
        .flush(&mut writer.runtime.engine.history);
    let path = writer.home().join("sizes.tsv");
    std::fs::write(&path, "S\tM\n1\t2\n").unwrap();
    let before = writer.documents();
    let ids_before: Vec<sue_core::EntityId> = writer
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.id())
        .collect();
    writer.command("table.from_file");
    writer.picked("table-file", &path, false);
    assert!(
        writer.rect_of("Table from File").is_some(),
        "the dialog is titled for the insert"
    );
    writer.click_text("The first row is a heading row");
    writer.click_text("Insert");
    let tables = writer.tables();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].1, [["S", "M"], ["1", "2"]]);
    assert_eq!(tables[0].2, None, "the box was unticked: no heading row");
    let after = writer.documents();
    let (_, text, marks) = after
        .iter()
        .find(|(_, text, _)| text.contains("Before the table."))
        .unwrap();
    assert!(
        text.starts_with(&format!("Before the table.\n{}\n", sue_model::OBJECT)),
        "{text:?}"
    );
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::EMBED
                && mark.value == sue_core::Value::Entity(tables[0].0))
    );
    // One Undo: the arena is as it was, the ids too.
    writer.undo(false);
    assert_eq!(writer.documents(), before, "the text and marks are back");
    assert!(writer.tables().is_empty(), "the table is gone");
    let ids_now: Vec<sue_core::EntityId> = writer
        .runtime
        .engine
        .world
        .arena
        .iter()
        .map(|(_, entity)| entity.id())
        .collect();
    assert_eq!(ids_now, ids_before);
    // Redo: as after.
    writer.undo(true);
    assert_eq!(writer.documents(), after);
    assert_eq!(writer.tables(), tables);
}

/// File, Export to Plain text with CRLF line ends, spaces at line ends
/// dropped and tabs as four spaces, all set in the Export dialog by
/// clicks; Markdown keeps its tabs.
#[test]
fn sue_writer_exports_plain_text_with_crlf_and_tabs_as_four_spaces() {
    let mut writer = WriterDriver::new("writer-plain-text");
    writer.step(vec![egui::Event::Text("one\ttwo  ".to_owned())]);
    writer.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    writer.step(vec![egui::Event::Text("three".to_owned())]);
    writer.settle(4);
    writer
        .runtime
        .typing
        .flush(&mut writer.runtime.engine.history);
    writer.command("compile.open");
    assert!(
        writer.rect_of("Export").is_some(),
        "the dialog reads Export"
    );
    // Markdown offers the line endings only.
    writer.click_text("Markdown");
    assert!(
        writer.rect_of("LINE ENDINGS").is_some(),
        "{:?}",
        writer
            .texts
            .iter()
            .map(|(text, rect)| format!("{text} {rect:?}"))
            .collect::<Vec<_>>()
    );
    assert!(writer.rect_of("Drop spaces at line ends").is_none());
    writer.click_text("Plain text");
    writer.click_text("CRLF (Windows)");
    writer.click_text("Drop spaces at line ends");
    // Tabs as 4 spaces: the field takes the arrow keys while typed in.
    let field = writer
        .rect_of("Tabs as")
        .expect("the tabs row")
        .right_center()
        + vec2(30.0, 0.0);
    let press = |pressed| egui::Event::PointerButton {
        pos: field,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    writer.step(vec![egui::Event::PointerMoved(field)]);
    writer.step(vec![press(true)]);
    writer.step(vec![press(false)]);
    writer.settle(2);
    for _ in 0..4 {
        writer.step(vec![key(egui::Key::ArrowUp, egui::Modifiers::NONE)]);
    }
    writer.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    writer.settle(4);
    writer.click_text("Export...");
    let path = writer.home().join("notes.txt");
    writer.picked("compile.txt", &path, true);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("written: {:?}", writer.runtime.shell.message()));
    assert!(text.contains("one    two\r\nthree\r\n"), "{text:?}");
    assert!(!text.contains('\t'), "{text:?}");
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "CRLF only: {text:?}"
    );
    // The same choices on Markdown: CRLF, but the tab stays.
    let markdown = writer.home().join("notes.md");
    writer.picked("compile.md", &markdown, true);
    let text = std::fs::read_to_string(&markdown).unwrap();
    assert!(text.contains("one\ttwo"), "{text:?}");
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "CRLF only: {text:?}"
    );
}

/// Edit, Select All and Edit, Cut from the menu do what their keys do in
/// the text, and Undo brings the words back.
#[test]
fn sue_writers_edit_menu_selects_all_and_cuts() {
    let mut writer = WriterDriver::new("writer-edit-menu");
    writer.step(vec![egui::Event::Text("Cut me.".to_owned())]);
    writer.settle(4);
    writer
        .runtime
        .typing
        .flush(&mut writer.runtime.engine.history);
    let text = |writer: &WriterDriver| {
        writer
            .documents()
            .into_iter()
            .map(|(_, text, _)| text)
            .find(|text| !text.trim().is_empty() || text.is_empty())
            .unwrap_or_default()
    };
    assert!(text(&writer).contains("Cut me."));
    writer.command("edit.select_all");
    writer.command("edit.cut");
    writer
        .runtime
        .typing
        .flush(&mut writer.runtime.engine.history);
    assert!(!text(&writer).contains("Cut me."), "{:?}", text(&writer));
    writer.undo(false);
    assert!(text(&writer).contains("Cut me."), "{:?}", text(&writer));
}

/// Help, About Sue Writer says what it is; the name 0.0.75 gave Back to
/// the Hub still goes there.
#[test]
fn sue_writer_shows_about_and_goes_back_to_the_hub() {
    let mut writer = WriterDriver::new("writer-about");
    writer.command("help.about");
    assert!(writer.rect_of("About Sue Writer").is_some());
    writer.click_text("Close");
    assert!(writer.rect_of("About Sue Writer").is_none());
    writer.command("studio.leave");
    assert_ne!(writer.runtime.shell.page(), Some(&WriterDriver::id()));
}
