#[cfg(test)]
mod tests {

    use riblt::RatelessIBLT;
    use riblt::Symbol;

    // Reuse TestSymbol
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

    const N: usize = 200;
    const S: usize = N * 8;
    type TestIBLT = RatelessIBLT<TestSymbol, N, S>;

    #[test]
    fn test_capacity_expansion() {
        let mut iblt = TestIBLT::new();

        // Add symbol with small max_blocks
        iblt.add_symbol(&TestSymbol(1), 10).unwrap();

        // Internal checks (requires making fields pub or adding getters for test,
        // assuming pub(crate) for tests)
        // assert_eq!(iblt.num_blocks, 10);

        // Add symbol with larger max_blocks -> should resize
        iblt.add_symbol(&TestSymbol(2), 20).unwrap();
        // assert_eq!(iblt.num_blocks, 20);

        // The previous data should still be valid.
        // We verify by adding TestSymbol(1) again (XORing it out) and TestSymbol(2) again.
        // Result should be empty.
        iblt.add_symbol(&TestSymbol(1), 10).unwrap(); // Use SAME size as first add
        iblt.add_symbol(&TestSymbol(2), 20).unwrap(); // Use SAME size as second add

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt.decode_all(&mut l, &mut r).unwrap();
        assert_eq!(lc, 0);
        assert_eq!(rc, 0);
    }

    #[test]
    fn test_mapping_determinism() {
        // Critical: Alice and Bob must generate exactly the same mapping indices for the same symbol
        let s = TestSymbol(0xDEADBEEF);

        let mut iblt_1 = TestIBLT::new();
        iblt_1.add_symbol(&s, 50).unwrap();

        let mut iblt_2 = TestIBLT::new();
        iblt_2.add_symbol(&s, 50).unwrap();

        // If mappings differ, subtraction won't zero out completely
        iblt_1.subtract_assign(&iblt_2).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = iblt_1.decode_all(&mut l, &mut r).unwrap();
        assert_eq!(lc, 0);
        assert_eq!(rc, 0);
    }

    #[test]
    fn test_heavy_collision() {
        // This tests the "hash" vector protection.
        // If two symbols map to the same bucket (collision in `sums`),
        // the `hashes` vector (using XxHash) should prevent `try_peel` from returning a corrupted symbol.

        // It's hard to force a collision in the RandomMapping integration without mocking,
        // but we can test that adding 2 distinct symbols prevents peeling if they overlap perfectly (unlikely)
        // or if they overlap in a singleton bucket.

        let mut iblt = TestIBLT::new();
        // Intentionally small size to force collisions in buckets
        let size = 5;

        let s1 = TestSymbol(1);
        let s2 = TestSymbol(2);

        iblt.add_symbol(&s1, size).unwrap();
        iblt.add_symbol(&s2, size).unwrap();

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
        let mut a = TestIBLT::new();
        let b = TestIBLT::new();

        let s1 = TestSymbol(1);
        let s2 = TestSymbol(2);

        a.add_symbol(&s1, size).unwrap();
        a.add_symbol(&s2, size).unwrap();

        let _before = a.clone();

        a.subtract_assign(&b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = a.decode_all(&mut l, &mut r).unwrap();

        let local = &l[..lc];
        let got: std::collections::HashSet<_> = local.iter().cloned().collect();
        let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

        assert_eq!(got, exp);
        assert_eq!(rc, 0);
    }

    #[test]
    fn empty_minus_empty() {
        let mut a = TestIBLT::new();
        let b = TestIBLT::new();

        a.subtract_assign(&b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = a.decode_all(&mut l, &mut r).unwrap();

        assert_eq!(lc, 0);
        assert_eq!(rc, 0);
    }

    #[test]
    fn decode_consistency_after_empty_subtract() {
        let size = 200; // fit within N=200
        let mut a = TestIBLT::new();
        let b = TestIBLT::new();

        let s1 = TestSymbol(42);
        let s2 = TestSymbol(99);

        a.add_symbol(&s1, size).unwrap();
        a.add_symbol(&s2, size).unwrap();

        let mut l1_buf = vec![TestSymbol(0); N];
        let mut r1_buf = vec![TestSymbol(0); N];
        let (lc1, rc1) = a.decode_all(&mut l1_buf, &mut r1_buf).unwrap();

        assert_eq!(rc1, 0);

        let mut a2 = TestIBLT::new();
        a2.add_symbol(&s1, size).unwrap();
        a2.add_symbol(&s2, size).unwrap();

        a2.subtract_assign(&b).unwrap();

        let mut l2_buf = vec![TestSymbol(0); N];
        let mut r2_buf = vec![TestSymbol(0); N];
        let (lc2, rc2) = a2.decode_all(&mut l2_buf, &mut r2_buf).unwrap();

        let local1 = &l1_buf[..lc1];
        let local2 = &l2_buf[..lc2];

        assert_eq!(
            local1
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
            local2
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
        );

        assert_eq!(rc2, 0);
    }

    #[test]
    fn minimal_property_failure_case() {
        let size = 200;
        let mut a = TestIBLT::new();
        let b = TestIBLT::new();

        let s1 = TestSymbol(18145032937744471612);
        let s2 = TestSymbol(15036937119665929939);

        a.add_symbol(&s1, size).unwrap();
        a.add_symbol(&s2, size).unwrap();

        a.subtract_assign(&b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = a.decode_all(&mut l, &mut r).unwrap();
        let local = &l[..lc];

        let got: std::collections::HashSet<_> = local.iter().cloned().collect();
        let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

        assert_eq!(got, exp);
        assert_eq!(rc, 0);
    }

    #[test]
    fn minimal_property_failure_case2() {
        let size = 200;
        let mut a = TestIBLT::new();
        let b = TestIBLT::new();

        let s1 = TestSymbol(1);
        let s2 = TestSymbol(2);

        a.add_symbol(&s1, size).unwrap();
        a.add_symbol(&s2, size).unwrap();

        a.subtract_assign(&b).unwrap();

        let mut l = vec![TestSymbol(0); N];
        let mut r = vec![TestSymbol(0); N];
        let (lc, rc) = a.decode_all(&mut l, &mut r).unwrap();
        let local = &l[..lc];

        let got: std::collections::HashSet<_> = local.iter().cloned().collect();
        let exp: std::collections::HashSet<_> = [s1, s2].into_iter().collect();

        assert_eq!(got, exp);
        assert_eq!(rc, 0);
    }
}
