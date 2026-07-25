//! The single bench target combining all criterion groups (one `[[bench]]`
//! section in Cargo.toml); see BENCHMARKS.md for the suite documentation.
//!
//! The `internals` group needs the `bench-internals` feature; without it
//! the module is compiled out and the rest of the suite runs.

mod common;
mod do_undo;
#[cfg(feature = "bench-internals")]
mod internals;
mod movegen;
mod perft;

#[cfg(feature = "bench-internals")]
criterion::criterion_main!(
    perft::benches,
    movegen::benches,
    do_undo::benches,
    internals::benches,
);
#[cfg(not(feature = "bench-internals"))]
criterion::criterion_main!(perft::benches, movegen::benches, do_undo::benches);
