//! Differential testing against `shogi_legality_lite`, the correctness oracle:
//! random playouts from the fixed position set, asserting set-equality of the
//! full legal-move sets at every node, plus Zobrist-key consistency between
//! incremental updates and rebuilds.

use shogi_core::{Move, PartialPosition};
use shogi_usi_parser::FromUsi;
use shunsai::Position;

const SEED_SFENS: &[&str] = &[
    // Max-moves position.
    "R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1",
    // Matsuri midgame position.
    "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1",
];

/// splitmix64 (public-domain algorithm by Sebastiano Vigna); fixed seeds
/// keep the playouts reproducible.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// A sortable identity for a move (shogi_core's `Ord` is feature-gated).
fn move_key(mv: Move) -> (u8, u8, u8, u8) {
    match mv {
        Move::Normal { from, to, promote } => (0, from.index(), to.index(), promote as u8),
        Move::Drop { piece, to } => (1, piece.as_u8(), to.index(), 0),
    }
}

fn assert_same_moves(partial: &PartialPosition, position: &Position, ply: usize) {
    let mut ours: Vec<_> = position.legal_moves().into_iter().map(move_key).collect();
    let mut oracle: Vec<_> = shogi_legality_lite::all_legal_moves_partial(partial)
        .into_iter()
        .map(move_key)
        .collect();
    ours.sort_unstable();
    oracle.sort_unstable();
    assert_eq!(
        ours,
        oracle,
        "legal-move sets differ at ply {ply} in\nsfen {}\n{position:?}",
        partial.to_sfen_owned()
    );
}

fn playout(start: &PartialPosition, games: u32, max_plies: u32, rng_seed: u64) {
    let mut state = rng_seed;
    for game in 0..games {
        let mut partial = start.clone();
        let mut position = Position::new(partial.clone());
        for ply in 0..max_plies {
            assert_same_moves(&partial, &position, ply as usize);
            let moves = position.legal_moves();
            if moves.is_empty() {
                break;
            }
            let mv = moves[splitmix64(&mut state) as usize % moves.len()];
            position.do_move(mv);
            partial.make_move(mv).unwrap_or_else(|| {
                panic!(
                    "oracle rejected {mv:?} at ply {ply} of game {game} in\nsfen {}",
                    partial.to_sfen_owned()
                )
            });
            // The incrementally updated key must match a from-scratch rebuild.
            assert_eq!(
                position.key(),
                Position::new(partial.clone()).key(),
                "zobrist key diverged at ply {ply} of game {game}"
            );
        }
    }
}

#[test]
fn differential_startpos() {
    playout(&PartialPosition::startpos(), 16, 80, 0x0053_4855_4e53_4101);
}

/// The real-game fixture as differential seeds. Broader than the two
/// hand-picked positions above — 40 middlegame/endgame positions, several of
/// them in check — which is where pins, discovered checks and interposition
/// actually occur.
#[test]
fn differential_sampled_positions() {
    let path = format!(
        "{}/benches/positions/sampled-v1.sfen",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("sampled-v1 fixture must be readable");
    let mut seeds = 0;
    for (index, line) in text.lines().enumerate() {
        let sfen = line.split('\t').next().unwrap_or(line).trim();
        if sfen.is_empty() || sfen.starts_with('#') {
            continue;
        }
        let partial = PartialPosition::from_usi(&format!("sfen {sfen}"))
            .unwrap_or_else(|_| panic!("fixture SFEN must parse: {sfen}"));
        playout(&partial, 2, 24, 0x0053_4855_4e53_4200 + index as u64);
        seeds += 1;
    }
    assert_eq!(seeds, 40, "sampled-v1 is frozen at 40 positions");
}

#[test]
fn differential_seed_positions() {
    for (index, sfen) in SEED_SFENS.iter().enumerate() {
        let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
        playout(&partial, 8, 64, 0x0053_4855_4e53_4102 + index as u64);
    }
}
