//! Helpers shared between bench targets (each `[[bench]]` compiles this
//! file separately via `mod common;`).

#![allow(dead_code)]

use shogi_core::{Color, Move, PartialPosition, Piece};
use shogi_usi_parser::FromUsi;
use shunsai::Position;

pub const MATSURI_SFEN: &str =
    "l6nl/5+P1gk/2np1S3/p1p4Pp/3P2Sp1/1PPb2P1P/P5GS1/R8/LN4bKL w GR5pnsg 1";
pub const MAX_MOVES_SFEN: &str = "R8/2K1S1SSk/4B4/9/9/9/9/9/1L1L1L3 b RBGSNLP3g3n17p 1";

pub fn position(sfen: &str) -> Position {
    let partial = if sfen == "startpos" {
        PartialPosition::startpos()
    } else {
        PartialPosition::from_usi(&format!("sfen {sfen}")).expect("fixture SFEN must parse")
    };
    Position::new(partial)
}

fn read_fixture(name: &str) -> String {
    let path = format!("{}/benches/positions/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read fixture {path}: {e}"))
}

/// The sampled-v1 movegen fixture: one `<sfen>\t# tags` per line, `#` lines
/// are comments.
pub fn sampled_positions() -> Vec<Position> {
    read_fixture("sampled-v1.sfen")
        .lines()
        .filter_map(|line| {
            let sfen = line.split('\t').next().unwrap_or(line).trim();
            (!sfen.is_empty() && !sfen.starts_with('#')).then(|| position(sfen))
        })
        .collect()
}

/// The games-v1 do/undo fixture: one `<name>\t<usi moves>` game per line,
/// each a move sequence from the initial position.
///
/// USI drop notation carries no side and the parser assumes Black, so drop
/// colors are fixed up from ply parity (Black always moves first here).
pub fn fixture_games() -> Vec<Vec<Move>> {
    read_fixture("games-v1.usi")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let moves = line
                .split('\t')
                .nth(1)
                .expect("game line format: <name>\\t<moves>");
            moves
                .split_whitespace()
                .enumerate()
                .map(|(ply, token)| {
                    let mv = Move::from_usi(token).expect("fixture move must parse");
                    match mv {
                        Move::Drop { piece, to } if ply % 2 == 1 => {
                            let (kind, _) = piece.to_parts();
                            Move::Drop {
                                piece: Piece::new(kind, Color::White),
                                to,
                            }
                        }
                        _ => mv,
                    }
                })
                .collect()
        })
        .collect()
}
