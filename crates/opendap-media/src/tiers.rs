//! Where a resource is: decoded in memory (ready to draw), encoded on disk (to decode), or
//! nowhere yet (to fetch: from the card, the library, a paired device). Two budgets: memory for
//! decoded pixels, disk for encoded bytes; each lets go of the least recently used.

use opendap_layout::data::cache::Cache;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Draw it.
    Memory,
    /// Decode it (a decode job), then it is in memory.
    Disk,
    /// Fetch it, store it on disk, decode it.
    Nowhere,
}

pub struct Tiers {
    pub memory: Cache<String, ()>,
    pub disk: Cache<String, ()>,
}

impl Tiers {
    /// `memory` bytes of decoded pixels, `disk` bytes of encoded files.
    pub fn new(memory: usize, disk: usize) -> Self {
        Tiers { memory: Cache::new(memory), disk: Cache::new(disk) }
    }

    pub fn find(&mut self, key: &str) -> Found {
        let key = key.to_string();
        if self.memory.get(&key).is_some() {
            Found::Memory
        } else if self.disk.get(&key).is_some() {
            Found::Disk
        } else {
            Found::Nowhere
        }
    }

    /// Fetched and stored: `bytes` on disk.
    pub fn stored(&mut self, key: &str, bytes: usize) -> Vec<String> {
        self.disk.put(key.to_string(), (), bytes)
    }

    /// Decoded: `pixels_bytes` in memory (width × height × 4).
    pub fn decoded(&mut self, key: &str, pixels_bytes: usize) -> Vec<String> {
        self.memory.put(key.to_string(), (), pixels_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_then_disk_then_fetch_each_within_its_budget() {
        let mut t = Tiers::new(96 * 96 * 4 * 2, 1_000_000);
        assert_eq!(t.find("a@96"), Found::Nowhere);
        t.stored("a@96", 30_000);
        assert_eq!(t.find("a@96"), Found::Disk);
        t.decoded("a@96", 96 * 96 * 4);
        assert_eq!(t.find("a@96"), Found::Memory);
        t.decoded("b@96", 96 * 96 * 4);
        let gone = t.decoded("c@96", 96 * 96 * 4);
        assert_eq!(gone, vec!["a@96".to_string()], "memory holds two");
        assert_eq!(t.find("a@96"), Found::Disk, "still on disk: decode again, no fetch");
    }
}
