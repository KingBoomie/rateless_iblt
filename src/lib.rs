// mod encoder;
mod encoder3;
mod mapping;
mod symbol;

pub use encoder3::{RatelessIBLT, PeelableResult};
pub use mapping::RandomMapping;
pub use symbol::{Symbol};


#[cfg(test)]
mod tests {
    use crate::Symbol;

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct TestSymbol(pub u64);
    impl Symbol for TestSymbol {
        const BYTE_LEN: usize = 8;
        fn encode_into(&self, buffer: &mut [u8]) { buffer.copy_from_slice(&self.0.to_le_bytes()); }
        fn decode_from(bytes: &[u8]) -> Self { TestSymbol(u64::from_le_bytes(bytes.try_into().unwrap())) }
    }

}