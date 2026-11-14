//! Old files still load: the `.clip` Sue Paint 0.0.75 saves.
//!
//! `tests/golden/picture.clip` was written once by Sue 0.0.75 (commit
//! `823b060`, `sue_paint::clip_io::encode`, by the generator in
//! `crates/sue-paint/tests/golden.rs`) and is never written again. It holds
//! a procedural picture as Clip Studio's layers (a folder, a clipped and
//! multiplied layer, a hidden one, a vector layer as pixels) and the
//! picture's own Sue project inside. `picture.clip.txt` beside it says
//! what the file holds, in words that do not depend on how it is encoded.
//!
//! A test that fails here means the new code no longer reads a 0.0.75
//! file the way 0.0.75 wrote it. That is a bug in the new code: fix the
//! reader, never the files.

use std::fmt::Write as _;
use std::path::PathBuf;

use sue_core::SueFile;
use sue_io::clip::{Clip, decode_clip, encode_clip};
use sue_io::psd::PsdKind;

/// Every committed fixture of this crate and its SHA-256. A fixture is
/// read, never rewritten (rule 12 of the 0.0.8 plan): a change of one byte
/// fails here before anything else is looked at.
const LEDGER: &[(&str, &str)] = &[
    (
        "picture.clip",
        "4850ecf975c972fcde1ec4078007203367179855d7c9046b547f465f238150e0",
    ),
    (
        "picture.clip.txt",
        "8dc0ada1efbe9ecf13b1932a6ca404819ce0d8e4a263dbec27a91b32172c379a",
    ),
];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn golden(name: &str) -> Vec<u8> {
    let path = golden_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// SHA-256 of `data`, as lowercase hex.
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for at in 16..64 {
            let (a, b) = (words[at - 15], words[at - 2]);
            let small = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let large = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            words[at] = words[at - 16]
                .wrapping_add(small)
                .wrapping_add(words[at - 7])
                .wrapping_add(large);
        }
        let mut v = state;
        for (constant, word) in K.iter().zip(words) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let choose = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(*constant)
                .wrapping_add(word);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(majority);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (sum, value) in state.iter_mut().zip(v) {
            *sum = sum.wrapping_add(value);
        }
    }
    state.iter().fold(String::new(), |mut out, word| {
        let _ = write!(out, "{word:08x}");
        out
    })
}

/// FNV-1a of `bytes`: short enough to read in a description, and enough
/// to tell one layer's pixels from another's.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The layers of `clip` in words, bottom first, without the project it
/// carries: what Clip Studio is given.
fn describe_layers(clip: &Clip) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "canvas {} by {} at {} pixels per inch",
        clip.psd.width, clip.psd.height, clip.dpi
    );
    let _ = writeln!(out, "layers {}, bottom first", clip.psd.layers.len());
    for layer in &clip.psd.layers {
        let kind = match layer.kind {
            PsdKind::Pixels => "pixels",
            PsdKind::FolderOpen { open: true } => "folder, shown open",
            PsdKind::FolderOpen { open: false } => "folder, shown closed",
            PsdKind::FolderEnd => "end of folder",
        };
        let painted = layer
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] != 0)
            .count();
        let _ = writeln!(
            out,
            "layer {:?} {kind}: at {} {}, {} by {}, opacity {}, visible {}, clipped {}, blend \
             {:?}, {painted} not clear, fnv {:016x}",
            layer.name,
            layer.left,
            layer.top,
            layer.width,
            layer.height,
            layer.opacity,
            layer.visible,
            layer.clip,
            String::from_utf8_lossy(&layer.blend),
            fnv(&layer.pixels)
        );
    }
    out
}

/// The whole file in words: the layers, then the Sue project inside with
/// its entities by id.
fn describe(clip: &Clip) -> String {
    let mut out = String::from("# picture.clip, as Sue 0.0.75 wrote it\n");
    out.push_str(&describe_layers(clip));
    let Some(sue) = &clip.sue else {
        out.push_str("no project inside\n");
        return out;
    };
    let file = SueFile::from_bytes(sue).expect("the project inside reads");
    let chunks: Vec<String> = file
        .chunks()
        .iter()
        .map(|chunk| format!("{} version {}", chunk.tag_name(), chunk.version))
        .collect();
    let _ = writeln!(
        out,
        "project inside: {} bytes, sha256 {}, chunks {}",
        sue.len(),
        sha256(sue),
        chunks.join(", ")
    );
    let arena = file
        .arena(&sue_model::registry())
        .expect("the project's arena reads");
    let mut entities: Vec<&sue_core::Entity> = arena.iter().map(|(_, entity)| entity).collect();
    entities.sort_by_key(|entity| entity.id());
    let _ = writeln!(out, "entities {}", entities.len());
    for entity in entities {
        let pixels = entity
            .component(sue_core::ComponentKind::RasterTile2D)
            .and_then(sue_core::ComponentData::as_raster)
            .and_then(sue_core::RasterTile2D::bytes)
            .map_or_else(String::new, |bytes| format!(", fnv {:016x}", fnv(bytes)));
        let _ = writeln!(
            out,
            "entity {} class {:?} name {:?}{pixels}",
            entity.id(),
            entity.class(),
            entity.name()
        );
    }
    let mut relations: Vec<sue_core::Relation> = arena.relations().iter().collect();
    relations
        .sort_by(|a, b| (a.from, &a.kind, &a.order, a.to).cmp(&(b.from, &b.kind, &b.order, b.to)));
    let _ = writeln!(out, "relations {}", relations.len());
    for relation in relations {
        let _ = writeln!(
            out,
            "relation {} {} {}",
            relation.from, relation.kind, relation.to
        );
    }
    out
}

/// Fails with the first line that differs, which says more than two long
/// texts side by side.
fn assert_same_text(found: &str, expected: &str, what: &str) {
    if found == expected {
        return;
    }
    let (mut left, mut right) = (found.lines(), expected.lines());
    let mut line = 1;
    loop {
        match (left.next(), right.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (a, b) => panic!(
                "{what}: line {line} differs\n  found:    {}\n  expected: {}",
                a.unwrap_or("(nothing)"),
                b.unwrap_or("(nothing)")
            ),
        }
    }
}

#[test]
fn sha256_is_the_real_one() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(&[b'a'; 119]),
        "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb"
    );
}

#[test]
fn every_golden_file_is_the_one_committed() {
    let mut found: Vec<String> = std::fs::read_dir(golden_dir())
        .expect("tests/golden is there")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    let mut listed: Vec<&str> = LEDGER.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();
    assert_eq!(found, listed, "tests/golden and the ledger hold the same");
    for (name, sum) in LEDGER {
        assert_eq!(
            sha256(&golden(name)),
            *sum,
            "{name} changed: golden files are read, never rewritten"
        );
    }
}

#[test]
fn a_clip_from_0_0_75_loads_to_the_same_layers_pixels_and_project() {
    let bytes = golden("picture.clip");
    assert!(sue_io::clip::sniff(&bytes), "it is known as a .clip");
    let clip = decode_clip(&bytes).expect("a .clip Sue 0.0.75 wrote reads");
    let expected = String::from_utf8(golden("picture.clip.txt")).expect("the description is UTF-8");
    assert_same_text(&describe(&clip), &expected, "picture.clip");
}

#[test]
fn a_clip_from_0_0_75_saved_again_holds_the_same() {
    let clip = decode_clip(&golden("picture.clip")).expect("a .clip Sue 0.0.75 wrote reads");
    let again =
        encode_clip(&clip.psd, clip.dpi, clip.sue.as_deref()).expect("what was read can be saved");
    let again = decode_clip(&again).expect("what this build saved reads");
    let expected = String::from_utf8(golden("picture.clip.txt")).expect("the description is UTF-8");
    assert_same_text(&describe(&again), &expected, "picture.clip, saved again");
}

/// Writes `picture.clip.txt` for the `picture.clip` that Sue Paint's
/// generator left in the temporary folder. Run once at `823b060`, after
/// that generator: `cargo test -p sue-io --test golden -- --ignored`.
/// The files are the ones committed; they are never made again.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_description() {
    let dir = std::env::temp_dir().join("sue-golden").join("sue-io");
    let bytes = std::fs::read(dir.join("picture.clip"))
        .expect("run Sue Paint's generate_the_clip first: it writes picture.clip");
    let clip = decode_clip(&bytes).expect("what was written reads");
    let words = describe(&clip);
    std::fs::write(dir.join("picture.clip.txt"), &words).expect("the file can be written");
    println!("wrote picture.clip.txt to {}", dir.display());
    println!("picture.clip {}", sha256(&bytes));
    println!("picture.clip.txt {}", sha256(words.as_bytes()));
}
