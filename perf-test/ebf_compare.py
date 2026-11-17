#!/usr/bin/env python3
r"""
Effective Branching Factor (EBF) head-to-head comparison for two UCI engines.

Usage:
    python ebf_compare.py \
        --engine-a "target/release/knightbyte" \
        --engine-b "/path/to/baseline/engine" \
        --name-a KnightByte --name-b baseline \
        --depth 18 --threads 1 --hash 256

Runs BOTH engines on the same positions, captures the deepest completed info line,
and reports depth / seldepth / nodes and the effective branching factor side by side:

    EBF = nodes ** (1 / depth)          depth ~= ln(nodes) / ln(EBF)

At equal node budgets a lower EBF is exactly what buys more depth. Use `--depth`
for machine-independent comparisons (same nodes on every host) and `--movetime`
for a quick time-to-depth check on one host. `--positions-file` loads a shared
FEN set; each line is either a FEN or `name | FEN`.

Notes:
    --engine-a / --engine-b each take a full command line (quote the whole thing).
    Standard `Hash` / `Threads` options are sent to both; `Own Book` is disabled if
    present (unknown options are harmlessly ignored by most engines).
    --json-out writes a machine-readable result for CI and baseline storage.
"""

import argparse
import json
import math
import os
import platform
import shlex
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(SCRIPT_DIR)

# Same fixed, reference-verified benchmark positions as perf_test.py.
POSITIONS = [
    ("startpos",   "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
    ("kiwipete",   "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"),
    ("middlegame", "r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP2BPPP/R1BQ1RK1 w - - 0 9"),
    ("endgame",    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
]


def load_positions_file(path):
    """Load a shared FEN set.

    One position per line: either a bare FEN or ``name | FEN``. Blank lines and
    lines starting with ``#`` are ignored. Keeping this in a file lets the cheap
    gates and the time-to-depth suite use exactly the same positions.
    """
    positions = []
    with open(path, "r", encoding="utf-8") as handle:
        for lineno, raw in enumerate(handle, 1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            if "|" in line:
                name, _, fen = line.partition("|")
                name, fen = name.strip(), fen.strip()
            else:
                name, fen = f"pos{len(positions) + 1}", line
            if not fen:
                raise ValueError(f"{path}:{lineno}: empty FEN")
            positions.append((name, fen))
    if not positions:
        raise ValueError(f"{path}: no positions found")
    return positions


def parse_args():
    p = argparse.ArgumentParser(description="Head-to-head effective branching factor comparison")
    _EXE = ".exe" if os.name == "nt" else ""
    DEFAULT_ENGINE_A = (
        os.environ.get("KNIGHTBYTE_ENGINE", "").strip()
        or os.environ.get("KNIGHTBYTE_BASELINE_ENGINE", "").strip()
        or os.path.join(REPO_ROOT, "target", "release", "knightbyte" + _EXE)
    )

    p.add_argument("--engine-a", default=DEFAULT_ENGINE_A,
                   help="engine A command, quoted (default: $KNIGHTBYTE_ENGINE, "
                        "$KNIGHTBYTE_BASELINE_ENGINE, or target/release/knightbyte)")
    p.add_argument("--engine-b", required=True,
                   help="engine B command, quoted (baseline engine)")
    p.add_argument("--name-a", default="KnightByte", help="label for engine A")
    p.add_argument("--name-b", default="reference", help="label for engine B")
    p.add_argument("--hash", type=int, default=256, help="hash size in MB (default: 256)")
    p.add_argument("--threads", type=int, default=1, help="threads (default: 1)")
    p.add_argument("--movetime", type=int, default=5000, help="ms per position (default: 5000)")
    p.add_argument("--depth", type=str, default=None, metavar="N | d1,d2,...",
                   help="search to fixed depth instead of fixed time. Node counts and EBF "
                        "are then machine-independent, which is what a cross-host baseline "
                        "needs. One depth for all positions, or one per position in order.")
    p.add_argument("--positions", type=int, default=0,
                   help="how many built-in positions (default: all)")
    p.add_argument("--positions-file", default=None,
                   help="load positions from a file instead of the built-in set "
                        "(one FEN or 'name | FEN' per line)")
    p.add_argument("--setoption-a", action="append", default=[], metavar="NAME=VALUE",
                   help="extra UCI setoption for engine A (repeatable), "
                        "e.g. --setoption-a \"UseLmrV2=false\"")
    p.add_argument("--setoption-b", action="append", default=[], metavar="NAME=VALUE",
                   help="extra UCI setoption for engine B (repeatable)")
    p.add_argument("--json-out", default=None, metavar="PATH",
                   help="write a machine-readable result (metadata + rows) to PATH")
    return p.parse_args()


class EngineSession:
    """One UCI engine process; captures the deepest completed info line per search."""

    def __init__(self, cmd, hash_mb, threads, extra_options=None):
        self.cmd = cmd
        env = dict(os.environ)
        if not env.get("LANG"):
            env["LANG"] = "en_US.UTF-8"
        if not env.get("LC_ALL"):
            env["LC_ALL"] = "en_US.UTF-8"

        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1, env=env)
        self._send("uci")
        self._wait_for("uciok", 20)
        self._send(f"setoption name Hash value {hash_mb}")
        self._send(f"setoption name Threads value {threads}")
        self._send("setoption name Own Book value false")  # ignored if unknown
        for name, value in (extra_options or {}).items():
            self._send(f"setoption name {name} value {value}")
        self._send("isready")
        self._wait_for("readyok", 20)

    def _send(self, cmd):
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.flush()

    def _wait_for(self, token, timeout_s):
        deadline = time.time() + timeout_s
        while time.time() < deadline:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("engine terminated during handshake")
            if line.strip() == token:
                return
        raise TimeoutError(f"engine did not send '{token}' within {timeout_s}s")

    def search(self, fen, movetime_ms, timeout_margin_s=30, fixed_depth=None):
        """Returns (depth, seldepth, nodes) from the deepest completed info line."""
        self._send("ucinewgame")
        self._send("isready")
        self._wait_for("readyok", 20)
        self._send(f"position fen {fen}")
        if fixed_depth is not None:
            self._send(f"go depth {fixed_depth}")
            budget_s = 120.0
        else:
            self._send(f"go movetime {movetime_ms}")
            budget_s = movetime_ms / 1000.0

        best = {"depth": 0, "seldepth": 0, "nodes": 0}
        deadline = time.time() + budget_s + timeout_margin_s
        while time.time() < deadline:
            line = self.proc.stdout.readline()
            if not line:
                break
            line = line.strip()
            if line.startswith("bestmove"):
                break
            if not line.startswith("info"):
                continue
            parts = line.split()
            info = {}
            for i, tok in enumerate(parts):
                if tok in ("depth", "seldepth", "nodes") and i + 1 < len(parts):
                    try:
                        info[tok] = int(parts[i + 1])
                    except ValueError:
                        pass
            if "depth" in info and "nodes" in info and info["depth"] >= best["depth"]:
                best["depth"] = info["depth"]
                best["nodes"] = info["nodes"]
                best["seldepth"] = info.get("seldepth", best["seldepth"])
        if best["depth"] == 0:
            raise RuntimeError(f"no info lines for: {fen}")
        return best["depth"], best["seldepth"], best["nodes"]

    def quit(self):
        try:
            self._send("quit")
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def ebf(nodes, depth):
    if depth <= 0 or nodes <= 0:
        return float("nan")
    return nodes ** (1.0 / depth)


def split_command(cmd):
    r"""Split a command line into argv, using the RULES OF THIS PLATFORM.

    `shlex.split()` defaults to posix=True, where a backslash is an ESCAPE
    character. On Windows that turns the natural

        target\\release\\engine.exe

    into `targetreleaseengine.exe`, so the tool reports the engine as
    not found even though the file is right there. Forward slashes hid this;
    that is a symptom, not a fix.

    On Windows use the native CommandLineToArgvW rules (posix=False), then drop
    the quotes that mode keeps, so a quoted path with spaces stays one argument.
    """
    if os.name == "nt":
        parts = shlex.split(cmd, posix=False)
        return [p[1:-1] if len(p) >= 2 and p[0] == p[-1] == '"' else p for p in parts]
    return shlex.split(cmd)


def resolve(cmd_str):
    cmd = split_command(cmd_str)
    ok = cmd and (os.path.sep in cmd[0] and os.path.exists(cmd[0]) or shutil.which(cmd[0]))
    if not ok:
        print(f"ERROR: engine executable not found: {cmd[0] if cmd else cmd_str}")
        sys.exit(1)
    return cmd


def run_engine(name, cmd, args, positions, extra_options, depths):
    print(f"  running {name} ...", flush=True)
    session = EngineSession(cmd, args.hash, args.threads, extra_options)
    rows = []
    try:
        for idx, (pos_name, fen) in enumerate(positions):
            fixed_depth = depths[idx % len(depths)] if depths else None
            depth, seldepth, nodes = session.search(
                fen, args.movetime, fixed_depth=fixed_depth)
            rows.append((pos_name, depth, seldepth, nodes, ebf(nodes, depth)))
    finally:
        session.quit()
    return rows


def main():
    args = parse_args()
    cmd_a = resolve(args.engine_a)
    cmd_b = resolve(args.engine_b)
    if args.positions_file:
        positions = load_positions_file(args.positions_file)
    else:
        positions = POSITIONS[: args.positions] if args.positions else POSITIONS

    depths = None
    if args.depth:
        try:
            depths = [int(x) for x in str(args.depth).split(",")]
        except ValueError:
            print(f"ERROR: --depth must be N or a comma list d1,d2,..., got: {args.depth}")
            sys.exit(1)
        if len(depths) > 1 and len(depths) != len(positions):
            print(f"ERROR: {len(depths)} depths given for {len(positions)} positions. "
                  "Give one depth, or one per position in order.")
            sys.exit(1)

    print("=" * 88)
    print("Effective Branching Factor (EBF) head-to-head")
    print("=" * 88)
    print(f"  {args.name_a:<10}: {cmd_a}")
    print(f"  {args.name_b:<10}: {cmd_b}")
    if depths:
        shown = (f"{depths[0]} (all positions)" if len(depths) == 1
                 else ", ".join(f"{n}={d}" for (n, _), d in zip(positions, depths)))
        print(f"  depth     : FIXED -> {shown} (machine-independent nodes/EBF)")
    else:
        print(f"  movetime  : {args.movetime} ms (machine-dependent)")
    print(f"  threads   : {args.threads}   hash: {args.hash} MB")
    print(f"  positions : {[n for n, _ in positions]}")

    def parse_options(entries, flag):
        opts = {}
        for entry in entries:
            if "=" not in entry:
                print(f"ERROR: {flag} must be NAME=VALUE, got: {entry}")
                sys.exit(1)
            name, _, value = entry.partition("=")
            opts[name.strip()] = value.strip()
        return opts

    extra_a = parse_options(args.setoption_a, "--setoption-a")
    extra_b = parse_options(args.setoption_b, "--setoption-b")
    if extra_a:
        print(f"  setoption-a: {', '.join(args.setoption_a)}")
    if extra_b:
        print(f"  setoption-b: {', '.join(args.setoption_b)}")
    print()

    rows_a = run_engine(args.name_a, cmd_a, args, positions, extra_a, depths)
    rows_b = run_engine(args.name_b, cmd_b, args, positions, extra_b, depths)
    print()

    hdr = (f"{'position':<11} | {args.name_a:>26} | {args.name_b:>26}")
    print(hdr)
    print(f"{'':<11} | {'depth  seld     nodes   EBF':>26} | {'depth  seld     nodes   EBF':>26}")
    print("-" * len(hdr))
    for (na, da, sa, nna, ea), (nb, db, sb, nnb, eb) in zip(rows_a, rows_b):
        print(f"{na:<11} | {da:>5} {sa:>5} {nna:>9,} {ea:>5.2f} | "
              f"{db:>5} {sb:>5} {nnb:>9,} {eb:>5.2f}")

    def avg(rows, idx):
        vals = [r[idx] for r in rows if not (isinstance(r[idx], float) and math.isnan(r[idx]))]
        return sum(vals) / len(vals) if vals else float("nan")

    print("-" * len(hdr))
    print(f"{'AVG':<11} | {avg(rows_a,1):>5.1f} {avg(rows_a,2):>5.1f} "
          f"{'':>9} {avg(rows_a,4):>5.2f} | "
          f"{avg(rows_b,1):>5.1f} {avg(rows_b,2):>5.1f} {'':>9} {avg(rows_b,4):>5.2f}")
    print("=" * 88)
    print(f"  {args.name_a} avg EBF = {avg(rows_a,4):.2f}   |   "
          f"{args.name_b} avg EBF = {avg(rows_b,4):.2f}")
    print("  Lower EBF -> more depth at the same node budget. Closing the EBF gap is the goal.")
    print("=" * 88)

    if args.json_out:
        payload = {
            "tool": "ebf_compare.py",
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "processor": platform.processor(),
            },
            "engine_a": {"name": args.name_a, "command": cmd_a},
            "engine_b": {"name": args.name_b, "command": cmd_b},
            "settings": {
                "hash_mb": args.hash,
                "threads": args.threads,
                "movetime_ms": None if depths else args.movetime,
                "depths": depths,
                "positions": [name for name, _ in positions],
                "setoption_a": extra_a,
                "setoption_b": extra_b,
            },
            "rows": [
                {
                    "position": row_a[0],
                    "a": {
                        "depth": row_a[1],
                        "seldepth": row_a[2],
                        "nodes": row_a[3],
                        "ebf": row_a[4],
                    },
                    "b": {
                        "depth": row_b[1],
                        "seldepth": row_b[2],
                        "nodes": row_b[3],
                        "ebf": row_b[4],
                    },
                }
                for row_a, row_b in zip(rows_a, rows_b)
            ],
            "averages": {
                "a": {
                    "depth": avg(rows_a, 1),
                    "seldepth": avg(rows_a, 2),
                    "ebf": avg(rows_a, 4),
                },
                "b": {
                    "depth": avg(rows_b, 1),
                    "seldepth": avg(rows_b, 2),
                    "ebf": avg(rows_b, 4),
                },
            },
        }
        with open(args.json_out, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        print(f"  wrote {args.json_out}")


if __name__ == "__main__":
    main()
