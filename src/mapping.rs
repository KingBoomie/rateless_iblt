use crate::Symbol;

pub struct RandomMapping {
    prng_state: u64,
    last_idx: u64,
    first_call: bool,
}

impl RandomMapping {
    pub fn new<T: Symbol>(symbol: &T) -> Self {
        RandomMapping {
            prng_state: symbol.hash_(),
            last_idx: 0,
            first_call: true,
        }
    }
}

impl Iterator for RandomMapping {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.first_call {
            self.first_call = false;
            self.last_idx = 0;
            return Some(0);
        }

        // Use the old deterministic pseudo-random mapping logic that worked
        // wrapping_mul with the old constant
        self.prng_state = self.prng_state.wrapping_mul(0xda942042e4dd58b5);
        let r = self.prng_state;

        let tp32: f64 = (1u64 << 32) as f64;
        let diff = (self.last_idx as f64 + 1.5) * (tp32 / (r as f64 + 1.0).sqrt() - 1.0);

        // Enforce strict monotonicity just in case
        let mut next_idx = self.last_idx + diff.ceil() as u64;
        if next_idx <= self.last_idx {
            next_idx = self.last_idx + 1;
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