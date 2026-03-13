use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::marker::PhantomData;

use crate::symbol::Symbol;
use crate::mapping::RandomMapping;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeelableResult<T: Symbol> {
    Local(T),
    Remote(T),
    NotPeelable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Add = 1,
    Remove = -1,
}

/// A Rateless IBLT block storage optimized for performance.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RatelessIBLT<T: Symbol> {
    /// Flattened buffer of sums.
    /// Block `i` is at `sums[i*len .. (i+1)*len]`
    sums: Vec<u8>,
    /// Vector of hash XOR sums
    hashes: Vec<u64>,
    /// Vector of counts (local - remote)
    counts: Vec<i64>,
    
    num_blocks: usize,
    _marker: PhantomData<T>,
}

impl<T: Symbol> Default for RatelessIBLT<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Symbol> RatelessIBLT<T> {
    pub fn new() -> Self {
        Self {
            sums: Vec::new(),
            hashes: Vec::new(),
            counts: Vec::new(),
            num_blocks: 0,
            _marker: PhantomData,
        }
    }

    /// Optimized XOR helper acting on u64 chunks for speed
    #[inline(always)]
    fn xor_block(dest: &mut [u8], src: &[u8]) {
        let len = dest.len();
        debug_assert_eq!(len, src.len());

        let (d_pre, d_mid, d_suf) = unsafe { dest.align_to_mut::<u64>() };
        let (s_pre, s_mid, s_suf) = unsafe { src.align_to::<u64>() };

        for (d, s) in d_pre.iter_mut().zip(s_pre) { *d ^= s; }
        for (d, s) in d_mid.iter_mut().zip(s_mid) { *d ^= s; }
        for (d, s) in d_suf.iter_mut().zip(s_suf) { *d ^= s; }
    }

    /// Ensures storage exists up to `max_index`.
    pub fn ensure_capacity(&mut self, max_index: usize) {
        if max_index < self.num_blocks {
            return;
        }
        let new_len = max_index + 1;
        self.sums.resize(new_len * T::BYTE_LEN, 0);
        self.hashes.resize(new_len, 0);
        self.counts.resize(new_len, 0);
        self.num_blocks = new_len;
    }

    /// Adds a symbol to the structure.
    pub fn add_symbol(&mut self, symbol: &T, max_blocks: usize) {
        // Ensure we have capacity for the range we are about to touch
        if max_blocks > 0 {
            self.ensure_capacity(max_blocks - 1);
        }
        
        let sym_hash = symbol.hash_();
        let mut encoded = vec![0u8; T::BYTE_LEN]; 
        symbol.encode_into(&mut encoded);

        // Pass max_blocks to create hashed offset for starting position
        let mapping = RandomMapping::new(symbol);
        
        for block_idx in mapping.take_while(|&idx| idx < max_blocks) {
            let start = block_idx * T::BYTE_LEN;
            let end = start + T::BYTE_LEN;
            
            Self::xor_block(&mut self.sums[start..end], &encoded);
            self.hashes[block_idx] ^= sym_hash;
            self.counts[block_idx] += 1;
        }
    }

    pub fn combine_assign(&mut self, other: &RatelessIBLT<T>) {
        if other.num_blocks == 0 { return; }
        self.ensure_capacity(other.num_blocks - 1);

        Self::xor_block(&mut self.sums[0..other.sums.len()], &other.sums);
        
        for (h_self, h_other) in self.hashes.iter_mut().zip(&other.hashes) {
            *h_self ^= h_other;
        }
        
        for (c_self, c_other) in self.counts.iter_mut().zip(&other.counts) {
            *c_self += c_other;
        }
    }

    /// Subtract another IBLT from this one (Difference).
    pub fn subtract_assign(&mut self, other: &RatelessIBLT<T>) {
        if other.num_blocks == 0 { return; }
        // Fix: Ensure we are large enough to receive the subtraction
        self.ensure_capacity(other.num_blocks - 1);

        // Vectorized merge (XOR is its own inverse)
        // zip will stop at the end of the shorter slice, but we ensured self is >= other
        // so this processes all of 'other'.
        let len_to_copy = other.sums.len();
        Self::xor_block(&mut self.sums[0..len_to_copy], &other.sums);
        
        for (h_self, h_other) in self.hashes.iter_mut().zip(&other.hashes) {
            *h_self ^= h_other;
        }

        for (c_self, c_other) in self.counts.iter_mut().zip(&other.counts) {
            *c_self -= c_other;
        }
    }

    fn try_peel(&self, block_idx: usize) -> Option<(T, Direction)> {
        let count = self.counts[block_idx];
        if count != 1 && count != -1 {
            return None;
        }

        let start = block_idx * T::BYTE_LEN;
        let end = start + T::BYTE_LEN;
        let symbol = T::decode_from(&self.sums[start..end]);
        
        // Critical collision check
        if symbol.hash_() != self.hashes[block_idx] {
            return None; 
        }

        let dir = if count == 1 { Direction::Add } else { Direction::Remove };
        Some((symbol, dir))
    }

    pub fn decode_all(&mut self) -> (Vec<T>, Vec<T>) {
        let mut local_unique = Vec::new();
        let mut remote_unique = Vec::new();
        let mut queue = Vec::new();

        // 1. Scan for decodable buckets
        for i in 0..self.num_blocks {
            if self.counts[i] == 1 || self.counts[i] == -1 {
                queue.push(i);
            }
        }

        while let Some(idx) = queue.pop() {
            // 2. Try to recover symbol
            let (symbol, dir) = match self.try_peel(idx) {
                Some(res) => res,
                None => continue, // Checksum failed or count changed
            };

            match dir {
                Direction::Add => local_unique.push(symbol.clone()),
                Direction::Remove => remote_unique.push(symbol.clone()),
            }

            // 3. Re-encode and subtract from table
            let mapping = RandomMapping::new(&symbol);
            let encoded_hash = symbol.hash_();
            let mut encoded_bytes = vec![0u8; T::BYTE_LEN];
            symbol.encode_into(&mut encoded_bytes);
            
            // Determine arithmetic modification
            // If we found a Local symbol (count 1), we remove it (-1).
            // If we found a Remote symbol (count -1), we remove it (-(-1) = +1).
            let count_delta = match dir {
                Direction::Add => -1,
                Direction::Remove => 1,
            };

            for other_idx in mapping.take_while(|&idx| idx < self.num_blocks) {
                let start = other_idx * T::BYTE_LEN;
                
                // XOR is self-inverse: works for both adding and removing
                Self::xor_block(&mut self.sums[start..start + T::BYTE_LEN], &encoded_bytes);
                self.hashes[other_idx] ^= encoded_hash;
                
                // Apply arithmetic update
                self.counts[other_idx] += count_delta;

                // Check if this neighbor is now decodable
                let c = self.counts[other_idx];
                if c == 1 || c == -1 {
                    queue.push(other_idx);
                }
            }
        }

        (local_unique, remote_unique)
    }
}


#[cfg(test)]
mod tests {
    use crate::{RatelessIBLT, tests::TestSymbol};

#[test]
fn xor_block_is_lossless_roundtrip() {
    use rand::Rng;

    let mut rng = rand::rng();

    for _ in 0..10_000 {
        let len = 37; // intentionally not multiple of 8
        let mut a = vec![0u8; len];
        let mut b = vec![0u8; len];

        rng.fill(&mut a[..]);
        rng.fill(&mut b[..]);

        let orig = a.clone();

        RatelessIBLT::<TestSymbol>::xor_block(&mut a, &b);
        RatelessIBLT::<TestSymbol>::xor_block(&mut a, &b);

        assert_eq!(a, orig, "xor_block is not reversible");
    }
}
}