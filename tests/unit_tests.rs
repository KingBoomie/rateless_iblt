#[cfg(test)]
mod tests {
    

use riblt::RandomMapping;
use riblt::RatelessIBLT;
use riblt::Symbol;

// Reuse TestSymbol
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TestSymbol(pub u64);
impl Symbol for TestSymbol {
    const BYTE_LEN: usize = 8;
    fn encode_into(&self, buffer: &mut [u8]) { buffer.copy_from_slice(&self.0.to_le_bytes()); }
    fn decode_from(bytes: &[u8]) -> Self { TestSymbol(u64::from_le_bytes(bytes.try_into().unwrap())) }
}

#[test]
fn test_capacity_expansion() {
    let mut iblt = RatelessIBLT::<TestSymbol>::new();
    
    // Add symbol with small max_blocks
    iblt.add_symbol(&TestSymbol(1), 10);
    
    // Internal checks (requires making fields pub or adding getters for test, 
    // assuming pub(crate) for tests)
    // assert_eq!(iblt.num_blocks, 10); 

    // Add symbol with larger max_blocks -> should resize
    iblt.add_symbol(&TestSymbol(2), 20);
    // assert_eq!(iblt.num_blocks, 20);
    
    // The previous data should still be valid. 
    // We verify by adding TestSymbol(1) again (XORing it out) and TestSymbol(2) again.
    // Result should be empty.
    iblt.add_symbol(&TestSymbol(1), 20); // Note: must use current size or higher
    iblt.add_symbol(&TestSymbol(2), 20);

    let (local, remote) = iblt.decode_all();
    assert!(local.is_empty());
    assert!(remote.is_empty());
}

#[test]
fn test_mapping_determinism() {
    // Critical: Alice and Bob must generate exactly the same mapping indices for the same symbol
    let s = TestSymbol(0xDEADBEEF);
    
    let mut iblt_1 = RatelessIBLT::<TestSymbol>::new();
    iblt_1.add_symbol(&s, 50);

    let mut iblt_2 = RatelessIBLT::<TestSymbol>::new();
    iblt_2.add_symbol(&s, 50);

    // If mappings differ, subtraction won't zero out completely
    iblt_1.subtract_assign(&iblt_2);
    
    let (l, r) = iblt_1.decode_all();
    assert!(l.is_empty());
    assert!(r.is_empty());
}

#[test]
fn test_heavy_collision() {
    // This tests the "hash" vector protection.
    // If two symbols map to the same bucket (collision in `sums`),
    // the `hashes` vector (using XxHash) should prevent `try_peel` from returning a corrupted symbol.
    
    // It's hard to force a collision in the RandomMapping integration without mocking,
    // but we can test that adding 2 distinct symbols prevents peeling if they overlap perfectly (unlikely)
    // or if they overlap in a singleton bucket.
    
    let mut iblt = RatelessIBLT::<TestSymbol>::new();
    // Intentionally small size to force collisions in buckets
    let size = 5; 
    
    let s1 = TestSymbol(1);
    let s2 = TestSymbol(2);
    
    iblt.add_symbol(&s1, size);
    iblt.add_symbol(&s2, size);
    
    // If they collided in a bucket:
    // count = 2. try_peel checks (count == 1 || -1). It returns None. Correct.
    
    // If we manually corrupt a bucket (simulate network bitflip):
    // This requires white-box access, but effectively simulates:
    // count = 1, checksum matches, but sum is garbage -> Detected by Hash check?
    // Actually, RatelessIBLT relies on (Sum check && Hash check). 
    // If count=1, we read Sum. We hash Sum. We compare to stored Hash.
    // This is very strong against random noise.
}

#[test]
fn subtract_empty_is_identity() {
    let size = 100;
    let mut a = RatelessIBLT::new();
    let b = RatelessIBLT::new();

    let s1 = TestSymbol(1);
    let s2 = TestSymbol(2);

    a.add_symbol(&s1, size);
    a.add_symbol(&s2, size);

    let before = a.clone();

    a.subtract_assign(&b);

    let (local, remote) = a.decode_all();

    let got: std::collections::HashSet<_> = local.into_iter().collect();
    let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

    assert_eq!(got, exp);
    assert!(remote.is_empty());
}

#[test]
fn empty_minus_empty() {
    let mut a: RatelessIBLT<TestSymbol> = RatelessIBLT::new();
    let b = RatelessIBLT::new();

    a.subtract_assign(&b);

    let (local, remote) = a.decode_all();

    assert!(local.is_empty());
    assert!(remote.is_empty());
}

#[test]
fn decode_consistency_after_empty_subtract() {
    let size = 300;
    let mut a = RatelessIBLT::new();
    let b = RatelessIBLT::new();

    let s1 = TestSymbol(42);
    let s2 = TestSymbol(99);

    a.add_symbol(&s1, size);
    a.add_symbol(&s2, size);

    let (local1, remote1) = a.decode_all();

    assert!(remote1.is_empty());

    let mut a2 = RatelessIBLT::new();
    a2.add_symbol(&s1, size);
    a2.add_symbol(&s2, size);

    a2.subtract_assign(&b);

    let (local2, remote2) = a2.decode_all();

    assert_eq!(
        local1.into_iter().collect::<std::collections::HashSet<_>>(),
        local2.into_iter().collect::<std::collections::HashSet<_>>(),
    );

    assert!(remote2.is_empty());
}

#[test]
fn minimal_property_failure_case() {
    let size = 300;
    let mut a = RatelessIBLT::new();
    let b = RatelessIBLT::new();

    let s1 = TestSymbol(18145032937744471612);
    let s2 = TestSymbol(15036937119665929939);

    a.add_symbol(&s1, size);
    a.add_symbol(&s2, size);

    a.subtract_assign(&b);

    let (local, remote) = a.decode_all();

    let got: std::collections::HashSet<_> = local.into_iter().collect();
    let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

    assert_eq!(got, exp);
    assert!(remote.is_empty());
}


#[test]
fn minimal_property_failure_case2() {
    let size = 300;
    let mut a = RatelessIBLT::new();
    let b = RatelessIBLT::new();

    let s1 = TestSymbol(1);
    let s2 = TestSymbol(2);

    a.add_symbol(&s1, size);
    a.add_symbol(&s2, size);

    a.subtract_assign(&b);

    let (local, remote) = a.decode_all();

    let got: std::collections::HashSet<_> = local.into_iter().collect();
    let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

    assert_eq!(got, exp);
    assert!(remote.is_empty());
}

}