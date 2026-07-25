//! `do_move` + `undo_move` round-trip throughput over real-game move
//! sequences (`benches/positions/games-v1.usi`), with no movegen in the
//! measured loop. Elements = moves, i.e. one element is one do+undo pair.

use criterion::{Criterion, Throughput, criterion_group};
use shunsai::Position;

use crate::common;

fn bench_do_undo(c: &mut Criterion) {
    let games = common::fixture_games();
    assert_eq!(games.len(), 4, "games-v1 is frozen at 4 games");
    let total_moves: u64 = games.iter().map(|game| game.len() as u64).sum();

    // Guard: replaying and rewinding each game restores the start position.
    let mut position = Position::startpos();
    for game in &games {
        for &mv in game {
            position.do_move(mv);
        }
        for &mv in game.iter().rev() {
            position.undo_move(mv);
        }
        assert_eq!(position, Position::startpos());
    }

    let mut group = c.benchmark_group("do_undo");
    group.throughput(Throughput::Elements(total_moves));
    group.bench_function("games-v1", |b| {
        b.iter(|| {
            for game in &games {
                for &mv in game {
                    position.do_move(mv);
                }
                for &mv in game.iter().rev() {
                    position.undo_move(mv);
                }
            }
            position.key()
        })
    });
    group.finish();
}

criterion_group!(benches, bench_do_undo);
