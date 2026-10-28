//! The Player's music library: the songs it knows (by file, with their
//! tags and length), the albums the tags make plus the ones people make,
//! and playlists. It is the listener's own, like the quick note, so it lives
//! in Sue's data folder and never goes into projects.
//!
//! The file is plain text, one line per song or collection, fields split by
//! tabs, under a versioned header:
//!
//! ```text
//! # sue player library 1
//! song <tab> C:\Music\one.flac <tab> One <tab> Artist <tab> Album <tab> Album Artist <tab> Genre <tab> 3 <tab> 213.4 <tab> 1761600000
//! playlist <tab> Road Trip <tab>  <tab> C:\Pictures\road.jpg <tab> C:\Music\one.flac|C:\Music\two.mp3
//! album <tab> Live Takes <tab> Artist <tab> C:\Pictures\live.png <tab> C:\Music\three.wav
//! liked <tab> C:\Music\one.flac|C:\Music\two.mp3
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The header of the library file.
pub const HEADER: &str = "# sue player library 1";

/// Audio files the Player imports.
pub const AUDIO: [&str; 8] = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "aiff"];

/// `true` for files the Player can play.
#[must_use]
pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| AUDIO.contains(&extension.as_str()))
}

/// One song of the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    /// The file.
    pub path: PathBuf,
    /// Title (the file name when the tags have none).
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Album.
    pub album: String,
    /// Album artist, for grouping compilations.
    pub album_artist: String,
    /// Genre.
    pub genre: String,
    /// Track number on its album (0 when unknown).
    pub number: u32,
    /// Length in seconds (0 when unknown).
    pub seconds: f64,
    /// When it was added, in seconds since 1970.
    pub added: u64,
}

impl Song {
    /// Who the song is by, for display.
    #[must_use]
    pub fn by(&self) -> &str {
        if self.artist.is_empty() {
            "Unknown artist"
        } else {
            &self.artist
        }
    }

    /// The album's artist, falling back to the song's.
    #[must_use]
    pub fn album_by(&self) -> &str {
        if self.album_artist.is_empty() {
            self.by()
        } else {
            &self.album_artist
        }
    }
}

/// What kind of collection someone made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An ordered list of any songs.
    Playlist,
    /// An album put together by hand.
    Album,
}

/// A playlist or a hand-made album.
#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    /// Playlist or album.
    pub kind: Kind,
    /// Its name.
    pub name: String,
    /// The album's artist (empty for playlists).
    pub artist: String,
    /// A picture for its cover.
    pub cover: Option<PathBuf>,
    /// Its songs, in order.
    pub songs: Vec<PathBuf>,
}

/// An album as the Player shows it: the songs the tags put together, or a
/// hand-made one.
#[derive(Clone, Debug, PartialEq)]
pub struct Album {
    /// Name.
    pub name: String,
    /// Artist.
    pub artist: String,
    /// Its songs, in track order.
    pub songs: Vec<PathBuf>,
    /// The collection it is, if someone made it.
    pub made: Option<usize>,
    /// Its cover.
    pub cover: Option<PathBuf>,
}

/// The whole library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    /// Every song, in the order they were added.
    pub songs: Vec<Song>,
    /// Playlists and hand-made albums.
    pub collections: Vec<Collection>,
    /// Liked songs.
    pub liked: Vec<PathBuf>,
}

fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

fn join(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| clean(&path.display().to_string()))
        .collect::<Vec<_>>()
        .join("|")
}

fn split(text: &str) -> Vec<PathBuf> {
    text.split('|')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

impl Library {
    /// Reads a library file. Lines it does not understand are skipped, so a
    /// newer file still opens.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut library = Self::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            let field = |index: usize| fields.get(index).copied().unwrap_or("").trim();
            match field(0) {
                "song" if !field(1).is_empty() => library.songs.push(Song {
                    path: PathBuf::from(field(1)),
                    title: field(2).to_owned(),
                    artist: field(3).to_owned(),
                    album: field(4).to_owned(),
                    album_artist: field(5).to_owned(),
                    genre: field(6).to_owned(),
                    number: field(7).parse().unwrap_or(0),
                    seconds: field(8).parse().unwrap_or(0.0),
                    added: field(9).parse().unwrap_or(0),
                }),
                kind @ ("playlist" | "album") => library.collections.push(Collection {
                    kind: if kind == "album" {
                        Kind::Album
                    } else {
                        Kind::Playlist
                    },
                    name: field(1).to_owned(),
                    artist: field(2).to_owned(),
                    cover: Some(field(3))
                        .filter(|cover| !cover.is_empty())
                        .map(PathBuf::from),
                    songs: split(field(4)),
                }),
                "liked" => library.liked = split(field(1)),
                _ => {}
            }
        }
        library
    }

    /// The library file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = format!("{HEADER}\n");
        for song in &self.songs {
            text.push_str(&format!(
                "song\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\n",
                clean(&song.path.display().to_string()),
                clean(&song.title),
                clean(&song.artist),
                clean(&song.album),
                clean(&song.album_artist),
                clean(&song.genre),
                song.number,
                song.seconds,
                song.added
            ));
        }
        for collection in &self.collections {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                match collection.kind {
                    Kind::Playlist => "playlist",
                    Kind::Album => "album",
                },
                clean(&collection.name),
                clean(&collection.artist),
                collection
                    .cover
                    .as_ref()
                    .map(|cover| clean(&cover.display().to_string()))
                    .unwrap_or_default(),
                join(&collection.songs)
            ));
        }
        if !self.liked.is_empty() {
            text.push_str(&format!("liked\t{}\n", join(&self.liked)));
        }
        text
    }

    /// The song of `path`.
    #[must_use]
    pub fn song(&self, path: &Path) -> Option<&Song> {
        self.songs.iter().find(|song| song.path == path)
    }

    /// Adds `song`, or refreshes the one with its file. Returns `true` if it
    /// was new.
    pub fn add(&mut self, song: Song) -> bool {
        match self.songs.iter_mut().find(|known| known.path == song.path) {
            Some(known) => {
                let added = known.added;
                *known = Song { added, ..song };
                false
            }
            None => {
                self.songs.push(song);
                true
            }
        }
    }

    /// Takes a song out of the library, its playlists and the likes.
    pub fn remove(&mut self, path: &Path) {
        self.songs.retain(|song| song.path != path);
        self.liked.retain(|liked| liked != path);
        for collection in &mut self.collections {
            collection.songs.retain(|song| song != path);
        }
    }

    /// `true` for a liked song.
    #[must_use]
    pub fn is_liked(&self, path: &Path) -> bool {
        self.liked.iter().any(|liked| liked == path)
    }

    /// Likes or unlikes a song.
    pub fn toggle_like(&mut self, path: &Path) {
        if self.is_liked(path) {
            self.liked.retain(|liked| liked != path);
        } else {
            self.liked.push(path.to_path_buf());
        }
    }

    /// Every album: the ones the tags make (grouped by album and album
    /// artist, in track order), then the hand-made ones.
    #[must_use]
    pub fn albums(&self) -> Vec<Album> {
        let mut tagged: BTreeMap<(String, String), Vec<&Song>> = BTreeMap::new();
        for song in &self.songs {
            if !song.album.is_empty() {
                tagged
                    .entry((song.album.to_lowercase(), song.album_by().to_lowercase()))
                    .or_default()
                    .push(song);
            }
        }
        let mut albums: Vec<Album> = tagged
            .into_values()
            .map(|mut songs| {
                songs.sort_by(|a, b| {
                    a.number
                        .cmp(&b.number)
                        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                });
                Album {
                    name: songs[0].album.clone(),
                    artist: songs[0].album_by().to_owned(),
                    songs: songs.iter().map(|song| song.path.clone()).collect(),
                    made: None,
                    cover: None,
                }
            })
            .collect();
        albums.extend(
            self.collections
                .iter()
                .enumerate()
                .filter(|(_, collection)| collection.kind == Kind::Album)
                .map(|(index, collection)| Album {
                    name: collection.name.clone(),
                    artist: collection.artist.clone(),
                    songs: collection.songs.clone(),
                    made: Some(index),
                    cover: collection.cover.clone(),
                }),
        );
        albums
    }

    /// Every artist with their songs, by name.
    #[must_use]
    pub fn artists(&self) -> Vec<(String, Vec<PathBuf>)> {
        let mut artists: BTreeMap<String, (String, Vec<PathBuf>)> = BTreeMap::new();
        for song in &self.songs {
            artists
                .entry(song.by().to_lowercase())
                .or_insert_with(|| (song.by().to_owned(), Vec::new()))
                .1
                .push(song.path.clone());
        }
        artists.into_values().collect()
    }

    /// A name for a new collection that no other has: `New Playlist`,
    /// `New Playlist 2` and so on.
    #[must_use]
    pub fn fresh_name(&self, base: &str) -> String {
        let taken = |name: &str| {
            self.collections
                .iter()
                .any(|collection| collection.name == name)
        };
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|index| format!("{base} {index}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }
}

/// A song from what its file says.
#[must_use]
pub fn song_from(path: &Path, info: &sue_io::AudioInfo, added: u64) -> Song {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let seconds = if info.sample_rate > 0 {
        info.frames as f64 / f64::from(info.sample_rate)
    } else {
        0.0
    };
    Song {
        path: path.to_path_buf(),
        title: info.title.clone().unwrap_or(stem),
        artist: info.artist.clone().unwrap_or_default(),
        album: info.album.clone().unwrap_or_default(),
        album_artist: info.album_artist.clone().unwrap_or_default(),
        genre: info.genre.clone().unwrap_or_default(),
        number: info.track.unwrap_or(0),
        seconds,
        added,
    }
}

/// Every audio file at `path`: the file itself, or everything under a
/// folder (at most `limit`).
#[must_use]
pub fn audio_under(path: &Path, limit: usize) -> Vec<PathBuf> {
    if path.is_file() {
        return if is_audio(path) {
            vec![path.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let mut found = Vec::new();
    let mut queue = std::collections::VecDeque::from([path.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = sue_platform::list_dir(&dir, false) else {
            continue;
        };
        for entry in entries {
            if entry.is_dir {
                queue.push_back(entry.path);
            } else if is_audio(&entry.path) {
                found.push(entry.path);
                if found.len() >= limit {
                    return found;
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(path: &str, album: &str, number: u32) -> Song {
        Song {
            path: PathBuf::from(path),
            title: path.to_owned(),
            artist: "Ada".to_owned(),
            album: album.to_owned(),
            number,
            seconds: 61.5,
            added: 7,
            ..Song::default()
        }
    }

    #[test]
    fn the_library_file_round_trips() {
        let mut library = Library::default();
        library.add(song("b.flac", "Night", 2));
        library.add(song("a.mp3", "Night", 1));
        library.collections.push(Collection {
            kind: Kind::Playlist,
            name: "Road Trip".to_owned(),
            artist: String::new(),
            cover: Some(PathBuf::from("road.jpg")),
            songs: vec![PathBuf::from("a.mp3"), PathBuf::from("b.flac")],
        });
        library.toggle_like(Path::new("b.flac"));
        let text = library.to_text();
        assert!(text.starts_with(HEADER));
        assert_eq!(Library::parse(&text), library);
        // Lines a newer Sue writes are skipped, not fatal.
        let newer = format!("{text}mood\tcalm\n");
        assert_eq!(Library::parse(&newer), library);
    }

    #[test]
    fn tags_make_albums_in_track_order_and_made_albums_follow() {
        let mut library = Library::default();
        library.add(song("b.flac", "Night", 2));
        library.add(song("a.mp3", "Night", 1));
        library.add(song("c.wav", "", 0));
        library.collections.push(Collection {
            kind: Kind::Album,
            name: "Live".to_owned(),
            artist: "Ada".to_owned(),
            cover: None,
            songs: vec![PathBuf::from("c.wav")],
        });
        let albums = library.albums();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].name, "Night");
        assert_eq!(
            albums[0].songs,
            [PathBuf::from("a.mp3"), PathBuf::from("b.flac")]
        );
        assert_eq!(albums[1].made, Some(0));
        assert_eq!(library.artists()[0].1.len(), 3);
        library.remove(Path::new("c.wav"));
        assert!(library.collections[0].songs.is_empty());
        assert_eq!(library.fresh_name("Live"), "Live 2");
    }

    #[test]
    fn adding_again_refreshes_the_tags_and_keeps_when_it_was_added() {
        let mut library = Library::default();
        assert!(library.add(song("a.mp3", "Night", 1)));
        let renamed = Song {
            title: "Better title".to_owned(),
            added: 99,
            ..song("a.mp3", "Night", 1)
        };
        assert!(!library.add(renamed));
        assert_eq!(library.songs.len(), 1);
        assert_eq!(library.songs[0].title, "Better title");
        assert_eq!(library.songs[0].added, 7);
        assert!(is_audio(Path::new("x/Song.FLAC")) && !is_audio(Path::new("x.png")));
    }
}
