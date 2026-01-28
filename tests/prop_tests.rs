use proptest::prelude::*;
use riblt::RatelessIBLT; // Ensure this matches your Cargo.toml package name
use riblt::Symbol;
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

prop_compose! {
    fn arb_symbol()(val in any::<u64>()) -> TestSymbol {
        TestSymbol(val)
    }
}

prop_compose! {
    fn arb_symbol_vec(max_len: usize)(vec in proptest::collection::vec(arb_symbol(), 0..max_len)) -> Vec<TestSymbol> {
        vec
    }
}

proptest! {

    #![proptest_config(ProptestConfig {
            
            failure_persistence: None,
            max_shrink_iters: 4096,
            // timeout: 10000,
            ..ProptestConfig::default()
        })]

    #[test]
    fn test_linearity_of_addition(
        set_a in arb_symbol_vec(50),
        set_b in arb_symbol_vec(50)
    ) {
        // Linearity should hold regardless of decoding success, 
        // but we use a large size to ensure we can inspect the results cleanly.
        let size = 500; 
        let mut iblt_a = RatelessIBLT::new();
        let mut iblt_b = RatelessIBLT::new();
        let mut iblt_combined_immed = RatelessIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size); }
        for x in &set_b { iblt_b.add_symbol(x, size); }
        
        for x in set_a.iter().chain(set_b.iter()) {
            iblt_combined_immed.add_symbol(x, size);
        }

        let mut iblt_combined_algebra = iblt_a.clone();
        iblt_combined_algebra.combine_assign(&iblt_b);

        iblt_combined_algebra.subtract_assign(&iblt_combined_immed);
        let (local, remote) = iblt_combined_algebra.decode_all();
        
        prop_assert!(local.is_empty(), "Local artifacts found (Linearity check)");
        prop_assert!(remote.is_empty(), "Remote artifacts found (Linearity check)");
    }

    #[test]
    fn test_set_difference_decoding(
        mut set_a in arb_symbol_vec(30),
        mut set_b in arb_symbol_vec(30)
    ) {
        // 1. Enforce Set Semantics (Deduplicate inputs)
        // IBLTs are multisets by default. To test Set Difference, we must strictly 
        // provide sets, otherwise count=2 (duplicate) looks like a collision.
        set_a.sort_by_key(|s| s.0); set_a.dedup();
        set_b.sort_by_key(|s| s.0); set_b.dedup();

        let set_a_hash: HashSet<_> = set_a.iter().cloned().collect();
        let set_b_hash: HashSet<_> = set_b.iter().cloned().collect();

        // 2. Calculate Expected Difference
        let diff_a_b: Vec<_> = set_a_hash.difference(&set_b_hash).collect();
        let diff_b_a: Vec<_> = set_b_hash.difference(&set_a_hash).collect();
        let total_diff = diff_a_b.len() + diff_b_a.len();

        // 3. Robust Sizing Strategy
        // Heavy-tail distributions have high variance for small N.
        // We set a minimum floor of 300 to effectively eliminate the 
        // "bad RNG roll" where all items jump past the buffer.
        let size = if total_diff < 20 {
             300 
        } else {
             (total_diff * 3) + 50
        };

        let mut iblt_a = RatelessIBLT::new();
        let mut iblt_b = RatelessIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size); }
        for x in &set_b { iblt_b.add_symbol(x, size); }

        // 4. Perform Subtraction and Decode
        iblt_a.subtract_assign(&iblt_b);
        let (unique_a, unique_b) = iblt_a.decode_all();

        let res_a_hash: HashSet<_> = unique_a.into_iter().collect();
        let res_b_hash: HashSet<_> = unique_b.into_iter().collect();

        let expected_a: HashSet<_> = set_a_hash.difference(&set_b_hash).cloned().collect();
        let expected_b: HashSet<_> = set_b_hash.difference(&set_a_hash).cloned().collect();

        prop_assert_eq!(&res_a_hash, &expected_a, "Decoded Local (A-B) mismatch");
        prop_assert_eq!(&res_b_hash, &expected_b, "Decoded Remote (B-A) mismatch");
    }

    #[test]
    fn test_subtraction_antisymmetry(
        set_a in arb_symbol_vec(20),
        set_b in arb_symbol_vec(20)
    ) {
        // Enforce large enough size to ensure decoding succeeds for the check
        let size = 300; 
        
        // Note: We don't strictly need to dedup here for the property to hold 
        // (A - B == -(B - A) is true for multisets too), but decoding logic 
        // expects singleton counts (1 or -1), so duplicates might cause peel failures.
        // For consistency, we rely on the decoder's best effort. 
        // If decoding is partial, the partial results should still be symmetric.

        let mut iblt_a = RatelessIBLT::new();
        let mut iblt_b = RatelessIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size); }
        for x in &set_b { iblt_b.add_symbol(x, size); }

        // Path 1: A - B
        let mut diff_1 = iblt_a.clone();
        diff_1.subtract_assign(&iblt_b);
        let (local_1, remote_1) = diff_1.decode_all();

        // Path 2: B - A
        let mut diff_2 = iblt_b.clone();
        diff_2.subtract_assign(&iblt_a);
        let (local_2, remote_2) = diff_2.decode_all();

        // Symmetry Check
        let s1: HashSet<_> = local_1.iter().collect();
        let s2: HashSet<_> = remote_2.iter().collect();
        prop_assert_eq!(s1, s2, "A-B local should equal B-A remote");

        let s3: HashSet<_> = remote_1.iter().collect();
        let s4: HashSet<_> = local_2.iter().collect();
        prop_assert_eq!(s3, s4, "A-B remote should equal B-A local");
    }
}