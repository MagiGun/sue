//! Zip archives, the container of Word (`.docx`), OpenDocument (`.odt`) and
//! EPUB files: written with deflate (or stored, for an EPUB's `mimetype`,
//! which must come first and uncompressed) and read back, stored or
//! deflated, through `miniz_oxide`.

use sue_core::codec::crc32;

/// One file to put in an archive.
struct Entry {
    name: String,
    crc: u32,
    size: u32,
    data: Vec<u8>,
    deflated: bool,
    offset: u32,
}

/// Builds a zip archive in memory.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

impl std::fmt::Debug for ZipWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipWriter")
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl ZipWriter {
    /// An empty archive.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `bytes` as `name`, deflated unless `store`.
    pub fn add(&mut self, name: &str, bytes: &[u8], store: bool) {
        let deflated = !store && bytes.len() > 64;
        let data = if deflated {
            miniz_oxide::deflate::compress_to_vec(bytes, 6)
        } else {
            bytes.to_vec()
        };
        let entry = Entry {
            name: name.to_owned(),
            crc: crc32(bytes),
            size: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            data,
            deflated,
            offset: u32::try_from(self.out.len()).unwrap_or(u32::MAX),
        };
        let out = &mut self.out;
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        out.extend_from_slice(&20_u16.to_le_bytes());
        // Bit 11: names are UTF-8.
        out.extend_from_slice(&0x0800_u16.to_le_bytes());
        out.extend_from_slice(&(if deflated { 8_u16 } else { 0 }).to_le_bytes());
        // 1980-01-01 00:00, so the same document always zips the same.
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0x0021_u16.to_le_bytes());
        out.extend_from_slice(&entry.crc.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(entry.data.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        out.extend_from_slice(&entry.size.to_le_bytes());
        out.extend_from_slice(
            &u16::try_from(entry.name.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(entry.name.as_bytes());
        out.extend_from_slice(&entry.data);
        self.entries.push(entry);
    }

    /// Adds UTF-8 text as `name`, deflated.
    pub fn add_text(&mut self, name: &str, text: &str) {
        self.add(name, text.as_bytes(), false);
    }

    /// The finished archive.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        let start = self.out.len();
        for entry in &self.entries {
            let out = &mut self.out;
            out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
            out.extend_from_slice(&20_u16.to_le_bytes());
            out.extend_from_slice(&20_u16.to_le_bytes());
            out.extend_from_slice(&0x0800_u16.to_le_bytes());
            out.extend_from_slice(&(if entry.deflated { 8_u16 } else { 0 }).to_le_bytes());
            out.extend_from_slice(&0_u16.to_le_bytes());
            out.extend_from_slice(&0x0021_u16.to_le_bytes());
            out.extend_from_slice(&entry.crc.to_le_bytes());
            out.extend_from_slice(
                &u32::try_from(entry.data.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(&entry.size.to_le_bytes());
            out.extend_from_slice(
                &u16::try_from(entry.name.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(&[0; 12]);
            out.extend_from_slice(&entry.offset.to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
        }
        let size = self.out.len() - start;
        let count = u16::try_from(self.entries.len()).unwrap_or(u16::MAX);
        let out = &mut self.out;
        out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&u32::try_from(size).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&u32::try_from(start).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        self.out
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Every file in a zip archive, by name, from its central directory.
///
/// # Errors
/// If the archive is broken, encrypted or uses a compression other than
/// deflate, or a file inflates past 256 MiB.
pub fn read_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let broken = || "the zip archive is broken".to_owned();
    // The end record sits in the last 64 KiB (after a comment, if any).
    let floor = bytes.len().saturating_sub(65_557);
    let end = (floor..bytes.len().saturating_sub(21))
        .rev()
        .find(|at| u32_at(bytes, *at) == Some(0x0605_4b50))
        .ok_or_else(broken)?;
    let count = usize::from(u16_at(bytes, end + 10).ok_or_else(broken)?);
    let mut at = u32_at(bytes, end + 16).ok_or_else(broken)? as usize;
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(bytes, at) != Some(0x0201_4b50) {
            return Err(broken());
        }
        let flags = u16_at(bytes, at + 8).ok_or_else(broken)?;
        let method = u16_at(bytes, at + 10).ok_or_else(broken)?;
        let packed = u32_at(bytes, at + 20).ok_or_else(broken)? as usize;
        let size = u32_at(bytes, at + 24).ok_or_else(broken)? as usize;
        let name_len = usize::from(u16_at(bytes, at + 28).ok_or_else(broken)?);
        let extra = usize::from(u16_at(bytes, at + 30).ok_or_else(broken)?);
        let comment = usize::from(u16_at(bytes, at + 32).ok_or_else(broken)?);
        let local = u32_at(bytes, at + 42).ok_or_else(broken)? as usize;
        let name = bytes.get(at + 46..at + 46 + name_len).ok_or_else(broken)?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + name_len + extra + comment;
        if flags & 1 != 0 {
            return Err("the zip archive is encrypted".to_owned());
        }
        let local_name = usize::from(u16_at(bytes, local + 26).ok_or_else(broken)?);
        let local_extra = usize::from(u16_at(bytes, local + 28).ok_or_else(broken)?);
        let start = local + 30 + local_name + local_extra;
        let data = bytes.get(start..start + packed).ok_or_else(broken)?;
        if size > 256 << 20 {
            return Err(format!("{name} is too large"));
        }
        let content = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size.max(1) + 64)
                .map_err(|_| format!("{name} does not inflate"))?,
            other => return Err(format!("{name} uses compression method {other}")),
        };
        files.push((name, content));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_round_trip_stored_and_deflated() {
        let mut zip = ZipWriter::new();
        zip.add("mimetype", b"application/epub+zip", true);
        let long = "Sue writes. ".repeat(200);
        zip.add_text("text/chapter one.xhtml", &long);
        let bytes = zip.finish();
        // An EPUB's mimetype is the first file, stored, at a fixed place.
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(&bytes[38..58], b"application/epub+zip");
        let files = read_zip(&bytes).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].0, "text/chapter one.xhtml");
        assert_eq!(files[1].1, long.as_bytes());
        assert!(bytes.len() < long.len(), "deflated");
        assert!(read_zip(b"not a zip").is_err());
    }
}
