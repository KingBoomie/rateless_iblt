**RFC: Type-Safe Rateless Set Reconciliation Protocol**

**Status:** Draft  
**Tracking Issue:** N/A  
**Related:** arXiv:2402.02668 (Rateless IBLT), `encoder3.rs`  

---

## Summary

Introduce a new public module `riblt::protocol` providing misuse-resistant wrappers around the existing `RatelessIBLT` implementation. The API eliminates the "modulo mismatch" and "premature decode" failure modes through ownership-based state transitions (distinct structs rather than type-state generics), mandatory parameter negotiation, and defensive programming against leaked reconstructions.

---

## Motivation

The current `RatelessIBLT<T>` API is flexible but permits catastrophic silent failures:

1. **Modulo Mismatch:** If Alice calls `add_symbol(msg, 100)` and Bob calls `add_symbol(msg, 200)`, subtraction yields cryptographic garbage. No compilation error occurs.
2. **State Confusion:** Users may call `decode_all()` on a raw IBLT without prior `subtract_assign()`, receiving meaningless data.
3. **Resource Exhaustion:** Malicious or buggy peers may specify `max_blocks = u32::MAX`, causing immediate OOM during sketch reconstruction.
4. **Partial Stream Ignorance:** Users may ignore incomplete decodes (the "tornado zone"), proceeding with partial difference sets.

This RFC proposes a protocol-oriented API where **these states are unrepresentable** through careful ownership design, while retaining the zero-cost performance of the underlying `encoder3.rs` implementation.

---

## Guide-Level Explanation

### The Three-Phase Protocol

All reconciliation follows a strict lifecycle enforced by Rust's ownership system:

```rust
// Phase 1: Accumulation (Mutable)
let sketch = SketchBuilder::<ChatMessage>::with_capacity(1000)?
    .insert(&msg1)
    .insert(&msg2)
    .seal(); // Consumes builder, produces immutable SealedSketch

// Phase 2: Transmission (Immutable, non-Clonable)
let desc = sketch.descriptor(); // Send this first
let chunk = sketch.extract(0..500)?; // Send this second

// Phase 3: Reconciliation (Consuming)
let result = Reconciler::<ChatMessage>::from_remote_desc(&desc, FINGERPRINT)?
    .insert_local(&local_msg1)
    .absorb(&chunk)?
    .resolve()?
    .complete()?; // Forces handling of Incomplete status
```

**Key Safety Properties:**
- You cannot insert into a `SealedSketch` (it doesn't have the method).
- You cannot `extract` from a `Reconciler` (different struct).
- You cannot create a `Reconciler` without validating the remote `Descriptor` first, locking `max_blocks` before local insertion.
- You cannot obtain results without explicitly handling the `DecodeStatus::Incomplete` case (via `Reconciliation::complete()`).
- You cannot accidentally broadcast divergent chunks to different peers ( `SealedSketch` does not implement `Clone`).

---

## Reference-Level Explanation

### Module Structure

```rust
pub mod protocol {
    // State structs (ownership-based transitions)
    pub struct SketchBuilder<T: Symbol> { ... }
    pub struct SealedSketch<T: Symbol> { ... }
    pub struct Reconciler<T: Symbol> { ... }
    pub struct Reconciliation<T: Symbol> { ... }
    
    // Wire format (Serde/Borsh compatible)
    pub mod wire {
        pub struct Descriptor { ... }
        pub struct BlockChunk { ... }
        pub type Fingerprint = [u8; 8];
    }
    
    // Errors (non-exhaustive)
    pub enum Error {
        VersionMismatch { ... },
        FingerprintMismatch { expected: Fingerprint, actual: Fingerprint },
        ResourceLimitExceeded { requested: u32, max: u32 },
        DuplicateBlockRange { start: u32, end: u32 },
        IncompleteStream { missing: Vec<Range<u32>> },
        IncompleteReconciliation { residue_cells: usize },
    }
}
```

### Detailed API Specification

#### `SketchBuilder<T>`

```rust
impl<T: Symbol> SketchBuilder<T> {
    /// Creates a builder with a hard upper bound on blocks (DoS protection).
    /// # Errors
    /// - `ResourceLimitExceeded` if max_blocks > 1_000_000 (configurable const)
    pub fn with_capacity(max_blocks: u32) -> Result<Self, Error>;
    
    /// Adds a local symbol using the specified capacity for mapping.
    /// O(K) complexity.
    pub fn insert(&mut self, symbol: &T) -> &mut Self;
    
    /// Finalizes the sketch. After this call, the set is immutable.
    /// Computes the cryptographic fingerprint of T::BYTE_LEN and type name.
    pub fn seal(self) -> SealedSketch<T>;
}
```

#### `SealedSketch<T>`

```rust
// Intentionally **not** Clone to prevent partition attacks (see Safety section)
pub struct SealedSketch<T: Symbol> {
    iblt: RatelessIBLT<T>,
    desc: wire::Descriptor,
    extracted_ranges: RangeSet<u32>, // Tracks what was sent
}

impl<T: Symbol> SealedSketch<T> {
    /// Returns the descriptor that must be transmitted first.
    pub fn descriptor(&self) -> &wire::Descriptor;
    
    /// Extracts a chunk for network transmission.
    /// # Errors
    /// - `OutOfBounds` if range exceeds max_blocks
    /// - `DuplicateBlockRange` if overlapping with previously extracted range
    pub fn extract(&mut self, range: Range<usize>) -> Result<wire::BlockChunk, Error>;
    
    /// Convenience: extracts the entire sketch [0..max_blocks].
    pub fn extract_all(&mut self) -> Result<wire::BlockChunk, Error>;
}
```

#### `Reconciler<T>`

```rust
pub struct Reconciler<T: Symbol> {
    local: RatelessIBLT<T>,
    remote: RatelessIBLT<T>,
    desc: wire::Descriptor,
    received_ranges: RangeSet<u32>,
    resolved: bool, // For Drop check
}

impl<T: Symbol> Reconciler<T> {
    /// Validates remote descriptor and initializes local storage.
    /// # Errors
    /// - `FingerprintMismatch` if T doesn't match remote
    /// - `VersionMismatch` if protocol version differs
    /// - `ResourceLimitExceeded` for sanity limits
    pub fn from_remote_desc(
        desc: &wire::Descriptor, 
        expected: Fingerprint
    ) -> Result<Self, Error>;
    
    /// Adds local symbols using the **remote's** max_blocks (guaranteed match).
    pub fn insert_local(&mut self, symbol: &T) -> &mut Self;
    
    /// Absorbs a network chunk.
    /// # Errors
    /// - `OutOfBounds` if chunk references unknown blocks
    /// - `DuplicateBlockRange` on overlap
    /// - `ChecksumMismatch` if chunk data is corrupted
    pub fn absorb(&mut self, chunk: &wire::BlockChunk) -> Result<&mut Self, Error>;
    
    /// Consumes self to perform subtraction and peeling.
    /// # Errors
    /// - `IncompleteStream` if blocks are missing (detects gaps via RangeSet)
    pub fn resolve(mut self) -> Result<Reconciliation<T>, Error>;
}

impl<T> Drop for Reconciler<T> {
    fn drop(&mut self) {
        if cfg!(debug_assertions) && !self.resolved && !self.received_ranges.is_empty() {
            panic!(
                "Reconciler<T> dropped without calling resolve(). \
                 This likely indicates a forgotten reconciliation or error handling omission."
            );
        }
    }
}
```

#### `Reconciliation<T>` (Result Wrapper)

```rust
pub struct Reconciliation<T: Symbol> {
    local_unique: Vec<T>,
    remote_unique: Vec<T>,
    status: DecodeStatus,
    // Private fields to prevent direct construction
}

pub enum DecodeStatus {
    Complete,
    Incomplete { residue_cells: usize },
}

impl<T: Symbol> Reconciliation<T> {
    /// Forces the caller to handle incomplete decodes.
    /// 
    /// # Returns
    /// - `Ok((missing_here, missing_there))` if Complete
    /// - `Err(self)` if Incomplete, allowing caller to retry with larger capacity
    pub fn complete(self) -> Result<(Vec<T>, Vec<T>), Self>;
    
    /// Accessor for status inspection without consuming.
    pub fn status(&self) -> &DecodeStatus;
    
    /// # Safety
    /// Returns partial results even if Incomplete. Use with caution.
    pub fn into_partial_results(self) -> (Vec<T>, Vec<T>) {
        (self.local_unique, self.remote_unique)
    }
}
```

### Wire Format Safety

The `wire::Descriptor` acts as a capability token:

```rust
pub struct Descriptor {
    pub version: u16,                    // Must equal PROTOCOL_VERSION
    pub fingerprint: Fingerprint,        // Type discriminator
    pub max_blocks: u32,                 // The N parameter (locked)
    pub checksum: u64,                   // xxhash of preceding fields
}

impl Descriptor {
    /// Validates integrity and version.
    pub fn validate(&self) -> Result<(), Error>;
}

/// Fingerprint computation (compile-time where possible)
pub const fn fingerprint<T: Symbol>() -> Fingerprint {
    // Hashes type_name::<T>() and T::BYTE_LEN via const fn
}
```

---

## Safety and Anti-Patterns

### 1. Partition Attack Prevention
`SealedSketch` intentionally does **not** derive `Clone`. If a user needs to send the same sketch to multiple peers, they must build multiple `SketchBuilder`s. This prevents the subtle bug where Alice sends chunks `[0..500]` to Bob and `[500..1000]` to Charlie, then finds her set reconciled against a "Frankenstein" sketch that never existed in one place.

### 2. Resource Limits
Both `with_capacity` and `from_remote_desc` enforce `MAX_SAFE_BLOCKS` (default 1M). This prevents the "zlib-style" resource exhaustion where a malicious 4-byte header (`max_blocks = 0xFFFFFFFF`) triggers multi-GB allocation.

### 3. Mandatory ACK of Incomplete State
The `Reconciliation::complete()` method returns `Result`. Users must write:

```rust
match recon.complete() {
    Ok((need, have)) => sync(need),
    Err(recon) => {
        eprintln!("Tornado zone detected, {} cells remaining", 
                  recon.status().residue_cells());
        // Must retry with larger max_blocks
    }
}
```

This pattern makes it impossible to accidentally use partial results in production (the "silent data loss" mode of traditional IBLTs).

### 4. Debug Assertions
The `Drop` impl on `Reconciler` acts as a debug-time lint against "forgetting" to resolve. While Rust's ownership usually prevents use-after-move, complex error handling (e.g., `?` early returns in a match arm) might accidentally leak a partially-filled reconciler. The debug panic catches this immediately in testing.

---

## Drawbacks & Rationale

**Drawback:** More allocations than raw API. The `SketchBuilder` API allocates `Vec<u8>` for the IBLT upfront, whereas expert users might want to use a pool allocator or stack storage.

**Mitigation:** The raw `RatelessIBLT<T>` remains public in `riblt::lib` for zero-cost scenarios. The `protocol` module is explicitly for networked applications where safety dominates micro-optimization.

**Drawback:** Cannot "resume" a `SealedSketch` after sealing. If a network connection drops mid-transfer, the user must rebuild the sketch from scratch.

**Rationale:** This is inherent to the reconciliation problem (the local set may have changed). Users wishing to support resumable rateless streams should instantiate a new `SketchBuilder` with updated data.

---

## Unresolved Questions

1. **Checksum Algorithm:** Should we use xxhash64 (fast) or blake3 (cryptographically secure)? For a malicious network, xxhash is vulnerable to collision attacks that could cause the receiver to reject valid chunks or accept malformed ones.

2. **`RangeSet` Implementation:** Should we depend on `range-set` crate or implement a simple interval merging vector internally? External deps increase supply chain risk; internal impl increases maintenance.

3. **Async Integration:** Should `extract` and `absorb` be `async fn`? For now, synchronous API with `Send + Sync` bounds is sufficient; users can wrap in `spawn_blocking` or use the raw `RatelessIBLT` with their own async framing.

4. **Constant-Time Operations:** The underlying XOR operations are data-dependent (branch predictor leaks size of set difference via timing). Does the `protocol` module need a `constant_time` feature flag that uses subtle crate operations?

---

## Prior Art

- **The IBLT Paper (Goodrich et al.):** Defines the core algorithm but provides no guidance on protocol safety.
- **libFuzzer:** Our `Drop` check is inspired by Fuzzer's leak detection—using RAII to enforce protocol completion.
- **RustTLS / Rusqlite:** Use of non-`Clone` session types to prevent key material reuse across contexts (similar to our `SealedSketch` anti-pattern prevention).

---

## Implementation Plan

1. **Phase 1:** Create `src/protocol/mod.rs` with wire types and error definitions.
2. **Phase 2:** Implement `SketchBuilder` and `SealedSketch` on top of existing `encoder3.rs` (no changes to core).
3. **Phase 3:** Implement `Reconciler` with `RangeSet` tracking and `Drop` assertions.
4. **Phase 4:** Add `Reconciliation::complete()` forcing function.
5. **Phase 5:** Comprehensive property-based tests (proptest) verifying that any sequence of `insert`/`extract`/`absorb` calls either yields correct reconciliation or returns a documented error (no panics, no silent corruption).

**Estimated LOC:** ~400 lines of new safe code wrapping ~300 lines of existing unsafe/fast code.

---

**End of RFC**