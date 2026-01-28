use riblt::{RatelessIBLT, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatMessage {
    pub timestamp: u64,
    pub author_id: u64,
    // Content is encrypted opaque bytes in a real app.
    // Fixed size array for simplicity in this Zero-Copy implementation,
    // or use padding.
    pub encrypted_content: [u8; 32],
}

impl Symbol for ChatMessage {
    const BYTE_LEN: usize = 48; // 8 (ts) + 8 (auth) + 32 (content)

    fn encode_into(&self, buffer: &mut [u8]) {
        // In a hot path, use byte shifting. For clarity here: bincode.
        let bytes = bincode::serialize(self).unwrap();
        // Pad or truncate to ensure fixed size symbol behavior
        buffer[..bytes.len()].copy_from_slice(&bytes);
    }

    fn decode_from(bytes: &[u8]) -> Self {
        bincode::deserialize(bytes).expect("Corruption or version mismatch")
    }
}

#[test]
fn chat_message_repro() {
    let set_a_vals = vec![8382124491921260229u64, 0];

    let mut set_b_vals = Vec::new();
    for i in 1..=13 {
        set_b_vals.push(i as u64);
    }
    set_b_vals.push(16168877346070745623u64);
    for i in 14..=17 {
        set_b_vals.push(i as u64);
    }

    let make_msg = |val: u64| ChatMessage {
        timestamp: val,
        author_id: 0xDEADBEEF,         // Static constant to ensure overlap
        encrypted_content: [0xAA; 32], // Static content
    };

    let mut set_a: Vec<ChatMessage> = set_a_vals.into_iter().map(make_msg).collect();
    let mut set_b: Vec<ChatMessage> = set_b_vals.into_iter().map(make_msg).collect();

    // Replicate test logic (sort/dedup to be sure, though they are unique by construction mostly)
    set_a.sort_by_key(|s| s.timestamp);
    set_a.dedup_by_key(|s| s.timestamp);
    set_b.sort_by_key(|s| s.timestamp);
    set_b.dedup_by_key(|s| s.timestamp);

    let set_a_hash: HashSet<_> = set_a.iter().cloned().collect();
    let set_b_hash: HashSet<_> = set_b.iter().cloned().collect();

    let diff_a_b: Vec<_> = set_a_hash.difference(&set_b_hash).collect();
    let diff_b_a: Vec<_> = set_b_hash.difference(&set_a_hash).collect();
    let total_diff = diff_a_b.len() + diff_b_a.len();

    println!("Total diff: {}", total_diff);
    // Size logic from test failure
    let size = if total_diff < 20 {
        300
    } else {
        (total_diff * 3) + 50
    };
    println!("IBLT Size: {}", size);

    let mut iblt_a = RatelessIBLT::new();
    let mut iblt_b = RatelessIBLT::new();

    for x in &set_a {
        iblt_a.add_symbol(x, size);
    }
    for x in &set_b {
        iblt_b.add_symbol(x, size);
    }

    println!("Subtracting B from A...");
    iblt_a.subtract_assign(&iblt_b);

    println!("Decoding...");
    let (unique_a, unique_b) = iblt_a.decode_all();

    println!("Decoded Local count: {}", unique_a.len());
    println!("Decoded Remote count: {}", unique_b.len());

    let res_a_hash: HashSet<_> = unique_a.into_iter().collect();
    let res_b_hash: HashSet<_> = unique_b.into_iter().collect();

    let expected_a: HashSet<_> = set_a_hash.difference(&set_b_hash).cloned().collect();
    let expected_b: HashSet<_> = set_b_hash.difference(&set_a_hash).cloned().collect();

    assert_eq!(res_a_hash, expected_a, "Decoded Local (A-B) mismatch");
    assert_eq!(res_b_hash, expected_b, "Decoded Remote (B-A) mismatch");
}
