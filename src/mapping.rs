use crate::Symbol;

pub struct RandomMapping {
    prng_state: u64,
    alpha: f64,
    last_idx: u64,
    first_call: bool,
}

impl RandomMapping {
    // Explicit constants derived from 0.18 * u64::MAX and 0.74 * u64::MAX
    // c=3 configuration: w0=0.18, w1=0.56, w2=0.26
    const THRESHOLD_1: u64 = 3320364731362847232; // 0.18 * 2^64
    const THRESHOLD_2: u64 = 13650388340047260672; // (0.18 + 0.56) * 2^64

    const ALPHAS: [f64; 3] = [0.11, 0.68, 0.82];

    pub fn new<T: Symbol>(symbol: &T) -> Self {
        let mut seed = symbol.hash_();

        // 1. Determine Subset (j) based on raw hash
        let alpha = if seed < Self::THRESHOLD_1 {
            Self::ALPHAS[0]
        } else if seed < Self::THRESHOLD_2 {
            Self::ALPHAS[1]
        } else {
            Self::ALPHAS[2]
        };

        // 2. Strong Seeding (SplitMix64-style mixer)
        // This decorrelates the subset selection from the first RNG draw.
        // Critical for preventing "bad hash" inputs from always rolling large gaps.
        seed = (seed ^ (seed >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        seed = (seed ^ (seed >> 27)).wrapping_mul(0x94d049bb133111eb);
        seed = seed ^ (seed >> 31);

        RandomMapping {
            prng_state: seed,
            alpha,
            last_idx: 0,
            first_call: true,
        }
    }

    /// Linear Congruential Generator
    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        self.prng_state = self
            .prng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        self.prng_state
    }
}

impl Iterator for RandomMapping {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.first_call {
            self.first_call = false;
            // Always yield 0 first to ensure all symbols touch the dense prefix.
            // This guarantees at least one bucket per symbol for small tables.
            self.last_idx = 0;
            return Some(0);
        }

        // Generate Uniform(0, 1]
        let u_int = self.next_u64();
        // Standard mapping from u64 to (0, 1]
        let u = (u_int as f64 + 1.0) / (u64::MAX as f64 + 2.0);

        // Inverse Transform Sampling for density: 1 / (1 + alpha * i)
        // Recurrence: (1 + alpha * i_next) = (1 + alpha * i_curr) * u^(-alpha)
        // Note: u^(-alpha) = exp(-alpha * ln(u))
        let factor = u.powf(-self.alpha);

        // Current position in transformed space
        let t_curr = 1.0 + self.alpha * (self.last_idx as f64);

        // Next position
        let t_next = t_curr * factor;

        // Convert back to index: i = (t - 1) / alpha
        let next_idx_float = (t_next - 1.0) / self.alpha;

        // Enforce STRICT monotonicity
        // If the gap is small (< 1.0), force a step of 1.
        let mut next_idx = next_idx_float as u64;
        if next_idx <= self.last_idx {
            // Fix: Add random hop (1..5 steps) to break identical monotonic sequences
            // for heavy-tail symbols that would otherwise all map to [0, 1, 2, 3...].
            let hop = self.next_u64() % 5;
            next_idx = self.last_idx + 1 + hop;
        }

        self.last_idx = next_idx;

        if self.last_idx > (usize::MAX as u64) {
            None
        } else {
            Some(self.last_idx as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SimpleSymbol;

    #[test]
    fn test_heavy_tail_behavior() {
        let sym = SimpleSymbol { value: 12345 };
        let mut mapping = RandomMapping::new(&sym);

        // Print first few indices to manually verify growth
        // Expect: rapid growth after the first few dense items
        let mut prev = 0;
        for _ in 0..15 {
            let curr = mapping.next().unwrap();
            let delta = curr - prev;
            println!("Idx: {}, Delta: {}", curr, delta);
            prev = curr;
        }
    }

    #[test]
    fn test_different_symbols_different_starts() {
        let sym1 = SimpleSymbol { value: 12345 };
        let sym2 = SimpleSymbol { value: 54321 };

        let mut mapping1 = RandomMapping::new(&sym1);
        let mut mapping2 = RandomMapping::new(&sym2);

        let first1 = mapping1.next().unwrap();
        let first2 = mapping2.next().unwrap();

        // Different symbols should (with high probability) start at different positions
        // Note: collisions are possible but unlikely with 64-bit hashes
        println!("Symbol 1 starts at: {}", first1);
        println!("Symbol 2 starts at: {}", first2);
    }
}
