//! Searches the magic multipliers for the magic slider backend and writes
//! `src/sliders/magics.rs`.
//!
//! ```sh
//! cargo run --release --example gen_magics             # regenerate
//! cargo run --release --example gen_magics -- --check  # verify, as CI does
//! ```
//!
//! Only the **multipliers** are generated. A multiplier is the result of a
//! search and cannot be derived; everything else about a magic — the mask and
//! both shifts — follows from the board geometry, so `src/sliders/magic.rs`
//! derives it at compile time instead, and the attack tables are
//! const-evaluated on top of that. Nothing is ever copied from another
//! engine: the search below is a plain brute force over a fixed-seed PRNG,
//! and it verifies every candidate exhaustively before accepting it, so the
//! output is reproducible and self-checking.
//!
//! `--check` regenerates into memory and compares against the committed file,
//! so "the constants in the repository are the ones this generator produces"
//! is a property CI holds rather than something verified by hand once.
//!
//! ## What a magic does here
//!
//! The board is file-major (`array_index = (file - 1) * 9 + (rank - 1)`), so
//! a rank is 9 bits at stride 9 and the diagonals are at stride 10 and 8.
//! For one line, the squares that can block are at most 7, and they always
//! span fewer than 64 bits once the lowest one is shifted down to bit 0.
//! The magic multiply gathers those scattered bits into a dense 7-bit index.

use std::fmt::Write as _;
use std::path::Path;

/// The file this generator owns. Absolute, so the working directory of the
/// `cargo run` does not matter.
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/sliders/magics.rs");

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

/// Finds a multiplier for one square/line, or panics. Returns it alongside
/// the index width it achieves, which is only reported for information.
///
/// The mask and shifts computed here are the same values `magic.rs` derives;
/// they are needed to *run* the search but are not part of its output.
fn find_magic(index: usize, line: i8, state: &mut u64) -> (u64, u32) {
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
            return (magic, bits.max(1));
        }
    }
    panic!("no magic found for square {index}, line {line}");
}

/// Four per row keeps every line inside 100 columns.
const PER_ROW: usize = 4;

fn emit(out: &mut String, name: &str, line: i8, state: &mut u64) {
    let mut widest = 0;
    let multipliers: Vec<u64> = (0..81)
        .map(|index| {
            let (magic, bits) = find_magic(index, line, state);
            widest = widest.max(bits);
            magic
        })
        .collect();
    eprintln!("{name}: widest index = {widest} bits");

    writeln!(
        out,
        "/// Multipliers for the {name} line (rank delta {line} per file step)."
    )
    .unwrap();
    // The layout is chosen here, so rustfmt must leave it alone.
    writeln!(out, "#[rustfmt::skip]").unwrap();
    writeln!(out, "pub(crate) static {name}_MULTIPLIERS: [u64; 81] = [").unwrap();
    for row in multipliers.chunks(PER_ROW) {
        let cells: Vec<String> = row.iter().map(|magic| format!("{magic:#018x}")).collect();
        writeln!(out, "    {},", cells.join(", ")).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
}

fn generate() -> String {
    let mut out = String::from(
        "//! Magic multipliers for the magic slider backend (the M4 default).\n\
         //!\n\
         //! GENERATED FILE - do not edit by hand. Regenerate from the\n\
         //! repository with:\n\
         //!\n\
         //! ```sh\n\
         //! cargo run --release --example gen_magics\n\
         //! ```\n\
         //!\n\
         //! The generator (`examples/gen_magics.rs`) brute-forces these from a fixed\n\
         //! splitmix64 seed and verifies each candidate against every occupancy of its\n\
         //! line before accepting it, so a rerun reproduces this file byte for byte —\n\
         //! which CI checks on every run. These multipliers are the *only* generated\n\
         //! part of the magic backend: `magic.rs` derives each mask and shift from the\n\
         //! board geometry and const-evaluates the attack tables from there, so no\n\
         //! table is ever transcribed from elsewhere.\n\
         \n",
    );
    // One PRNG stream for the whole file keeps the output reproducible.
    let mut state = 0x0000_5348_554e_5341;
    emit(&mut out, "RANK", 0, &mut state);
    emit(&mut out, "DIAGONAL_UP", 1, &mut state);
    emit(&mut out, "DIAGONAL_DOWN", -1, &mut state);
    // `emit` separates arrays with a blank line; the file does not end in one.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn main() {
    let check = std::env::args().any(|argument| argument == "--check");
    let generated = generate();
    let path = Path::new(OUTPUT);

    if check {
        let committed = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        if committed != generated {
            eprintln!(
                "{} is not what the generator produces.\n\
                 Regenerate it with:\n\
                 \n    cargo run --release --example gen_magics\n",
                path.display()
            );
            std::process::exit(1);
        }
        eprintln!("{} is up to date", path.display());
        return;
    }

    std::fs::write(path, &generated)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
    eprintln!("wrote {}", path.display());
}
