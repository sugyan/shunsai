#!/usr/bin/env python3
"""Record a criterion run as a committed benchmark-history entry.

Workflow (see BENCHMARKS.md):

    cargo bench --features _bench-internals
    python3 scripts/bench_snapshot.py --note "why this run is worth keeping"

Reads every ``target/criterion/**/new/{benchmark,estimates}.json``, writes
``benches/history/YYYY-MM-DD-<shortrev>.json`` (refuses to overwrite without
``--force``), and regenerates the history table between the
``BENCH_HISTORY`` markers in BENCHMARKS.md from all history files.

Python stdlib only.
"""

import argparse
import datetime
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CRITERION_DIR = REPO / "target" / "criterion"
HISTORY_DIR = REPO / "benches" / "history"
POSITIONS_DIR = REPO / "benches" / "positions"
BENCHMARKS_MD = REPO / "BENCHMARKS.md"
MARKER_BEGIN = "<!-- BENCH_HISTORY_BEGIN -->"
MARKER_END = "<!-- BENCH_HISTORY_END -->"

def _mnps(r: dict) -> str:
    return f"{r['elements_per_sec'] / 1e6:.1f}"


def _us_per_element(r: dict) -> str:
    return f"{r['per_element_ns'] / 1e3:.2f}"


# ``(header, full_id, formatter)`` for the headline columns of the table.
# Formatters get the per-result dict; missing ids render "-", so an entry
# recorded before an id existed simply leaves that column blank.
#
# Two ids per position, one per leaf convention: `-cb`, the path this crate
# recommends and the one haitaka's bulk path is comparable to, and `-mat-wi`,
# the materializing path every other engine in the cross-engine table drives.
# Never read one against the other (see "Fairness" in BENCHMARKS.md).
#
# This is a summary, not the record: `benches/history/*.json` holds every id
# each run measured, so a column dropped here loses nothing and can be restored
# by adding it back. What earns a column is being readable for change across
# rows — which is why the allocating `Vec` ids are absent, BENCHMARKS.md having
# established that they drift across days and that their `-cb` twins are the
# tell.
HEADLINE_COLUMNS = [
    ("perft startpos-d4 -cb", "perft/startpos-cb/4", _mnps),
    ("perft startpos-d4 -mat-wi", "perft/startpos-mat-wi/4", _mnps),
    ("perft matsuri-d3 -cb", "perft/matsuri-cb/3", _mnps),
    ("perft matsuri-d3 -mat-wi", "perft/matsuri-mat-wi/3", _mnps),
    ("movegen sampled-v1 -cb", "movegen/sampled-v1-cb", _us_per_element),
    ("do_undo ns/pair", "do_undo/games-v1", lambda r: f"{r['per_element_ns']:.1f}"),
]


# The table is one line per run, so a note that runs to paragraphs makes the
# whole thing unreadable. The snapshot keeps the note whole; only this cell is
# cut. Why a figure is what it is belongs in DECISIONS.md.
NOTE_CELL_CHARS = 60


def _note_cell(note: str) -> str:
    note = " ".join(note.split()).replace("|", "\\|")
    if len(note) <= NOTE_CELL_CHARS:
        return note
    cut = note[:NOTE_CELL_CHARS]
    head, sep, _ = cut.rpartition(" ")
    return (head if sep and len(head) > NOTE_CELL_CHARS // 2 else cut) + " …"


def run(*cmd: str) -> str:
    return subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=REPO).stdout.strip()


def collect_results() -> dict:
    if not CRITERION_DIR.is_dir():
        sys.exit(f"error: {CRITERION_DIR} not found — run `cargo bench --features _bench-internals` first")
    results = {}
    for benchmark_json in sorted(CRITERION_DIR.rglob("new/benchmark.json")):
        benchmark = json.loads(benchmark_json.read_text())
        estimates = json.loads((benchmark_json.parent / "estimates.json").read_text())
        entry = {
            "mean_ns": estimates["mean"]["point_estimate"],
            "mean_stderr_ns": estimates["mean"]["standard_error"],
            "median_ns": estimates["median"]["point_estimate"],
            "std_dev_ns": estimates["std_dev"]["point_estimate"],
            "throughput": benchmark.get("throughput"),
        }
        throughput = entry["throughput"]
        if isinstance(throughput, dict) and "Elements" in throughput:
            elements = throughput["Elements"]
            entry["per_element_ns"] = entry["mean_ns"] / elements
            entry["elements_per_sec"] = elements / (entry["mean_ns"] * 1e-9)
        results[benchmark["full_id"]] = entry
    if not results:
        sys.exit(f"error: no results under {CRITERION_DIR}")
    return results


def cpu_model() -> str:
    if platform.system() == "Darwin":
        try:
            return run("sysctl", "-n", "machdep.cpu.brand_string")
        except (OSError, subprocess.CalledProcessError):
            pass
    return platform.processor() or platform.machine()


def criterion_version() -> str:
    lock = REPO / "Cargo.lock"
    if lock.is_file():
        match = re.search(r'name = "criterion"\nversion = "([^"]+)"', lock.read_text())
        if match:
            return match.group(1)
    return "unknown"


def position_sets() -> list[str]:
    sets = []
    for path in sorted(POSITIONS_DIR.iterdir()):
        # Stray local entries (.DS_Store, directories, binary or empty
        # files) must not block snapshot creation.
        if not path.is_file() or path.name.startswith("."):
            continue
        try:
            first_line = path.read_text().splitlines()[0]
        except (UnicodeDecodeError, IndexError):
            continue
        match = re.search(r"fixture: (\S+)", first_line)
        sets.append(match.group(1) if match else path.name)
    return sets


def collect_meta(note: str) -> dict:
    dirty = run("git", "status", "--porcelain") != ""
    if dirty:
        print("WARNING: working tree is dirty — the recorded rev does not reproduce this run", file=sys.stderr)
    return {
        "date": datetime.date.today().isoformat(),
        "git": {
            "rev": run("git", "rev-parse", "HEAD"),
            "short_rev": run("git", "rev-parse", "--short", "HEAD"),
            "branch": run("git", "branch", "--show-current"),
            "dirty": dirty,
            # Orders the history table. Several entries a day are normal
            # during an optimization run, and sorting those by revision
            # hash would scramble them.
            "committed": run("git", "show", "-s", "--format=%cI", "HEAD"),
        },
        "rustc": run("rustc", "-V"),
        "criterion": criterion_version(),
        "cpu": cpu_model(),
        "os": platform.platform(),
        "arch": platform.machine(),
        "position_sets": position_sets(),
        "note": note,
    }


def markdown_table() -> str:
    rows = []
    for path in sorted(HISTORY_DIR.glob("*.json")):
        snapshot = json.loads(path.read_text())
        meta, results = snapshot["meta"], snapshot["results"]
        cells = [meta["date"], meta["git"]["short_rev"]]
        for _, full_id, fmt in HEADLINE_COLUMNS:
            result = results.get(full_id)
            cells.append(fmt(result) if result else "-")
        cells.append(_note_cell(meta.get("note", "")))
        # Entries recorded before `committed` existed fall back to the date.
        order = meta["git"].get("committed") or meta["date"]
        rows.append((order, path.name, "| " + " | ".join(cells) + " |"))
    rows.sort(key=lambda row: (row[0], row[1]))
    header = ["date", "rev"] + [name for name, _, _ in HEADLINE_COLUMNS] + ["note"]
    lines = [
        "| " + " | ".join(header) + " |",
        "|" + "|".join("---" for _ in header) + "|",
    ] + [row[2] for row in rows]
    return "\n".join(lines)


def update_benchmarks_md() -> None:
    text = BENCHMARKS_MD.read_text()
    begin = text.index(MARKER_BEGIN) + len(MARKER_BEGIN)
    end = text.index(MARKER_END)
    BENCHMARKS_MD.write_text(text[:begin] + "\n" + markdown_table() + "\n" + text[end:])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--note", default="", help="free-text note shown in the history table")
    parser.add_argument("--force", action="store_true", help="overwrite an existing entry for today+rev")
    args = parser.parse_args()

    meta = collect_meta(args.note)
    results = collect_results()
    snapshot = {"meta": meta, "results": results}

    HISTORY_DIR.mkdir(parents=True, exist_ok=True)
    out = HISTORY_DIR / f"{meta['date']}-{meta['git']['short_rev']}.json"
    if out.exists() and not args.force:
        sys.exit(f"error: {out} exists (use --force to overwrite)")
    out.write_text(json.dumps(snapshot, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {out.relative_to(REPO)} ({len(results)} results)")

    update_benchmarks_md()
    print(f"updated {BENCHMARKS_MD.name} history table")


if __name__ == "__main__":
    main()
