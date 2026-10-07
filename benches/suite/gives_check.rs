//! Whether a move checks, asked of every legal move of the sampled real-game
//! fixture (`benches/positions/sampled-v1.sfen`): through
//! `Position::gives_check`, and through the round trip a caller has without
//! it — `do_move`, `in_check`, `undo_move` — on the same moves.
//! Elements = moves.

use criterion::{Criterion, Throughput, criterion_group};
use shogi_core::Move;
use shunsai::Position;

use crate::common;

/// The `sampled-v1` id's body: `gives_check` asked of every move.
fn count_by_gives_check(cases: &[(Position, Vec<Move>)]) -> u32 {
    let mut checks = 0;
    for (position, moves) in cases {
        for &mv in moves {
            checks += position.gives_check(mv) as u32;
        }
    }
    checks
}

/// The `sampled-v1-make-unmake` id's body: the same answers by making each
/// move, asking `in_check`, and taking it back, on `positions` (copies of
/// the cases' positions, left as they were).
fn count_by_make_unmake(cases: &[(Position, Vec<Move>)], positions: &mut [Position]) -> u32 {
    let mut checks = 0;
    for ((_, moves), position) in cases.iter().zip(positions) {
        for &mv in moves {
            let undo = position.do_move(mv);
            checks += position.in_check() as u32;
            position.undo_move(mv, undo);
        }
    }
    checks
}

fn bench_gives_check(c: &mut Criterion) {
    let sampled = common::sampled_positions();
    assert_eq!(sampled.len(), 40, "sampled-v1 is frozen at 40 positions");
    // Each position with its moves, collected outside the measured loop.
    let cases: Vec<(Position, Vec<Move>)> = sampled
        .into_iter()
        .map(|position| {
            let moves = position.legal_moves();
            (position, moves)
        })
        .collect();
    let total_moves: u64 = cases.iter().map(|(_, moves)| moves.len() as u64).sum();

    // Guard: the two paths agree on every move, some of them check, and each
    // timed body counts exactly those checks, so neither id can time a wrong
    // answer, a fixture with nothing to find, or a body that drifted from
    // what was checked here.
    let mut checks = 0;
    for (position, moves) in &cases {
        let mut position = position.clone();
        for &mv in moves {
            let undo = position.do_move(mv);
            let expected = position.in_check();
            position.undo_move(mv, undo);
            assert_eq!(
                position.gives_check(mv),
                expected,
                "paths disagree on {mv:?}"
            );
            checks += expected as u32;
        }
    }
    assert!(checks > 0, "sampled-v1 has no checking move");
    let mut positions: Vec<Position> = cases.iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(count_by_gives_check(&cases), checks);
    assert_eq!(count_by_make_unmake(&cases, &mut positions), checks);

    let mut group = c.benchmark_group("gives_check");
    group.throughput(Throughput::Elements(total_moves));
    group.bench_function("sampled-v1", |b| b.iter(|| count_by_gives_check(&cases)));
    group.bench_function("sampled-v1-make-unmake", |b| {
        b.iter(|| count_by_make_unmake(&cases, &mut positions))
    });
    group.finish();
}

criterion_group!(benches, bench_gives_check);
