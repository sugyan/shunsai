//! Searches the magic multipliers for the magic slider backend and prints
//! `src/sliders/magics.rs`.
//!
//! Usage: `cargo run --release --example gen_magics > src/sliders/magics.rs`
//!
//! Only the *constants* are generated here; the attack tables themselves are
//! const-evaluated from them at compile time (see `src/sliders/magic.rs`).
//! Nothing is ever copied from another engine — the search below is a plain
//! brute force over a fixed-seed PRNG, and it verifies every candidate
//! exhaustively before accepting it, so the output is reproducible and
//! self-checking.
//!
//! ## What a magic does here
//!
//! The board is file-major (`array_index = (file - 1) * 9 + (rank - 1)`), so
//! a rank is 9 bits at stride 9 and the diagonals are at stride 10 and 8.
//! For one line, the squares that can block are at most 7, and they always
//! span fewer than 64 bits once the lowest one is shifted down to bit 0.
//! The magic multiply gathers those scattered bits into a dense 7-bit index.

/// splitmix64 (public-domain algorithm by Sebastiano Vigna); the fixed seed
/// keeps the generated file byte-identical across runs.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

const fn file_of(index: usize) -> i8 {
    (index / 9) as i8 + 1
}
const fn rank_of(index: usize) -> i8 {
    (index % 9) as i8 + 1
}
const fn index_of(file: i8, rank: i8) -> usize {
    ((file - 1) * 9 + (rank - 1)) as usize
}
const fn on_board(file: i8, rank: i8) -> bool {
    1 <= file && file <= 9 && 1 <= rank && rank <= 9
}

/// Independent re-derivation of `sliders::walk_line` (the crate-internal one
/// is not reachable from an example). Any disagreement between the two shows
/// up as a failure of the exhaustive backend tests.
fn walk_line(index: usize, occupied: u128, line: i8) -> u128 {
    let mut bits = 0u128;
    for sign in [1i8, -1] {
        let (mut file, mut rank) = (file_of(index), rank_of(index));
        loop {
            file += sign;
            rank += sign * line;
            if !on_board(file, rank) {
                break;
            }
            let to = index_of(file, rank);
            bits |= 1 << to;
            if occupied & (1 << to) != 0 {
                break;
            }
        }
    }
    bits
}

fn relevant_mask(index: usize, line: i8) -> u128 {
    let mut bits = 0u128;
    for sign in [1i8, -1] {
        let (mut file, mut rank) = (file_of(index), rank_of(index));
        loop {
            file += sign;
            rank += sign * line;
            if !on_board(file, rank) {
                break;
            }
            if on_board(file + sign, rank + sign * line) {
                bits |= 1 << index_of(file, rank);
            }
        }
    }
    bits
}

/// Every subset of `mask`, via the carry-rippler trick.
fn subsets(mask: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut subset = 0u64;
    loop {
        out.push(subset);
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 {
            break;
        }
    }
    out
}

struct Magic {
    mask: u64,
    magic: u64,
    shift_in: u32,
    shift_out: u32,
}

/// Finds a magic for one square/line, or panics. `shift_in` normalizes the
/// relevant squares down to bit 0 so the whole computation fits in a `u64`.
fn find_magic(index: usize, line: i8, state: &mut u64) -> Magic {
    let relevant = relevant_mask(index, line);
    let shift_in = if relevant == 0 {
        0
    } else {
        relevant.trailing_zeros()
    };
    let span = 128 - relevant.leading_zeros() - shift_in;
    assert!(
        span <= 64,
        "square {index} line {line}: relevant span {span}"
    );
    let mask = (relevant >> shift_in) as u64;
    let bits = mask.count_ones();
    // A short diagonal can have no blockable square at all; a shift of 64
    // would be UB, and shifting the (always zero) product by 63 gives the
    // single slot such a line needs.
    let shift_out = 64 - bits.max(1);

    // The attack set each occupancy must map to.
    let occupancies = subsets(mask);
    let expected: Vec<u128> = occupancies
        .iter()
        .map(|&occupancy| walk_line(index, (occupancy as u128) << shift_in, line))
        .collect();

    for _ in 0..100_000_000u64 {
        // Sparse candidates (the AND of three randoms) succeed far sooner
        // than uniform ones.
        let magic = splitmix64(state) & splitmix64(state) & splitmix64(state);
        let mut table = vec![u128::MAX; 1 << bits];
        let mut ok = true;
        for (&occupancy, &attacks) in occupancies.iter().zip(&expected) {
            let slot = (occupancy.wrapping_mul(magic) >> shift_out) as usize;
            if table[slot] == u128::MAX {
                table[slot] = attacks;
            } else if table[slot] != attacks {
                // A collision is fine only when both occupancies agree.
                ok = false;
                break;
            }
        }
        if ok {
            return Magic {
                mask,
                magic,
                shift_in,
                shift_out,
            };
        }
    }
    panic!("no magic found for square {index}, line {line}");
}

fn emit(name: &str, line: i8, state: &mut u64) {
    println!("/// Magics for the {name} line (rank delta {line} per file step).");
    println!("pub(crate) static {name}_MAGICS: [Magic; 81] = [");
    let mut widest = 0;
    for index in 0..81 {
        let magic = find_magic(index, line, state);
        widest = widest.max(64 - magic.shift_out);
        println!(
            "    Magic {{ mask: {:#018x}, magic: {:#018x}, shift_in: {}, shift_out: {} }},",
            magic.mask, magic.magic, magic.shift_in, magic.shift_out
        );
    }
    println!("];");
    println!();
    eprintln!("{name}: widest index = {widest} bits");
}

fn main() {
    println!("//! Magic multipliers for the magic slider backend (the M4 default).");
    println!("//!");
    println!("//! GENERATED FILE - do not edit by hand. Regenerate with:");
    println!("//!");
    println!("//! ```sh");
    println!("//! cargo run --release --example gen_magics > src/sliders/magics.rs");
    println!("//! ```");
    println!("//!");
    println!("//! The generator ([`examples/gen_magics.rs`]) brute-forces these from a");
    println!("//! fixed splitmix64 seed and verifies each candidate against every");
    println!("//! occupancy of its line before accepting it, so a rerun reproduces this");
    println!("//! file byte for byte. The attack tables are const-evaluated from these");
    println!("//! constants in `magic.rs`; no table is ever transcribed from elsewhere.");
    println!();
    println!("use super::magic::Magic;");
    println!();
    // One PRNG stream for the whole file keeps the output reproducible.
    let mut state = 0x0000_5348_554e_5341;
    emit("RANK", 0, &mut state);
    emit("DIAGONAL_UP", 1, &mut state);
    emit("DIAGONAL_DOWN", -1, &mut state);
}
