//! The tools that are not studios (the Converter).

use super::*;

#[test]
fn the_converter_turns_a_picture_a_song_and_text_into_new_files() {
    let (mut runtime, root) = runtime("convert");
    std::fs::create_dir_all(&root).unwrap();
    let picture = root.join("card.png");
    let pixels = [40_u8, 80, 120, 255].repeat(6 * 4);
    std::fs::write(&picture, sue_io::encode_png_rgba(6, 4, &pixels).unwrap()).unwrap();
    let song = root.join("tone.wav");
    let pcm = sue_core::PCMFloat1D::new(8_000, 1, vec![0.2; 4_000]).unwrap();
    std::fs::write(&song, sue_io::encode_wav(&pcm)).unwrap();
    let notes = root.join("notes.md");
    std::fs::write(&notes, "# Hello\n\nSome *words*.\n").unwrap();
    let mut now = Instant::now();
    let convert = AppletId::new(crate::convert::ID).unwrap();
    // A tool, with a page of its own like a studio's.
    assert_eq!(
        runtime
            .applets
            .manifest(&convert)
            .map(|manifest| manifest.role),
        Some(sue_wm::AppletRole::Tool)
    );
    runtime.request(ShellRequest::Open(convert.clone()), &mut NoTextures, now);
    assert_eq!(runtime.shell.page(), Some(&convert));
    for path in [&picture, &song, &notes] {
        runtime.request(
            ShellRequest::SendFile {
                to: convert.clone(),
                purpose: "add".to_owned(),
                path: path.clone(),
            },
            &mut NoTextures,
            now,
        );
    }
    // The usual targets: a PNG stays PNG (a new file beside it), the
    // sound becomes FLAC and the text is tidied.
    runtime.request(
        ShellRequest::AppletCommand {
            applet: convert,
            command: "convert.run".to_owned(),
        },
        &mut NoTextures,
        now,
    );
    let outputs = [
        root.join("card (2).png"),
        root.join("tone.flac"),
        root.join("notes.txt"),
    ];
    for _ in 0..400 {
        if outputs.iter().all(|path| path.is_file()) {
            break;
        }
        runtime.timer(&mut NoTextures, now);
        now += Duration::from_millis(50);
        std::thread::sleep(Duration::from_millis(5));
    }
    for path in &outputs {
        assert!(path.is_file(), "{} was written", path.display());
    }
    let (flac, _) =
        sue_io::decode_audio(std::fs::read(&outputs[1]).unwrap(), Some("flac")).unwrap();
    assert_eq!(flac.pcm.frames(), 4_000);
    assert!(
        std::fs::read(&picture).unwrap().starts_with(b"\x89PNG"),
        "the source is untouched"
    );
    let _ = std::fs::remove_dir_all(&root);
}
