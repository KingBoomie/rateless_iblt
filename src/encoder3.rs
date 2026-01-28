use core::fmt::Debug;
use core::marker::PhantomData;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::RibltError;
use crate::mapping::RandomMapping;
use crate::ringbuffer::IndexQueue;
use crate::symbol::Symbol;
use serde_big_array::BigArray;

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
///
/// Uses fixed-size arrays driven by const generics (no heap allocation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RatelessIBLT<T: Symbol, const NUM_BLOCKS: usize, const SUMS_BYTES: usize> {
    /// Flattened buffer of sums.
    /// Block `i` is at `sums[i*len .. (i+1)*len]`
    #[serde(with = "BigArray")]
    sums: [u8; SUMS_BYTES],
    /// Vector of hash XOR sums
    #[serde(with = "BigArray")]
    hashes: [u64; NUM_BLOCKS],
    /// Vector of counts (local - remote)
    #[serde(with = "BigArray")]
    counts: [i64; NUM_BLOCKS],
    /// Tracks max capacity usage (for compatibility, though fixed capacity is enforced)
    active_blocks: usize,
    _marker: PhantomData<T>,
}

// No-op - removed unused K constant

impl<T: Symbol, const NUM_BLOCKS: usize, const SUMS_BYTES: usize>
    RatelessIBLT<T, NUM_BLOCKS, SUMS_BYTES>
{
    pub fn new() -> Self {
        // Assert sums buffer is large enough for N blocks * symbol size.
        // In stable Rust, this is a runtime panic in new (or compile time if we used static assertions, but simple assert is fine).
        // Since SUMS_BYTES is const, this will likely be optimized out or hit immediately.
        assert!(
            SUMS_BYTES >= NUM_BLOCKS * T::BYTE_LEN,
            "RatelessIBLT: SUMS_BYTES must be >= NUM_BLOCKS * T::BYTE_LEN"
        );

        Self {
            sums: [0u8; SUMS_BYTES],
            hashes: [0u64; NUM_BLOCKS],
            counts: [0i64; NUM_BLOCKS],
            active_blocks: 0,
            _marker: PhantomData,
        }
    }

    /// XOR helper acting on slices.
    #[inline(always)]
    fn xor_block(dest: &mut [u8], src: &[u8]) {
        debug_assert_eq!(dest.len(), src.len());
        for (d, s) in dest.iter_mut().zip(src) {
            *d ^= s;
        }
    }

    /// No-op in static impl, or basic check.
    /// Returns error if requesting more than static capacity.
    pub fn ensure_capacity(&mut self, max_index: usize) -> Result<(), RibltError> {
        if max_index >= NUM_BLOCKS {
            return Err(RibltError::CapacityExceeded);
        }
        // Current implementation is fixed size, so memory is always there.
        // We just track 'active' blocks if we wanted to support smaller logical sizes,
        // but for now we just validate bounds.
        if max_index >= self.active_blocks {
            self.active_blocks = max_index + 1;
        }
        Ok(())
    }

    /// Adds a symbol to the structure.
    pub fn add_symbol(&mut self, symbol: &T, max_blocks: usize) -> Result<(), RibltError> {
        // Ensure we have capacity for the range we are about to touch
        if max_blocks > 0 {
            self.ensure_capacity(max_blocks - 1)?;
        }

        let sym_hash = symbol.hash_();
        // Stack allocation for encoded bytes.
        // We need a const size for this buffer.
        // Limitation: generic const exprs not stable to do [0u8; T::BYTE_LEN].
        // Workaround: We use a small scratch buffer and panic if Symbol is too large?
        // Or we rely on `encode_into` which takes a slice.
        // For no-alloc, we can't Vec.
        // Let's assume T::BYTE_LEN is relatively small (it usually is for IBLT symbols).
        // Since we can't stack allocate dynamic size, and can't use T::BYTE_LEN in array decl without nightly.
        // We will user a "safe max" buffer or ask user to provide one?
        // Actually, if we link `no_std` with `alloc` we can use `vec!`.
        // BUT the user asked for "no-alloc".
        // Solution: We iterate. `xor_block` uses slices.
        // We can't allocate a temp buffer on stack without nightly.
        // Wait, T::BYTE_LEN is a const. We CAN do [0u8; T::BYTE_LEN] on Nightly, but not Stable.
        // For Stable no-alloc, we might need a generic parameter `const SYM_LEN: usize` on `RatelessIBLT`?
        // Or we pass a scratch buffer to `add_symbol`?
        // Let's use `alloc` if available, otherwise?
        // Since `RatelessIBLT` is `no_std` but `no_alloc` phase implies we shouldn't use `Vec`.
        // However, `lib.rs` currently DOES extern crate alloc.
        // User asked: "can we have all allocations static?".
        // If we strictly follow that, we cannot use `vec!`.
        // Let's require the user to genericize `SYM_LEN` or similar.
        // Or we just hardcode a MAX_SYM_LEN (e.g. 256 bytes) for the scratch?
        // Let's use a reasonable stack buffer limit for now, e.g. 1024 bytes.
        const MAX_SYM_SIZE: usize = 512;
        assert!(
            T::BYTE_LEN <= MAX_SYM_SIZE,
            "Symbol too large for stack buffer"
        );
        let mut encoded = [0u8; MAX_SYM_SIZE];
        let encoded_slice = &mut encoded[..T::BYTE_LEN];

        symbol.encode_into(encoded_slice);

        // Pass max_blocks to create hashed offset for starting position
        let mapping = RandomMapping::new(symbol);

        for block_idx in mapping.take_while(|&idx| idx < max_blocks) {
            let start = block_idx * T::BYTE_LEN;
            let end = start + T::BYTE_LEN;

            Self::xor_block(&mut self.sums[start..end], encoded_slice);
            self.hashes[block_idx] ^= sym_hash;
            self.counts[block_idx] += 1;
        }
        Ok(())
    }

    pub fn combine_assign(
        &mut self,
        other: &RatelessIBLT<T, NUM_BLOCKS, SUMS_BYTES>,
    ) -> Result<(), RibltError> {
        if other.active_blocks == 0 {
            return Ok(());
        }
        self.ensure_capacity(other.active_blocks - 1)?;

        // Sums are same size by type definition
        Self::xor_block(&mut self.sums, &other.sums);

        for (h_self, h_other) in self.hashes.iter_mut().zip(&other.hashes) {
            *h_self ^= h_other;
        }

        for (c_self, c_other) in self.counts.iter_mut().zip(&other.counts) {
            *c_self += c_other;
        }
        Ok(())
    }

    /// Subtract another IBLT from this one (Difference).
    pub fn subtract_assign(
        &mut self,
        other: &RatelessIBLT<T, NUM_BLOCKS, SUMS_BYTES>,
    ) -> Result<(), RibltError> {
        if other.active_blocks == 0 {
            return Ok(());
        }
        self.ensure_capacity(other.active_blocks - 1)?;

        Self::xor_block(&mut self.sums, &other.sums);

        for (h_self, h_other) in self.hashes.iter_mut().zip(&other.hashes) {
            *h_self ^= h_other;
        }

        for (c_self, c_other) in self.counts.iter_mut().zip(&other.counts) {
            *c_self -= c_other;
        }
        Ok(())
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

        let dir = if count == 1 {
            Direction::Add
        } else {
            Direction::Remove
        };
        Some((symbol, dir))
    }

    pub fn decode_all(
        &mut self,
        local_out: &mut [T],
        remote_out: &mut [T],
    ) -> Result<(usize, usize), RibltError> {
        let mut local_count = 0;
        let mut remote_count = 0;

        if NUM_BLOCKS == 0 {
            return Ok((0, 0));
        }

        // Stack-allocated queue.
        // Note: Function stack usage is approx sizeof(usize) * NUM_BLOCKS + sizeof(bool) * NUM_BLOCKS
        let mut queue = IndexQueue::<NUM_BLOCKS>::new();

        // 1. Scan for initially decodable buckets
        for i in 0..self.active_blocks {
            let c = self.counts[i];
            if c == 1 || c == -1 {
                queue.push_unique(i)?;
            }
        }

        // 2. Process queue
        while let Some(idx) = queue.pop() {
            // Try to recover symbol
            let (symbol, dir) = match self.try_peel(idx) {
                Some(res) => res,
                None => continue, // Checksum failed or count changed since enqueue
            };

            match dir {
                Direction::Add => {
                    if local_count >= local_out.len() {
                        return Err(RibltError::OutputBufferTooSmall);
                    }
                    local_out[local_count] = symbol.clone();
                    local_count += 1;
                }
                Direction::Remove => {
                    if remote_count >= remote_out.len() {
                        return Err(RibltError::OutputBufferTooSmall);
                    }
                    remote_out[remote_count] = symbol.clone();
                    remote_count += 1;
                }
            }

            // Re-encode and subtract from table
            // [Note: This section would be replaced by Refactor #1 in the future]
            let mapping = RandomMapping::new(&symbol);
            let encoded_hash = symbol.hash_();

            const MAX_SYM_SIZE: usize = 512;
            let mut encoded = [0u8; MAX_SYM_SIZE];
            let encoded_slice = &mut encoded[..T::BYTE_LEN];
            symbol.encode_into(encoded_slice);

            let count_delta = match dir {
                Direction::Add => -1,
                Direction::Remove => 1,
            };

            for other_idx in mapping.take_while(|&idx| idx < self.active_blocks) {
                let start = other_idx * T::BYTE_LEN;

                Self::xor_block(&mut self.sums[start..start + T::BYTE_LEN], encoded_slice);
                self.hashes[other_idx] ^= encoded_hash;
                self.counts[other_idx] += count_delta;

                let c = self.counts[other_idx];
                if c == 1 || c == -1 {
                    queue.push_unique(other_idx)?;
                }
            }
        }

        Ok((local_count, remote_count))
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

            RatelessIBLT::<TestSymbol, 0, 0>::xor_block(&mut a, &b);
            RatelessIBLT::<TestSymbol, 0, 0>::xor_block(&mut a, &b);

            assert_eq!(a, orig, "xor_block is not reversible");
        }
    }
}
