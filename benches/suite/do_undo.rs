//! `do_move` + `undo_move` round-trip throughput over real-game move
//! sequences (`benches/positions/games-v1.usi`), with no movegen in the
//! measured loop. Elements = moves, i.e. one element is one do+undo pair.

use criterion::{Criterion, Throughput, criterion_group};
use shunsai::{Position, Undo};

use crate::common;

fn bench_do_undo(c: &mut Criterion) {
    let games = common::fixture_games();
    assert_eq!(games.len(), 4, "games-v1 is frozen at 4 games");
    let total_moves: u64 = games.iter().map(|game| game.len() as u64).sum();
    let longest = games.iter().map(|game| game.len()).max().unwrap_or(0);

    // The undo stack the position does not own. Allocated once, outside the
    // measured loop, and never grown inside it: a search keeps one of these
    // per thread, so its reallocation is not what this bench is asking about.
    let mut undos: Vec<Undo> = Vec::with_capacity(longest);

    // Guard: every fixture move is legal where it is played (`do_move`'s
    // contract — a round-trip alone would also pass on garbage moves), and
    // replaying then rewinding each game restores the start position.
    let mut position = Position::startpos();
    for game in &games {
        for &mv in game {
            assert!(
                position.legal_moves().contains(&mv),
                "games-v1 plays an illegal move: {mv:?}"
            );
            undos.push(position.do_move(mv));
        }
        for &mv in game.iter().rev() {
            position.undo_move(mv, undos.pop().unwrap());
        }
        assert_eq!(position, Position::startpos());
    }

    let mut group = c.benchmark_group("do_undo");
    group.throughput(Throughput::Elements(total_moves));
    group.bench_function("games-v1", |b| {
        b.iter(|| {
            for game in &games {
                for &mv in game {
                    undos.push(position.do_move(mv));
                }
                for &mv in game.iter().rev() {
                    position.undo_move(mv, undos.pop().unwrap());
                }
            }
            position.key()
        })
    });
    group.finish();
}

criterion_group!(benches, bench_do_undo);
