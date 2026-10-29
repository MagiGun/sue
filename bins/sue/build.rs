//! Gives `sue.exe` Sue's icon on Windows, so Explorer, shortcuts and the
//! taskbar show it. The icon's sizes are copied from `assets/icons/sue_icon.ico`
//! into a compiled resource file (`.res`: one icon resource per size and an
//! icon group pointing at them), which the MSVC linker takes as it is. Other
//! targets build without it.

use std::path::PathBuf;

/// A resource of type `kind` and id `name`, as a `.res` file holds it.
fn resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    let mut header = Vec::with_capacity(32);
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&32_u32.to_le_bytes());
    for ordinal in [kind, name] {
        header.extend_from_slice(&0xFFFF_u16.to_le_bytes());
        header.extend_from_slice(&ordinal.to_le_bytes());
    }
    header.extend_from_slice(&0_u32.to_le_bytes()); // data version
    header.extend_from_slice(&flags.to_le_bytes());
    header.extend_from_slice(&(if kind == 0 { 0_u16 } else { 0x0409 }).to_le_bytes());
    header.extend_from_slice(&0_u32.to_le_bytes()); // version
    header.extend_from_slice(&0_u32.to_le_bytes()); // characteristics
    out.extend_from_slice(&header);
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// The `.res` bytes for an `.ico` file: its images as icon resources 1 to
/// n and an icon group (id 1) listing them.
fn res_of(ico: &[u8]) -> Option<Vec<u8>> {
    let count = usize::from(u16::from_le_bytes([*ico.get(4)?, *ico.get(5)?]));
    let mut out = Vec::new();
    // Every .res file starts with an empty resource.
    resource(&mut out, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0_u16.to_le_bytes());
    group.extend_from_slice(&1_u16.to_le_bytes());
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for index in 0..count {
        let entry = ico.get(6 + index * 16..22 + index * 16)?;
        let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let image = ico.get(offset..offset.checked_add(size)?)?;
        let id = (index + 1) as u16;
        resource(&mut out, 3, id, 0x1010, image);
        group.extend_from_slice(&entry[..12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut out, 14, 1, 0x1030, &group);
    Some(out)
}

fn main() {
    let icon = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
        .join("../../assets/icons/sue_icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if !(windows && msvc) {
        return;
    }
    let Some(res) = std::fs::read(&icon).ok().and_then(|ico| res_of(&ico)) else {
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("sue.res");
    if std::fs::write(&out, res).is_ok() {
        println!("cargo:rustc-link-arg-bins={}", out.display());
    }
}
