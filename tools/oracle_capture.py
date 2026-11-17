#!/usr/bin/env python3
"""Capture machine-readable golden data from a UCI reference engine.

The script is intentionally engine-agnostic: pass the local reference binary
with ``--engine``. It records:

* deterministic benchmark totals for depths 12, 16 and 18;
* perft counts for the standard test positions;
* raw network evaluations for the shared FEN list;
* the UCI handshake and a few lifecycle transcripts.

The output is derived data only; the reference source tree and binary are not
copied into the project.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# Standard perft positions. Depths are chosen to keep the capture quick while
# still exercising castling, en passant, promotions, checks and pins.
PERFT_CASES = [
    ("startpos", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 4),
    ("kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 3),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 4),
    ("position4", "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 3),
    ("position5", "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 3),
]

# Shared FEN list for the network evaluation parity tests. It varies side to
# move, king square/region, mirror state and material count.
NNUE_FENS = [
    ("startpos", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
    ("kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    ("position4", "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"),
    ("position5", "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"),
    ("position6", "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10"),
    ("kiwipete_black", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 0 1"),
    ("kings_deep", "4k3/8/8/8/8/8/4P3/4K3 b - - 0 1"),
    ("king_a1_white", "8/8/8/8/8/8/8/K6k w - - 0 1"),
    ("king_a1_black", "8/8/8/8/8/8/8/K6k b - - 0 1"),
    ("king_a8_black", "K6k/8/8/8/8/8/8/8 b - - 0 1"),
    ("castling_rook", "4k3/8/8/8/8/8/8/4K2R w K - 0 1"),
    ("castling_rook_no_rights", "8/8/8/8/8/8/6k1/4K2R w K - 0 1"),
    ("uho_opening", "r2q1rk1/pp1bppbp/2np1np1/2p5/P1B1PB2/3P1N2/1PP2PPP/RN1Q1RK1 w - - 1 9"),
    ("sharp_castled", "r1bq1rk1/p1pnbppp/1p3n2/3p2B1/3P4/2N1PN2/PPQ2PPP/R3KB1R w KQ - 0 9"),
]


def run(engine: Path, commands: str, timeout: int = 180) -> str:
    """Runs one command batch and returns stdout."""
    result = subprocess.run(
        [str(engine)],
        input=commands,
        text=True,
        capture_output=True,
        timeout=timeout,
        cwd=str(engine.parent),
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"reference engine failed ({result.returncode}): {result.stderr.strip()}"
        )
    return result.stdout


def _number(text: str) -> int:
    """Normalizes a localized integer such as ``3.169.859`` or ``3,169,859``."""
    return int(text.replace(".", "").replace(",", ""))


def capture_bench(engine: Path, depths: list[int]) -> dict:
    """Captures the deterministic benchmark totals for each requested depth."""
    captures = {}
    for depth in depths:
        output = run(engine, f"bench {depth} 1\nquit\n")
        nodes = re.search(r"Total Nodes:\s*([\d.,]+)", output)
        usec = re.search(r"Total Time:\s*([\d.,]+)\s*ms", output)
        nps = re.search(r"Average NPS:\s*([\d.,]+)", output)
        if not nodes:
            raise RuntimeError(f"bench {depth}: no total node count found")
        captures[str(depth)] = {
            "total_nodes": _number(nodes.group(1)),
            "total_time_ms": _number(usec.group(1)) if usec else None,
            "average_nps": _number(nps.group(1)) if nps else None,
        }
    return captures


def capture_perft(engine: Path) -> list[dict]:
    """Captures perft counts for the standard positions."""
    rows = []
    for name, fen, depth in PERFT_CASES:
        output = run(engine, f"position fen {fen}\nperft {depth}\nquit\n")
        match = re.search(r"Nodes:\s*([\d,]+)", output)
        if not match:
            raise RuntimeError(f"perft {name} depth {depth}: no node count found")
        rows.append(
            {
                "name": name,
                "fen": fen,
                "depth": depth,
                "nodes": _number(match.group(1)),
            }
        )
    return rows


def capture_nnue(engine: Path) -> list[dict]:
    """Captures the raw network evaluation for each shared FEN."""
    rows = []
    for name, fen in NNUE_FENS:
        output = run(engine, f"position fen {fen}\nnnueeval\nquit\n")
        match = re.search(r"nnueeval\s+(-?\d+)", output)
        if not match:
            raise RuntimeError(f"nnueeval {name}: no evaluation found")
        rows.append({"name": name, "fen": fen, "cp": int(match.group(1))})
    return rows


def parse_option(line: str) -> dict:
    """Parses one UCI ``option name ...`` line into a structured record."""
    body = line[len("option name ") :]
    name, _, rest = body.partition(" type ")
    parts = rest.split()
    option_type = parts[0] if parts else ""
    default = None
    minimum = None
    maximum = None

    if "default" in parts:
        index = parts.index("default")
        if index + 1 < len(parts) and parts[index + 1] not in ("min", "max"):
            default = parts[index + 1]
        else:
            default = ""
    if "min" in parts:
        minimum = int(parts[parts.index("min") + 1])
    if "max" in parts:
        maximum = int(parts[parts.index("max") + 1])

    return {
        "name": name,
        "type": option_type,
        "default": default,
        "min": minimum,
        "max": maximum,
    }


def capture_uci(engine: Path) -> dict:
    """Captures the sanitized handshake, option catalog and lifecycle tokens."""
    handshake = run(engine, "uci\nisready\nucinewgame\nquit\n")
    unknown = run(engine, "this-command-does-not-exist\nquit\n")
    options = [
        parse_option(line.strip())
        for line in handshake.splitlines()
        if line.strip().startswith("option name ")
    ]
    responses = [
        line.strip()
        for line in handshake.splitlines()
        if line.strip() in ("uciok", "readyok")
    ]
    unknown_response = next(
        (line.strip() for line in unknown.splitlines() if "Unknown command" in line),
        "",
    )
    return {
        "options": options,
        "responses": responses,
        "unknown": unknown_response,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True, help="reference UCI binary")
    parser.add_argument(
        "--out-dir",
        default="tests/golden",
        help="directory for the derived JSON files (default: tests/golden)",
    )
    parser.add_argument(
        "--bench-depths",
        default="12,16,18",
        help="comma-separated benchmark depths (default: 12,16,18)",
    )
    args = parser.parse_args()

    engine = Path(args.engine).resolve()
    if not engine.is_file():
        print(f"error: engine not found: {engine}", file=sys.stderr)
        return 1

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    depths = [int(value) for value in args.bench_depths.split(",") if value]

    data = {
        "bench": capture_bench(engine, depths),
        "perft": capture_perft(engine),
        "nnue": capture_nnue(engine),
        "uci": capture_uci(engine),
    }

    for name, payload in data.items():
        path = out_dir / f"{name}.json"
        with path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
