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

// Define constants for tests
const N: usize = 300;
const S: usize = N * 8;
type TestIBLT = RatelessIBLT<TestSymbol, N, S>;

proptest! {

    #![proptest_config(ProptestConfig {
            failure_persistence: None,
            max_shrink_iters: 4096,
            ..ProptestConfig::default()
        })]

    #[test]
    fn test_linearity_of_addition(
        set_a in arb_symbol_vec(50),
        set_b in arb_symbol_vec(50)
    ) {
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

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_combined_algebra.decode_all(&mut l, &mut r).unwrap();

        prop_assert!(lc == 0, "Local artifacts found (Linearity check)");
        prop_assert!(rc == 0, "Remote artifacts found (Linearity check)");
    }

    #[test]
    fn test_set_difference_decoding(
        mut set_a in arb_symbol_vec(30),
        mut set_b in arb_symbol_vec(30)
    ) {
        set_a.sort_by_key(|s| s.0); set_a.dedup();
        set_b.sort_by_key(|s| s.0); set_b.dedup();

        let set_a_hash: HashSet<_> = set_a.iter().cloned().collect();
        let set_b_hash: HashSet<_> = set_b.iter().cloned().collect();

        let diff_a_b: Vec<_> = set_a_hash.difference(&set_b_hash).collect();
        let diff_b_a: Vec<_> = set_b_hash.difference(&set_a_hash).collect();
        let total_diff = diff_a_b.len() + diff_b_a.len();

        let size = if total_diff < 20 {
             100
        } else {
             ((total_diff * 3) + 50).min(N-1)
        };

        let mut iblt_a = TestIBLT::new();
        let mut iblt_b = TestIBLT::new();

        for x in &set_a { iblt_a.add_symbol(x, size).unwrap(); }
        for x in &set_b { iblt_b.add_symbol(x, size).unwrap(); }

        iblt_a.subtract_assign(&iblt_b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_a.decode_all(&mut l, &mut r).unwrap();

        // Manual conversion to compare
        let res_a_vec = l[..lc].to_vec();
        let res_b_vec = r[..rc].to_vec();

        let res_a_hash: HashSet<_> = res_a_vec.into_iter().collect();
        let res_b_hash: HashSet<_> = res_b_vec.into_iter().collect();

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
        let size = 200;

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

        // Path 2: B - A
        let mut diff_2 = iblt_b.clone();
        diff_2.subtract_assign(&iblt_a).unwrap();

        let mut l2 = vec![TestSymbol(0); N];
        let mut r2 = vec![TestSymbol(0); N];
        let (lc2, rc2) = diff_2.decode_all(&mut l2, &mut r2).unwrap();

        let s1: HashSet<_> = l1[..lc1].iter().collect();
        let s2: HashSet<_> = r2[..rc2].iter().collect();
        prop_assert_eq!(s1, s2, "A-B local should equal B-A remote");

        let s3: HashSet<_> = r1[..rc1].iter().collect();
        let s4: HashSet<_> = l2[..lc2].iter().collect();
        prop_assert_eq!(s3, s4, "A-B remote should equal B-A local");
    }
}
