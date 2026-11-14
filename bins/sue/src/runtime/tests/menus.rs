//! Menus driven by real frames: the keyboard in Sue Paint's menu bar and
//! in the Sue menu, and a menu taller than the window.

use super::*;

/// A frame of `size` points with `events`, at the context's next tick:
/// every piece of text it shows, each with the part of its rect that is
/// not clipped away.
fn shown(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    size: egui::Vec2,
    now: &mut Instant,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
        events,
        time: Some(ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    *now += Duration::from_millis(16);
    let at = *now;
    let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, at));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size())
                    .intersect(clipped.clip_rect);
                if rect.is_positive() {
                    found.push((text.galley.text().to_owned(), rect));
                }
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    found
}

fn has(texts: &[(String, egui::Rect)], text: &str) -> bool {
    texts.iter().any(|(found, _)| found == text)
}

/// Where `text` shows to the right of `of`: in a menu nested beside it.
fn beside(texts: &[(String, egui::Rect)], text: &str, of: egui::Rect) -> Option<egui::Rect> {
    texts
        .iter()
        .find(|(found, rect)| found == text && rect.left() > of.right())
        .map(|(_, rect)| *rect)
}

fn rect_in(texts: &[(String, egui::Rect)], text: &str) -> Option<egui::Rect> {
    texts
        .iter()
        .filter(|(found, _)| found == text)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

fn typed(text: &str) -> egui::Event {
    egui::Event::Text(text.to_owned())
}

fn plain(name: egui::Key) -> egui::Event {
    key(name, egui::Modifiers::NONE)
}

/// The label of the first row of `items` that runs a command.
fn first_action(items: &[sue_wm::MenuItem]) -> String {
    items
        .iter()
        .find(|item| matches!(item, sue_wm::MenuItem::Action { .. }))
        .map(|item| item.label().to_owned())
        .expect("a menu runs something")
}

/// Sue Paint's menu bar answers the keyboard, frame by frame: Right and
/// Left go from menu to menu, typing finds a row, Right opens a nested
/// menu and Left leaves it, Escape closes, Enter runs the row.
#[test]
fn sue_paint_menus_answer_the_keyboard() {
    let menus = sue_paint::menus();
    let edit_first = first_action(&menus[1].items);
    let duplicate = menus[0]
        .items
        .iter()
        .find_map(|item| match item {
            sue_wm::MenuItem::Sub { label, items } if label == "Save Duplicate" => {
                Some(first_action(items))
            }
            _ => None,
        })
        .expect("File has Save Duplicate");

    let mut paint = PaintDriver::new("menus-keyboard");
    paint.settle(1);
    paint.click_text("File");
    assert!(paint.rect_of("Save As...").is_some(), "File is open");
    assert!(paint.rect_of(&edit_first).is_none());

    // Right with no row chosen goes to the menu beside, Left comes back.
    paint.step(vec![plain(egui::Key::ArrowRight)]);
    paint.settle(3);
    assert!(paint.rect_of(&edit_first).is_some(), "Edit is open");
    assert!(paint.rect_of("Save As...").is_none(), "File closed");
    paint.step(vec![plain(egui::Key::ArrowLeft)]);
    paint.settle(3);
    assert!(paint.rect_of("Save As...").is_some(), "File again");

    // Typing finds Save Duplicate (the space bar sends a key and a typed
    // space, and inside a word it is part of the word); Right opens it,
    // Left closes it.
    let space = || vec![plain(egui::Key::Space), typed(" ")];
    for letter in ["s", "a", "v", "e", " ", "d"] {
        paint.step(if letter == " " {
            space()
        } else {
            vec![typed(letter)]
        });
    }
    assert!(
        paint.rect_of("Save As...").is_some(),
        "the space ran nothing"
    );
    paint.step(vec![plain(egui::Key::ArrowRight)]);
    paint.settle(3);
    let nested = paint
        .rect_of(&duplicate)
        .expect("Save Duplicate opened by Right");
    let row = paint.rect_of("Save Duplicate").unwrap();
    assert!(nested.left() > row.right(), "{nested:?} beside {row:?}");
    paint.step(vec![plain(egui::Key::ArrowLeft)]);
    paint.settle(3);
    assert!(paint.rect_of(&duplicate).is_none(), "Left closed it");
    assert!(paint.rect_of("Save As...").is_some(), "File stays open");

    // Once the typing has stopped for a moment, Space opens the row as
    // Enter does.
    paint.settle(70);
    paint.step(space());
    paint.settle(3);
    assert!(paint.rect_of(&duplicate).is_some(), "Space opened it");
    paint.step(vec![plain(egui::Key::ArrowLeft)]);
    paint.settle(3);
    assert!(paint.rect_of(&duplicate).is_none());

    // Escape closes the menu and nothing else: the canvas stays.
    paint.step(vec![plain(egui::Key::Escape)]);
    paint.settle(3);
    assert!(paint.rect_of("Save As...").is_none(), "Escape closed File");
    assert!(paint.rect_of("Draw").is_none(), "the canvas is still open");

    // Enter runs the row the typing found.
    paint.click_text("File");
    for letter in ["s", "t", "a", "r", "t"] {
        paint.step(vec![typed(letter)]);
    }
    paint.step(vec![plain(egui::Key::Enter)]);
    paint.settle(6);
    assert!(paint.rect_of("Save As...").is_none(), "Enter closed File");
    assert!(
        paint.rect_of("Draw").is_some(),
        "Enter ran Start Page: {:?}",
        paint.texts
    );
}

/// A menu taller than the window keeps its place under its title and
/// scrolls; nothing of it is cut off by the window's edge.
#[test]
fn a_menu_taller_than_the_window_scrolls_by_the_keyboard() {
    let size = vec2(1200.0, 420.0);
    let (mut runtime, root) = runtime("menus-tall");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::Enter)],
    );
    let mut texts = Vec::new();
    for _ in 0..60 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }

    // The menu with the most rows of its own.
    let menus = sue_paint::menus();
    let tall = menus
        .iter()
        .max_by_key(|menu| menu.items.len())
        .expect("Sue Paint has menus");
    let labels: Vec<&str> = tall
        .items
        .iter()
        .map(sue_wm::MenuItem::label)
        .filter(|label| !label.is_empty())
        .collect();
    let (first, last) = (labels[0], labels[labels.len() - 1]);
    let title = rect_in(&texts, &tall.title).expect("the title is in the bar");
    let at = title.center();
    for event in [
        vec![egui::Event::PointerMoved(at)],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        // The pointer leaves the bar, so that it lights no row.
        vec![egui::Event::PointerGone],
    ] {
        shown(&mut runtime, &ctx, size, &mut now, event);
    }
    for _ in 0..30 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    let screen = egui::Rect::from_min_size(Pos2::ZERO, size);
    let rows = |texts: &[(String, egui::Rect)]| -> Vec<egui::Rect> {
        texts
            .iter()
            .filter(|(text, _)| labels.contains(&text.as_str()))
            .map(|(_, rect)| *rect)
            .collect()
    };
    assert!(has(&texts, first), "the menu is open: {texts:?}");
    assert!(!has(&texts, last), "its last row is out of view");
    assert!(rows(&texts).iter().all(|rect| screen.contains_rect(*rect)));
    let top = rect_in(&texts, first).unwrap();
    assert!(top.top() > title.bottom(), "it stays under its title");

    // End scrolls to the last row; Home back to the first.
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::End)],
    );
    for _ in 0..3 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    assert!(has(&texts, last) && !has(&texts, first), "{texts:?}");
    assert!(rows(&texts).iter().all(|rect| screen.contains_rect(*rect)));
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::Home)],
    );
    for _ in 0..3 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    assert!(has(&texts, first) && !has(&texts, last));
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::Escape)],
    );
    for _ in 0..3 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    assert!(!has(&texts, first), "Escape closed it");
    let _ = std::fs::remove_dir_all(&root);
}

/// The Sue menu, opened by its button, answers the keyboard as the
/// studios' menus do.
#[test]
fn the_sue_menu_answers_the_keyboard() {
    let (mut runtime, root) = runtime("menus-sue");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    let mut now = start;
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let button = runtime.shell.sue_button();
    frames(&mut runtime, &ctx, size, start, click_at(button.center()));
    assert!(runtime.shell.sue_menu_open());
    let studios = runtime.shell.sue_menu_labels(&["Studios"]);
    assert!(!studios.is_empty(), "Studios lists the studios");

    // Typing finds Studios, Right opens it on its first row.
    for letter in ["s", "t", "u"] {
        shown(&mut runtime, &ctx, size, &mut now, vec![typed(letter)]);
    }
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::ArrowRight)],
    );
    let mut texts = Vec::new();
    for _ in 0..3 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    // The Hub shows the studios too: the nested menu's rows are the
    // ones at the right of the row that opened it.
    let row = rect_in(&texts, "Studios").unwrap();
    let nested = beside(&texts, &studios[0], row).expect("Studios is open");
    assert!(nested.top() < row.bottom(), "{nested:?} beside {row:?}");

    // Left leaves it; Left again has no menu beside to go to.
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::ArrowLeft)],
    );
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::ArrowLeft)],
    );
    for _ in 0..3 {
        texts = shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    }
    assert!(runtime.shell.sue_menu_open());
    assert!(beside(&texts, &studios[0], row).is_none(), "Studios closed");
    assert!(has(&texts, "Studios"));

    // Escape closes the Sue menu.
    shown(
        &mut runtime,
        &ctx,
        size,
        &mut now,
        vec![plain(egui::Key::Escape)],
    );
    shown(&mut runtime, &ctx, size, &mut now, Vec::new());
    assert!(!runtime.shell.sue_menu_open());
    let _ = std::fs::remove_dir_all(&root);
}
