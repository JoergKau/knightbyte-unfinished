#!/usr/bin/env python3
r"""
================================================================================
UCI engine performance + search-quality benchmark  (works with any UCI engine)
================================================================================

WHAT THIS TOOL IS FOR
---------------------
This is a set of FAST, DETERMINISTIC "cheap gates" used to triage a search-tuning
change BEFORE spending hours on a self-play (SPRT) match. It runs one engine on a
small set of fixed positions at a fixed time-per-move and reports metrics that are
sensitive to pruning / reduction / move-ordering changes.

THE GOLDEN RULE (read this before trusting any number below):
    These gates detect BREAKAGE, not WEAKNESS.
    - A gate that trips tells you a value is TOO AGGRESSIVE (the search visibly
      broke: a known tactic is missed, depth collapses, the score thrashes).
    - Passing all gates does NOT mean the change is stronger. A change can pass
      every gate here and still lose Elo in quiet positions no fixed test exercises.
    Therefore: use these gates to REJECT bad values cheaply and to RANK the
    survivors. Confirm strength with SPRT (self-play) only, and SPRT a value
    comfortably BELOW the point where these gates start to trip -- never the edge.

The decision table is deliberately conservative: a gate failure rejects a change;
a pass only ranks it. Self-play/SPRT remains the strength arbiter.

--------------------------------------------------------------------------------
THE FIVE CHEAP GATES (why each one matters for pruning/reduction changes)
--------------------------------------------------------------------------------

1. NODE COUNT at fixed movetime  [most sensitive proxy]
   For a pruning change, node count moves BEFORE depth does. At equal time:
     - fewer nodes + tactics still solved  -> pruning harder at no cost (good)
     - more nodes                          -> you triggered extra re-searches (bad)
   This is the earliest, loudest cheap signal that a pruning change is helping
   or backfiring.

2. EFFECTIVE BRANCHING FACTOR (EBF) per position
     EBF = nodes ** (1 / depth)
   A lower EBF at equal depth is exactly what buys more depth per unit time.
   A change that lowers EBF WITHOUT breaking the tactical gate (#5) is the best
   cheap "worth an SPRT" signal available. Watch it PER POSITION: a change that
   lowers EBF in quiet positions but RAISES it in tactical ones is over-pruning
   the quiet ones (a red flag even if the average looks fine).

3. SELECTIVE MARGIN  =  seldepth - depth
   How much deeper the forcing lines (checks, recaptures, singular replies) are
   explored than the nominal completed depth. A reduction change that SHRINKS
   this margin is pulling the search away from forcing lines -- a warning sign
   even when nominal depth holds.

4. SCORE / BEST-MOVE STABILITY over the final iterations
   Captures the (depth, score, best-move) stream and reports whether the score
   and the best move were stable across the last few completed iterations. Late
   oscillation means the pruning is now discarding relevant moves and the search
   is thrashing -- invisible to a WAC solve-count, visible here.

5. TACTICAL MINI-SUITE  (a hard, binary pass/fail gate)  [run with --tactics]
   ~20 curated positions with a KNOWN best move, chosen for the line types that
   pruning changes tend to break (quiet sacrifices, zugzwang, deep single-reply
   forcing lines). Each is searched at a fixed movetime and asserted to still
   find its known move. Unlike WAC's aggregate solve-count (a weak signal), each
   position is binary, so a SINGLE regression is loud. This is the fastest,
   most targeted "did I break tactics?" gate -- run it on every change.

--------------------------------------------------------------------------------
USAGE
--------------------------------------------------------------------------------
Scaling / quality benchmark (default: threads 1,4,8; 5 s/position):
    python perf_test.py --engine "target/release/knightbyte"

Single-thread quality gate only (fastest; the usual per-change check):
    python perf_test.py --engine "target/release/knightbyte" --threads 1

Machine-independent node/EBF gate (fixed depth):
    python perf_test.py --engine "target/release/knightbyte" --depth 16 --threads 1

Tactical mini-suite hard gate (binary pass/fail):
    python perf_test.py --engine "target/release/knightbyte" --tactics

Everything, one shot (scaling + quality metrics + tactical gate):
    python perf_test.py --engine "target/release/knightbyte" --tactics

A/B a search lever (repeatable):
    python perf_test.py --engine "..." --setoption "UseLmrV2=true"

Compare against a baseline engine (any UCI engine works):
    python perf_test.py --engine "/path/to/baseline/engine" --depth 16 --threads 1

--------------------------------------------------------------------------------
REUSE IN OTHER PROJECTS
--------------------------------------------------------------------------------
Nothing here is engine-specific except the default engine path and the built-in
FENs. It only speaks standard UCI (uci/isready/ucinewgame/position/go movetime or
go depth) and parses the standard `info ... depth/seldepth/nodes/nps/time/score/pv`
fields. Point --engine at any UCI engine. Swap the POSITIONS / TACTICS lists, or
use --positions-file, for your own suite.
================================================================================
"""

import argparse
import hashlib
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


def file_sha256(path):
    """SHA-256 of a file, or None if it is absent."""
    if not path or not os.path.exists(path):
        return None
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def net_sha256():
    """Hash the embedded production net when the standard repo layout is used."""
    return file_sha256(os.path.join(SCRIPT_DIR, "..", "net", "jokausch.nnue"))

# ------------------------------------------------------------------------------
# Benchmark positions for the SCALING + QUALITY metrics (gates 1-4).
# A small, fixed, reference-verified set spanning opening / tactical-middlegame /
# quiet-middlegame / endgame, so the metrics reflect a mix of position types.
# ------------------------------------------------------------------------------
POSITIONS = [
    ("startpos",   "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
    ("kiwipete",   "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"),
    ("middlegame", "r1bq1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP2BPPP/R1BQ1RK1 w - - 0 9"),
    ("endgame",    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
]

# ------------------------------------------------------------------------------
# TACTICAL MINI-SUITE (gate 5).  Each entry: (id, fen, [accepted_best_moves_uci]).
#
# These are chosen to STRESS the line-types that aggressive pruning / reduction
# tends to break, so they act as an early-warning tripwire:
#   - forced mates (short, must be found)                 -> pruning must not cut the mate line
#   - a hanging piece / clear material win                -> ordering + SEE must surface it
#   - a defensive-only save                               -> search must not prune the one save
#
# Each position has a UNIQUE, engine-verifiable best move (or a small set of
# equally-winning moves). The gate is BINARY per position: the engine's bestmove
# at a fixed movetime must be in the accepted set.
#
# IMPORTANT: keep every entry independently verified (e.g. against a reference engine at
# high depth) before adding it. A wrong "known best move" here would reject GOOD
# changes -- worse than having no gate. Extend this list over time as you find
# positions that specific pruning changes regress on (that is the most valuable
# kind of entry: a real regression captured as a permanent test).
# ------------------------------------------------------------------------------
TACTICS = [
    # Every entry below was validated with a reference engine (MultiPV=2, 3 s): the listed
    # move is the UNIQUE best with a large gap to the 2nd-best move, so a healthy
    # search must return it. Positions with several near-equal good moves were
    # deliberately EXCLUDED (e.g. K+2R mates have many equivalent solutions, which
    # makes them useless as a binary gate).
    #
    # id        best   why it's a good tripwire                         SF verdict
    # -------------------------------------------------------------------------------
    ("wac001", "2rr3k/pp3pp1/1nnqbN1p/3pN3/2pP4/2P3Q1/PPB4P/R4RK1 w - - 0 1", ["g3g6"]),
    #   Qg6 = forced mate in 2 (a quiet queen move into the mate net; aggressive
    #   pruning that cuts quiet moves can miss it). ref: mate 2 vs +512 for 2nd.
    ("wac004", "5rk1/1ppb3p/p1pb4/6q1/3P1p1r/2P1R2P/PP1BQ1P1/5RKN w - - 0 1", ["e3g3"]),
    #   Rg3 = the only good move; SF +405 vs -170 for 2nd (a huge gap). A defensive
    #   resource; over-pruning tends to miss the single saving/winning line.
    ("wac005", "r1bq2rk/pp3pbp/2p1p1pQ/7P/3P4/2PB1N2/PP3PP1/2KR3R w - - 0 1", ["h6h7"]),
    #   Qxh7+ = forced mate in 2 (a sacrifice into mate; the kind of forcing line a
    #   too-shallow reduced search fails to prove). ref: mate 2.
    ("hang_q", "k7/8/8/3q4/8/8/3R4/K7 w - - 0 1",                             ["d2d5"]),
    #   Rxd5 wins the hanging queen (ref: winning/mating vs -423 for 2nd). Basic
    #   material tactic; ordering + SEE must surface it.
    ("fork",   "r3k2r/ppp2ppp/2n5/1B1qp3/1b1P4/2N2N2/PPP2PPP/R2Q1RK1 w kq - 0 1", ["c3d5"]),
    #   Nxd5 wins material via a knight fork/discovery (SF +913 vs +655 for 2nd).
    #
    # NOTE: keep every entry independently reference-verified before adding it (a
    # wrong "known best move" would reject GOOD changes -- worse than no gate). The
    # mate-in-2/mate-in-4 unit tests in the C# suite remain the primary mate gate;
    # this list adds fast, engine-level (full-search, fixed-time) binary checks on
    # top. GROW this list whenever a real regression is found -- capturing a
    # regression as a permanent tripwire is the most valuable kind of entry.
]


def load_positions_file(path):
    """Load a shared FEN set: one bare FEN or ``name | FEN`` per line."""
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
    p = argparse.ArgumentParser(
        description="UCI engine performance + search-quality benchmark (cheap gates)")
    # Default to the KnightByte release binary. KNIGHTBYTE_ENGINE wins; the
    # predecessor's KNIGHTBYTE_BASELINE_ENGINE is kept as a fallback for A/B runs.
    _EXE = ".exe" if os.name == "nt" else ""
    DEFAULT_ENGINE = (
        os.environ.get("KNIGHTBYTE_ENGINE", "").strip()
        or os.environ.get("KNIGHTBYTE_BASELINE_ENGINE", "").strip()
        or os.path.join(SCRIPT_DIR, "..", "target", "release", "knightbyte" + _EXE)
    )

    p.add_argument("--engine", default=DEFAULT_ENGINE,
                   help="UCI engine command, quoted (default: $KNIGHTBYTE_ENGINE, "
                        "$KNIGHTBYTE_BASELINE_ENGINE, or target/release/knightbyte)")
    p.add_argument("--hash", type=int, default=256, help="hash size in MB (default: 256)")
    p.add_argument("--movetime", type=int, default=5000,
                   help="search time per position in ms (default: 5000)")
    p.add_argument("--depth", type=str, default=None, metavar="N | d1,d2,d3,d4",
                   help="search to FIXED depths instead of a fixed time. Machine-"
                        "independent: nodes/EBF/stability then reproduce on any "
                        "hardware, which is what a baseline needs. Either one "
                        "depth for every position (\"18\") or one per position in "
                        "order (\"15,17,18,22\"). With this set, --movetime is "
                        "ignored for the quality/scaling gates.")
    p.add_argument("--threads", default="1,4,8", help="thread counts to test (default: 1,4,8)")
    p.add_argument("--positions", type=int, default=0,
                   help="how many of the built-in scaling positions (default: all)")
    p.add_argument("--positions-file", default=None,
                   help="load positions from a file instead of the built-in set "
                        "(one FEN or 'name | FEN' per line)")
    p.add_argument("--json-out", default=None, metavar="PATH",
                   help="write a machine-readable result (metadata + gates) to PATH")
    p.add_argument("--tactics", action="store_true",
                   help="run the tactical mini-suite hard gate (binary pass/fail)")
    p.add_argument("--tactics-movetime", type=int, default=2000,
                   help="movetime per tactical-suite position in ms (default: 2000)")
    p.add_argument("--stability-window", type=int, default=4,
                   help="how many final iterations to check for score/move stability (default: 4)")
    p.add_argument("--setoption", action="append", default=[], metavar="NAME=VALUE",
                   help="extra UCI setoption to send at startup (repeatable), "
                        "e.g. --setoption \"Use Improving Flag=false\"")
    return p.parse_args()


class EngineSession:
    """
    One UCI engine process. Handles the handshake, lets Threads be re-configured
    between searches, and runs a single search while capturing the full iteration
    stream (needed for the stability and selective-margin gates).

    Only standard UCI is used, so this class is reusable with any UCI engine.
    """

    def __init__(self, cmd, hash_mb, extra_options=None):
        env = dict(os.environ)
        # Ensure a UTF-8 locale is set: some .NET engines on macOS fail at startup
        # with 'System.ArgumentException: 1 is not a supported code page' when
        # LANG/LC_ALL are unset. Harmless for native engines.
        if not env.get("LANG"):
            env["LANG"] = "en_US.UTF-8"
        if not env.get("LC_ALL"):
            env["LC_ALL"] = "en_US.UTF-8"

        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1, env=env)
        self._send("uci")
        self._wait_for("uciok", 15)
        self._send(f"setoption name Hash value {hash_mb}")
        # Disable the opening book: we measure SEARCH, not book moves. Without
        # this, in-book root positions return an instant book move and emit no
        # info lines. (Harmlessly ignored by engines without an "Own Book" option.)
        self._send("setoption name Own Book value false")
        # Extra UCI options (repeatable --setoption "NAME=VALUE"), e.g. to flip a
        # hidden search toggle for A/B isolation.
        for name, value in (extra_options or []):
            self._send(f"setoption name {name} value {value}")
            print(f"  setoption : {name} = {value}")
        self._send("isready")
        self._wait_for("readyok", 15)

    def _send(self, cmd):
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.flush()

    def _wait_for(self, token, timeout_s):
        deadline = time.time() + timeout_s
        while time.time() < deadline:
            line = self.proc.stdout.readline()
            if not line:
                err = ""
                try:
                    err = self.proc.stderr.read()
                except Exception:
                    pass
                err_msg = f": {err.strip()}" if err and err.strip() else ""
                raise RuntimeError(f"engine terminated during handshake{err_msg}")
            if line.strip() == token:
                return
        raise TimeoutError(f"engine did not send '{token}' within {timeout_s}s")

    def set_threads(self, n):
        self._send(f"setoption name Threads value {n}")
        self._send("isready")
        self._wait_for("readyok", 15)

    @staticmethod
    def _parse_info(parts):
        """
        Parse one `info ...` line into a dict of the fields we care about.
        Handles: depth, seldepth, nodes, nps, time, score cp/mate, pv (first move).
        Returns {} for info lines without a depth (e.g. `info string ...`).
        """
        info = {}
        i = 0
        while i < len(parts):
            tok = parts[i]
            if tok in ("depth", "seldepth", "nodes", "nps", "time") and i + 1 < len(parts):
                try:
                    info[tok] = int(parts[i + 1])
                except ValueError:
                    pass
                i += 2
                continue
            if tok == "score" and i + 2 < len(parts):
                kind = parts[i + 1]            # "cp" or "mate"
                try:
                    val = int(parts[i + 2])
                except ValueError:
                    val = 0
                # Normalise to a single comparable integer. Mate scores are mapped
                # to a large magnitude so they dominate cp scores in comparisons.
                if kind == "mate":
                    info["score"] = (30000 - abs(val)) * (1 if val > 0 else -1)
                    info["is_mate"] = True
                else:
                    info["score"] = val
                    info["is_mate"] = False
                i += 3
                continue
            if tok == "pv" and i + 1 < len(parts):
                info["bestmove"] = parts[i + 1]   # first PV move = current best
                break
            i += 1
        return info

    def search(self, fen, movetime_ms, timeout_margin_s=20, fixed_depth=None):
        """
        Search ONE position, capturing the whole iteration stream.

        Two mutually exclusive budgets:
          movetime_ms  — wall-clock search, `go movetime`. Convenient, but the
                         depth reached DEPENDS ON MACHINE SPEED: on a slower
                         box the final iteration does not fit in the budget, so
                         the tool reports the last COMPLETED depth instead. A
                         baseline recorded this way does not reproduce on other
                         hardware (middlegame: depth 18 needs 2.88 s here and
                         1.8 s there, so 2000 ms reports 17 on one machine and
                         18 on the other, while the NODE COUNTS stay identical).
          fixed_depth  — `go depth N`. Machine-INDEPENDENT: the same nodes are
                         searched everywhere, so nodes/EBF/stability reproduce
                         exactly and nps becomes the machine-speed metric. Use
                         this for baselines you expect to reproduce later.

        Returns a dict:
            depth      : deepest completed iteration
            seldepth   : max selective depth reported (0 if the engine never emits it)
            nodes      : node count of the deepest iteration
            nps        : nodes/sec of the deepest iteration
            time       : ms of the deepest iteration
            bestmove   : first PV move of the deepest iteration (UCI)
            ebf        : effective branching factor = nodes ** (1/depth)
            sel_margin : seldepth - depth (selective margin; 0 if seldepth absent)
            iterations : list of (depth, score, bestmove) for EVERY completed depth
                         (used by the stability gate)
        """
        self._send("ucinewgame")
        self._send("isready")
        self._wait_for("readyok", 15)
        self._send(f"position fen {fen}")
        if fixed_depth is not None:
            self._send(f"go depth {fixed_depth}")
            budget_s = 120          # a depth search is bounded by the engine, not by us
        else:
            self._send(f"go movetime {movetime_ms}")
            budget_s = movetime_ms / 1000.0

        best = {"depth": 0, "seldepth": 0, "nodes": 0, "nps": 0, "time": 0, "bestmove": ""}
        iterations = []   # (depth, score, bestmove) per completed iteration
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
            info = self._parse_info(line.split())
            if "depth" not in info:
                continue
            # Record one entry per (deepest-per-depth) info line for stability tracking.
            if "score" in info and "bestmove" in info:
                iterations.append((info["depth"], info["score"], info["bestmove"]))
            # Track the deepest completed iteration for the summary metrics.
            if "nodes" in info and info["depth"] >= best["depth"]:
                best = {**best, **info}

        if best["depth"] == 0:
            raise RuntimeError(f"engine produced no info lines for: {fen}")

        depth = best["depth"]
        nodes = best.get("nodes", 0)
        seldepth = best.get("seldepth", 0)
        ebf = nodes ** (1.0 / depth) if depth > 0 and nodes > 0 else 0.0
        sel_margin = (seldepth - depth) if seldepth else 0

        return {
            "depth": depth,
            "seldepth": seldepth,
            "nodes": nodes,
            "nps": best.get("nps", 0),
            "time": best.get("time", 0),
            "bestmove": best.get("bestmove", ""),
            "ebf": ebf,
            "sel_margin": sel_margin,
            "iterations": iterations,
        }

    def quit(self):
        try:
            self._send("quit")
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()


def stability_report(iterations, window):
    """
    GATE 4 -- score / best-move stability over the final `window` iterations.

    Given the per-iteration (depth, score, bestmove) stream, look at the last
    `window` DISTINCT completed depths and report:
        move_stable  : the best move was identical across all of them
        score_spread : max(score) - min(score) over that window (cp)

    A stable best move + small score spread late in the search means the engine
    is confident. Late move-changes or a large score spread mean the search is
    thrashing -- a warning that a pruning change is discarding relevant moves.
    """
    # Reduce to the last reported entry per depth (engines emit several per depth).
    last_per_depth = {}
    for depth, score, move in iterations:
        last_per_depth[depth] = (score, move)
    depths = sorted(last_per_depth)[-window:]
    if not depths:
        return {"move_stable": True, "score_spread": 0, "depths_used": 0}
    moves = [last_per_depth[d][1] for d in depths]
    scores = [last_per_depth[d][0] for d in depths]
    return {
        "move_stable": len(set(moves)) == 1,
        "score_spread": (max(scores) - min(scores)) if scores else 0,
        "depths_used": len(depths),
    }


def run_quality_and_scaling(session, thread_counts, positions, movetime, stability_window,
                            fixed_depth=None):
    """
    Run the scaling + quality metrics (gates 1-4) for each thread count.
    Prints a per-position table (with nodes / EBF / selective margin / stability)
    and the classic scaling summary at the end.

    Pass fixed_depth to search `go depth N` instead of `go movetime` — the only
    mode whose nodes/EBF reproduce on other machines. It may be ONE depth for
    every position, or ONE PER POSITION in order (a list/tuple), which is what
    makes a historical movetime baseline reproducible: the baseline reached a
    different depth per position (15/17/18/22), so reproducing it needs those
    same depths again.
    """
    results = {}  # threads -> list of per-position dicts

    # one depth for all, or one per position in order
    depths = None
    if fixed_depth is not None:
        depths = list(fixed_depth) if isinstance(fixed_depth, (list, tuple)) else [fixed_depth]

    for n in thread_counts:
        session.set_threads(n)
        print(f"--- Threads = {n} " + "-" * 62)
        # Per-position detail table with the new cheap-gate columns.
        print(f"  {'position':<11} {'depth':>5} {'sel':>4} {'margin':>6} "
              f"{'nodes':>13} {'EBF':>5} {'nps':>11}  stability")
        rows = []
        for idx, (name, fen) in enumerate(positions):
            d = depths[idx % len(depths)] if depths else None
            r = session.search(fen, movetime, fixed_depth=d)
            stab = stability_report(r["iterations"], stability_window)
            r["stability"] = stab
            r["name"] = name
            rows.append(r)
            stab_str = ("move-stable" if stab["move_stable"] else "MOVE-CHANGING") + \
                       f", spread {stab['score_spread']:>4}cp"
            print(f"  {name:<11} {r['depth']:>5} {r['seldepth']:>4} {r['sel_margin']:>6} "
                  f"{r['nodes']:>13,} {r['ebf']:>5.2f} {r['nps']:>11,}  {stab_str}")
        results[n] = rows
        print()

    # ------------------------------------------------------------------
    # Classic scaling summary (unchanged): avg depth / avg NPS / speedup / efficiency.
    # ------------------------------------------------------------------
    print("=" * 78)
    print(f"{'Threads':>7} | {'avg depth':>9} | {'avg NPS':>14} | {'speedup':>8} | {'efficiency':>10}")
    print("-" * 78)
    base_nps = None
    for n in thread_counts:
        rows = results.get(n, [])
        if not rows:
            continue
        avg_nps = sum(r["nps"] for r in rows) / len(rows)
        avg_depth = sum(r["depth"] for r in rows) / len(rows)
        if n == thread_counts[0]:
            base_nps = avg_nps
        speedup = avg_nps / base_nps if base_nps else 0.0
        eff = speedup / n * 100.0
        print(f"{n:>7} | {avg_depth:>9.1f} | {avg_nps:>14,.0f} | {speedup:>7.2f}x | {eff:>9.1f} %")
    print("=" * 78)

    # ------------------------------------------------------------------
    # Cheap-gate aggregate summary (single-thread row is the cleanest signal:
    # no SMP nondeterminism). These are the numbers to record per change and
    # compare against the baseline. LOWER nodes/EBF at equal depth + intact
    # tactics = a strong SPRT candidate (see §8.4 decision table).
    # ------------------------------------------------------------------
    base_rows = results.get(thread_counts[0], [])
    if base_rows:
        avg_ebf = sum(r["ebf"] for r in base_rows) / len(base_rows)
        avg_margin = sum(r["sel_margin"] for r in base_rows) / len(base_rows)
        total_nodes = sum(r["nodes"] for r in base_rows)
        any_unstable = any(not r["stability"]["move_stable"] for r in base_rows)
        print(f"  cheap-gate summary @ {thread_counts[0]} thread(s):")
        print(f"    total nodes (all positions) : {total_nodes:,}")
        print(f"    avg EBF                      : {avg_ebf:.3f}   (lower = deeper per node)")
        print(f"    avg selective margin         : {avg_margin:.2f}  (seldepth - depth)")
        print(f"    any late move-instability    : {'YES (suspicious)' if any_unstable else 'no'}")
        print("=" * 78)

    return results


def run_tactics_gate(session, movetime_ms):
    """
    GATE 5 -- tactical mini-suite (binary pass/fail).

    Runs each curated position at a fixed movetime on a SINGLE thread (for
    determinism) and checks the engine's best move against the accepted set.
    Returns ``(ok, details)``. A single failure is a hard stop: the change under
    test is over-pruning and must be backed off.
    """
    session.set_threads(1)
    print("=" * 78)
    print("  TACTICAL MINI-SUITE (hard gate: every position must find its known move)")
    print("=" * 78)
    passed = 0
    failed = []
    details = []
    for tid, fen, accepted in TACTICS:
        r = session.search(fen, movetime_ms)
        got = r["bestmove"]
        ok = got in accepted
        status = "PASS" if ok else "FAIL"
        print(f"  [{status}] {tid:<14} got {got:<6} (accepted: {', '.join(accepted)})  "
              f"depth {r['depth']}")
        details.append({
            "id": tid,
            "fen": fen,
            "got": got,
            "accepted": list(accepted),
            "pass": ok,
            "depth": r["depth"],
        })
        if ok:
            passed += 1
        else:
            failed.append(tid)
    print("-" * 78)
    print(f"  RESULT: {passed}/{len(TACTICS)} passed"
          + ("" if not failed else f"   FAILED: {', '.join(failed)}"))
    print("=" * 78)
    return len(failed) == 0, details


def split_command(cmd):
    r"""Split a command line into argv, using the RULES OF THIS PLATFORM.

    `shlex.split()` defaults to posix=True, where a backslash is an ESCAPE
    character. That is wrong on Windows, and it fails in a way that looks like a
    missing file rather than a parse error: the perfectly natural

        target\\release\\engine.exe

    is split into

        targetreleaseengine.exe

    so `--engine` silently becomes a different (non-existent) path and the tool
    reports "engine executable not found". Forward slashes masked this for years;
    it is not a workaround, it is a symptom.

    On Windows, use the native CommandLineToArgvW rules (posix=False) and then
    drop the quotes that mode deliberately keeps, so a quoted path containing
    spaces still arrives as one argument.
    """
    if os.name == "nt":
        parts = shlex.split(cmd, posix=False)
        return [p[1:-1] if len(p) >= 2 and p[0] == p[-1] == '"' else p for p in parts]
    return shlex.split(cmd)


def main():
    args = parse_args()

    engine_cmd = split_command(args.engine)
    if not engine_cmd or not (os.path.sep in engine_cmd[0] and os.path.exists(engine_cmd[0])
                              or shutil.which(engine_cmd[0])):
        print(f"ERROR: engine executable not found: {engine_cmd[0]}")
        sys.exit(1)

    thread_counts = [int(t) for t in args.threads.split(",")]
    if args.positions_file:
        try:
            positions = load_positions_file(args.positions_file)
        except (OSError, ValueError) as exc:
            print(f"ERROR: {exc}")
            sys.exit(1)
    else:
        positions = POSITIONS[: args.positions] if args.positions else POSITIONS

    # Parse --setoption "NAME=VALUE" entries into (name, value) pairs.
    extra_options = []
    for entry in args.setoption:
        if "=" not in entry:
            print(f"ERROR: --setoption must be NAME=VALUE, got: {entry}")
            sys.exit(1)
        name, _, value = entry.partition("=")
        extra_options.append((name.strip(), value.strip()))

    # --depth may be one depth for every position ("18") or one per position
    # in order ("15,17,18,22") — the latter is what reproduces a historical
    # movetime baseline, whose depths were an artifact of the machine it ran on.
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

    print("=" * 78)
    print("UCI engine performance + search-quality benchmark  (cheap gates)")
    print("=" * 78)
    print(f"  engine    : {engine_cmd}")
    print(f"  hash      : {args.hash} MB")
    if depths:
        shown = (f"{depths[0]} (all positions)" if len(depths) == 1
                 else ", ".join(f"{n}={d}" for (n, _), d in zip(positions, depths)))
        print(f"  depth     : FIXED per position -> {shown}")
        print(f"               machine-INDEPENDENT: nodes/EBF/stability reproduce anywhere;")
        print(f"               nps is the machine-speed readout.")
    else:
        print(f"  movetime  : {args.movetime} ms per position  (machine-DEPENDENT: the depth")
        print(f"               reached depends on nps, so this does not reproduce on other hardware)")
    print(f"  threads   : {thread_counts}")
    print(f"  positions : {[name for name, _ in positions]}")
    if args.tactics:
        print(f"  tactics   : ON ({len(TACTICS)} positions @ {args.tactics_movetime} ms)")
    print()

    session = EngineSession(engine_cmd, args.hash, extra_options)
    tactics_ok = True
    quality_results = {}
    tactics_details = []
    try:
        quality_results = run_quality_and_scaling(session, thread_counts, positions,
                                                  args.movetime, args.stability_window,
                                                  fixed_depth=depths)
        if args.tactics:
            print()
            tactics_ok, tactics_details = run_tactics_gate(session, args.tactics_movetime)
    except KeyboardInterrupt:
        print("\n  interrupted")
    finally:
        session.quit()

    if args.json_out:
        def clean_row(row):
            return {key: value for key, value in row.items() if key != "iterations"}

        payload = {
            "tool": "perf_test.py",
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "host": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "processor": platform.processor(),
            },
            "engine": {"command": engine_cmd},
            "net_sha256": net_sha256(),
            "settings": {
                "hash_mb": args.hash,
                "threads": thread_counts,
                "movetime_ms": None if depths else args.movetime,
                "depths": depths,
                "positions": [name for name, _ in positions],
                "setoption": extra_options,
                "tactics": args.tactics,
                "tactics_movetime_ms": args.tactics_movetime if args.tactics else None,
            },
            "quality": {
                str(threads): [clean_row(row) for row in rows]
                for threads, rows in quality_results.items()
            },
            "tactics": {
                "ok": tactics_ok,
                "total": len(TACTICS) if args.tactics else 0,
                "rows": tactics_details,
            },
        }
        with open(args.json_out, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        print(f"  wrote {args.json_out}")

    # Exit non-zero if the tactical hard gate failed, so CI / scripts can detect it.
    if args.tactics and not tactics_ok:
        print("\n  GATE FAILED: tactical mini-suite regressed -- back off the change.")
        sys.exit(1)


if __name__ == "__main__":
    main()
