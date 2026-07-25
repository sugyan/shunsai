//! Generates the committed benchmark fixtures (`benches/positions/`) from
//! floodgate CSA game records.
//!
//! Kifu I/O is a project non-goal (DESIGN.md): the library has no kifu API,
//! and this dev-only example is the whole extent of CSA handling. The
//! download step and the fixture-versioning rules are documented in
//! BENCHMARKS.md; raw CSA files are never committed. Regeneration from the
//! same input files and seed is byte-identical.
//!
//! Every game move is validated against `legal_moves()` before being
//! accepted, so a successful run is also a differential test of the move
//! generator on real games.
//!
//! Usage:
//!   cargo run --release --example gen_bench_positions -- \
//!       --csa-dir <dir with *.csa files> --out-dir benches/positions \
//!       [--seed <u64, default 0x53554E5341492D31>]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use shogi_core::{Color, Move, PartialPosition, Piece, PieceKind, Square, ToUsi};
use shunsai::Position;

/// Game filter: both players' floodgate rates must be at least this.
const MIN_RATE: u32 = 3000;
/// Game filter: minimum game length in plies.
const MIN_GAME_PLIES: usize = 60;
/// Number of qualifying games positions are sampled from.
const NUM_SOURCE_GAMES: usize = 24;
/// Number of games emitted to `games-v1.usi` (the do/undo fixture).
const NUM_FIXTURE_GAMES: usize = 4;
/// Minimum length of a `games-v1.usi` game.
const MIN_FIXTURE_GAME_PLIES: usize = 120;
/// `(phase, first ply, last ply, quota)` sampling buckets.
const BUCKETS: [(&str, usize, usize, usize); 3] = [
    ("opening", 11, 30, 12),
    ("middle", 31, 90, 16),
    ("end", 91, usize::MAX, 12),
];
/// Minimum number of in-check positions across all buckets.
const MIN_IN_CHECK: usize = 8;
/// Maximum positions sampled from a single game.
const MAX_PER_GAME: usize = 3;
const DEFAULT_SEED: u64 = 0x53554E5341492D31;

/// splitmix64 (public-domain algorithm by Sebastiano Vigna), as in
/// `tests/differential.rs`.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn shuffle<T>(items: &mut [T], state: &mut u64) {
    for i in (1..items.len()).rev() {
        let j = (splitmix64(state) % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

struct CsaMove {
    black: bool,
    /// `None` for drops.
    from: Option<(u8, u8)>,
    to: (u8, u8),
    /// The piece kind *after* the move (CSA notation).
    kind: PieceKind,
}

struct CsaGame {
    name: String,
    moves: Vec<CsaMove>,
}

fn piece_kind_from_csa(code: &str) -> Option<PieceKind> {
    Some(match code {
        "FU" => PieceKind::Pawn,
        "KY" => PieceKind::Lance,
        "KE" => PieceKind::Knight,
        "GI" => PieceKind::Silver,
        "KI" => PieceKind::Gold,
        "KA" => PieceKind::Bishop,
        "HI" => PieceKind::Rook,
        "OU" => PieceKind::King,
        "TO" => PieceKind::ProPawn,
        "NY" => PieceKind::ProLance,
        "NK" => PieceKind::ProKnight,
        "NG" => PieceKind::ProSilver,
        "UM" => PieceKind::ProBishop,
        "RY" => PieceKind::ProRook,
        _ => return None,
    })
}

/// Parses one floodgate CSA file; returns the game only if it passes the
/// rate / termination / length filters.
fn parse_qualifying_game(name: &str, text: &str) -> Option<CsaGame> {
    let mut black_rate = None;
    let mut white_rate = None;
    let mut last_special = None;
    let mut moves = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("'black_rate:") {
            black_rate = rest.rsplit(':').next()?.parse::<f64>().ok();
        } else if let Some(rest) = line.strip_prefix("'white_rate:") {
            white_rate = rest.rsplit(':').next()?.parse::<f64>().ok();
        } else if line.starts_with('%') {
            last_special = Some(line.split(',').next().unwrap_or(line).to_owned());
        } else if let Some(mv) = parse_csa_move(line) {
            moves.push(mv);
        }
    }
    let ok_rate = |rate: Option<f64>| rate.is_some_and(|r| r >= MIN_RATE as f64);
    if !ok_rate(black_rate) || !ok_rate(white_rate) {
        return None;
    }
    if !matches!(last_special.as_deref(), Some("%TORYO") | Some("%KACHI")) {
        return None;
    }
    if moves.len() < MIN_GAME_PLIES {
        return None;
    }
    Some(CsaGame {
        name: name.to_owned(),
        moves,
    })
}

/// Parses a `[+-]XXYYPP` move statement (a statement may carry `,T<secs>`).
fn parse_csa_move(line: &str) -> Option<CsaMove> {
    let statement = line.split(',').next().unwrap_or(line);
    let bytes = statement.as_bytes();
    if bytes.len() != 7 || !matches!(bytes[0], b'+' | b'-') {
        return None;
    }
    if !bytes[1..5].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let digit = |i: usize| bytes[i] - b'0';
    let (from_file, from_rank) = (digit(1), digit(2));
    let to = (digit(3), digit(4));
    let kind = piece_kind_from_csa(&statement[5..7])?;
    let from = if (from_file, from_rank) == (0, 0) {
        None
    } else {
        Some((from_file, from_rank))
    };
    Some(CsaMove {
        black: bytes[0] == b'+',
        from,
        to,
        kind,
    })
}

/// A candidate position: the state after `ply` moves of `game`.
struct Snapshot {
    game: usize,
    ply: usize,
    sfen: String,
    in_check: bool,
    key: u64,
    bucket: usize,
}

struct ReplayedGame {
    name: String,
    usi_moves: Vec<Move>,
}

fn bucket_of(ply: usize) -> Option<usize> {
    BUCKETS
        .iter()
        .position(|&(_, lo, hi, _)| (lo..=hi).contains(&ply))
}

/// Replays a game from the initial position, validating every move against
/// `legal_moves()`. Returns the USI move list and per-ply snapshots, or
/// `None` if anything about the record is off.
fn replay(game_index: usize, game: &CsaGame) -> Option<(ReplayedGame, Vec<Snapshot>)> {
    let mut position = Position::startpos();
    let mut partial = PartialPosition::startpos();
    let mut usi_moves = Vec::with_capacity(game.moves.len());
    let mut snapshots = Vec::new();
    for (index, csa) in game.moves.iter().enumerate() {
        let side = position.side_to_move();
        if csa.black != (side == Color::Black) {
            return None;
        }
        let to = Square::new(csa.to.0, csa.to.1)?;
        let mv = match csa.from {
            None => Move::Drop {
                piece: Piece::new(csa.kind, side),
                to,
            },
            Some((file, rank)) => {
                let from = Square::new(file, rank)?;
                let (kind_before, color_before) = position.piece_at(from)?.to_parts();
                if color_before != side {
                    return None;
                }
                let promote = kind_before != csa.kind;
                if promote && kind_before.promote() != Some(csa.kind) {
                    return None;
                }
                Move::Normal { from, to, promote }
            }
        };
        if !position.legal_moves().contains(&mv) {
            return None;
        }
        position.do_move(mv);
        partial.make_move(mv)?;
        usi_moves.push(mv);
        let ply = index + 1;
        if let Some(bucket) = bucket_of(ply) {
            snapshots.push(Snapshot {
                game: game_index,
                ply,
                sfen: partial.to_sfen_owned(),
                in_check: position.in_check(),
                key: position.key(),
                bucket,
            });
        }
    }
    Some((
        ReplayedGame {
            name: game.name.clone(),
            usi_moves,
        },
        snapshots,
    ))
}

/// Adds `snap` to `picked` unless a quota (bucket, per-game) or the dedup
/// set forbids it.
fn try_pick<'a>(
    snap: &'a Snapshot,
    picked: &mut Vec<&'a Snapshot>,
    used_keys: &mut HashSet<u64>,
    per_game: &mut HashMap<usize, usize>,
    bucket_counts: &mut [usize; BUCKETS.len()],
) -> bool {
    let game_count = per_game.entry(snap.game).or_insert(0);
    if bucket_counts[snap.bucket] < BUCKETS[snap.bucket].3
        && *game_count < MAX_PER_GAME
        && used_keys.insert(snap.key)
    {
        *game_count += 1;
        bucket_counts[snap.bucket] += 1;
        picked.push(snap);
        true
    } else {
        false
    }
}

struct Args {
    csa_dir: PathBuf,
    out_dir: PathBuf,
    seed: u64,
}

fn parse_args() -> Args {
    let mut csa_dir = None;
    let mut out_dir = None;
    let mut seed = DEFAULT_SEED;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{arg} needs a value"));
        match arg.as_str() {
            "--csa-dir" => csa_dir = Some(PathBuf::from(value())),
            "--out-dir" => out_dir = Some(PathBuf::from(value())),
            "--seed" => {
                let value = value();
                let parsed = match value.strip_prefix("0x") {
                    Some(hex) => u64::from_str_radix(hex, 16),
                    None => value.parse(),
                };
                seed = parsed.expect("--seed must be a u64");
            }
            _ => panic!("unknown argument {arg}"),
        }
    }
    Args {
        csa_dir: csa_dir.expect("--csa-dir is required"),
        out_dir: out_dir.expect("--out-dir is required"),
        seed,
    }
}

fn main() {
    let args = parse_args();

    let mut files: Vec<PathBuf> = std::fs::read_dir(&args.csa_dir)
        .expect("cannot read --csa-dir")
        .map(|entry| entry.expect("cannot read dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "csa"))
        .collect();
    files.sort();
    eprintln!("{} CSA files in {}", files.len(), args.csa_dir.display());

    // Qualifying games in filename order: the first NUM_SOURCE_GAMES feed
    // position sampling, the first NUM_FIXTURE_GAMES long ones feed the
    // do/undo fixture.
    let mut source_games: Vec<ReplayedGame> = Vec::new();
    let mut snapshots: Vec<Snapshot> = Vec::new();
    let mut skipped_replay = 0;
    for path in &files {
        if source_games.len() >= NUM_SOURCE_GAMES
            && source_games
                .iter()
                .filter(|g| g.usi_moves.len() >= MIN_FIXTURE_GAME_PLIES)
                .count()
                >= NUM_FIXTURE_GAMES
        {
            break;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("csa filename must be unicode")
            .to_owned();
        let text = std::fs::read_to_string(path).expect("cannot read csa file");
        let Some(game) = parse_qualifying_game(&name, &text) else {
            continue;
        };
        let Some((replayed, game_snapshots)) = replay(source_games.len(), &game) else {
            skipped_replay += 1;
            eprintln!("skipping {name}: replay failed");
            continue;
        };
        if source_games.len() < NUM_SOURCE_GAMES {
            snapshots.extend(game_snapshots);
        }
        source_games.push(replayed);
    }
    let fixture_games: Vec<&ReplayedGame> = source_games
        .iter()
        .filter(|g| g.usi_moves.len() >= MIN_FIXTURE_GAME_PLIES)
        .take(NUM_FIXTURE_GAMES)
        .collect();
    assert!(
        source_games.len() >= NUM_SOURCE_GAMES,
        "only {} qualifying games (need {NUM_SOURCE_GAMES}); add more input days",
        source_games.len(),
    );
    assert!(
        fixture_games.len() >= NUM_FIXTURE_GAMES,
        "only {} qualifying games with >= {MIN_FIXTURE_GAME_PLIES} plies",
        fixture_games.len(),
    );
    eprintln!(
        "{} source games ({} candidate positions, {} replay failures)",
        NUM_SOURCE_GAMES,
        snapshots.len(),
        skipped_replay,
    );

    // Sampling. Phase 1 secures the in-check quota, phase 2 fills each
    // bucket; both draw in shuffled order under dedup (zobrist key) and
    // per-game caps, so the result is deterministic in the seed.
    let mut rng = args.seed;
    let mut picked: Vec<&Snapshot> = Vec::new();
    let mut used_keys: HashSet<u64> = HashSet::new();
    let mut per_game: HashMap<usize, usize> = HashMap::new();
    let mut bucket_counts = [0usize; BUCKETS.len()];

    let mut in_check_candidates: Vec<&Snapshot> = snapshots.iter().filter(|s| s.in_check).collect();
    shuffle(&mut in_check_candidates, &mut rng);
    let mut in_check_picked = 0;
    for &snap in &in_check_candidates {
        if in_check_picked >= MIN_IN_CHECK {
            break;
        }
        if try_pick(
            snap,
            &mut picked,
            &mut used_keys,
            &mut per_game,
            &mut bucket_counts,
        ) {
            in_check_picked += 1;
        }
    }
    assert!(
        in_check_picked >= MIN_IN_CHECK,
        "only {in_check_picked} in-check positions available (need {MIN_IN_CHECK})",
    );

    for bucket in 0..BUCKETS.len() {
        let mut candidates: Vec<&Snapshot> =
            snapshots.iter().filter(|s| s.bucket == bucket).collect();
        shuffle(&mut candidates, &mut rng);
        for &snap in &candidates {
            if bucket_counts[bucket] >= BUCKETS[bucket].3 {
                break;
            }
            try_pick(
                snap,
                &mut picked,
                &mut used_keys,
                &mut per_game,
                &mut bucket_counts,
            );
        }
        assert_eq!(
            bucket_counts[bucket], BUCKETS[bucket].3,
            "bucket {} could not be filled",
            BUCKETS[bucket].0,
        );
    }
    picked.sort_by_key(|s| (s.bucket, &source_games[s.game].name, s.ply));
    eprintln!(
        "picked {} positions ({} in check)",
        picked.len(),
        picked.iter().filter(|s| s.in_check).count(),
    );

    // Emission. Headers are functions of inputs and seed only, so
    // regeneration stays byte-identical.
    std::fs::create_dir_all(&args.out_dir).expect("cannot create --out-dir");
    let mut sampled = String::new();
    sampled.push_str(&format!(
        "\
# shunsai movegen fixture: sampled-v1 (frozen; a different set must become sampled-v2)
# source: floodgate CSA records, wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/
# selection: both rates >= {MIN_RATE}, %TORYO/%KACHI end, >= {MIN_GAME_PLIES} plies,
#            first {NUM_SOURCE_GAMES} games in filename order that replay fully legally
# sampling: ply buckets 11-30/31-90/91+ = 12/16/12, >= {MIN_IN_CHECK} in check,
#           dedup by zobrist key, <= {MAX_PER_GAME} per game, splitmix64 seed {seed:#018x}
# generated by: examples/gen_bench_positions.rs (see BENCHMARKS.md; do not edit by hand)
# format: <sfen>\\t# game=<csa-basename> ply=<moves played> phase=<phase> check=<0|1>
",
        seed = args.seed,
    ));
    for snap in &picked {
        sampled.push_str(&format!(
            "{}\t# game={} ply={} phase={} check={}\n",
            snap.sfen,
            source_games[snap.game].name,
            snap.ply,
            BUCKETS[snap.bucket].0,
            snap.in_check as u8,
        ));
    }
    let sampled_path = args.out_dir.join("sampled-v1.sfen");
    std::fs::write(&sampled_path, sampled).expect("cannot write sampled fixture");
    eprintln!("wrote {}", sampled_path.display());

    let mut games = String::new();
    games.push_str(&format!(
        "\
# shunsai do/undo fixture: games-v1 (frozen; a different set must become games-v2)
# source: floodgate CSA records, wdoor.c.u-tokyo.ac.jp/shogi/x/2026/06/01/
# selection: first {NUM_FIXTURE_GAMES} qualifying games with >= {MIN_FIXTURE_GAME_PLIES} plies
#            (same game filter as sampled-v1); every move validated as legal
# generated by: examples/gen_bench_positions.rs (see BENCHMARKS.md; do not edit by hand)
# format: <csa-basename>\\t<space-separated USI moves from the initial position>
",
    ));
    for game in &fixture_games {
        let moves: Vec<String> = game.usi_moves.iter().map(|mv| mv.to_usi_owned()).collect();
        games.push_str(&format!("{}\t{}\n", game.name, moves.join(" ")));
    }
    let games_path = args.out_dir.join("games-v1.usi");
    std::fs::write(&games_path, games).expect("cannot write games fixture");
    eprintln!("wrote {}", games_path.display());
}
