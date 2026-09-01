//! Determinism kernel (A28): stable hashing and seeded RNG.
//!
//! Every ranked response must be a pure function of (state, config, user, day).
//! Rules enforced from here on: randomness only through `DetRng` seeded via
//! `seed_for`; hashing only through FNV-1a (never std's default hasher, which
//! is not stable across Rust releases or processes); no wall-clock reads
//! outside the dispatch-time day default in main.rs.

/// FNV-1a 64-bit. Stable across platforms, releases, and processes.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Derive the seed for one (user, day, purpose) triple. `purpose` separates
/// streams so feed exploration and any future shuffle never share a sequence.
pub fn seed_for(user_id: &str, day_epoch: u64, purpose: &str) -> u64 {
    let mut key = Vec::with_capacity(user_id.len() + purpose.len() + 10);
    key.extend_from_slice(user_id.as_bytes());
    key.push(0x1f);
    key.extend_from_slice(&day_epoch.to_le_bytes());
    key.push(0x1f);
    key.extend_from_slice(purpose.as_bytes());
    fnv1a(&key)
}

/// SplitMix64: tiny, well-distributed, fully deterministic. Not cryptographic,
/// which is fine: this stream picks feed slots, it does not guard secrets.
pub struct DetRng {
    state: u64,
}

impl DetRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, n). n = 0 returns 0. Modulo bias is negligible for
    /// slot-pool sizes against a 64-bit stream.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_pinned() {
        // The empty-string hash is the FNV offset basis by definition. If this
        // moves, every seed, config hash, and golden in the engine moves too.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a(b"aura"), fnv1a(b"arua"));
    }

    #[test]
    fn seeds_separate_by_user_day_purpose() {
        let a = seed_for("local", 20000, "feed-explore");
        assert_eq!(a, seed_for("local", 20000, "feed-explore"));
        assert_ne!(a, seed_for("local", 20001, "feed-explore"));
        assert_ne!(a, seed_for("other", 20000, "feed-explore"));
        assert_ne!(a, seed_for("local", 20000, "other-purpose"));
    }

    #[test]
    fn rng_is_reproducible() {
        let mut a = DetRng::new(42);
        let mut b = DetRng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_eq!(DetRng::new(7).below(0), 0);
    }
}
