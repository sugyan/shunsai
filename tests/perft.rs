//! Known perft values (DESIGN.md §6). All values assume fully legal
//! generation including pawn-drop-mate exclusion, with leaf bulk counting.

use shogi_core::PartialPosition;
use shogi_usi_parser::FromUsi;
use shunsai::Position;

const MAX_MOVES_SFEN: &str = "R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1";
const MATSURI_SFEN: &str = "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1";

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
        position.do_move(mv);
        nodes += perft(position, depth - 1);
        position.undo_move(mv);
    }
    nodes
}

fn perft_sfen(sfen: &str, depth: u32) -> u64 {
    let partial = PartialPosition::from_usi(&format!("sfen {sfen}")).unwrap();
    perft(&mut Position::new(partial), depth)
}

#[test]
fn initial_position() {
    let mut position = Position::startpos();
    assert_eq!(perft(&mut position, 1), 30);
    assert_eq!(perft(&mut position, 2), 900);
    assert_eq!(perft(&mut position, 3), 25470);
    assert_eq!(perft(&mut position, 4), 719731);
}

#[test]
#[ignore = "slow; run with --release -- --ignored"]
fn initial_position_deep() {
    let mut position = Position::startpos();
    assert_eq!(perft(&mut position, 5), 19861490);
    assert_eq!(perft(&mut position, 6), 547581517);
}

#[test]
fn max_moves_position() {
    assert_eq!(perft_sfen(MAX_MOVES_SFEN, 1), 593);
    assert_eq!(perft_sfen(MAX_MOVES_SFEN, 2), 105677);
}

/// No independently confirmed perft values for the matsuri position yet
/// (DESIGN.md: establish them by cross-perft against cshogi / YaneuraOu);
/// until then this is a smoke test, and the depth-1/2 counts are checked
/// against the `shogi_legality_lite` oracle in `differential.rs`.
#[test]
fn matsuri_position_smoke() {
    assert!(perft_sfen(MATSURI_SFEN, 2) > 0);
}
