//! Perft throughput (nodes/sec) on the fixed positions of BENCHMARKS.md.

use std::ops::ControlFlow;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group};
use shogi_core::Move;
use shunsai::Position;

use crate::common;

/// The M1 allocating `Vec` API (leaf bulk counting).
fn perft(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = position.legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0;
    for mv in moves {
        let undo = position.do_move(mv);
        nodes += perft(position, depth - 1);
        position.undo_move(mv, undo);
    }
    nodes
}

/// The M3 callback API, as `examples/perft.rs` uses it: leaf parents count
/// straight off the destination bitboards and never build a `Move`.
fn perft_cb(position: &mut Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        let mut nodes = 0;
        let _ = position.generate_moves(|set| {
            nodes += set.len() as u64;
            ControlFlow::Continue(())
        });
        return nodes;
    }
    let mut moves = Vec::with_capacity(128);
    let _ = position.generate_moves(|set| {
        moves.extend(set);
        ControlFlow::Continue(())
    });
    let mut nodes = 0;
    for mv in moves {
        let undo = position.do_move(mv);
        nodes += perft_cb(position, depth - 1);
        position.undo_move(mv, undo);
    }
    nodes
}

/// The callback API again, but with one buffer reused for the whole tree
/// instead of a fresh `Vec` per internal node.
///
/// The leaf path is identical to `perft_cb`, so the difference this id
/// measures is purely the internal-node collection that the callback still
/// forces: `do_move` needs `&mut Position`, which the listener's borrow
/// blocks, so anything deeper than one ply has to collect first. Each ply
/// takes a slice of the shared buffer and truncates back on the way out.
fn perft_cb_buf(position: &mut Position, depth: u32, buf: &mut Vec<Move>) -> u64 {
    if depth == 0 {
        return 1;
    }
    if depth == 1 {
        let mut nodes = 0;
        let _ = position.generate_moves(|set| {
            nodes += set.len() as u64;
            ControlFlow::Continue(())
        });
        return nodes;
    }
    let base = buf.len();
    let _ = position.generate_moves(|set| {
        buf.extend(set);
        ControlFlow::Continue(())
    });
    let mut nodes = 0;
    let mut i = base;
    while i < buf.len() {
        let mv = buf[i];
        let undo = position.do_move(mv);
        nodes += perft_cb_buf(position, depth - 1, buf);
        position.undo_move(mv, undo);
        i += 1;
    }
    buf.truncate(base);
    nodes
}

/// The same tree again, but leaf parents obtain their count by
/// **materializing** every legal move instead of popcounting the destination
/// bitboards — which is what every engine in the cross-engine comparison does
/// (YaneuraOu counts `MoveList<LEGAL_ALL>(pos).size()`, the apery driver
/// `MoveList<LegalAll>`, yasai / rshogi / cshogi their legal-move lists).
///
/// This is the id whose numbers are comparable to theirs, and the gap to
/// `-cb` is the size of the advantage `MoveSet::len()` buys on perft and only
/// on perft: a search must have the moves. Mirrors the `--materialize` driver
/// in `examples/perft.rs`, which is what the cross-engine harness runs.
///
/// The buffer is threaded through the whole tree and each ply truncates back
/// on the way out, so like `perft_cb_buf` the walk allocates nothing — the
/// other engines' leaf move lists are stack arrays. That holds at any depth
/// rather than only at the fixture depths, because the capacity comes from
/// `common::tree_buf_capacity`.
///
/// `WI` picks how the moves are materialized: `false` drives the
/// `MoveSetIter` that `Extend` uses (`-mat`), `true` uses
/// `MoveSet::write_into` (`-mat-wi`, and what `examples/perft.rs
/// --materialize` — hence the cross-engine harness — runs). A const
/// parameter rather than a flag, so neither id carries the other's branch.
fn perft_mat<const WI: bool>(position: &mut Position, depth: u32, buf: &mut Vec<Move>) -> u64 {
    if depth == 0 {
        return 1;
    }
    let base = buf.len();
    let _ = position.generate_moves(|set| {
        if WI {
            set.write_into(buf);
        } else {
            buf.extend(set);
        }
        ControlFlow::Continue(())
    });
    if depth == 1 {
        let nodes = (buf.len() - base) as u64;
        buf.truncate(base);
        return nodes;
    }
    let mut nodes = 0;
    let mut i = base;
    while i < buf.len() {
        let mv = buf[i];
        let undo = position.do_move(mv);
        nodes += perft_mat::<WI>(position, depth - 1, buf);
        position.undo_move(mv, undo);
        i += 1;
    }
    buf.truncate(base);
    nodes
}

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("perft");
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(20)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(10));
    for (name, sfen, depth, nodes) in [
        ("startpos", "startpos", 4u32, 719_731u64),
        ("matsuri", common::MATSURI_SFEN, 3, 4_809_015),
        ("maxmoves", common::MAX_MOVES_SFEN, 2, 105_677),
    ] {
        let mut position = common::position(sfen);
        // Guard: the measured work is exactly the claimed node count, and
        // all four drivers walk the same tree.
        assert_eq!(perft(&mut position, depth), nodes);
        assert_eq!(perft_cb(&mut position, depth), nodes);
        assert_eq!(
            perft_cb_buf(
                &mut position,
                depth,
                &mut Vec::with_capacity(common::tree_buf_capacity(depth))
            ),
            nodes
        );
        assert_eq!(
            perft_mat::<false>(
                &mut position,
                depth,
                &mut Vec::with_capacity(common::tree_buf_capacity(depth))
            ),
            nodes
        );
        assert_eq!(
            perft_mat::<true>(
                &mut position,
                depth,
                &mut Vec::with_capacity(common::tree_buf_capacity(depth))
            ),
            nodes
        );
        group.throughput(Throughput::Elements(nodes));
        group.bench_with_input(BenchmarkId::new(name, depth), &depth, |b, &depth| {
            b.iter(|| perft(&mut position, depth))
        });
        group.bench_with_input(
            BenchmarkId::new(format!("{name}-cb"), depth),
            &depth,
            |b, &depth| b.iter(|| perft_cb(&mut position, depth)),
        );
        group.bench_with_input(
            BenchmarkId::new(format!("{name}-cb-buf"), depth),
            &depth,
            |b, &depth| {
                // Allocated once, outside the measured closure: the point of
                // this id is that the tree walk itself never allocates.
                let mut buf = Vec::with_capacity(common::tree_buf_capacity(depth));
                b.iter(|| perft_cb_buf(&mut position, depth, &mut buf))
            },
        );
        group.bench_with_input(
            BenchmarkId::new(format!("{name}-mat"), depth),
            &depth,
            |b, &depth| {
                let mut buf = Vec::with_capacity(common::tree_buf_capacity(depth));
                b.iter(|| perft_mat::<false>(&mut position, depth, &mut buf))
            },
        );
        group.bench_with_input(
            BenchmarkId::new(format!("{name}-mat-wi"), depth),
            &depth,
            |b, &depth| {
                let mut buf = Vec::with_capacity(common::tree_buf_capacity(depth));
                b.iter(|| perft_mat::<true>(&mut position, depth, &mut buf))
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_perft);
