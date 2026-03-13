use riblt::{RatelessIBLT, Symbol};
use std::collections::HashSet;

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

#[test]
fn test_reproduce_failure() {
    let set_a_vals = vec![
        0,
        16214372863835230174,
        1,
        2,
        3,
        4,
        5,
    ];
    let set_b_vals = vec![
        6,
        7,
        8,
        9,
        7783824481870909809,
        10,
        11,
        12,
        13,
        14,
        15,
        16,
        17,
    ];

    let mut set_a: Vec<TestSymbol> = set_a_vals.into_iter().map(TestSymbol).collect();
    let mut set_b: Vec<TestSymbol> = set_b_vals.into_iter().map(TestSymbol).collect();

    set_a.sort_by_key(|s| s.0); set_a.dedup();
    set_b.sort_by_key(|s| s.0); set_b.dedup();

    let set_a_hash: HashSet<_> = set_a.iter().cloned().collect();
    let set_b_hash: HashSet<_> = set_b.iter().cloned().collect();

    let total_diff = set_a_hash.difference(&set_b_hash).count() + set_b_hash.difference(&set_a_hash).count();

    // The user's sizing logic in the failed test:
    // if total_diff < 20 { 300 } else { (total_diff * 3) + 50 }
    // total_diff is 20, so size is 20 * 3 + 50 = 110.
    let size = if total_diff < 20 {
         300 
    } else {
         (total_diff * 3) + 50
    };
    
    println!("Total diff: {}, Size: {}", total_diff, size);

    let mut iblt_a = RatelessIBLT::new();
    let mut iblt_b = RatelessIBLT::new();

    for x in &set_a { iblt_a.add_symbol(x, size); }
    for x in &set_b { iblt_b.add_symbol(x, size); }

    iblt_a.subtract_assign(&iblt_b);
    let (unique_a, unique_b) = iblt_a.decode_all();

    let res_a_hash: HashSet<_> = unique_a.into_iter().collect();
    let res_b_hash: HashSet<_> = unique_b.into_iter().collect();

    let expected_a: HashSet<_> = set_a_hash.difference(&set_b_hash).cloned().collect();
    let expected_b: HashSet<_> = set_b_hash.difference(&set_a_hash).cloned().collect();

    assert_eq!(res_a_hash, expected_a, "Decoded Local (A-B) mismatch");
    assert_eq!(res_b_hash, expected_b, "Decoded Remote (B-A) mismatch");
}
