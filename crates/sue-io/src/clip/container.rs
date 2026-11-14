//! The chunk file around a `.clip`: the `CSFCHUNK` header, the chunks in
//! it, and the big-endian numbers and UTF-16 names everything inside is
//! spelled with.

use std::collections::HashMap;

pub(super) fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

pub(super) fn be64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

pub(super) fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// `true` if `head` starts a Clip Studio file.
#[must_use]
pub fn sniff(head: &[u8]) -> bool {
    head.starts_with(b"CSFCHUNK")
}

/// The chunks of a `.clip`: the `Exta` payloads by id, and the database.
/// A `.clip`'s bitmap chunks by id.
pub(super) type Bitmaps<'a> = HashMap<String, &'a [u8]>;

pub(super) fn chunks(bytes: &[u8]) -> Result<(Bitmaps<'_>, &[u8]), String> {
    if !sniff(bytes) {
        return Err("not a Clip Studio file".to_owned());
    }
    let mut at = usize::try_from(be64(bytes, 16).ok_or("the file is cut short")?)
        .map_err(|_| "the file header is broken")?;
    let mut exta = HashMap::new();
    let mut database = None;
    while at + 16 <= bytes.len() {
        if &bytes[at..at + 4] != b"CHNK" {
            return Err(format!("no chunk where one should start, at byte {at}"));
        }
        let kind = &bytes[at + 4..at + 8];
        let size = usize::try_from(be64(bytes, at + 8).ok_or("a chunk is cut short")?)
            .map_err(|_| "a chunk is too big")?;
        let data = bytes
            .get(at + 16..at + 16 + size)
            .ok_or("a chunk runs past the end of the file")?;
        match kind {
            b"Exta" => {
                let len = usize::try_from(be64(data, 0).ok_or("a bitmap chunk is short")?)
                    .map_err(|_| "a bitmap chunk is broken")?;
                let id = data.get(8..8 + len).ok_or("a bitmap chunk is short")?;
                let payload = data.get(8 + len + 8..).ok_or("a bitmap chunk is short")?;
                exta.insert(String::from_utf8_lossy(id).into_owned(), payload);
            }
            b"SQLi" => database = Some(data),
            _ => {}
        }
        at += 16 + size;
    }
    Ok((exta, database.ok_or("the file has no layer database")?))
}

pub(super) fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn put_string(out: &mut Vec<u8>, text: &str) {
    put32(out, u32::try_from(text.encode_utf16().count()).unwrap_or(0));
    out.extend_from_slice(&utf16(text));
}
