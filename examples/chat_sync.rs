use crate::riblt::RatelessIBLT; // Assuming code is in lib
use crate::riblt::Symbol;
use postcard;
use riblt;
/// This example demonstrates a "Set Reconciliation" protocol.
/// It defines a ChatMessage symbol, simulates two diverging devices (Alice and Bob),
/// and reconciles them blindly using the IBLT.
// examples/chat_sync.rs
use serde::{Deserialize, Serialize};

// --- 1. The Domain Object ---
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        // In a hot path, use byte shifting. For clarity here: postcard.
        let bytes = postcard::to_stdvec(self).unwrap();
        // Pad or truncate to ensure fixed size symbol behavior
        buffer[..bytes.len()].copy_from_slice(&bytes);
    }

    fn decode_from(bytes: &[u8]) -> Self {
        postcard::from_bytes(bytes).expect("Corruption or version mismatch")
    }
}

impl ChatMessage {
    pub fn new(ts: u64, author: u64, text: &str) -> Self {
        let mut content = [0u8; 32];
        let bytes = text.as_bytes();
        let len = bytes.len().min(32);
        content[..len].copy_from_slice(&bytes[..len]);
        Self {
            timestamp: ts,
            author_id: author,
            encrypted_content: content,
        }
    }
}

// --- 2. The Sync Logic ---
fn main() {
    println!("--- Starting Encrypted Chat Sync ---");

    // 1. Setup Initial State (Shared History)
    let msg1 = ChatMessage::new(100, 1, "Hello Bob");
    let msg2 = ChatMessage::new(101, 2, "Hi Alice");

    let mut alice_db = vec![msg1.clone(), msg2.clone()];
    let mut bob_db = vec![msg1.clone(), msg2.clone()];

    // 2. Divergence (Offline Mode)
    let msg3_alice = ChatMessage::new(102, 1, "Alice: I am offline");
    alice_db.push(msg3_alice.clone());

    let msg4_bob = ChatMessage::new(103, 2, "Bob: Me too");
    let msg5_bob = ChatMessage::new(104, 2, "Bob: Still offline");
    bob_db.push(msg4_bob.clone());
    bob_db.push(msg5_bob.clone());

    println!(
        "Alice has {} msgs, Bob has {} msgs",
        alice_db.len(),
        bob_db.len()
    );

    // 3. Generate Rateless IBLTs
    // "prefix_size" determines the probability of decoding.
    // In static API, we define MAX blocks.
    const N_BLOCKS: usize = 200;
    // No-op - removed unused SYM_SIZE
    // We calculate buffer size (N * 48)
    const SUMS_BYTES: usize = N_BLOCKS * 48;

    type SyncIBLT = RatelessIBLT<ChatMessage, N_BLOCKS, SUMS_BYTES>;

    // We use a smaller logic size for encoding if we want, but usually we fill up to N_BLOCKS
    // or use a `active_window` param if API supported it.
    // `add_symbol` takes `max_blocks`.
    // We'll use 50 buckets (sync_prefix_size) for this demo to show "Rateless" nature (subset usage),
    // but the struct is statically sized for N_BLOCKS.
    let sync_prefix_size = 50;

    // Note: unwrap() used for demo; real code handles CapacityExceeded
    let mut iblt_alice = SyncIBLT::new();
    for msg in &alice_db {
        iblt_alice.add_symbol(msg, sync_prefix_size).unwrap();
    }

    let mut iblt_bob = SyncIBLT::new();
    for msg in &bob_db {
        iblt_bob.add_symbol(msg, sync_prefix_size).unwrap();
    }

    // 4. Transmission & Subtraction (Bob initiates sync)
    // Bob receives Alice's IBLT and subtracts his own.
    // Result = Alice - Bob
    let mut diff_iblt = iblt_alice.clone();
    diff_iblt.subtract_assign(&iblt_bob).unwrap();

    // 5. Decode
    // Prepare output buffers
    let mut alice_unique_buf = vec![msg1.clone(); N_BLOCKS]; // Dummy init
    let mut bob_unique_buf = vec![msg1.clone(); N_BLOCKS];

    let (alice_count, bob_count) = diff_iblt
        .decode_all(&mut alice_unique_buf, &mut bob_unique_buf)
        .unwrap();

    let alice_unique = &alice_unique_buf[..alice_count];
    let bob_unique = &bob_unique_buf[..bob_count];

    println!("\n--- Decoding Results ---");
    println!("Messages Alice has (Bob needs to download):");
    for m in alice_unique {
        println!(
            " - [TS: {}] Author {}: {:?}",
            m.timestamp, m.author_id, m.encrypted_content
        );
    }

    println!("Messages Bob has (Alice needs to download):");
    for m in bob_unique {
        println!(
            " - [TS: {}] Author {}: {:?}",
            m.timestamp, m.author_id, m.encrypted_content
        );
    }

    // 6. Verification
    assert!(alice_unique.contains(&msg3_alice));
    assert!(bob_unique.contains(&msg4_bob));
    assert!(bob_unique.contains(&msg5_bob));
    assert_eq!(alice_unique.len(), 1);
    assert_eq!(bob_unique.len(), 2);

    println!("\nSync Successful. Protocol Terminated.");
}
