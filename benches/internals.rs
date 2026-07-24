//! Internal attack primitives (the M4 optimization swap boundary) measured
//! in isolation, via the `bench-internals` feature:
//! `cargo bench --features bench-internals`.
//!
//! Each id sweeps all 81 squares of the three fixed positions with their
//! real occupancy, so Elements = calls and throughput is calls/sec.

mod common;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use shunsai::shogi_core::{Color, Square};
use shunsai::{Bitboard, Position, internals};

fn bench_internals(c: &mut Criterion) {
    let positions: Vec<Position> = ["startpos", common::MATSURI_SFEN, common::MAX_MOVES_SFEN]
        .iter()
        .map(|sfen| common::position(sfen))
        .collect();
    let occupancies: Vec<Bitboard> = positions.iter().map(|p| p.occupied()).collect();
    let squares: Vec<Square> = Square::all().collect();
    let calls = (occupancies.len() * squares.len()) as u64;

    let mut group = c.benchmark_group("internals");

    group.throughput(Throughput::Elements(calls));
    group.bench_function("bishop-attacks", |b| {
        b.iter(|| {
            let mut acc = Bitboard::EMPTY;
            for &occupied in &occupancies {
                for &square in &squares {
                    acc |= internals::bishop_attacks(square, occupied);
                }
            }
            acc
        })
    });
    group.bench_function("rook-attacks", |b| {
        b.iter(|| {
            let mut acc = Bitboard::EMPTY;
            for &occupied in &occupancies {
                for &square in &squares {
                    acc |= internals::rook_attacks(square, occupied);
                }
            }
            acc
        })
    });

    group.throughput(Throughput::Elements(calls * 2));
    group.bench_function("lance-attacks", |b| {
        b.iter(|| {
            let mut acc = Bitboard::EMPTY;
            for &occupied in &occupancies {
                for &square in &squares {
                    acc |= internals::lance_attacks(Color::Black, square, occupied);
                    acc |= internals::lance_attacks(Color::White, square, occupied);
                }
            }
            acc
        })
    });

    group.throughput(Throughput::Elements(calls));
    group.bench_function("attackers-to", |b| {
        b.iter(|| {
            let mut acc = Bitboard::EMPTY;
            for position in &positions {
                let them = position.side_to_move().flip();
                let occupied = position.occupied();
                let mask = position.player_bb(them);
                for &square in &squares {
                    acc |= internals::attackers_to(position, square, occupied, them, mask);
                }
            }
            acc
        })
    });

    group.finish();
}

criterion_group!(benches, bench_internals);
criterion_main!(benches);
