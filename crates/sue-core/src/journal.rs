//! The autosave journal: work kept while nobody saved it, written as one
//! whole `.sue` file (the base) and a journal of the changes since, so a
//! kept copy after one stroke costs the tiles that stroke touched, not
//! the whole document.
//!
//! ```text
//! magic     8 bytes   "SUE\0JRNL"
//! version   u32       1
//! base      u64       length of the base file in bytes
//! digest    32 bytes  SHA-256 of the base file
//! records   repeated  length u64, payload, crc32 u32 of the payload
//! ```
//!
//! Numbers are little-endian. A record's payload is a `.sue` file
//! ([`SueFile`]) holding every chunk of the work as it stood, except two:
//! its `PIXL` chunk holds only the blobs that were not in the work before
//! it (a blob under a key the work already had replaces it), and it holds
//! no `THMB` (the base's preview stands until the next compaction).
//!
//! Replaying reads the base, then each record in turn: the record's chunks
//! replace the work's, its blobs join the work's store. A journal whose
//! digest does not name the base (the base was rewritten, and the old
//! journal not yet removed) is ignored. A record cut short or failing its
//! check ends the replay: the work is what the records before it made,
//! each of which is whole. So a crash while appending loses at most the
//! last record, and a crash while compacting loses nothing.
//!
//! Compaction writes the base anew from the work and starts an empty
//! journal for it. `docs/autosave-journal.md` describes the format for
//! people who read kept work with their own tools.

use sha2::{Digest, Sha256};

use crate::SueFile;
use crate::codec::{CodecError, crc32};
use crate::file::{Chunk, PIXELS, THUMBNAIL};
use crate::pixels::{store_delta, store_merge};

/// The first eight bytes of a journal.
pub const MAGIC: [u8; 8] = *b"SUE\0JRNL";
/// The journal version this build writes and reads.
pub const VERSION: u32 = 1;
/// Magic, version, base length and digest.
pub const HEADER_LEN: usize = 8 + 4 + 8 + 32;

/// The SHA-256 of `bytes`.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The header of a journal for the base file `base`.
#[must_use]
pub fn header(base: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(base.len() as u64).to_le_bytes());
    out.extend_from_slice(&sha256(base));
    out
}

/// `true` if `journal` starts with a header of this version for `base`.
#[must_use]
pub fn belongs(base: &[u8], journal: &[u8]) -> bool {
    journal.len() >= HEADER_LEN && journal[..HEADER_LEN] == header(base)[..]
}

/// The record that takes the work from `previous` to `next`, framed and
/// ready to append. `None` when it cannot be a record, because the two
/// pixel stores differ in tile size or key width: write a new base then.
#[must_use]
pub fn record(previous: &SueFile, next: &SueFile) -> Option<Vec<u8>> {
    let mut chunks = Vec::with_capacity(next.chunks().len());
    for chunk in next.chunks() {
        match chunk.tag {
            THUMBNAIL => {}
            PIXELS => {
                let payload = match previous.chunk(PIXELS) {
                    Some(old) if old.version == chunk.version => {
                        store_delta(&old.payload, &chunk.payload)?
                    }
                    Some(_) => return None,
                    None => chunk.payload.clone(),
                };
                chunks.push(Chunk {
                    tag: PIXELS,
                    version: chunk.version,
                    payload,
                });
            }
            _ => chunks.push(chunk.clone()),
        }
    }
    let payload = next.with_chunks(chunks).to_bytes();
    let mut out = Vec::with_capacity(payload.len() + 12);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&crc32(&payload).to_le_bytes());
    Some(out)
}

/// What [`replay`] made of a base and its journal.
#[derive(Debug)]
pub struct Replay {
    /// The work as the last whole record left it.
    pub file: SueFile,
    /// How many records were applied.
    pub records: usize,
    /// `true` if the journal held more than those records (cut short or
    /// damaged), or did not belong to the base and was ignored.
    pub dropped: bool,
}

/// The work kept as the base file `base` and the journal `journal` (empty
/// when there is none).
///
/// # Errors
/// Only when the base itself cannot be read; a damaged journal is cut back
/// to its last whole record instead.
pub fn replay(base: &[u8], journal: &[u8]) -> Result<Replay, CodecError> {
    let mut file = SueFile::from_bytes(base)?;
    if journal.is_empty() {
        return Ok(Replay {
            file,
            records: 0,
            dropped: false,
        });
    }
    if !belongs(base, journal) {
        return Ok(Replay {
            file,
            records: 0,
            dropped: true,
        });
    }
    let mut at = HEADER_LEN;
    let mut records = 0;
    while at < journal.len() {
        let Some(next) = read_record(journal, at).and_then(|(payload, end)| {
            let record = SueFile::from_bytes(payload).ok()?;
            Some((apply(&file, &record)?, end))
        }) else {
            return Ok(Replay {
                file,
                records,
                dropped: true,
            });
        };
        (file, at) = next;
        records += 1;
    }
    Ok(Replay {
        file,
        records,
        dropped: false,
    })
}

/// The payload of the record at `at` and where the next one starts, if
/// the record is whole and passes its check.
fn read_record(journal: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let length = usize::try_from(u64::from_le_bytes(
        journal.get(at..at + 8)?.try_into().ok()?,
    ))
    .ok()?;
    let start = at + 8;
    let end = start.checked_add(length)?;
    let payload = journal.get(start..end)?;
    let check = u32::from_le_bytes(journal.get(end..end + 4)?.try_into().ok()?);
    (crc32(payload) == check).then_some((payload, end + 4))
}

/// The work `file` with `record` applied, or `None` if their stores do
/// not fit together.
fn apply(file: &SueFile, record: &SueFile) -> Option<SueFile> {
    let mut chunks = Vec::with_capacity(record.chunks().len() + 1);
    for chunk in record.chunks() {
        if chunk.tag == PIXELS {
            let payload = match file.chunk(PIXELS) {
                Some(old) if old.version == chunk.version => {
                    store_merge(&old.payload, &chunk.payload)?
                }
                Some(_) => return None,
                None => chunk.payload.clone(),
            };
            chunks.push(Chunk {
                tag: PIXELS,
                version: chunk.version,
                payload,
            });
        } else {
            chunks.push(chunk.clone());
        }
    }
    if record.chunk(THUMBNAIL).is_none()
        && let Some(preview) = file.chunk(THUMBNAIL)
    {
        chunks.push(preview.clone());
    }
    Some(record.with_chunks(chunks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arena, ComponentKind, ComponentRegistry, PixelFormat, RasterTile2D};

    fn picture(shade: u8) -> RasterTile2D {
        let mut raster = RasterTile2D::new(600, 300, PixelFormat::Rgba8).unwrap();
        for (index, byte) in raster.bytes_mut().unwrap().iter_mut().enumerate() {
            *byte = (index as u32 / 7 % 251) as u8 ^ shade;
        }
        raster
    }

    fn arena_with(raster: &RasterTile2D) -> Arena {
        let mut arena = Arena::new();
        arena.spawn("Layer", raster.clone());
        arena
    }

    fn saved(arena: &Arena, into: &SueFile) -> SueFile {
        let mut file = into.clone();
        file.set_arena(arena);
        file
    }

    #[test]
    fn records_replay_to_the_work_and_damage_ends_the_replay() {
        let mut raster = picture(0);
        let first = saved(&arena_with(&raster), &SueFile::new());
        let base = first.to_bytes();
        let mut journal = header(&base);
        // One tile changes: the record carries one blob.
        raster.bytes_mut_in([0, 0, 4, 4]).unwrap()[0] ^= 0xff;
        let second = saved(&arena_with(&raster), &first);
        let one = record(&first, &second).unwrap();
        journal.extend_from_slice(&one);
        // Then another.
        raster.bytes_mut_in([300, 0, 4, 4]).unwrap()[300 * 4] ^= 0x0f;
        let third = saved(&arena_with(&raster), &second);
        let two = record(&second, &third).unwrap();
        journal.extend_from_slice(&two);
        let whole = second.to_bytes().len();
        assert!(
            one.len() * 2 < whole,
            "a record of one tile is smaller than the work"
        );

        let back = replay(&base, &journal).unwrap();
        assert_eq!((back.records, back.dropped), (2, false));
        let registry = ComponentRegistry::new();
        let pixels = |file: &SueFile| {
            let arena = file.arena(&registry).unwrap();
            let (_, entity) = arena.iter().next().unwrap();
            entity
                .component(ComponentKind::RasterTile2D)
                .and_then(|data| data.as_raster().cloned())
                .unwrap()
        };
        assert_eq!(pixels(&back.file), raster);
        assert_eq!(pixels(&third), raster);

        // Cut anywhere inside the second record: the first one stands.
        for cut in [1, 8, 9, two.len() / 2, two.len() - 1] {
            let short = &journal[..journal.len() - two.len() + cut];
            let back = replay(&base, short).unwrap();
            assert_eq!((back.records, back.dropped), (1, true), "cut at {cut}");
            assert_eq!(pixels(&back.file), pixels(&second));
        }
        // A flipped byte fails the check.
        let mut damaged = journal.clone();
        let last = damaged.len() - 10;
        damaged[last] ^= 1;
        assert_eq!(replay(&base, &damaged).unwrap().records, 1);
        // A journal of another base is not applied.
        let other = saved(&arena_with(&picture(9)), &SueFile::new()).to_bytes();
        let back = replay(&other, &journal).unwrap();
        assert_eq!((back.records, back.dropped), (0, true));
        // No journal: the base.
        assert_eq!(replay(&base, &[]).unwrap().file, first);
    }
}
