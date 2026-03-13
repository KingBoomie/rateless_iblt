use core::fmt::Debug;
use core::hash::Hasher;

#[cfg(not(feature = "std"))]
use alloc::vec;

use twox_hash::XxHash64; // Add dependency: twox-hash = "1.6"

pub trait Symbol: Clone + Debug {
    const BYTE_LEN: usize;

    /// Writes the symbol's binary representation into the provided buffer.
    /// Panics if buffer.len() != Self::BYTE_LEN
    fn encode_into(&self, buffer: &mut [u8]);

    /// Reads a symbol from the binary representation.
    fn decode_from(bytes: &[u8]) -> Self;

    /// Calculates a deterministic hash of the symbol.
    /// Crucial: Must use a stable hasher (XxHash64 seeded with 0) so
    /// distinct peers (Alice/Bob) generate identical hashes for the same symbol.
    fn hash_(&self) -> u64 {
        let mut hasher = XxHash64::with_seed(0);
        // We use a small scratch buffer here. For max perf,
        // implementers might override hash_ to hash fields directly.
        let mut buffer = vec![0u8; Self::BYTE_LEN];
        self.encode_into(&mut buffer);
        hasher.write(&buffer);
        hasher.finish()
    }
}

// --- Test Helper ---
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleSymbol {
    pub value: u64,
}

#[cfg(test)]
impl Symbol for SimpleSymbol {
    const BYTE_LEN: usize = 8;

    fn encode_into(&self, buffer: &mut [u8]) {
        buffer.copy_from_slice(&self.value.to_le_bytes());
    }

    fn decode_from(bytes: &[u8]) -> Self {
        let arr: [u8; 8] = bytes.try_into().expect("Slice len must be 8");
        SimpleSymbol {
            value: u64::from_le_bytes(arr),
        }
    }
}
