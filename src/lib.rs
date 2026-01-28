#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

// mod encoder;
mod encoder3;
mod mapping;
mod symbol;

pub use encoder3::{PeelableResult, RatelessIBLT};
pub use mapping::RandomMapping;
pub use symbol::Symbol;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RibltError {
    /// Insufficient space in the IBLT to add symbols or increase capacity.
    CapacityExceeded,
    /// The provided output buffer for decoding is too small to hold the result.
    OutputBufferTooSmall,
    /// Internal decoding queue overflowed (should generally be sized MAX_BLOCKS).
    QueueOverflow,
}

#[cfg(test)]
mod tests {
    use crate::Symbol;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct TestSymbol(pub u64);
    impl Symbol for TestSymbol {
        const BYTE_LEN: usize = 8;
        fn encode_into(&self, buffer: &mut [u8]) {
            buffer.copy_from_slice(&self.0.to_le_bytes());
        }
        fn decode_from(bytes: &[u8]) -> Self {
            TestSymbol(u64::from_le_bytes(bytes.try_into().unwrap()))
        }
    }
}
