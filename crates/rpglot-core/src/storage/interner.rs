use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use xxhash_rust::xxh3::xxh3_64;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct StringInterner {
    // Map hash to the actual string
    strings: HashMap<u64, String>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a string and returns its hash (ID).
    pub fn intern(&mut self, s: &str) -> u64 {
        let hash = xxh3_64(s.as_bytes());
        self.strings.entry(hash).or_insert_with(|| s.to_string());
        hash
    }

    /// Resolves a hash back to a string.
    pub fn resolve(&self, hash: u64) -> Option<&str> {
        self.strings.get(&hash).map(|s| s.as_str())
    }

    pub fn clear(&mut self) {
        self.strings.clear();
        self.strings.shrink_to_fit();
    }

    /// Merges another interner into this one.
    pub fn merge(&mut self, other: &StringInterner) {
        for (hash, s) in &other.strings {
            self.strings.entry(*hash).or_insert_with(|| s.clone());
        }
    }

    /// Creates a new interner containing only strings with hashes in the given set.
    /// Used to optimize chunk storage by removing unused strings.
    pub fn filter(&self, used_hashes: &HashSet<u64>) -> StringInterner {
        let strings = self
            .strings
            .iter()
            .filter(|(hash, _)| used_hashes.contains(hash))
            .map(|(h, s)| (*h, s.clone()))
            .collect();
        StringInterner { strings }
    }

    /// Returns the number of interned strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Returns true if the interner contains no strings.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interning() {
        let mut interner = StringInterner::new();
        let s1 = "very long string xxxxxxxxxxxxxxxxxxxxxxx";
        let h1 = interner.intern(s1);
        let h2 = interner.intern(s1);

        assert_eq!(h1, h2);
        assert_eq!(interner.resolve(h1), Some(s1));
        assert_eq!(interner.strings.len(), 1);
    }
}
