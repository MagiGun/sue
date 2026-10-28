//! A byte-budgeted LRU cache of encoded (`.jxl`) tiles.

use std::collections::{BTreeMap, HashMap};

/// Identifies one tile of one layer at one mip / progressive level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    /// Canvas layer.
    pub layer: u32,
    /// Mip or progressive level (0 is full resolution).
    pub level: u8,
    /// Tile column.
    pub x: u32,
    /// Tile row.
    pub y: u32,
}

/// Cache counters, for the diagnostics overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileCacheStats {
    /// Lookups that found a tile.
    pub hits: u64,
    /// Lookups that found nothing.
    pub misses: u64,
    /// Tiles dropped to stay under budget.
    pub evictions: u64,
}

#[derive(Clone, Debug)]
struct CachedTile {
    bytes: Vec<u8>,
    tick: u64,
}

/// Least-recently-used cache of encoded tiles under a byte budget. Recency is
/// a monotonic tick kept in a `BTreeMap`, so lookups, inserts and evictions
/// are `O(log n)`.
#[derive(Clone, Debug)]
pub struct TileCache {
    budget_bytes: usize,
    used_bytes: usize,
    tick: u64,
    tiles: HashMap<TileKey, CachedTile>,
    recency: BTreeMap<u64, TileKey>,
    stats: TileCacheStats,
}

impl TileCache {
    /// An empty cache that holds at most `budget_bytes` of tile data.
    #[must_use]
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            tick: 0,
            tiles: HashMap::new(),
            recency: BTreeMap::new(),
            stats: TileCacheStats::default(),
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// Stores an encoded tile, replacing any tile with the same key and
    /// evicting the least recently used tiles as needed. Returns `false` (and
    /// keeps the cache intact) if the tile alone exceeds the budget.
    pub fn insert(&mut self, key: TileKey, encoded: Vec<u8>) -> bool {
        if encoded.len() > self.budget_bytes {
            return false;
        }
        self.remove(key);
        while self.used_bytes + encoded.len() > self.budget_bytes {
            let Some((_, oldest)) = self.recency.pop_first() else {
                break;
            };
            if let Some(tile) = self.tiles.remove(&oldest) {
                self.used_bytes -= tile.bytes.len();
                self.stats.evictions += 1;
            }
        }
        let tick = self.next_tick();
        self.used_bytes += encoded.len();
        self.recency.insert(tick, key);
        self.tiles.insert(
            key,
            CachedTile {
                bytes: encoded,
                tick,
            },
        );
        true
    }

    /// The tile at `key`, marking it most recently used.
    pub fn get(&mut self, key: TileKey) -> Option<&[u8]> {
        let tick = self.next_tick();
        match self.tiles.get_mut(&key) {
            Some(tile) => {
                self.recency.remove(&tile.tick);
                tile.tick = tick;
                self.recency.insert(tick, key);
                self.stats.hits += 1;
                Some(&tile.bytes)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// `true` if a tile is cached at `key` (does not touch recency).
    #[must_use]
    pub fn contains(&self, key: TileKey) -> bool {
        self.tiles.contains_key(&key)
    }

    /// Removes and returns the tile at `key`.
    pub fn remove(&mut self, key: TileKey) -> Option<Vec<u8>> {
        let tile = self.tiles.remove(&key)?;
        self.recency.remove(&tile.tick);
        self.used_bytes -= tile.bytes.len();
        Some(tile.bytes)
    }

    /// Drops every tile of `layer`, e.g. after the layer was edited. Returns
    /// how many tiles were dropped.
    pub fn invalidate_layer(&mut self, layer: u32) -> usize {
        let keys: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|key| key.layer == layer)
            .copied()
            .collect();
        for &key in &keys {
            self.remove(key);
        }
        keys.len()
    }

    /// Number of cached tiles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// `true` if no tile is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Bytes of tile data currently held.
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// The byte budget.
    #[must_use]
    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    /// Hit, miss and eviction counters.
    #[must_use]
    pub fn stats(&self) -> TileCacheStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(layer: u32, x: u32) -> TileKey {
        TileKey {
            layer,
            level: 0,
            x,
            y: 0,
        }
    }

    #[test]
    fn evicts_least_recently_used_first() {
        let mut cache = TileCache::new(30);
        assert!(cache.insert(key(0, 0), vec![0; 10]));
        assert!(cache.insert(key(0, 1), vec![1; 10]));
        assert!(cache.insert(key(0, 2), vec![2; 10]));
        assert_eq!(cache.get(key(0, 0)), Some(&[0_u8; 10][..]));
        assert!(cache.insert(key(0, 3), vec![3; 10]));
        assert!(cache.contains(key(0, 0)));
        assert!(!cache.contains(key(0, 1)));
        assert_eq!(cache.used_bytes(), 30);
        assert_eq!(
            cache.stats(),
            TileCacheStats {
                hits: 1,
                misses: 0,
                evictions: 1
            }
        );
        assert_eq!(cache.get(key(0, 1)), None);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn replacing_a_tile_updates_the_byte_count() {
        let mut cache = TileCache::new(100);
        cache.insert(key(0, 0), vec![0; 40]);
        cache.insert(key(0, 0), vec![0; 10]);
        assert_eq!((cache.len(), cache.used_bytes()), (1, 10));
        assert_eq!(cache.stats().evictions, 0);
    }

    #[test]
    fn oversized_tiles_are_refused_without_flushing() {
        let mut cache = TileCache::new(16);
        cache.insert(key(0, 0), vec![0; 8]);
        assert!(!cache.insert(key(0, 1), vec![0; 17]));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidates_whole_layers() {
        let mut cache = TileCache::new(1024);
        for x in 0..4 {
            cache.insert(key(1, x), vec![1; 8]);
            cache.insert(key(2, x), vec![2; 8]);
        }
        assert_eq!(cache.invalidate_layer(1), 4);
        assert_eq!((cache.len(), cache.used_bytes()), (4, 32));
        assert!(cache.remove(key(2, 0)).is_some());
        assert!(!cache.is_empty());
        assert_eq!(cache.budget_bytes(), 1024);
    }
}
