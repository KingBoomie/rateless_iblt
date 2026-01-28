use proptest::prelude::*;
use riblt::RatelessIBLT;
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

const N: usize = 300;
const S: usize = N * 8;
type TestIBLT = RatelessIBLT<TestSymbol, N, S>;

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
            // max_local_rejects: 10000,
            // max_global_rejects: 10000,
            // verbose: 1,  // See progress
            failure_persistence: None,
            max_shrink_iters: 2048,
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
        // For static tests in property testing, we use the const N defined above (300)
        // Ensure size passed to add_symbol <= N.
        let size = 200;

        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();
        let mut iblt_combined_immed = TestIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size).unwrap(); }
        for x in &set_b { iblt_b.add_symbol(x, size).unwrap(); }

        for x in set_a.iter().chain(set_b.iter()) {
            iblt_combined_immed.add_symbol(x, size).unwrap();
        }

        let mut iblt_combined_algebra = iblt_a.clone();
        iblt_combined_algebra.combine_assign(&iblt_b).unwrap();

        iblt_combined_algebra.subtract_assign(&iblt_combined_immed).unwrap();

        let mut local = vec![TestSymbol(0); N];
        let mut remote = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_combined_algebra.decode_all(&mut local, &mut remote).unwrap();
        let local = &local[..lc];
        let remote = &remote[..rc];

        prop_assert!(local.is_empty(), "Local artifacts found (Linearity check)");
        prop_assert!(remote.is_empty(), "Remote artifacts found (Linearity check)");
    }

    #[test]
    fn test_set_difference_decoding(
        mut set_a in arb_symbol_vec(30),
        mut set_b in arb_symbol_vec(30)
    ) {
        // TODO set up the generation such that there won't be collisions
        // for small sizes.

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
             300 // Use full capacity for small sets
        } else {
             ((total_diff * 3) + 50).min(N - 1)
        };

        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size).unwrap(); }
        for x in &set_b { iblt_b.add_symbol(x, size).unwrap(); }

        // 4. Perform Subtraction and Decode
        iblt_a.subtract_assign(&iblt_b).unwrap();

        let mut local = vec![TestSymbol(0); N];
        let mut remote = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_a.decode_all(&mut local, &mut remote).unwrap();
        let unique_a = &local[..lc];
        let unique_b = &remote[..rc];

        let res_a_hash: HashSet<_> = unique_a.iter().cloned().collect();
        let res_b_hash: HashSet<_> = unique_b.iter().cloned().collect();

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
        let size = 200;

        // Note: We don't strictly need to dedup here for the property to hold
        // (A - B == -(B - A) is true for multisets too), but decoding logic
        // expects singleton counts (1 or -1), so duplicates might cause peel failures.
        // For consistency, we rely on the decoder's best effort.
        // If decoding is partial, the partial results should still be symmetric.

        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size).unwrap(); }
        for x in &set_b { iblt_b.add_symbol(x, size).unwrap(); }

        // Path 1: A - B
        let mut diff_1 = iblt_a.clone();
        diff_1.subtract_assign(&iblt_b).unwrap();

        let mut l1 = vec![TestSymbol(0); N];
        let mut r1 = vec![TestSymbol(0); N];
        let (lc1, rc1) = diff_1.decode_all(&mut l1, &mut r1).unwrap();
        let local_1 = &l1[..lc1];
        let remote_1 = &r1[..rc1];

        // Path 2: B - A
        let mut diff_2 = iblt_b.clone();
        diff_2.subtract_assign(&iblt_a).unwrap();

        let mut l2 = vec![TestSymbol(0); N];
        let mut r2 = vec![TestSymbol(0); N];
        let (lc2, rc2) = diff_2.decode_all(&mut l2, &mut r2).unwrap();
        let local_2 = &l2[..lc2];
        let remote_2 = &r2[..rc2];

        // Symmetry Check
        let s1: HashSet<_> = local_1.iter().cloned().collect();
        let s2: HashSet<_> = remote_2.iter().cloned().collect();
        prop_assert_eq!(s1, s2, "A-B local should equal B-A remote");

        let s3: HashSet<_> = remote_1.iter().cloned().collect();
        let s4: HashSet<_> = local_2.iter().cloned().collect();
        prop_assert_eq!(s3, s4, "A-B remote should equal B-A local");
    }
}

// Additional property-based tests
proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        max_shrink_iters: 2048,
        .. ProptestConfig::default()
    })]

    /// Test that adding and then "removing" the same symbol results in empty decode
    #[test]
    fn test_add_then_subtract_same_set(
        set in arb_symbol_vec(30)
    ) {
        let size = 200;
        let mut iblt = TestIBLT::new();

        // Add all symbols
        for x in &set {
            iblt.add_symbol(x, size).unwrap();
        }

        // Subtract all symbols (by adding with negative count via subtraction)
        let mut iblt_neg = TestIBLT::new();
        for x in &set {
            iblt_neg.add_symbol(x, size).unwrap();
        }

        iblt.subtract_assign(&iblt_neg).unwrap();

        let mut local_buf = vec![TestSymbol(0); N];
        let mut remote_buf = vec![TestSymbol(0); N];
        let (lc, rc) = iblt.decode_all(&mut local_buf, &mut remote_buf).unwrap();
        let local = &local_buf[..lc];
        let remote = &remote_buf[..rc];

        prop_assert!(local.is_empty(), "Should have no local symbols after A - A");
        prop_assert!(remote.is_empty(), "Should have no remote symbols after A - A");
    }

    /// Test that empty set differences work correctly
    #[test]
    fn test_empty_set_difference(
        set in arb_symbol_vec(30)
    ) {
        let size = 200;
        let mut iblt_a = TestIBLT::new();
        let iblt_b = TestIBLT::new();

        // A has symbols, B is empty
        for x in &set {
            iblt_a.add_symbol(x, size).unwrap();
        }

        // A - B should give us all of A
        iblt_a.subtract_assign(&iblt_b).unwrap();

        let mut local_buf = vec![TestSymbol(0); N];
        let mut remote_buf = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_a.decode_all(&mut local_buf, &mut remote_buf).unwrap();
        let local = &local_buf[..lc];
        let remote = &remote_buf[..rc];

        let expected: HashSet<_> = set.into_iter().collect();
        // local is &[TestSymbol], so we clone elements to get HashSet<TestSymbol>
        let actual: HashSet<_> = local.iter().cloned().collect();

        prop_assert!(remote.is_empty(), "Should have no remote symbols when B is empty");
        prop_assert_eq!(actual, expected, "A - empty should equal A");
    }

    /// Test that identical sets produce empty difference
    #[test]
    fn test_identical_sets(
        mut set in arb_symbol_vec(30)
    ) {
        // Deduplicate to ensure set semantics
        set.sort_by_key(|s| s.0);
        set.dedup();

        let size = 200;
        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();

        for x in &set {
            iblt_a.add_symbol(x, size).unwrap();
            iblt_b.add_symbol(x, size).unwrap();
        }

        iblt_a.subtract_assign(&iblt_b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_a.decode_all(&mut l, &mut r).unwrap();
        let local = &l[..lc];
        let remote = &r[..rc];

        prop_assert!(local.is_empty(), "Identical sets should have no A-B difference");
        prop_assert!(remote.is_empty(), "Identical sets should have no B-A difference");
    }

    /// Test commutativity of combine_assign
    #[test]
    fn test_combine_assign_commutativity(
        set_a in arb_symbol_vec(20),
        set_b in arb_symbol_vec(20)
    ) {
        let size = 200;
        let mut iblt_a1 = TestIBLT::new();
        let mut iblt_a2 = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();

        for x in &set_a {
            iblt_a1.add_symbol(x, size).unwrap();
            iblt_a2.add_symbol(x, size).unwrap();
        }
        for x in &set_b {
            iblt_b.add_symbol(x, size).unwrap();
        }

        // A + B
        iblt_a1.combine_assign(&iblt_b).unwrap();

        // B + A (simulated by adding to B)
        let mut iblt_b_copy = iblt_b.clone();
        iblt_b_copy.combine_assign(&iblt_a2).unwrap();

        // Both should decode to the same union
        let mut l1 = vec![TestSymbol(0); N];
        let mut r1 = vec![TestSymbol(0); N];
        let (lc1, _) = iblt_a1.decode_all(&mut l1, &mut r1).unwrap();
        let local1 = &l1[..lc1];

        let mut l2 = vec![TestSymbol(0); N];
        let mut r2 = vec![TestSymbol(0); N];
        let (lc2, _) = iblt_b_copy.decode_all(&mut l2, &mut r2).unwrap();
        let local2 = &l2[..lc2];

        let set1: HashSet<_> = local1.into_iter().collect();
        let set2: HashSet<_> = local2.into_iter().collect();

        // Note: Due to collisions, decoding might be partial, but whatever
        // we decode should be consistent
        prop_assert_eq!(set1, set2, "A+B should equal B+A");
    }

    /// Test that single symbol can be decoded
    #[test]
    fn test_single_symbol_roundtrip(val in any::<u64>()) {
        let size = 100;
        let mut iblt = TestIBLT::new();
        let symbol = TestSymbol(val);

        iblt.add_symbol(&symbol, size).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt.decode_all(&mut l, &mut r).unwrap();
        let local = &l[..lc];
        let remote = &r[..rc];

        prop_assert_eq!(local.len(), 1, "Should decode exactly one symbol");
        prop_assert!(remote.is_empty(), "Should have no remote symbols");
        prop_assert_eq!(local[0].clone(), symbol, "Decoded symbol should match original");
    }

    /// Test associativity: (A + B) + C == A + (B + C)
    #[test]
    fn test_combine_assign_associativity(
        set_a in arb_symbol_vec(15),
        set_b in arb_symbol_vec(15),
        set_c in arb_symbol_vec(15)
    ) {
        let size = 200;

        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();
        let mut iblt_c = TestIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size).unwrap(); }
        for x in &set_b { iblt_b.add_symbol(x, size).unwrap(); }
        for x in &set_c { iblt_c.add_symbol(x, size).unwrap(); }

        // (A + B) + C
        let mut left = iblt_a.clone();
        left.combine_assign(&iblt_b).unwrap();
        left.combine_assign(&iblt_c).unwrap();

        // A + (B + C)
        let mut right = iblt_b.clone();
        right.combine_assign(&iblt_c).unwrap();
        right.combine_assign(&iblt_a).unwrap();

        // Both should represent the same multiset
        // Verify by subtracting one from the other
        left.subtract_assign(&right).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = left.decode_all(&mut l, &mut r).unwrap();
        let local = &l[..lc];
        let remote = &r[..rc];

        prop_assert!(local.is_empty(), "(A+B)+C should equal A+(B+C)");
        prop_assert!(remote.is_empty(), "(A+B)+C should equal A+(B+C)");
    }
}
