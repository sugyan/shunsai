//! `legal_moves()` cost per position: the fixed positions of DESIGN.md §4
//! plus the sampled real-game fixture (`benches/positions/sampled-v1.sfen`).
//!
//! The plain ids measure the M1 allocating `Vec` API; the `-cb` ids measure
//! the M3 callback API on the same work (bench ids are append-only, see
//! BENCHMARKS.md).
//!
//! The `-cb` variants sum `MoveSet::len()`, which is the callback API's
//! reason to exist: no `Move` is ever materialized and nothing is
//! allocated.
//!
//! The `-buf` variants materialize every move into a buffer allocated
//! *outside* the measured closure. They exist because the two ids above
//! differ in two things at once — whether a `Move` is built and whether the
//! list is allocated — and only one of those is interesting: a search must
//! have the moves, but it owns its own buffer. `-buf` minus `-cb` is the
//! expansion loop, and the plain id minus `-buf` is the allocation.

use criterion::{Criterion, Throughput, criterion_group};
use shogi_core::Move;
use shunsai::Position;
use std::ops::ControlFlow;

use crate::common;

/// Counts legal moves through the callback API without building any.
fn count_moves(position: &Position) -> usize {
    let mut total = 0;
    let _ = position.generate_moves(|set| {
        total += set.len();
        ControlFlow::Continue(())
    });
    total
}

/// Materializes every legal move into a caller-owned buffer, through the
/// `MoveSetIter` the `IntoIterator` impl hands out.
fn fill_moves(position: &Position, buf: &mut Vec<Move>) -> usize {
    buf.clear();
    let _ = position.generate_moves(|set| {
        buf.extend(set);
        ControlFlow::Continue(())
    });
    buf.len()
}

/// The same, through `MoveSet::write_into`, which drains each destination
/// set in its own loop instead of re-deciding per move.
fn fill_moves_wi(position: &Position, buf: &mut Vec<Move>) -> usize {
    buf.clear();
    let _ = position.generate_moves(|set| {
        set.write_into(buf);
        ControlFlow::Continue(())
    });
    buf.len()
}

fn bench_movegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("movegen");

    for (name, sfen) in [
        ("startpos", "startpos"),
        ("matsuri", common::MATSURI_SFEN),
        ("maxmoves", common::MAX_MOVES_SFEN),
    ] {
        let position = common::position(sfen);
        assert_eq!(count_moves(&position), position.legal_moves().len());
        assert_eq!(
            fill_moves(&position, &mut Vec::new()),
            position.legal_moves().len()
        );
        assert_eq!(
            fill_moves_wi(&position, &mut Vec::new()),
            position.legal_moves().len()
        );
        group.bench_function(name, |b| b.iter(|| position.legal_moves()));
        group.bench_function(format!("{name}-cb"), |b| b.iter(|| count_moves(&position)));
        group.bench_function(format!("{name}-buf"), |b| {
            // Allocated once, outside the measured closure.
            let mut buf = Vec::with_capacity(common::MOVE_BUF_CAPACITY);
            b.iter(|| fill_moves(&position, &mut buf))
        });
        group.bench_function(format!("{name}-wi"), |b| {
            let mut buf = Vec::with_capacity(common::MOVE_BUF_CAPACITY);
            b.iter(|| fill_moves_wi(&position, &mut buf))
        });
    }

    let sampled = common::sampled_positions();
    assert_eq!(sampled.len(), 40, "sampled-v1 is frozen at 40 positions");
    let in_check: Vec<Position> = sampled.iter().filter(|p| p.in_check()).cloned().collect();
    assert!(
        in_check.len() >= 8,
        "sampled-v1 must contain >= 8 in-check positions"
    );

    // One iteration sweeps the whole set, so per-position noise averages out;
    // Elements = positions, i.e. throughput is positions/sec.
    for (name, positions) in [("sampled-v1", &sampled), ("sampled-v1-check", &in_check)] {
        group.throughput(Throughput::Elements(positions.len() as u64));
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut total = 0;
                for position in positions {
                    total += position.legal_moves().len();
                }
                total
            })
        });
        group.bench_function(format!("{name}-cb"), |b| {
            b.iter(|| {
                let mut total = 0;
                for position in positions {
                    total += count_moves(position);
                }
                total
            })
        });
        group.bench_function(format!("{name}-buf"), |b| {
            let mut buf = Vec::with_capacity(common::MOVE_BUF_CAPACITY);
            b.iter(|| {
                let mut total = 0;
                for position in positions {
                    total += fill_moves(position, &mut buf);
                }
                total
            })
        });
        group.bench_function(format!("{name}-wi"), |b| {
            let mut buf = Vec::with_capacity(common::MOVE_BUF_CAPACITY);
            b.iter(|| {
                let mut total = 0;
                for position in positions {
                    total += fill_moves_wi(position, &mut buf);
                }
                total
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_movegen);
