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
    // Sizes come from the file: none is added without a check.
    while let Some(start) = at.checked_add(16).filter(|start| *start <= bytes.len()) {
        if &bytes[at..at + 4] != b"CHNK" {
            return Err(format!("no chunk where one should start, at byte {at}"));
        }
        let kind = &bytes[at + 4..at + 8];
        let size = usize::try_from(be64(bytes, at + 8).ok_or("a chunk is cut short")?)
            .map_err(|_| "a chunk is too big")?;
        let end = start
            .checked_add(size)
            .ok_or("a chunk runs past the end of the file")?;
        let data = bytes
            .get(start..end)
            .ok_or("a chunk runs past the end of the file")?;
        match kind {
            b"Exta" => {
                let len = usize::try_from(be64(data, 0).ok_or("a bitmap chunk is short")?)
                    .map_err(|_| "a bitmap chunk is broken")?;
                let id_end = len.checked_add(8).ok_or("a bitmap chunk is short")?;
                let id = data.get(8..id_end).ok_or("a bitmap chunk is short")?;
                let payload = id_end
                    .checked_add(8)
                    .and_then(|from| data.get(from..))
                    .ok_or("a bitmap chunk is short")?;
                exta.insert(String::from_utf8_lossy(id).into_owned(), payload);
            }
            b"SQLi" => database = Some(data),
            _ => {}
        }
        at = end;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of `chunks` (kind, declared size, data).
    fn file(first: u64, chunks: &[(&[u8; 4], u64, &[u8])]) -> Vec<u8> {
        let mut out = b"CSFCHUNK".to_vec();
        out.extend_from_slice(&0_u64.to_be_bytes());
        out.extend_from_slice(&first.to_be_bytes());
        for (kind, size, data) in chunks {
            out.extend_from_slice(b"CHNK");
            out.extend_from_slice(*kind);
            out.extend_from_slice(&size.to_be_bytes());
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn sizes_too_big_to_add_are_refused_not_a_crash() {
        // A first chunk offset, a chunk size and a bitmap id length at the
        // top of what 64 bits hold.
        for huge in [u64::MAX, u64::MAX - 15, u64::MAX - 40, 1 << 63] {
            assert!(chunks(&file(huge, &[])).is_err(), "offset {huge}");
            assert!(
                chunks(&file(24, &[(b"SQLi", huge, b"abc")])).is_err(),
                "size {huge}"
            );
            let mut exta = huge.to_be_bytes().to_vec();
            exta.extend_from_slice(b"extrnlid0123");
            assert!(
                chunks(&file(24, &[(b"Exta", exta.len() as u64, &exta)])).is_err(),
                "id length {huge}"
            );
        }
    }
}
