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

/// One chunk of a `.clip`, as it lies in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// Its four letter kind: `Head`, `Exta`, `SQLi`, `Foot` or one Sue
    /// does not know.
    pub kind: [u8; 4],
    /// Where its `CHNK` mark is in the file.
    pub offset: usize,
    /// Its data, after the 16 bytes of mark, kind and size.
    pub data: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// The id and the payload of a stored bitmap, when this is an `Exta`
    /// chunk.
    ///
    /// # Errors
    /// When the chunk is shorter than its own lengths say.
    pub fn external(&self) -> Result<(String, &'a [u8]), String> {
        let data = self.data;
        let len = usize::try_from(be64(data, 0).ok_or("a bitmap chunk is short")?)
            .map_err(|_| "a bitmap chunk is broken")?;
        let id_end = len.checked_add(8).ok_or("a bitmap chunk is short")?;
        let id = data.get(8..id_end).ok_or("a bitmap chunk is short")?;
        let payload = id_end
            .checked_add(8)
            .and_then(|from| data.get(from..))
            .ok_or("a bitmap chunk is short")?;
        Ok((String::from_utf8_lossy(id).into_owned(), payload))
    }
}

/// Walks the chunks of a `.clip` from the offset its header names. Sizes
/// come from the file: none is added without a check.
struct Walk<'a> {
    bytes: &'a [u8],
    at: usize,
    failed: bool,
}

impl<'a> Walk<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if !sniff(bytes) {
            return Err("not a Clip Studio file".to_owned());
        }
        let at = usize::try_from(be64(bytes, 16).ok_or("the file is cut short")?)
            .map_err(|_| "the file header is broken")?;
        Ok(Self {
            bytes,
            at,
            failed: false,
        })
    }

    fn chunk(&mut self, start: usize) -> Result<Chunk<'a>, String> {
        let (bytes, at) = (self.bytes, self.at);
        if &bytes[at..at + 4] != b"CHNK" {
            return Err(format!("no chunk where one should start, at byte {at}"));
        }
        let mut kind = [0; 4];
        kind.copy_from_slice(&bytes[at + 4..at + 8]);
        let size = usize::try_from(be64(bytes, at + 8).ok_or("a chunk is cut short")?)
            .map_err(|_| "a chunk is too big")?;
        let end = start
            .checked_add(size)
            .ok_or("a chunk runs past the end of the file")?;
        let data = bytes
            .get(start..end)
            .ok_or("a chunk runs past the end of the file")?;
        self.at = end;
        Ok(Chunk {
            kind,
            offset: at,
            data,
        })
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = Result<Chunk<'a>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let start = self
            .at
            .checked_add(16)
            .filter(|start| *start <= self.bytes.len())?;
        let chunk = self.chunk(start);
        self.failed = chunk.is_err();
        Some(chunk)
    }
}

/// How a `.clip` is laid out: what its header says and every chunk that
/// could be walked, for looking at a file rather than opening it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout<'a> {
    /// The size of the file as its header gives it.
    pub declared_size: u64,
    /// Where the header says the first chunk starts.
    pub first_chunk: u64,
    /// The chunks, in file order, up to the first broken one.
    pub chunks: Vec<Chunk<'a>>,
    /// Bytes after the last whole chunk that are no chunk.
    pub trailing: usize,
    /// Why the walk stopped early, when it did.
    pub problem: Option<String>,
}

/// The layout of the `.clip` in `bytes`. A file broken part way still
/// answers the chunks before the break, with the reason in `problem`.
///
/// # Errors
/// When `bytes` does not start as a `.clip` at all.
pub fn layout(bytes: &[u8]) -> Result<Layout<'_>, String> {
    let walk = Walk::new(bytes)?;
    let mut end = walk.at;
    let mut chunks = Vec::new();
    let mut problem = None;
    for chunk in walk {
        match chunk {
            Ok(chunk) => {
                end = chunk.offset + 16 + chunk.data.len();
                chunks.push(chunk);
            }
            Err(error) => problem = Some(error),
        }
    }
    Ok(Layout {
        declared_size: be64(bytes, 8).unwrap_or(0),
        first_chunk: be64(bytes, 16).unwrap_or(0),
        chunks,
        trailing: bytes.len().saturating_sub(end),
        problem,
    })
}

/// The chunks of a `.clip`: the `Exta` payloads by id, and the database.
/// A `.clip`'s bitmap chunks by id.
pub(super) type Bitmaps<'a> = HashMap<String, &'a [u8]>;

pub(super) fn chunks(bytes: &[u8]) -> Result<(Bitmaps<'_>, &[u8]), String> {
    let mut exta = HashMap::new();
    let mut database = None;
    for chunk in Walk::new(bytes)? {
        let chunk = chunk?;
        match &chunk.kind {
            b"Exta" => {
                let (id, payload) = chunk.external()?;
                exta.insert(id, payload);
            }
            b"SQLi" => database = Some(chunk.data),
            _ => {}
        }
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

    #[test]
    fn the_layout_lists_every_chunk_and_what_is_left_over() {
        let mut exta = 8_u64.to_be_bytes().to_vec();
        exta.extend_from_slice(b"extrnlid");
        exta.extend_from_slice(&3_u64.to_be_bytes());
        exta.extend_from_slice(b"abc");
        let mut bytes = file(
            24,
            &[
                (b"Head", 2, b"hi"),
                (b"Exta", exta.len() as u64, &exta),
                (b"Wxyz", 0, b""),
                (b"SQLi", 4, b"data"),
                (b"Foot", 0, b""),
            ],
        );
        let whole = bytes.len();
        bytes[8..16].copy_from_slice(&(whole as u64).to_be_bytes());
        let layout = layout(&bytes).unwrap();
        assert_eq!(
            (layout.declared_size, layout.first_chunk),
            (whole as u64, 24)
        );
        let kinds: Vec<&[u8; 4]> = layout.chunks.iter().map(|chunk| &chunk.kind).collect();
        assert_eq!(kinds, [b"Head", b"Exta", b"Wxyz", b"SQLi", b"Foot"]);
        assert_eq!(layout.chunks[0].offset, 24);
        assert_eq!(layout.chunks[1].offset, 24 + 16 + 2);
        assert_eq!(
            layout.chunks[1].external().unwrap(),
            ("extrnlid".to_owned(), b"abc".as_slice())
        );
        assert!(layout.chunks[0].external().is_err());
        assert_eq!((layout.trailing, &layout.problem), (0, &None));
        // The reader finds the same bitmap and database.
        let (bitmaps, database) = chunks(&bytes).unwrap();
        assert_eq!(bitmaps.get("extrnlid"), Some(&b"abc".as_slice()));
        assert_eq!(database, b"data");

        // Bytes that are no chunk, and a chunk cut short.
        let mut longer = bytes.clone();
        longer.extend_from_slice(b"tail!");
        assert_eq!(super::layout(&longer).unwrap().trailing, 5);
        let cut = super::layout(&bytes[..whole - 18]).unwrap();
        assert_eq!(cut.chunks.len(), 3);
        assert!(cut.problem.is_some());
        let short = file(24, &[(b"Head", 2, b"hi")]);
        let lost = super::layout(&short[..40]);
        assert!(lost.unwrap().problem.is_some());
        let mut wrong = bytes.clone();
        wrong[24] = b'X';
        let wrong = super::layout(&wrong).unwrap();
        assert!(wrong.chunks.is_empty() && wrong.problem.is_some());
        assert!(super::layout(b"8BPS").is_err());
        assert!(super::layout(b"CSFCHUNK").is_err());
    }
}
