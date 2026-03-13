use csv::Writer;
use indicatif::{ProgressBar, ProgressStyle};
use rand::prelude::*;
use riblt::{RatelessIBLT, Symbol};
use serde::Serialize;
use std::fs::File;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
struct TestSymbol {
    id: u64,
    payload: [u8; 32],
}

impl Symbol for TestSymbol {
    const BYTE_LEN: usize = 40;

    fn encode_into(&self, buffer: &mut [u8]) {
        buffer[..8].copy_from_slice(&self.id.to_le_bytes());
        buffer[8..].copy_from_slice(&self.payload);
    }

    fn decode_from(bytes: &[u8]) -> Self {
        let id = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let mut payload = [0u8; 32];
        payload.copy_from_slice(&bytes[8..]);
        TestSymbol { id, payload }
    }
}

#[derive(Serialize)]
struct BenchmarkRecord {
    scenario: String,
    total_size: usize,
    diff_size: usize,
    prefix_size: usize,
    ratio: f64,
    encode_time_us: u64,
    decode_time_us: u64,
    memory_bytes: usize,
    bandwidth_bytes: usize,
    success: bool,
    recovered_local: usize,
    recovered_remote: usize,
    trial: usize,
}

fn generate_sets(
    rng: &mut ThreadRng,
    total_size: usize,
    diff_size: usize,
) -> (Vec<TestSymbol>, Vec<TestSymbol>) {
    let shared_count = total_size.saturating_sub(diff_size / 2);
    let shared: Vec<TestSymbol> = (0..shared_count)
        .map(|i| TestSymbol {
            id: i as u64,
            payload: rng.random::<[u8; 32]>(),
        })
        .collect();

    let alice_unique: Vec<TestSymbol> = (0..diff_size / 2)
        .map(|i| TestSymbol {
            id: (shared_count + i) as u64,
            payload: rng.random::<[u8; 32]>(),
        })
        .collect();

    let bob_unique: Vec<TestSymbol> = (0..diff_size / 2 + diff_size % 2)
        .map(|i| TestSymbol {
            id: (shared_count + diff_size / 2 + i) as u64,
            payload: rng.random::<[u8; 32]>(),
        })
        .collect();

    let mut alice_set = shared.clone();
    alice_set.extend(alice_unique);
    alice_set.sort_by_key(|s| s.id);

    let mut bob_set = shared.clone();
    bob_set.extend(bob_unique);
    bob_set.sort_by_key(|s| s.id);

    (alice_set, bob_set)
}

// Define a static IBLT type large enough for all benchmarks
// Max diff size in main is 10000, max ratio 3.0 => 30000 blocks likely needed?
// S1: 10000 diff * 1.5 = 15000.
// S2: 1000 diff * 3.0 = 3000.
// S3: 1000 diff * 1.5 = 1500.
// Largest needed is ~15000. Let's use 20000.
const N_BLOCKS: usize = 20_000;
const SUMS_BYTES: usize = N_BLOCKS * 48; // A bit more than symbol size
type BenchIBLT = RatelessIBLT<TestSymbol, N_BLOCKS, SUMS_BYTES>;

fn run_trial(
    alice_set: &[TestSymbol],
    bob_set: &[TestSymbol],
    prefix_size: usize,
) -> (u64, u64, usize, usize, bool, usize, usize) {
    // Assert we fit
    assert!(
        prefix_size <= N_BLOCKS,
        "Benchmark param exceeds static capacity"
    );

    let start = Instant::now();
    // Use Box to avoid stack overflow
    let mut iblt_alice = Box::new(BenchIBLT::new());
    for sym in alice_set {
        iblt_alice.add_symbol(sym, prefix_size).unwrap();
    }
    let encode_time = start.elapsed().as_micros() as u64;

    let mut iblt_bob = Box::new(BenchIBLT::new());
    for sym in bob_set {
        iblt_bob.add_symbol(sym, prefix_size).unwrap();
    }

    let mem_per_block = TestSymbol::BYTE_LEN + 16;
    let memory_bytes = prefix_size * mem_per_block;

    let start = Instant::now();
    // diff_iblt on heap
    let mut diff_iblt = iblt_alice.clone(); // Box clone
    diff_iblt.subtract_assign(&iblt_bob).unwrap();

    // Output buffers on heap
    let mut l_buf = vec![
        TestSymbol {
            id: 0,
            payload: [0; 32]
        };
        N_BLOCKS
    ];
    let mut r_buf = vec![
        TestSymbol {
            id: 0,
            payload: [0; 32]
        };
        N_BLOCKS
    ];

    let (lc, rc) = diff_iblt.decode_all(&mut l_buf, &mut r_buf).unwrap();
    let decode_time = start.elapsed().as_micros() as u64;

    let success = lc > 0 || rc > 0; // Rough success check or valid count > 0 if diff > 0

    (
        encode_time,
        decode_time,
        memory_bytes,
        prefix_size * mem_per_block,
        success,
        lc,
        rc,
    )
}

fn main() {
    let mut rng = rand::rng();
    let mut records: Vec<BenchmarkRecord> = Vec::new();

    // Configuration
    let diff_sizes = [10, 50, 100, 500, 1000, 5000, 10000];
    let ratios = [1.0f64, 1.2, 1.5, 2.0, 2.5, 3.0];
    let total_sizes = [1000, 10000, 100000, 1000000];
    let trials = 10;
    let total_size_const = 100000;

    // Calculate total iterations for the progress bar
    let total_iters = diff_sizes.len() * trials
        + ratios.len() * trials
        + total_sizes.iter().filter(|&&t| t >= 1000).count() * trials;

    println!("🚀 RIBLT Benchmark Suite");
    println!("   Total iterations: {}\n", total_iters);

    // Setup tqdm-style progress bar
    let pb = ProgressBar::new(total_iters as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) | ETA: {eta} | {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  ")
    );

    // --- Scenario 1: Scale Difference Size ---
    pb.set_message("S1: Scaling diff size");

    for &diff in &diff_sizes {
        for trial in 0..trials {
            let (alice, bob) = generate_sets(&mut rng, total_size_const, diff);
            let prefix = (diff as f64 * 1.5) as usize;

            let (enc, dec, mem, bw, success, rec_loc, rec_rem) = run_trial(&alice, &bob, prefix);

            records.push(BenchmarkRecord {
                scenario: "scale_diff".to_string(),
                total_size: total_size_const,
                diff_size: diff,
                prefix_size: prefix,
                ratio: 1.5,
                encode_time_us: enc,
                decode_time_us: dec,
                memory_bytes: mem,
                bandwidth_bytes: bw,
                success,
                recovered_local: rec_loc,
                recovered_remote: rec_rem,
                trial,
            });

            pb.inc(1);
        }
        pb.set_message(format!("S1: diff={} ✓", diff));
    }

    // --- Scenario 2: Varying Overhead Ratio ---
    pb.set_message("S2: Varying overhead ratio");
    let diff_const = 1000;

    for &ratio in &ratios {
        for trial in 0..trials {
            let (alice, bob) = generate_sets(&mut rng, total_size_const, diff_const);
            let prefix = (diff_const as f64 * ratio) as usize;

            let (enc, dec, mem, bw, success, rec_loc, rec_rem) = run_trial(&alice, &bob, prefix);

            records.push(BenchmarkRecord {
                scenario: "varying_overhead".to_string(),
                total_size: total_size_const,
                diff_size: diff_const,
                prefix_size: prefix,
                ratio,
                encode_time_us: enc,
                decode_time_us: dec,
                memory_bytes: mem,
                bandwidth_bytes: bw,
                success,
                recovered_local: rec_loc,
                recovered_remote: rec_rem,
                trial,
            });

            pb.inc(1);
        }
        pb.set_message(format!("S2: ratio={:.1} ✓", ratio));
    }

    // --- Scenario 3: Varying Density ---
    pb.set_message("S3: Varying set density");

    for &tot in &total_sizes {
        if tot < diff_const {
            pb.inc(trials as u64);
            continue;
        }
        for trial in 0..trials {
            let (alice, bob) = generate_sets(&mut rng, tot, diff_const);
            let prefix = (diff_const as f64 * 1.5) as usize;

            let (enc, dec, mem, bw, success, rec_loc, rec_rem) = run_trial(&alice, &bob, prefix);

            records.push(BenchmarkRecord {
                scenario: "varying_density".to_string(),
                total_size: tot,
                diff_size: diff_const,
                prefix_size: prefix,
                ratio: 1.5,
                encode_time_us: enc,
                decode_time_us: dec,
                memory_bytes: mem,
                bandwidth_bytes: bw,
                success,
                recovered_local: rec_loc,
                recovered_remote: rec_rem,
                trial,
            });

            pb.inc(1);
        }
        pb.set_message(format!("S3: size={} ✓", tot));
    }

    pb.finish_with_message("All benchmarks complete!");

    // Write results
    let file = File::create("riblt_benchmarks.csv").unwrap();
    let mut wtr = Writer::from_writer(file);
    for record in &records {
        wtr.serialize(record).unwrap();
    }
    wtr.flush().unwrap();

    println!("\n📊 Results written to riblt_benchmarks.csv");
    println!("📝 Total records: {}", records.len());
}
